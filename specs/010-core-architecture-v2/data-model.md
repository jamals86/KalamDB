# Data Model: Core Architecture Refactoring v2

**Date**: 2025-11-06  
**Feature**: 010-core-architecture-v2  
**Purpose**: Define key entities, relationships, and state transitions for refactored architecture

## Core Entities

### AppContext (Modified)

**Purpose**: Singleton containing all global configuration and state. Single owner of NodeId.

**Fields**:
```rust
pub struct AppContext {
    node_id: Arc<NodeId>,                        // NEW: Loaded from config.toml
    schema_registry: Arc<SchemaRegistry>,        // Existing
    system_tables: Arc<SystemTablesRegistry>,    // Existing
    job_manager: Arc<UnifiedJobManager>,         // Existing
    live_query_manager: Arc<LiveQueryManager>,   // Modified (consolidated)
    user_table_store: Arc<dyn StorageBackend>,   // Existing
    shared_table_store: Arc<dyn StorageBackend>, // Existing
    stream_table_store: Arc<dyn StorageBackend>, // Existing
}
```

**Relationships**:
- Owns NodeId (1:1, loaded from config)
- References SchemaRegistry (1:1 singleton)
- References SystemTablesRegistry (1:1 singleton)
- References LiveQueryManager (1:1 consolidated)
- Passed by Arc reference to all components

**Validation Rules**:
- NodeId must be valid identifier (alphanumeric + hyphens, max 64 chars)
- All Arc fields must be non-null (initialization validates)
- Thread-safe (Arc enables cross-thread sharing)

**State Transitions**: None (immutable after initialization)

---

### SchemaCache (Modified) → Will be renamed to SchemaRegistry

**Purpose**: Unified cache for table metadata with embedded Arrow schema memoization.

**Note**: This struct will be renamed from `SchemaCache` to `SchemaRegistry` to better reflect its role as the central schema management component.

**Fields**:
```rust
pub struct SchemaCache {  // TODO: Rename to SchemaRegistry
    cache: DashMap<TableId, Arc<CachedTableData>>,     // Single source of truth
    user_table_shared: DashMap<TableId, Arc<UserTableShared>>, // Existing
    lru_timestamps: DashMap<TableId, AtomicU64>,        // Existing
    providers: DashMap<TableId, Arc<dyn TableProvider>>, // Cached providers
}

/// **Clone Semantics**: Cloning creates a new Arc pointing to the same RwLock,
/// meaning all clones share the cached Arrow schema. This is intentional.
#[derive(Debug, Clone)]
pub struct CachedTableData {
    pub table: Arc<TableDefinition>,
    pub storage_id: Option<StorageId>,
    pub storage_path_template: String,
    pub schema_version: u32,
    
    // NEW: Lazy-initialized Arrow schema (compute once, cache forever)
    arrow_schema: Arc<RwLock<Option<Arc<Schema>>>>,  // std::sync::RwLock
}
```

**Relationships**:
- Contains CachedTableData with embedded Arrow Schemas (1:N, one per table)
- CachedTableData owns lazy-initialized Arrow schema (1:1, computed on first access)
- Referenced by SchemaRegistry (1:1 composition)

**Validation Rules**:
- TableId must exist in cache before arrow_schema() called
- Arc<Schema> never modified after initialization (immutable)
- Invalidation removes CachedTableData (Arrow schema removed automatically)

**State Transitions**:
```
[Empty] --insert--> [Cached (schema=None)]
[Cached (schema=None)] --arrow_schema()--> [Cached (schema=Some)]
[Cached (schema=Some)] --invalidate()--> [Empty]
[Cached (schema=Some)] --ALTER TABLE--> [Empty] --insert--> [Cached (schema=None)]
```

**Performance Characteristics**:
- arrow_schema() first call: 50-100μs (compute + cache)
- arrow_schema() subsequent calls: 1-2μs (Arc::clone from RwLock read)
- Memory overhead: ~16 bytes per CachedTableData (Arc + RwLock), ~1-2KB per computed schema
- Concurrency: Lock-free DashMap + read-optimized RwLock (write once, read many)

---

### TableProviderCore (Modified)

**Purpose**: Common core struct for all TableProvider implementations.

**Fields** (actual current state):
```rust
pub struct TableProviderCore {
    pub table_id: Arc<TableId>,
    pub table_type: TableType,
    // WILL REMOVE: pub schema: SchemaRef,  // OLD: Pre-computed schema (redundant)
    pub created_at_ms: u64,
    pub storage_id: Option<StorageId>,
    pub unified_cache: Arc<SchemaCache>,  // Actual field name
}
```

**New Method**:
```rust
impl TableProviderCore {
    pub fn arrow_schema(&self) -> Result<Arc<Schema>> {
        self.unified_cache.get_arrow_schema(&self.table_id)
    }
}
```

**Changes Required**:
1. Remove `schema: SchemaRef` field (redundant with memoization)
2. Add `arrow_schema()` method delegating to `unified_cache`
3. Update constructor to not require pre-computed schema

**Relationships**:
- Embedded in 11 TableProvider implementations (composition)
- References SchemaCache (via `unified_cache` field) for memoized schema access

**Validation Rules**:
- table_id must exist in SchemaCache before arrow_schema() called
- Returns Error if table not found or schema invalid
- TableProvider.schema() must panic on error (not return empty schema)

---

### LiveQueryManager (Consolidated)

**Purpose**: Unified manager for all live query subscriptions, WebSocket connections, and filter caching.

**Fields**:
```rust
pub struct LiveQueryManager {
    // Merged from 3 structs
    subscriptions: RwLock<LiveQueryRegistry>,           // From LiveQueryManager
    connections: DashMap<ConnectionId, UserConnection>, // From UserConnections
    filter_cache: RwLock<FilterCache>,                  // From UserTableChangeDetector
    
    // Shared dependencies (from AppContext)
    schema_registry: Arc<SchemaRegistry>,
    node_id: Arc<NodeId>,
    live_queries_provider: Arc<LiveQueriesTableProvider>,
}
```

**Relationships**:
- Contains LiveQueryRegistry (1:1, subscription metadata)
- Contains UserConnection map (1:N, one per WebSocket)
- Contains FilterCache (1:1, compiled filters)
- References SchemaRegistry for table metadata
- References NodeId for distributed logging

**Validation Rules**:
- ConnectionId must be unique per connection
- Subscription must reference existing table_id (validated via schema_registry)
- Filter must be valid SQL WHERE clause (parsed on subscription)

**State Transitions**:
```
[No Connection] --connect()--> [Connected]
[Connected] --subscribe()--> [Connected + Subscribed]
[Connected + Subscribed] --data change--> [Notification Sent]
[Connected + Subscribed] --disconnect()--> [Cleaned Up] (atomic removal)
```

**Cleanup Logic** (Event-Driven):
```rust
pub async fn handle_disconnect(&self, conn_id: &ConnectionId) {
    // Atomic cleanup (no periodic job needed)
    if let Some((_, connection)) = self.connections.remove(conn_id) {
        let mut subs = self.subscriptions.write().await;
        subs.remove_by_connection(conn_id);
        
        let mut cache = self.filter_cache.write().await;
        cache.remove_by_connection(conn_id);
    }
}
```

---

### ViewDefinition (New)

**Purpose**: Metadata for virtual views over base tables.

**Fields**:
```rust
pub struct ViewDefinition {
    view_name: String,                   // e.g., "v_active_users"
    namespace: NamespaceId,              // Namespace containing the view
    sql_definition: String,              // SQL string (single source of truth)
    dependent_table_ids: Vec<TableId>,   // For cache invalidation tracking
    created_at: DateTime<Utc>,
    created_by: UserId,
}
```

**Storage**: Stored in system.tables with table_type='VIEW'

**Relationships**:
- References base tables (1:N, tracked in dependent_table_ids)
- Owned by namespace (N:1)
- Created by user (N:1)

**Validation Rules**:
- sql_definition must be valid SELECT statement (validated on CREATE VIEW)
- dependent_table_ids extracted from SQL parsing
- Query-time validation: dependent tables must exist when view is queried

**State Transitions**:
```
[Not Exists] --CREATE VIEW--> [Registered]
[Registered] --SELECT FROM view--> [Query Executed] (rewrite to base tables)
[Registered] --DROP base table--> [Broken] (no CASCADE enforcement)
[Broken] --SELECT FROM view--> [Error: Missing table]
[Registered] --DROP VIEW--> [Not Exists]
```

**Error Handling**:
- Query-time validation: If base table missing, return clear error
- No prevention during DROP TABLE (operator must manually clean up views)

---

### SystemSchemaVersion (New)

**Purpose**: Track system table schema version for migration support.

**Fields**:
```rust
pub struct SystemSchemaVersion {
    version: u32,              // e.g., 1, 2, 3
    updated_at: DateTime<Utc>,
}
```

**Storage**: RocksDB key `"system:schema_version"` → u32

**Relationships**: None (singleton value)

**Validation Rules**:
- Version must be non-negative
- Version can only increase (never decrease)

**State Transitions**:
```
[Not Exists] --initialize()--> [Version 1]
[Version N] --upgrade detected--> [Create missing tables] --> [Version N+1]
```

**Migration Logic**:
```rust
const CURRENT_SCHEMA_VERSION: u32 = 1;

pub fn initialize_system_tables(storage: &dyn StorageBackend) -> Result<()> {
    let stored_version = storage.get_schema_version()?.unwrap_or(0);
    
    if stored_version < CURRENT_SCHEMA_VERSION {
        // Create missing system tables
        for table_def in get_system_tables_for_version(CURRENT_SCHEMA_VERSION) {
            if !table_exists(&table_def.table_id)? {
                create_system_table(&table_def)?;
            }
        }
        storage.set_schema_version(CURRENT_SCHEMA_VERSION)?;
    }
    Ok(())
}
```

---

## Entity Relationships Diagram

```
┌─────────────────┐
│   AppContext    │ (Singleton)
│  - node_id      │◄─────────────┐
│  - schema_reg   │              │
│  - system_tbls  │              │
│  - live_qry_mgr │              │
└────────┬────────┘              │
         │                       │
         │ owns                  │ references
         ▼                       │
┌─────────────────┐              │
│  SchemaRegistry │              │
│  - schema_cache │              │
└────────┬────────┘              │
         │                       │
         │ contains              │
         ▼                       │
┌─────────────────┐              │
│   SchemaCache   │              │
│  - tables       │              │
│  - arrow_schemas│◄──┐          │
└────────┬────────┘   │          │
         │            │          │
         │ caches     │ accesses │
         ▼            │          │
┌─────────────────┐   │          │
│ CachedTableData │   │          │
│  - definition   │   │          │
└─────────────────┘   │          │
                      │          │
┌─────────────────┐   │          │
│ TableProviderCore│   │          │
│  - table_id     │───┘          │
│  + arrow_schema()│              │
└─────────────────┘              │
                                 │
┌──────────────────┐             │
│ LiveQueryManager │             │
│  - subscriptions │─────────────┘
│  - connections   │
│  - filter_cache  │
│  - node_id       │ (from AppContext)
└──────────────────┘

┌──────────────────┐
│  ViewDefinition  │
│  - view_name     │
│  - sql_def       │
│  - dependencies  │───► [TableId, TableId, ...]
└──────────────────┘
```

---

## Invariants

### Critical Invariants (Must Never Violate)

1. **NodeId Uniqueness**: AppContext.node_id is allocated exactly once per server instance
2. **Arrow Schema Consistency**: Cached Arc<Schema> must match TableDefinition.to_arrow_schema() (verified on cache miss)
3. **Subscription Atomicity**: Connection disconnect removes ALL associated subscriptions and filter cache entries (no orphans)
4. **Schema Version Monotonicity**: System schema version can only increase (never decrease)
5. **View Dependency Accuracy**: ViewDefinition.dependent_table_ids must match actual tables referenced in sql_definition

### Performance Invariants (Success Criteria)

1. **Cache Hit Rate**: ≥99% for stable workloads (SC-001)
2. **Schema Lookup Latency**: <2μs for cached, 50-100μs for uncached (SC-002)
3. **Memory Overhead**: <2MB for 1000 tables (~2KB per table) (SC-003)
4. **Notification Latency**: <100ms from INSERT to WebSocket delivery (User Story 2, Scenario 1)
5. **System Table Init**: <100ms on startup (SC-010)

---

## Migration Considerations

### Backward Compatibility

- Existing 477 tests must pass without modification (FR-017)
- Existing TableProvider.schema() method signature unchanged (returns SchemaRef)
- Configuration file retains all existing fields (node_id is additive)

### Deprecation Path

- Old patterns (NodeId instantiation) remain compilable but generate warnings
- LiveQueryManager consolidation provides compatibility wrappers during transition
- Full removal of deprecated code deferred to Phase 10.5 (post-refactoring cleanup)

---

## Data Flow Examples

### Example 1: Schema Lookup (Arrow Memoization)

```
Query: SELECT * FROM shared.orders
         │
         ├─► SqlExecutor.execute()
         │       │
         │       ├─► TableProvider.schema()
         │       │       │
         │       │       ├─► TableProviderCore.arrow_schema()
         │       │       │       │
         │       │       │       ├─► SchemaCache.get_arrow_schema(table_id)
         │       │       │       │       │
         │       │       │       │       ├─► DashMap.get(table_id)
         │       │       │       │       │       │
         │       │       │       │       │       ├─ HIT: Return Arc::clone (1-2μs)
         │       │       │       │       │       └─ MISS: Compute + cache (50-100μs)
         │       │       │       │       │
         │       │       │       │       └──► Return Arc<Schema>
         │       │       │       │
         │       │       │       └──► Return Arc<Schema>
         │       │       │
         │       │       └──► Return SchemaRef
         │       │
         │       └─► DataFusion plans query using schema
         │
         └──► Execute query, return results
```

### Example 2: Live Query Subscription

```
WebSocket CONNECT
         │
         ├─► LiveQueryManager.connect(user_id, conn_id)
         │       │
         │       └─► connections.insert(conn_id, UserConnection)
         │
WebSocket SUBSCRIBE "SELECT * FROM shared.orders WHERE status='pending'"
         │
         ├─► LiveQueryManager.subscribe(conn_id, query)
         │       │
         │       ├─► Parse filter: status='pending'
         │       ├─► filter_cache.insert(filter_id, compiled_filter)
         │       └─► subscriptions.insert(sub_id, Subscription)
         │
INSERT INTO shared.orders VALUES (123, 'pending')
         │
         ├─► LiveQueryManager.on_table_change(table_id, batch)
         │       │
         │       ├─► filter_cache.get_matching(table_id)
         │       │       │
         │       │       └─ Returns: [filter_id for status='pending']
         │       │
         │       ├─► Apply filter to batch → matched_rows
         │       │
         │       └─► For each matched subscription:
         │               │
         │               ├─► connections.get(conn_id)
         │               └─► websocket.send(notification)
         │
WebSocket DISCONNECT
         │
         └─► LiveQueryManager.handle_disconnect(conn_id)
                 │
                 ├─► connections.remove(conn_id)
                 ├─► subscriptions.remove_by_connection(conn_id)
                 └─► filter_cache.remove_by_connection(conn_id)
                         (atomic cleanup, <10ms)
```

### Example 3: System Table Upgrade

```
Server START
         │
         ├─► Load config.toml
         │       │
         │       └─► node_id = "node-prod-01"
         │
         ├─► AppContext.init(config, ...)
         │       │
         │       └─► node_id: Arc::new(NodeId::from("node-prod-01"))
         │
         └─► initialize_system_tables(storage)
                 │
                 ├─► stored_version = storage.get("system:schema_version") → 1
                 ├─► current_version = 2 (code constant)
                 │
                 ├─► IF stored_version < current_version:
                 │       │
                 │       ├─► For table_def in SYSTEM_TABLES_V2:
                 │       │       │
                 │       │       └─► IF NOT table_exists(table_def.id):
                 │       │               │
                 │       │               └─► create_system_table(table_def)
                 │       │                       (e.g., system.audit_logs)
                 │       │
                 │       └─► storage.set("system:schema_version", 2)
                 │
                 └─► Server continues startup
```

---

## Testing Strategy

### Unit Tests (Per Entity)

- **SchemaCache**: Test get_arrow_schema() cache hit/miss, invalidate() removes both entries
- **AppContext**: Test node_id loaded from config, validate error on missing config field
- **LiveQueryManager**: Test handle_disconnect() removes all related data atomically
- **ViewDefinition**: Test query-time validation fails on missing table

### Integration Tests

- **Arrow Memoization**: Benchmark 1000 queries, verify first 50-100μs, subsequent <2μs
- **NodeId Consistency**: Start server, inspect logs from multiple components, verify identical node_id
- **Subscription Cleanup**: Establish 100 subscriptions, force disconnect, verify zero leaks
- **System Table Upgrade**: Simulate version 1→2 upgrade, verify new table created

### Regression Tests

- **Existing 477 Tests**: Must pass without modification (FR-017)
- **Performance**: SC-001 to SC-010 success criteria validated

---

**Phase 1 Complete**: Data model defined, ready for contracts generation (N/A for internal refactoring) and quickstart guide.

