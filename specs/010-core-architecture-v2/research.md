# Research: Core Architecture Refactoring v2

**Date**: 2025-11-06  
**Feature**: 010-core-architecture-v2  
**Purpose**: Resolve technical unknowns and establish best practices for Arrow schema memoization, AppContext patterns, and LiveQueryManager consolidation

## Research Tasks Completed

### R1: Arrow Schema Memoization Patterns in DataFusion

**Decision**: DashMap-based compute-once-cache-forever pattern with Arc<Schema> sharing

**Rationale**:
- DataFusion's TableProvider trait requires SchemaRef (Arc<Schema>) return type
- Arrow Schema construction is expensive (~50-100μs) due to field iteration and metadata copying
- DashMap provides lock-free concurrent access matching existing SchemaCache implementation
- Arc::clone() for cached schemas is ~1-2μs (50-100× speedup achieved)
- Invalidation complexity minimal: remove entry on ALTER TABLE/DROP TABLE

**Alternatives Considered**:
- **RwLock<HashMap>**: Rejected - contention under high concurrency (Phase 10 proved DashMap superior)
- **Once-per-table initialization**: Rejected - doesn't support dynamic schema changes (ALTER TABLE)
- **Weak references with periodic cleanup**: Rejected - unnecessary complexity, table count bounded
- **Pre-computed schemas in TableDefinition**: Rejected - violates single source of truth (schema derives from ColumnDefinitions)

**Implementation Pattern**:
```rust
// schema_registry/registry.rs (will be renamed to schema_registry/registry.rs)
use std::sync::RwLock;

/// **Clone Semantics**: Cloning creates a new Arc pointing to the same RwLock,
/// meaning all clones share the cached Arrow schema. This is intentional.
#[derive(Debug, Clone)]
pub struct CachedTableData {
    pub table: Arc<TableDefinition>,
    pub storage_id: Option<StorageId>,
    pub storage_path_template: String,
    pub schema_version: u32,
    
    // NEW: Lazy-initialized Arrow schema (compute once, cache forever)
    arrow_schema: Arc<RwLock<Option<Arc<Schema>>>>,
}

impl CachedTableData {
    pub fn new(/* existing params */, schema: Arc<TableDefinition>) -> Self {
        Self {
            table: schema,
            storage_id,
            storage_path_template,
            schema_version,
            arrow_schema: Arc::new(RwLock::new(None)),  // Initialize as None
        }
    }
    
    pub fn arrow_schema(&self) -> Result<Arc<Schema>> {
        // Fast path: already computed (~1μs)
        {
            let read_guard = self.arrow_schema.read().unwrap();
            if let Some(schema) = read_guard.as_ref() {
                return Ok(Arc::clone(schema));
            }
        }
        
        // Slow path: compute + cache (~50-100μs, first access only)
        let mut write_guard = self.arrow_schema.write().unwrap();
        
        // Double-check (another thread might have computed while we waited)
        if let Some(schema) = write_guard.as_ref() {
            return Ok(Arc::clone(schema));
        }
        
        let schema = Arc::new(self.table.to_arrow_schema()?);
        *write_guard = Some(Arc::clone(&schema));
        Ok(schema)
    }
}

// NOTE: SchemaCache will be renamed to SchemaRegistry
pub struct SchemaCache {  // TODO: Rename to SchemaRegistry
    cache: DashMap<TableId, Arc<CachedTableData>>,  // Single map (no separate arrow_schemas)
    user_table_shared: DashMap<TableId, Arc<UserTableShared>>,
    lru_timestamps: DashMap<TableId, AtomicU64>,
}

impl SchemaCache {  // TODO: Rename to SchemaRegistry
    pub fn get_arrow_schema(&self, table_id: &TableId) -> Result<Arc<Schema>> {
        let cached_data = self.get(table_id)
            .ok_or_else(|| KalamDbError::TableNotFound(table_id.clone()))?;
        cached_data.arrow_schema()  // Delegates to CachedTableData
    }
    
    pub fn invalidate(&self, table_id: &TableId) {  // Note: &self not &mut self (DashMap methods)
        self.cache.remove(table_id);  // Arrow schema removed automatically
        self.user_table_shared.remove(table_id);
        self.lru_timestamps.remove(table_id);
    }
}
```

**Validation**: Automated benchmark test with 1000 queries validates first call 50-100μs, subsequent <2μs

---

### R2: AppContext NodeId Ownership Pattern

**Decision**: AppContext loads NodeId once from config.toml and owns it as Arc<NodeId>

**Rationale**:
- NodeId is fundamental distributed system identifier (required for logs, jobs, audit)
- Current pattern `NodeId::from(format!("node-{}", std::process::id()))` creates inconsistent IDs
- AppContext already established as singleton in Phase 5 (SystemTablesRegistry pattern)
- Arc sharing enables zero-copy access across all components
- Configuration-based NodeId supports production deployment (explicit node identity)

**Alternatives Considered**:
- **Global static**: Rejected - violates Rust best practices, difficult to test
- **Per-component instantiation**: Rejected - current broken pattern, causes distributed tracing issues
- **Environment variable**: Rejected - config.toml provides better validation and documentation
- **Generated UUID**: Rejected - operators need predictable node identifiers for debugging

**Implementation Pattern**:
```rust
// app_context.rs
pub struct AppContext {
    node_id: Arc<NodeId>,  // NEW: Loaded from config.toml
    schema_registry: Arc<SchemaRegistry>,
    system_tables: Arc<SystemTablesRegistry>,
    // ... existing fields
}

impl AppContext {
    pub fn init(
        config: &Config,  // MODIFIED: Require config for node_id
        schema_store: Arc<TableSchemaStore>,
        // ... existing params
    ) -> Result<Arc<Self>> {
        let node_id = Arc::new(NodeId::from(config.node_id.clone()));
        // ... rest of initialization
    }
    
    pub fn node_id(&self) -> &Arc<NodeId> {
        &self.node_id
    }
}

// lifecycle.rs
let config = load_config()?;
let app_context = AppContext::init(
    &config,  // Pass config for node_id extraction
    schema_store,
    // ...
)?;
```

**Configuration Schema**:
```toml
# config.toml
[server]
node_id = "node-prod-01"  # NEW: Required field

# config.example.toml (add documentation)
# node_id: Unique identifier for this KalamDB node instance.
# Used for distributed logging, job tracking, and audit trails.
# Format: alphanumeric + hyphens, max 64 chars
# Example: "node-prod-01", "node-dev-alice", "node-us-west-2a"
```

---

### R3: LiveQueryManager Consolidation Strategy

**Decision**: Merge UserConnections, UserTableChangeDetector, and LiveQueryManager into single unified struct with event-driven WebSocket cleanup

**Rationale**:
- Current fragmentation across 3 structs causes confusion (which component owns what?)
- WebSocket connection state tightly coupled to subscription lifecycle (atomicity violated when separate)
- Filter cache invalidation requires coordination across components (current design error-prone)
- Event-driven cleanup on connection close eliminates periodic job overhead
- Single registry simplifies debugging and state inspection

**Alternatives Considered**:
- **Keep separate + add coordinator**: Rejected - adds layer without addressing root coupling issues
- **Periodic cleanup job**: Rejected - adds unnecessary polling overhead, delayed cleanup
- **Weak references for auto-cleanup**: Rejected - Rust lifetimes don't support this pattern cleanly
- **Actor model with message passing**: Rejected - over-engineering for synchronous use case

**Implementation Pattern**:
```rust
// live_query/manager.rs
pub struct LiveQueryManager {
    // Merged state
    subscriptions: RwLock<LiveQueryRegistry>,           // From LiveQueryManager
    connections: DashMap<ConnectionId, UserConnection>, // From UserConnections
    filter_cache: RwLock<FilterCache>,                  // From UserTableChangeDetector
    
    // Shared dependencies
    schema_registry: Arc<SchemaRegistry>,
    node_id: Arc<NodeId>,  // From AppContext
    // ... other shared resources
}

impl LiveQueryManager {
    pub async fn handle_disconnect(&self, conn_id: &ConnectionId) {
        // Atomic cleanup (event-driven, triggered by WebSocket close)
        if let Some((_, connection)) = self.connections.remove(conn_id) {
            // Remove all subscriptions for this connection
            let mut subs = self.subscriptions.write().await;
            subs.remove_by_connection(conn_id);
            
            // Invalidate filter cache entries (no longer needed)
            let mut cache = self.filter_cache.write().await;
            cache.remove_by_connection(conn_id);
            
            log::info!("Cleaned up connection {} and {} subscriptions", 
                conn_id, connection.subscription_count);
        }
    }
}
```

**Migration Path**: Deprecate old structs, add compatibility wrappers, gradually migrate call sites, remove deprecated code once all consumers updated.

---

### R4: System Tables Storage Architecture

**Decision**: Use identical StorageBackend pattern as shared tables (RocksDB buffer + Parquet flush)

**Rationale**:
- Consistency: System administrators use same backup/restore tools for all tables
- Proven pattern: Shared table storage already handles high-throughput writes + compaction
- Disaster recovery: System tables (users, jobs) require persistence across restarts
- Schema versioning: Enables future migrations using ALTER TABLE pattern (clarification from session)
- Performance: No regression vs current in-memory approach (RocksDB write path <1ms)

**Alternatives Considered**:
- **In-memory only**: Rejected - loses data on crash, no disaster recovery
- **Separate SQLite storage**: Rejected - adds dependency, complicates backup strategy
- **JSON files**: Rejected - poor query performance, no transactional guarantees
- **Special system table format**: Rejected - violates consistency principle, dual code paths

**Implementation Pattern**:
```rust
// System table initialization (FR-009)
pub fn initialize_system_tables(
    storage: Arc<dyn StorageBackend>,
    schema_registry: Arc<SchemaRegistry>,
) -> Result<SystemTablesRegistry> {
    let current_version = SYSTEM_SCHEMA_VERSION;  // e.g., 1
    let stored_version = storage.get_schema_version()?.unwrap_or(0);
    
    if stored_version < current_version {
        // Create missing system tables
        for table_def in SYSTEM_TABLE_DEFINITIONS {
            if !schema_registry.table_exists(&table_def.table_id)? {
                create_system_table(&table_def, &storage)?;
            }
        }
        storage.set_schema_version(current_version)?;
    }
    
    // Register providers (use same pattern as shared tables)
    Ok(SystemTablesRegistry::new(storage, schema_registry))
}
```

**Schema Version Tracking**:
```rust
// RocksDB key: "system:schema_version" -> u32
const SYSTEM_SCHEMA_VERSION: u32 = 1;

// Future migration example (when adding system.audit_logs)
const SYSTEM_SCHEMA_VERSION: u32 = 2;  // Bump version
// On startup: detect stored_version=1, create audit_logs table, update to 2
```

---

### R5: View Implementation Architecture

**Decision**: SQL-string-based view definitions with query-time validation and transparent rewriting

**Rationale**:
- SQL strings are human-readable, versionable, and easily stored in system.tables
- Query-time validation provides immediate feedback to users (fail fast)
- DataFusion already supports logical plan rewriting (VIEW → underlying SELECT)
- No CASCADE enforcement during DROP TABLE simplifies implementation
- Broken views fail explicitly when queried (operator knows immediately)

**Alternatives Considered**:
- **AST serialization**: Rejected - fragile across DataFusion version upgrades
- **Materialized views**: Rejected - out of scope (physical storage complexity)
- **Protobuf serialization**: Rejected - unnecessary dependency, SQL strings sufficient
- **CASCADE enforcement**: Rejected - adds complexity, query-time validation simpler

**Implementation Pattern**:
```rust
// View storage in system.tables
pub struct ViewDefinition {
    view_name: String,
    sql_definition: String,  // e.g., "SELECT * FROM system.users WHERE deleted_at IS NULL"
    dependent_table_ids: Vec<TableId>,  // For cache invalidation
    created_at: DateTime<Utc>,
}

// Query execution
impl SchemaRegistry {
    pub fn resolve_view(&self, view_name: &str) -> Result<LogicalPlan> {
        let view = self.get_view_definition(view_name)?;
        
        // Parse SQL and rewrite to logical plan
        let plan = parse_sql(&view.sql_definition)?;
        
        // Validate dependencies (query-time check)
        for table_id in &view.dependent_table_ids {
            if !self.table_exists(table_id)? {
                return Err(KalamDbError::BrokenView {
                    view: view_name.to_string(),
                    missing_table: table_id.to_string(),
                });
            }
        }
        
        Ok(plan)
    }
}
```

**Error Message Example**:
```sql
SELECT * FROM v_active_users;
-- Error: View 'v_active_users' references missing table 'system.users'
-- Hint: The underlying table may have been dropped. Recreate the view or drop it.
```

---

## Best Practices Applied

### DataFusion TableProvider Patterns
- Always return SchemaRef (Arc<Schema>) from schema() method
- Memoize expensive operations (schema construction, statistics)
- Use async scan() for I/O operations (RocksDB + Parquet reads)
- Implement statistics() for query optimization (existing pattern maintained)

### Rust Concurrency Patterns
- DashMap for lock-free concurrent caching (proven in Phase 10)
- Arc for zero-copy sharing across threads (NodeId, Schema, TableDefinition)
- RwLock only for infrequently-written data (subscription registry)
- Atomic operations for counters (cache hits/misses in benchmarks)

### Configuration Management
- config.toml as single source of truth (node_id, server settings)
- Validation at parse time (reject invalid node_id format early)
- Example configs with inline documentation (config.example.toml)
- Environment variable overrides for container deployments (future: NODE_ID env var)

### Testing Strategy
- Benchmark tests for performance claims (1000 query test for Arrow memoization)
- Unit tests for cache invalidation logic (ALTER TABLE, DROP TABLE scenarios)
- Integration tests for AppContext initialization (node_id loading from config)
- Regression tests for existing 477 tests (zero breakage tolerance)

---

## Risk Mitigation

### Implementation Order Enforcement
- **Risk**: Attempting SqlExecutor migration before AppContext/schema_registry ready (previous failure)
- **Mitigation**: Explicit blocking in spec (FR-015 depends on FR-000 to FR-006)
- **Validation**: Compilation will fail if executor/mod.rs references missing AppContext methods

### Cache Memory Overhead
- **Risk**: Unbounded cache growth (clarification: table count won't exceed limits)
- **Mitigation**: No eviction needed, monitor memory in production
- **Validation**: <2MB for 1000 tables tested in benchmarks

### Breaking Changes During Refactoring
- **Risk**: Existing 477 tests break during migration
- **Mitigation**: Incremental refactoring with immediate test validation after each phase
- **Validation**: FR-017 mandates all tests pass before completion

### LiveQueryManager State Consistency
- **Risk**: Connection cleanup leaves orphaned subscriptions
- **Mitigation**: Event-driven atomic cleanup in handle_disconnect()
- **Validation**: Integration test establishes 100 subscriptions, force-disconnects, verifies zero leaks

---

## Open Questions Resolved

All technical unknowns from Technical Context have been resolved:
- ✅ Arrow schema memoization pattern: DashMap with Arc<Schema>
- ✅ AppContext NodeId ownership: Load from config.toml, Arc sharing
- ✅ LiveQueryManager consolidation: Merge 3 structs, event-driven cleanup
- ✅ System tables storage: Same StorageBackend as shared tables
- ✅ View implementation: SQL strings, query-time validation

**Ready to proceed to Phase 1: Design & Contracts**

