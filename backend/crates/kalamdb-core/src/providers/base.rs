//! Base trait for table providers with unified DML operations
//!
//! This module provides:
//! - BaseTableProvider<K, V> trait for generic table operations
//! - TableProviderCore shared structure for common services
//!
//! **Design Rationale**:
//! - Eliminates ~1200 lines of duplicate code across User/Shared/Stream providers
//! - Generic over storage key (K) and value (V) types
//! - No separate handlers - DML logic implemented directly in providers
//! - Shared core reduces memory overhead (Arc<TableProviderCore> vs per-provider fields)

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::live_query::LiveQueryManager;
use crate::schema_registry::TableType;
use crate::storage::storage_registry::StorageRegistry;
use async_trait::async_trait;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::catalog::Session;
use datafusion::datasource::TableProvider;
use datafusion::logical_expr::Expr;
use kalamdb_commons::models::{NamespaceId, TableName, UserId};
use kalamdb_commons::{StorageKey, TableId};
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// Shared core state for all table providers
///
/// **Memory Optimization**: All provider types share this core structure,
/// reducing per-table memory footprint from 3× allocation to 1× allocation.
///
/// **Phase 12 Refactoring**: Uses kalamdb-registry services directly
///
/// **Services**:
/// - `schema_registry`: Table schema management and caching (from kalamdb-registry)
/// - `system_columns`: SeqId generation, _deleted flag handling (from kalamdb-registry)
/// - `live_query_manager`: WebSocket notifications (optional, from kalamdb-core)
/// - `storage_registry`: Storage path resolution (optional, from kalamdb-core)
pub struct TableProviderCore {
    /// Schema registry for table metadata and Arrow schema caching
    pub schema_registry: Arc<crate::schema_registry::SchemaRegistry>,
    
    /// System columns service for _seq and _deleted management
    pub system_columns: Arc<crate::system_columns::SystemColumnsService>,
    
    /// LiveQueryManager for WebSocket notifications (optional)
    pub live_query_manager: Option<Arc<LiveQueryManager>>,
    
    /// Storage registry for resolving full storage paths (optional)
    pub storage_registry: Option<Arc<StorageRegistry>>,
}

impl TableProviderCore {
    /// Create new core with required services from AppContext
    pub fn from_app_context(app_context: &Arc<AppContext>) -> Self {
        Self {
            schema_registry: app_context.schema_registry(),
            system_columns: system_columns,
            live_query_manager: None,
            storage_registry: None,
        }
    }
    
    /// Add LiveQueryManager to core
    pub fn with_live_query_manager(mut self, manager: Arc<LiveQueryManager>) -> Self {
        self.live_query_manager = Some(manager);
        self
    }
    
    /// Add StorageRegistry to core
    pub fn with_storage_registry(mut self, registry: Arc<StorageRegistry>) -> Self {
        self.storage_registry = Some(registry);
        self
    }
}

/// Unified trait for all table providers with generic storage abstraction
///
/// **Key Design Decisions**:
/// - Generic K: StorageKey (UserTableRowId, SharedTableRowId, StreamTableRowId)
/// - Generic V: Row type (UserTableRow, SharedTableRow, StreamTableRow)
/// - Extends DataFusion::TableProvider (same struct serves both custom DML + SQL)
/// - No separate handlers - all DML logic in provider implementations
/// - Stateless providers - user_id passed per-operation, not stored per-user
///
/// **Architecture**:
/// ```text
/// ExecutionContext → SessionState.extensions (SessionUserContext)
///                 ↓
/// Provider.scan_rows(state) → extract_user_context(state)
///                           ↓
/// Provider.scan_with_version_resolution_to_kvs(user_id, filter)
/// ```
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
    ///
    /// Named differently from DataFusion's TableProvider::table_type to avoid ambiguity.
    fn provider_table_type(&self) -> TableType;
    
    /// Get namespace ID from table_id (default implementation)
    fn namespace_id(&self) -> &NamespaceId {
        self.table_id().namespace_id()
    }
    
    /// Get table name from table_id (default implementation)
    fn table_name(&self) -> &TableName {
        self.table_id().table_name()
    }
    
    /// Get RocksDB column family name (default implementation)
    fn column_family_name(&self) -> String {
        format!(
            "{}:{}:{}",
            match <Self as BaseTableProvider<K, V>>::provider_table_type(self) {
                TableType::User => "user_table",
                TableType::Shared => "shared_table",
                TableType::Stream => "stream_table",
                _ => "table",
            },
            self.namespace_id().as_str(),
            self.table_name().as_str()
        )
    }
    
    // ===========================
    // Storage Access
    // ===========================
    
    /// Access to AppContext for SystemColumnsService, SnowflakeGenerator, etc.
    fn app_context(&self) -> &Arc<AppContext>;
    
    /// Primary key field name from schema definition (e.g., "id", "email")
    fn primary_key_field_name(&self) -> &str;
    
    // ===========================
    // DML Operations (Synchronous - No Handlers)
    // ===========================
    
    /// Insert a single row (auto-generates system columns: _seq, _deleted)
    ///
    /// **Implementation**: Calls unified_dml helpers directly
    ///
    /// # Arguments
    /// * `user_id` - Subject user ID for RLS (User/Stream use it, Shared ignores it)
    /// * `row_data` - JSON object containing user-defined columns
    ///
    /// # Returns
    /// Generated storage key (UserTableRowId, SharedTableRowId, or StreamTableRowId)
    ///
    /// # Architecture Note
    /// Providers are stateless. The user_id is passed per-operation by the SQL executor
    /// from ExecutionContext, enabling:
    /// - AS USER impersonation (executor passes subject_user_id)
    /// - Per-request user scoping without per-user provider instances
    /// - Clean separation: executor handles auth/context, provider handles storage
    fn insert(&self, user_id: &UserId, row_data: JsonValue) -> Result<K, KalamDbError>;
    
    /// Insert multiple rows in a batch (optimized for bulk operations)
    ///
    /// # Arguments
    /// * `user_id` - Subject user ID for RLS
    /// * `rows` - Vector of JSON objects
    ///
    /// # Default Implementation
    /// Iterates over rows and calls insert() for each. Providers may override
    /// with batch-optimized implementation.
    fn insert_batch(&self, user_id: &UserId, rows: Vec<JsonValue>) -> Result<Vec<K>, KalamDbError> {
        rows.into_iter()
            .map(|row| self.insert(user_id, row))
            .collect()
    }
    
    /// Update a row by key (appends new version with incremented _seq)
    ///
    /// **Implementation**: Uses version_resolution helpers + unified_dml
    ///
    /// # Arguments
    /// * `user_id` - Subject user ID for RLS
    /// * `key` - Storage key identifying the row
    /// * `updates` - JSON object with column updates
    ///
    /// # Returns
    /// New storage key (new SeqId for versioning)
    fn update(&self, user_id: &UserId, key: &K, updates: JsonValue) -> Result<K, KalamDbError>;
    
    /// Delete a row by key (appends tombstone with _deleted=true)
    ///
    /// **Implementation**: Uses version_resolution helpers + unified_dml
    ///
    /// # Arguments
    /// * `user_id` - Subject user ID for RLS
    /// * `key` - Storage key identifying the row
    fn delete(&self, user_id: &UserId, key: &K) -> Result<(), KalamDbError>;
    
    /// Update multiple rows in a batch (default implementation)
    fn update_batch(&self, user_id: &UserId, updates: Vec<(K, JsonValue)>) -> Result<Vec<K>, KalamDbError> {
        updates.into_iter()
            .map(|(key, update)| self.update(user_id, &key, update))
            .collect()
    }
    
    /// Delete multiple rows in a batch (default implementation)
    fn delete_batch(&self, user_id: &UserId, keys: Vec<K>) -> Result<Vec<()>, KalamDbError> {
        keys.into_iter()
            .map(|key| self.delete(user_id, &key))
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
    /// # Arguments
    /// * `user_id` - Subject user ID for RLS scoping
    /// * `id_value` - Value to search for in the ID field
    ///
    /// # Performance
    /// - User tables: RocksDB prefix scan on {user_id} for efficient scoping
    /// - Shared tables: Full table scan (consider adding index for large tables)
    fn find_row_key_by_id_field(&self, user_id: &UserId, id_value: &str) -> Result<Option<K>, KalamDbError> {
        // Default implementation: scan rows with user scoping and version resolution
        let rows = self.scan_with_version_resolution_to_kvs(user_id, None)?;
        
        for (key, row) in rows {
            if let Some(fields) = Self::extract_fields(&row) {
                if let Some(id) = fields.get(self.primary_key_field_name()) {
                    if id.as_str() == Some(id_value) {
                        return Ok(Some(key));
                    }
                }
            }
        }
        
        Ok(None)
    }
    
    /// Update a row by searching for matching ID field value
    fn update_by_id_field(&self, user_id: &UserId, id_value: &str, updates: JsonValue) -> Result<K, KalamDbError> {
        let key = self.find_row_key_by_id_field(user_id, id_value)?
            .ok_or_else(|| KalamDbError::NotFound(format!("Row with {}={} not found", self.primary_key_field_name(), id_value)))?;
        self.update(user_id, &key, updates)
    }
    
    /// Delete a row by searching for matching ID field value
    fn delete_by_id_field(&self, user_id: &UserId, id_value: &str) -> Result<(), KalamDbError> {
        let key = self.find_row_key_by_id_field(user_id, id_value)?
            .ok_or_else(|| KalamDbError::NotFound(format!("Row with {}={} not found", self.primary_key_field_name(), id_value)))?;
        self.delete(user_id, &key)
    }
    
    // ===========================
    // Scan Operations (with version resolution)
    // ===========================
    
    /// Scan rows with optional filter (merges hot + cold storage with version resolution)
    ///
    /// **Called by DataFusion during query execution via TableProvider::scan()**
    ///
    /// The `state` parameter contains SessionUserContext in extensions,
    /// which providers extract to apply RLS filtering.
    ///
    /// **User/Shared Tables**:
    /// 1. Extract user_id from SessionState.config().options().extensions
    /// 2. Scan RocksDB (hot storage)
    /// 3. Scan Parquet files (cold storage)
    /// 4. Apply version resolution (MAX(_seq) per primary key) via DataFusion
    /// 5. Filter _deleted = false
    /// 6. Apply user filter expression
    /// 7. For User tables: Apply RLS (user_id = subject)
    ///
    /// **Stream Tables**:
    /// 1. Extract user_id from SessionState
    /// 2. Scan ONLY RocksDB (hot storage, no Parquet)
    /// 3. Apply TTL filtering
    /// 4. Filter _deleted = false (if applicable)
    /// 5. Apply user filter expression
    /// 6. Apply RLS (user_id = subject)
    ///
    /// # Arguments
    /// * `state` - DataFusion SessionState (contains SessionUserContext)
    /// * `filter` - Optional DataFusion expression for filtering
    ///
    /// # Returns
    /// RecordBatch with resolved, filtered rows
    ///
    /// # Note
    /// Called by DataFusion's TableProvider::scan(). For direct DML operations,
    /// use scan_with_version_resolution_to_kvs().
    fn scan_rows(&self, state: &dyn Session, filter: Option<&Expr>) -> Result<RecordBatch, KalamDbError>;
    
    /// Scan with version resolution returning key-value pairs (for internal DML use)
    ///
    /// Used by UPDATE/DELETE to find current version before appending new version.
    /// Unlike scan_rows(), this is called directly by DML operations with user_id
    /// passed explicitly.
    ///
    /// # Arguments
    /// * `user_id` - Subject user ID for RLS scoping
    /// * `filter` - Optional DataFusion expression for filtering
    fn scan_with_version_resolution_to_kvs(
        &self,
        user_id: &UserId,
        filter: Option<&Expr>,
    ) -> Result<Vec<(K, V)>, KalamDbError>;
    
    /// Extract fields JSON from row (provider-specific)
    ///
    /// Each provider implements this to access the `fields: JsonValue` from their row type.
    fn extract_fields(row: &V) -> Option<&JsonValue>;
}
