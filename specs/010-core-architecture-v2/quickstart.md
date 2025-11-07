# Quickstart: Core Architecture Refactoring v2

**Date**: 2025-11-06  
**Feature**: 010-core-architecture-v2  
**Audience**: Developers implementing the refactoring

## Prerequisites

- Rust 1.90+ installed (`rustc --version`)
- Existing KalamDB workspace cloned
- Branch `010-core-architecture-v2` checked out
- All 477 kalamdb-core tests passing: `cargo test -p kalamdb-core`
- Familiarity with Phase 5 (SchemaRegistry) and Phase 10 (Cache Consolidation) architecture

## Implementation Order (CRITICAL)

**⚠️ WARNING**: Implementation MUST follow this strict order to avoid rework:

1. **AppContext centralization** (FR-000, FR-014) - ✅ Can start immediately
2. **schema/ → schema_registry/ rename** (FR-001) - ✅ Can start after step 1
3. **Arrow schema memoization** (FR-002 to FR-006) - ✅ Can start after step 2
4. **SqlExecutor migration** (FR-015, FR-016) - ⚠️ BLOCKED until steps 1-3 complete
5. **LiveQueryManager consolidation** (FR-007, FR-008) - ⚠️ BLOCKED until step 4 complete
6. **System tables storage** (FR-009, FR-010) - Can parallelize with step 5
7. **Views support** (FR-011 to FR-013) - ⚠️ BLOCKED until step 3 complete
8. **Testing** (FR-017) - Final validation after all steps

## Step 1: AppContext Centralization (FR-000, FR-014)

### Estimated Time: 2 hours

### 1.1 Add node_id to Config struct

**File**: `backend/src/config.rs`

```rust
#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default = "default_node_id")]
    pub node_id: String,  // NEW
    // ... existing fields
}

fn default_node_id() -> String {
    format!("node-{}", std::process::id())  // Fallback for dev
}
```

### 1.2 Update config.toml

**File**: `backend/config.toml`

```toml
[server]
node_id = "node-dev-01"  # NEW: Add this line
# ... existing fields
```

### 1.3 Modify AppContext to own NodeId

**File**: `backend/crates/kalamdb-core/src/app_context.rs`

```rust
pub struct AppContext {
    node_id: Arc<NodeId>,  // NEW: Add this field
    schema_registry: Arc<SchemaRegistry>,
    // ... existing fields
}

impl AppContext {
    pub fn init(
        config: &Config,  // NEW: Add config parameter
        schema_store: Arc<TableSchemaStore>,
        // ... existing parameters
    ) -> Result<Arc<Self>> {
        let node_id = Arc::new(NodeId::from(config.node_id.clone()));  // NEW
        
        Ok(Arc::new(Self {
            node_id,  // NEW
            schema_registry: Arc::new(SchemaRegistry::new(schema_cache, schema_store)),
            // ... existing initialization
        }))
    }
    
    pub fn node_id(&self) -> &Arc<NodeId> {  // NEW
        &self.node_id
    }
}
```

### 1.4 Update lifecycle.rs

**File**: `backend/src/lifecycle.rs`

```rust
pub async fn bootstrap() -> Result<(ApplicationComponents, Arc<AppContext>)> {
    let config = load_config()?;  // Existing
    
    // ... existing initialization
    
    let app_context = AppContext::init(
        &config,  // NEW: Pass config for node_id
        schema_store,
        // ... existing parameters
    )?;
    
    // ... rest of function
}
```

### 1.5 Validate

```bash
cargo test -p kalamdb-core --lib app_context
cargo run --bin kalamdb-server  # Check logs show consistent node_id
```

**Success Criteria**: All components log with identical NodeId from config.toml

---

## Step 2: Rename schema/ → schema_registry/ (FR-001)

### Estimated Time: 1 hour

### 2.1 Rename directory

```powershell
# PowerShell
cd backend\crates\kalamdb-core\src
Move-Item schema schema_registry
```

### 2.2 Update mod.rs

**File**: `backend/crates/kalamdb-core/src/lib.rs`

```rust
pub mod schema_registry;  // Changed from: pub mod schema;
```

### 2.3 Find and replace all imports

```powershell
# Find all occurrences (verify manually)
Get-ChildItem -Recurse -Include *.rs | Select-String "kalamdb_core::schema::"

# Replace pattern:
# FROM: use kalamdb_core::schema::
# TO:   use kalamdb_core::schema_registry::
```

**Affected files** (~20 files):
- `sql/executor.rs` (and `executor/mod.rs`)
- `tables/*/mod.rs` (user_tables, shared_tables, stream_tables)
- `tables/system/*/mod.rs` (8 system table providers)
- Test files in `tests/` directory

### 2.4 Validate

```bash
cargo check -p kalamdb-core  # Must compile with no errors
cargo test -p kalamdb-core   # All 477 tests must pass
```

---

## Step 3: Arrow Schema Memoization (FR-002 to FR-006)

### Estimated Time: 4-6 hours

### 3.1 Add arrow_schema to CachedTableData (FR-002)

**File**: `backend/crates/kalamdb-core/src/schema_registry/registry.rs`

**Note**: This file will later be renamed to `schema_registry/registry.rs` when SchemaCache becomes SchemaRegistry.

```rust
use datafusion::arrow::datatypes::Schema;
use std::sync::RwLock;  // Built-in, no new dependency needed

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
    pub fn new(/* existing params */) -> Self {
        Self {
            table: schema,
            storage_id,
            storage_path_template,
            schema_version,
            arrow_schema: Arc::new(RwLock::new(None)),  // NEW: Start uninitialized
        }
    }
}
```

### 3.2 Implement arrow_schema() method (FR-003)

**File**: Same file as above

```rust
impl CachedTableData {
    /// Get or compute Arrow schema (lazy initialization)
    pub fn arrow_schema(&self) -> Result<Arc<Schema>, KalamDbError> {
        // Fast path: already computed (~1μs)
        {
            let read_guard = self.arrow_schema.read()
                .expect("RwLock poisoned - unrecoverable");
            if let Some(schema) = read_guard.as_ref() {
                return Ok(Arc::clone(schema));
            }
        }
        
        // Slow path: compute + cache (~50-100μs, first access only)
        let mut write_guard = self.arrow_schema.write()
            .expect("RwLock poisoned - unrecoverable");
        
        // Double-check (another thread might have computed while we waited)
        if let Some(schema) = write_guard.as_ref() {
            return Ok(Arc::clone(schema));
        }
        
        // Compute from TableDefinition
        let schema = Arc::new(self.table.to_arrow_schema()?);
        *write_guard = Some(Arc::clone(&schema));
        
        Ok(schema)
    }
}

impl SchemaCache {  // TODO: Will be renamed to SchemaRegistry
    pub fn get_arrow_schema(&self, table_id: &TableId) -> Result<Arc<Schema>> {
        let cached_data = self.get(table_id)
            .ok_or_else(|| KalamDbError::TableNotFound(table_id.clone()))?;
        
        cached_data.arrow_schema()  // Delegates to CachedTableData
    }
}
```

### 3.3 Update invalidation methods (FR-004)

**File**: Same file as above

**Note**: With embedded design, Arrow schema is automatically removed when CachedTableData is removed. No separate cleanup needed!

```rust
impl SchemaCache {  // TODO: Will be renamed to SchemaRegistry
    pub fn invalidate(&self, table_id: &TableId) {  // Note: &self not &mut (DashMap methods)
        self.cache.remove(table_id);  // Arrow schema removed automatically (embedded)
        self.user_table_shared.remove(table_id);
        self.lru_timestamps.remove(table_id);
        self.providers.remove(table_id);  // Also remove cached provider
    }
    
    pub fn clear(&self) {  // Note: &self not &mut (DashMap methods)
        self.cache.clear();  // Arrow schemas cleared automatically (embedded)
        self.user_table_shared.clear();
        self.lru_timestamps.clear();
        self.providers.clear();
    }
}
```

### 3.4 Add arrow_schema() to TableProviderCore (FR-005)

**File**: `backend/crates/kalamdb-core/src/tables/base_table_provider.rs`

**IMPORTANT**: Also remove the old `schema: SchemaRef` field from TableProviderCore (it's redundant with Arrow memoization).

```rust
pub struct TableProviderCore {
    pub table_id: Arc<TableId>,
    pub table_type: TableType,
    // REMOVED: pub schema: SchemaRef,  // OLD: Pre-computed schema (redundant)
    pub created_at_ms: u64,
    pub storage_id: Option<StorageId>,
    pub unified_cache: Arc<SchemaCache>,  // Access point for memoized schemas
}

impl TableProviderCore {
    pub fn arrow_schema(&self) -> Result<Arc<Schema>> {
        self.unified_cache.get_arrow_schema(&self.table_id)
    }
}
```

**Update constructor signature**:
```rust
impl TableProviderCore {
    pub fn new(
        table_id: Arc<TableId>,
        table_type: TableType,
        // REMOVED: schema: SchemaRef,  // No longer needed!
        storage_id: Option<StorageId>,
        unified_cache: Arc<SchemaCache>,
    ) -> Self {
        // ... implementation
    }
}
```

### 3.5 Update all 11 TableProvider implementations (FR-006)

**Pattern** (apply to each provider):

```rust
// BEFORE:
#[async_trait]
impl TableProvider for UserTableAccess {
    fn schema(&self) -> SchemaRef {
        Arc::clone(&self.core.schema)  // OLD: Uses pre-computed schema
    }
}

// AFTER:
#[async_trait]
impl TableProvider for UserTableAccess {
    fn schema(&self) -> SchemaRef {
        // CRITICAL: Panic on error rather than returning empty schema
        // Empty schema causes confusing DataFusion errors later
        self.core.arrow_schema()
            .expect("Arrow schema must be valid - table should exist in cache")
    }
}
```

**Files to modify** (11 total):
1. `tables/user_tables/user_table_provider.rs` (UserTableAccess)
2. `tables/shared_tables/shared_table_provider.rs` (SharedTableProvider)
3. `tables/stream_tables/stream_table_provider.rs` (StreamTableProvider)
4. `tables/system/users/users_provider.rs` (UsersTableProvider)
5. `tables/system/jobs/jobs_provider.rs` (JobsTableProvider)
6. `tables/system/namespaces/namespaces_provider.rs` (NamespacesTableProvider)
7. `tables/system/storages/storages_provider.rs` (StoragesTableProvider)
8. `tables/system/live_queries/live_queries_provider.rs` (LiveQueriesTableProvider)
9. `tables/system/tables/tables_provider.rs` (TablesTableProvider)
10. `tables/system/audit_logs/audit_logs_provider.rs` (AuditLogsTableProvider)
11. `tables/system/stats.rs` (StatsTableProvider)

### 3.6 Add benchmark test

**File**: `backend/crates/kalamdb-core/tests/test_arrow_schema_memoization.rs` (new file)

```rust
#[test]
fn test_arrow_schema_memoization_performance() {
    let test_db = TestDB::new();
    let app_ctx = create_test_app_context(&test_db);
    
    // Create a test table
    let table_id = TableId::new("test_namespace", "test_table");
    // ... create table via schema_registry
    
    let schema_cache = app_ctx.schema_registry().schema_cache();
    
    // First call: uncached (50-100μs expected)
    let start = Instant::now();
    let schema1 = schema_cache.get_arrow_schema(&table_id).unwrap();
    let uncached_duration = start.elapsed();
    
    // Subsequent calls: cached (<2μs expected)
    let mut cached_durations = vec![];
    for _ in 0..1000 {
        let start = Instant::now();
        let schema = schema_cache.get_arrow_schema(&table_id).unwrap();
        cached_durations.push(start.elapsed());
        
        // Verify Arc equality (same instance)
        assert!(Arc::ptr_eq(&schema1, &schema));
    }
    
    let avg_cached = cached_durations.iter().sum::<Duration>() / cached_durations.len() as u32;
    
    println!("Uncached: {:?}", uncached_duration);
    println!("Avg cached (1000 calls): {:?}", avg_cached);
    
    // Success criteria: 50-100× speedup
    assert!(uncached_duration.as_micros() >= 50, "Uncached too fast: {:?}", uncached_duration);
    assert!(avg_cached.as_micros() < 2, "Cached too slow: {:?}", avg_cached);
    assert!(uncached_duration > avg_cached * 50, "Speedup < 50×");
}
```

### 3.7 Validate

```bash
cargo test -p kalamdb-core test_arrow_schema_memoization_performance
cargo test -p kalamdb-core  # All 477 existing tests must still pass
```

**Success Criteria**: Benchmark shows 50-100× speedup, all tests pass

---

## Step 4: SqlExecutor Migration (FR-015, FR-016)

### ⚠️ BLOCKED until Steps 1-3 complete

### Estimated Time: 6-8 hours

**Note**: This step was attempted previously but incomplete. Now that AppContext and schema_registry foundations are ready, complete the migration:

1. Keep `executor.rs` functional until `executor/mod.rs` complete
2. Update all handler methods to receive `&AppContext` parameter
3. Replace all NodeId instantiations with `app_context.node_id()`
4. Replace all schema lookups with `app_context.schema_registry().get_arrow_schema()`
5. Test each handler independently
6. Delete `executor.rs` only after all tests pass

**Detailed steps deferred to separate task breakdown.**

---

## Step 5: LiveQueryManager Consolidation (FR-007, FR-008)

### Estimated Time: 4-6 hours

**Prerequisites**: SqlExecutor migration complete (Step 4)

1. Create new `live_query/manager.rs` with unified struct
2. Merge UserConnections map into LiveQueryManager
3. Merge UserTableChangeDetector filter cache
4. Implement atomic cleanup in `handle_disconnect()`
5. Add compatibility wrappers for old interfaces
6. Migrate call sites incrementally
7. Remove deprecated structs

**Detailed steps deferred to separate task breakdown.**

---

## Step 6: System Tables Storage (FR-009, FR-010)

### Estimated Time: 6-8 hours

**Can parallelize with Step 5**

1. Add schema version tracking to RocksDB (`system:schema_version` key)
2. Implement `initialize_system_tables()` with version comparison
3. Create `SystemTableStorage` abstraction using StorageBackend
4. Update system table providers to use RocksDB + Parquet
5. Implement flush operations for system tables
6. Test upgrade path (version 1 → 2)

**Detailed steps deferred to separate task breakdown.**

---

## Step 7: Views Support (FR-011 to FR-013)

### Estimated Time: 6-8 hours

**Prerequisites**: Arrow schema memoization complete (Step 3)

1. Add ViewDefinition model to kalamdb-commons
2. Extend system.tables to store views (table_type='VIEW')
3. Implement view registration in SchemaRegistry
4. Add query-time dependency validation
5. Implement logical plan rewriting (VIEW → underlying SELECT)
6. Test view queries and error cases

**Detailed steps deferred to separate task breakdown.**

---

## Step 8: Final Testing (FR-017)

### Estimated Time: 2-4 hours

### 8.1 Regression Testing

```bash
# All 477 existing tests must pass
cargo test -p kalamdb-core

# Specific test suites
cargo test -p kalamdb-core --lib schema_registry
cargo test -p kalamdb-core --lib tables
cargo test -p kalamdb-core --lib sql
```

### 8.2 Performance Validation

```bash
# Run benchmark tests
cargo test -p kalamdb-core test_arrow_schema_memoization_performance -- --nocapture
```

**Success Criteria** (from spec):
- ✅ SC-000: NodeId identical across all logged events
- ✅ SC-001: Cache hit rate >99% for stable workloads
- ✅ SC-002: Schema lookup <2μs cached, 50-100μs uncached (50-100× speedup)
- ✅ SC-003: Memory overhead <2MB for 1000 tables
- ✅ SC-004: All 11 TableProviders use memoized schemas successfully
- ✅ SC-005: LiveQueryManager code reduced by ≥30%
- ✅ SC-006: System table queries within 10% of shared table performance
- ✅ SC-007: Zero duplicate NodeId instantiations (code review)
- ✅ SC-008: All 477 tests pass
- ✅ SC-009: View queries within 5% of direct table query performance
- ✅ SC-010: System table init <100ms

### 8.3 Integration Testing

```bash
# Start server
cargo run --bin kalamdb-server

# Check logs for consistent NodeId
grep "node_id" logs/kalamdb-server.log | sort | uniq

# Test live queries
# (Use test script from tests/quickstart.sh)
```

---

## Common Issues & Solutions

### Issue 1: "Table not found in cache" after rename

**Cause**: Imports still reference `kalamdb_core::schema::`  
**Solution**: Find all occurrences and replace with `schema_registry::`

```powershell
Get-ChildItem -Recurse -Include *.rs | Select-String "kalamdb_core::schema::"
```

### Issue 2: Arrow schema test shows <50× speedup

**Cause**: Test table too simple (schema construction already fast)  
**Solution**: Use realistic table with 50+ columns for benchmark

### Issue 3: Compilation error "NodeId not found in AppContext"

**Cause**: Forgot to add `node_id()` getter method  
**Solution**: Add getter in `impl AppContext` block

### Issue 4: SqlExecutor migration fails due to missing methods

**Cause**: Attempting step 4 before steps 1-3 complete  
**Solution**: Complete AppContext and schema_registry work first (strict order!)

---

## Development Workflow

1. **Create feature branch** (already done): `010-core-architecture-v2`
2. **Implement one step at a time** following order above
3. **Validate after each step**: Run relevant tests
4. **Commit frequently**: One commit per completed step
5. **Document blockers**: Note any dependencies not mentioned in this guide
6. **Final validation**: Run all 477 tests before merging

---

## Success Metrics

Track these metrics throughout implementation:

| Metric | Baseline | Target | Validated By |
|--------|----------|--------|--------------|
| Schema lookup latency (cached) | 75μs | <2μs | Benchmark test |
| Schema lookup latency (uncached) | N/A | 50-100μs | Benchmark test |
| Cache hit rate | N/A | >99% | Integration test |
| Memory overhead per table | N/A | ~2KB | Profiling |
| Test pass rate | 477/477 | 477/477 | `cargo test` |
| LiveQueryManager LOC | ~1500 | <1050 | `cloc` |
| System table init time | N/A | <100ms | Profiling |

---

## Next Steps After Completion

1. **Update AGENTS.md**: Document Phase 10 completion
2. **Performance profiling**: Validate production performance
3. **Documentation**: Update architecture docs with new patterns
4. **Cleanup**: Remove deprecated code (deferred to Phase 10.5)
5. **Migration guide**: Create guide for external consumers (if kalamdb-core embeddable)

---

**Questions?** Refer to:
- [spec.md](./spec.md) - Feature specification
- [research.md](./research.md) - Technical decisions
- [data-model.md](./data-model.md) - Entity relationships
- [AGENTS.md](../../AGENTS.md) - Development guidelines

