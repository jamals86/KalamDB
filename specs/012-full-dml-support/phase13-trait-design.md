# Phase 13: BaseTableProvider Trait Design

## Objective
Create a unified trait-based architecture that eliminates ~1200 lines of duplicate code across User/Shared/Stream table providers.

## Current Architecture Issues

**UserTableProvider** (~1460 lines):
- Wraps `Arc<UserTableShared>` with delegated methods
- Duplicate DML logic (insert, update, delete, scan)
- Handler-based architecture (InsertHandler, UpdateHandler, DeleteHandler)

**SharedTableProvider** (~915 lines):
- Similar structure with duplicate DML methods
- Different storage key (SeqId vs UserTableRowId)
- Nearly identical version resolution logic

**StreamTableProvider** (~923 lines):
- Similar DML methods
- Hot-only storage (no Parquet merging)
- TTL-based eviction

**UserTableShared** (200+ lines):
- Singleton wrapper holding handlers and defaults
- Created once per table, Arc-cloned per request
- Adds indirection layer

**TableProviderCore** (130 lines):
- Common fields (table_id, cache, storage_id, created_at)
- Shared by all provider types

## Simplified Design (Phase 13)

### Trait Definition

```rust
use kalamdb_store::StorageKey;
use datafusion::datasource::TableProvider;
use async_trait::async_trait;

/// Unified trait for all table providers with generic storage abstraction
///
/// **Key Design Decisions**:
/// - Generic K: StorageKey (UserTableRowId, SeqId, StreamTableRowId)
/// - Generic V: Row type (UserTableRow, SharedTableRow, StreamTableRow)
/// - Extends DataFusion::TableProvider (same struct serves both custom DML + SQL)
/// - No separate handlers - all DML logic in provider implementations
/// - Shared core (AppContext, LiveQueryManager, StorageRegistry) via TableProviderCore
///
#[async_trait]
pub trait BaseTableProvider<K: StorageKey, V>: Send + Sync + TableProvider {
    // ===========================
    // Core Metadata (read-only)
    // ===========================
    
    /// Table identifier (namespace + table name)
    fn table_id(&self) -> &TableId;
    
    /// Memoized Arrow schema (Phase 10 optimization: 50-100× faster than recomputation)
    fn schema_ref(&self) -> SchemaRef;
    
    /// Logical table type (User, Shared, Stream)
    fn table_type(&self) -> TableType;
    
    /// Get namespace ID from table_id (default implementation)
    fn namespace_id(&self) -> &NamespaceId {
        self.table_id().namespace_id()
    }
    
    /// Get table name from table_id (default implementation)
    fn table_name(&self) -> &TableName {
        self.table_id().table_name()
    }
    
    // ===========================
    // Storage Access
    // ===========================
    
    /// Access to underlying EntityStore (RocksDB-backed)
    fn store(&self) -> &Arc<dyn EntityStore<K, V>>;
    
    /// Access to AppContext for SystemColumnsService, SnowflakeGenerator, etc.
    fn app_context(&self) -> &Arc<AppContext>;
    
    // ===========================
    // DML Operations (Synchronous - No Handlers)
    // ===========================
    
    /// Insert a single row (auto-generates system columns: _seq, _deleted)
    ///
    /// **Implementation**: Calls unified_dml::append_version_sync() directly
    ///
    /// # Arguments
    /// * `row_data` - JSON object containing user-defined columns
    ///
    /// # Returns
    /// Generated storage key (UserTableRowId, SeqId, or StreamTableRowId)
    fn insert(&self, row_data: JsonValue) -> Result<K, KalamDbError>;
    
    /// Insert multiple rows in a batch (optimized for bulk operations)
    fn insert_batch(&self, rows: Vec<JsonValue>) -> Result<Vec<K>, KalamDbError>;
    
    /// Update a row by key (appends new version with incremented _seq)
    ///
    /// **Implementation**: Uses version_resolution helpers + unified_dml::append_version_sync()
    ///
    /// # Arguments
    /// * `key` - Storage key identifying the row
    /// * `updates` - JSON object with column updates
    ///
    /// # Returns
    /// New storage key (new SeqId for versioning)
    fn update(&self, key: &K, updates: JsonValue) -> Result<K, KalamDbError>;
    
    /// Delete a row by key (appends tombstone with _deleted=true)
    ///
    /// **Implementation**: Uses version_resolution helpers + unified_dml::append_version_sync()
    fn delete(&self, key: &K) -> Result<(), KalamDbError>;
    
    /// Update multiple rows in a batch
    fn update_batch(&self, updates: Vec<(K, JsonValue)>) -> Result<Vec<K>, KalamDbError> {
        updates.into_iter()
            .map(|(key, update)| self.update(&key, update))
            .collect()
    }
    
    /// Delete multiple rows in a batch
    fn delete_batch(&self, keys: Vec<K>) -> Result<Vec<()>, KalamDbError> {
        keys.into_iter()
            .map(|key| self.delete(&key))
            .collect()
    }
    
    // ===========================
    // Convenience Methods (with default implementations)
    // ===========================
    
    /// Find row key by ID field value
    /// 
    /// Scans rows with version resolution and returns the key of the first row
    /// where `fields.id == id_value`. The returned key K already contains user_id
    /// for user/stream tables (embedded in UserTableRowId/StreamTableRowId).
    ///
    /// # Performance
    /// - User tables: Can optimize with RocksDB prefix scan if provider extracts user_id from context
    /// - Shared tables: Full table scan (consider adding index for large tables)
    fn find_row_key_by_id_field(&self, id_value: &str) -> Result<Option<K>, KalamDbError> {
        // Default implementation: scan all rows with version resolution
        let rows = self.scan_with_version_resolution_to_kvs(None)?;
        
        for (key, row) in rows {
            if let Some(fields) = Self::extract_fields(&row) {
                if let Some(id) = fields.get("id") {
                    if id.as_str() == Some(id_value) {
                        return Ok(Some(key));
                    }
                }
            }
        }
        
        Ok(None)
    }
    
    /// Update a row by searching for matching ID field value
    fn update_by_id_field(&self, id_value: &str, updates: JsonValue) -> Result<K, KalamDbError> {
        let key = self.find_row_key_by_id_field(id_value)?
            .ok_or_else(|| KalamDbError::RowNotFound(format!("Row with id={} not found", id_value)))?;
        self.update(&key, updates)
    }
    
    /// Delete a row by searching for matching ID field value
    fn delete_by_id_field(&self, id_value: &str) -> Result<(), KalamDbError> {
        let key = self.find_row_key_by_id_field(id_value)?
            .ok_or_else(|| KalamDbError::RowNotFound(format!("Row with id={} not found", id_value)))?;
        self.delete(&key)
    }
    
    // ===========================
    // Scan Operations (with version resolution)
    // ===========================
    
    /// Scan rows with optional filter (merges hot + cold storage with version resolution)
    ///
    /// **User/Shared Tables**:
    /// 1. Scan RocksDB (hot storage) → fast_batch
    /// 2. Scan Parquet files (cold storage) → cold_batch
    /// 3. Apply version resolution (MAX(_seq) per primary key) via DataFusion
    /// 4. Filter _deleted = false via DataFusion
    /// 5. Apply user filter expression
    ///
    /// **Stream Tables**:
    /// 1. Scan ONLY RocksDB (hot storage)
    /// 2. Apply TTL filtering
    /// 3. Filter _deleted = false
    /// 4. Apply user filter expression
    ///
    /// # Arguments
    /// * `filter` - Optional DataFusion expression for filtering
    ///
    /// # Returns
    /// RecordBatch with resolved, filtered rows
    fn scan_rows(&self, filter: Option<&Expr>) -> Result<RecordBatch, KalamDbError>;
    
    /// Scan with version resolution returning key-value pairs (for internal DML use)
    ///
    /// Used by UPDATE/DELETE to find current version before appending new version.
    fn scan_with_version_resolution_to_kvs(
        &self,
        filter: Option<&Expr>,
    ) -> Result<Vec<(K, V)>, KalamDbError>;
    
    /// Extract fields JSON from row (provider-specific)
    ///
    /// Each provider implements this to access the `fields: JsonValue` from their row type.
    fn extract_fields(row: &V) -> Option<&JsonValue>;
    
    /// Get RocksDB column family name for this table
    fn column_family_name(&self) -> String {
        format!(
            "{}:{}:{}",
            match self.table_type() {
                TableType::User => "user_table",
                TableType::Shared => "shared_table",
                TableType::Stream => "stream_table",
                _ => "table",
            },
            self.namespace_id().as_str(),
            self.table_name().as_str()
        )
    }
}
```

### Provider Implementations

#### UserTableProvider (Simplified)

```rust
pub struct UserTableProvider {
    // Core fields (no wrappers)
    table_id: Arc<TableId>,
    schema: SchemaRef,
    table_type: TableType,
    
    // Storage
    store: Arc<UserTableStore>,
    app_context: Arc<AppContext>,
    
    // Optional features (builder pattern)
    live_query_manager: Option<Arc<LiveQueryManager>>,
    storage_registry: Option<Arc<StorageRegistry>>,
    
    // Cached metadata
    column_defaults: Arc<HashMap<String, ColumnDefault>>,
}

impl BaseTableProvider<UserTableRowId, UserTableRow> for UserTableProvider {
    fn table_id(&self) -> &TableId { &self.table_id }
    fn schema_ref(&self) -> SchemaRef { self.schema.clone() }
    fn table_type(&self) -> TableType { self.table_type }
    fn store(&self) -> &Arc<dyn EntityStore<UserTableRowId, UserTableRow>> { &self.store }
    fn app_context(&self) -> &Arc<AppContext> { &self.app_context }
    
    async fn insert(&self, row_data: JsonValue) -> Result<UserTableRowId, KalamDbError> {
        // Extract user_id from context (not parameter!)
        let user_id = self.extract_user_id_from_context()?;
        
        // Call unified_dml module
        unified_dml::insert_user_table_row(
            &self.store,
            &self.app_context,
            &self.table_id,
            &user_id,
            row_data,
        ).await
    }
    
    async fn update(&self, key: &UserTableRowId, updates: JsonValue) -> Result<UserTableRowId, KalamDbError> {
        // 1. Scan RocksDB for hot versions
        // 2. Scan Parquet for cold versions  
        // 3. Apply version resolution (MAX(_seq))
        // 4. Merge updates into latest version
        // 5. Append new version via unified_dml
        unified_dml::update_user_table_row(
            &self.store,
            &self.app_context,
            &self.table_id,
            key,
            updates,
        ).await
    }
    
    async fn scan_rows(&self, filter: Option<Expr>) -> Result<RecordBatch, KalamDbError> {
        // Extract user_id for RLS filtering
        let user_id = self.extract_user_id_from_context()?;
        
        // Scan RocksDB + Parquet with version resolution
        scan_with_version_resolution(
            &self.store,
            &self.table_id,
            &user_id,
            filter,
        ).await
    }
}
```

#### SharedTableProvider (Simplified)

```rust
pub struct SharedTableProvider {
    table_id: Arc<TableId>,
    schema: SchemaRef,
    table_type: TableType,
    store: Arc<SharedTableStore>,
    app_context: Arc<AppContext>,
    column_defaults: Arc<HashMap<String, ColumnDefault>>,
}

impl BaseTableProvider<SeqId, SharedTableRow> for SharedTableProvider {
    // Same structure as UserTableProvider
    // DML methods use identical unified_dml functions
    // scan_rows() merges RocksDB + Parquet (no RLS filtering)
}
```

#### StreamTableProvider (Simplified)

```rust
pub struct StreamTableProvider {
    table_id: Arc<TableId>,
    schema: SchemaRef,
    table_type: TableType,
    store: Arc<StreamTableStore>,
    app_context: Arc<AppContext>,
    ttl_seconds: Option<u64>,
}

impl BaseTableProvider<StreamTableRowId, StreamTableRow> for StreamTableProvider {
    // DML methods use ONLY RocksDB (hot storage)
    // scan_rows() does NOT merge Parquet (ephemeral data)
    // TTL filtering applied in scan_rows()
}
```

## Code Reduction Analysis

### Eliminated Components

1. **UserTableShared** (~200 lines) - Eliminated wrapper
2. **TableProviderCore** (~130 lines) - Merged into providers
3. **Duplicate DML methods** (~800 lines):
   - insert(): 3 implementations → 1 trait + 3 thin wrappers
   - update(): 3 implementations → 1 unified function
   - delete(): 3 implementations → 1 unified function
   - scan_rows(): 3 implementations → 1 trait with hot/cold strategy

### Total Reduction: ~1200 lines (36% of provider code)

## Migration Strategy

### Phase 1: Create Trait (T200-T204)
- Define BaseTableProvider<K, V> trait
- Document trait methods and semantics

### Phase 2: StreamTableProvider (T205-T210) ✅ COMPLETE
- Refactor StreamTableRow to MVCC
- Implement trait for StreamTableProvider

### Phase 3: UserTableProvider (T211-T218)
- Eliminate UserTableShared wrapper
- Implement trait
- Update all call sites

### Phase 4: SharedTableProvider (T219-T225)
- Same pattern as UserTableProvider
- Implement trait

### Phase 5: Cleanup (T233-T239)
- Delete old code
- Run full test suite
- Measure reduction

## Testing Strategy

- **Unit Tests**: Test each provider implementation independently
- **Integration Tests**: Verify DataFusion SQL queries work through trait
- **Smoke Tests**: Run existing smoke tests (should pass without changes)
- **Performance Tests**: Verify no regression in DML operation latency
