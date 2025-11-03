# Phase 10 - Common Provider Architecture Enhancement

**Date**: 2025-11-02  
**Status**: 🔄 IN PROGRESS (Task Addition Complete)  
**Branch**: `008-schema-consolidation`

---

## Overview

Extension to Phase 10 (Cache Consolidation) adding common provider architecture to eliminate memory waste from duplicate provider instances.

### Problem Identified

**Current Architecture (Inefficient)**:
- `UserTableProvider` creates new instance for **every user query**
- `StreamTableProvider` creates new instance for **every query**
- User context (`user_id`, `user_role`) is **baked into provider struct**
- Result: **N users × M tables = N×M provider instances in memory** (massive waste!)

**Example**:
- 100 users querying 10 tables = **1,000 provider instances** in memory
- Each provider: ~200+ bytes (metadata, schema, store reference, user_id, user_role, etc.)
- Total waste: ~200 KB for data that should be shared

### Solution Design

**Target Architecture (Memory Efficient)**:
- **ONE provider instance per table** (not per user × table)
- Store `Arc<dyn BaseTableProvider>` in unified SchemaCache
- Pass user context **per-request** via `scan_user(user_id, user_role, ...)`
- Remove user-specific fields from provider structs

**Memory Impact**:
- Before: N users × M tables = **N×M instances**
- After: **M instances** (single provider per table, shared across ALL users)
- Expected Savings: **~99% reduction** for workloads with many concurrent users

**Example** (same 100 users, 10 tables):
- Before: 1,000 provider instances
- After: **10 provider instances**
- Savings: 990 instances eliminated = **99% reduction**

---

## Architecture Changes

### 1. Common Base Infrastructure

**New File**: `backend/crates/kalamdb-core/src/tables/base_table_provider.rs`

```rust
/// Common trait for all table providers
pub trait BaseTableProvider: TableProvider + Send + Sync {
    fn table_id(&self) -> &Arc<TableId>;
    fn schema(&self) -> SchemaRef;
    fn table_type(&self) -> TableType;
}

/// Shared fields for all provider types
pub struct TableProviderCore {
    pub table_id: Arc<TableId>,      // Created once at registration
    pub table_type: TableType,
    pub schema: SchemaRef,
    pub created_at: DateTime<Utc>,
    pub storage_id: Option<StorageId>,
}

impl TableProviderCore {
    /// Zero-allocation access to namespace via table_id
    pub fn namespace(&self) -> &NamespaceId { &self.table_id.namespace_id }
    
    /// Zero-allocation access to table name via table_id
    pub fn table_name(&self) -> &TableName { &self.table_id.table_name }
}
```

### 2. UserTableProvider Refactoring

**Before** (user context baked in):
```rust
pub struct UserTableProvider {
    table_metadata: TableMetadata,
    schema: SchemaRef,
    table_store: Arc<UserTableStore>,
    current_user_id: UserId,    // ❌ Creates duplicate instances per user!
    access_role: Role,          // ❌ Creates duplicate instances per user!
    parquet_paths: Vec<String>,
}

// Creates NEW instance per user query
let provider = UserTableProvider::new(
    metadata, schema, store, 
    user_id,    // ❌ Different for each user
    user_role,  // ❌ Different for each user
    paths
);
```

**After** (user context passed per-request):
```rust
pub struct UserTableProvider {
    core: TableProviderCore,           // ✅ Shared metadata
    table_store: Arc<UserTableStore>,  // ✅ Shared store
    parquet_paths: Vec<String>,        // ✅ Shared paths
    // REMOVED: current_user_id, access_role
}

impl BaseTableProvider for UserTableProvider {
    fn table_id(&self) -> &Arc<TableId> { &self.core.table_id }
    fn schema(&self) -> SchemaRef { self.core.schema.clone() }
    fn table_type(&self) -> TableType { self.core.table_type }
}

// New per-request methods accepting user context
impl UserTableProvider {
    pub async fn scan_user(
        &self,
        user_id: &UserId,      // ✅ Passed per-request
        user_role: &Role,      // ✅ Passed per-request
        filters: Vec<Expr>,
        projection: Option<Vec<usize>>,
        limit: Option<usize>,
    ) -> Result<Arc<dyn ExecutionPlan>> { ... }
    
    pub async fn insert_user(
        &self,
        user_id: &UserId,
        user_role: &Role,
        batch: RecordBatch,
    ) -> Result<()> { ... }
}
```

### 3. StreamTableProvider Enhancement

**Before** (missing Arc<TableId>):
```rust
pub struct StreamTableProvider {
    table_metadata: TableMetadata,  // ❌ No cached table_id
    schema: SchemaRef,
    store: Arc<StreamTableStore>,
    // Missing: table_id field
}
```

**After** (with TableProviderCore):
```rust
pub struct StreamTableProvider {
    core: TableProviderCore,         // ✅ Includes Arc<TableId>
    store: Arc<StreamTableStore>,
}

impl BaseTableProvider for StreamTableProvider {
    fn table_id(&self) -> &Arc<TableId> { &self.core.table_id }
    fn schema(&self) -> SchemaRef { self.core.schema.clone() }
    fn table_type(&self) -> TableType { TableType::Stream }
}
```

### 4. SchemaCache Provider Storage

**Enhancement**: Store provider instances alongside metadata

```rust
pub struct CachedTableData {
    // Existing fields
    pub table_id: TableId,
    pub table_type: TableType,
    pub storage_id: Option<StorageId>,
    pub storage_path_template: String,
    pub schema: Arc<TableDefinition>,
    
    // NEW: Cache provider instance
    pub provider: Option<Arc<dyn BaseTableProvider>>,
}

impl SchemaCache {
    /// Store provider instance with metadata (created ONCE at registration)
    pub fn insert_provider(
        &self,
        table_id: TableId,
        provider: Arc<dyn BaseTableProvider>,
    ) {
        if let Some(mut entry) = self.cache.get_mut(&table_id) {
            entry.provider = Some(provider);
        }
    }
    
    /// Get cached provider (reused for ALL queries)
    pub fn get_provider(
        &self,
        table_id: &TableId,
    ) -> Option<Arc<dyn BaseTableProvider>> {
        self.cache.get(table_id)
            .and_then(|entry| entry.provider.clone())
    }
}
```

### 5. Query Execution Update

**Before** (creates new provider per query):
```rust
// executor.rs execute_query()
let provider = UserTableProvider::new(
    metadata, schema, store, 
    user_id,    // ❌ Creates new instance per user
    user_role,
    paths
);
let plan = provider.scan(...).await?;
```

**After** (reuses cached provider):
```rust
// executor.rs execute_query()
let provider = self.unified_cache
    .get_provider(&table_id)?    // ✅ Cached instance
    .ok_or(KalamDbError::TableNotFound)?;

// Downcast to UserTableProvider to access scan_user()
let user_provider = provider
    .as_any()
    .downcast_ref::<UserTableProvider>()
    .ok_or(KalamDbError::ProviderTypeMismatch)?;

let plan = user_provider
    .scan_user(user_id, user_role, filters, projection, limit)  // ✅ Pass context
    .await?;
```

---

## Implementation Plan

### Phase 3B: Common Provider Architecture (10 Tasks)

Added to Phase 10 as tasks **T323-T332**:

1. **T323**: Create `base_table_provider.rs` with trait and core struct
2. **T324**: Refactor `UserTableProvider` to use `TableProviderCore`
   - Remove `current_user_id` and `access_role` fields
   - Add `scan_user()` and `insert_user()` methods accepting user context
3. **T325**: Refactor `StreamTableProvider` with `TableProviderCore` + `Arc<TableId>`
4. **T326**: Update `SharedTableProvider` if needed
5. **T327**: Add provider storage to `SchemaCache` (`insert_provider()`, `get_provider()`)
6. **T328**: Update `execute_query()` to use cached providers
7. **T329**: Update `CREATE TABLE` paths to cache provider instances
8. **T330-T332**: Update unit and integration tests

### Updated Phase 10 Summary

**Total Tasks**: 49 (was 41, added 8 for provider architecture)
- Phase 1 (Cache Creation): T300-T308 (9 tasks) ✅ **COMPLETE**
- Phase 2 (Executor Integration): T309-T314 (6 tasks) ⏳ 2/6 complete
- Phase 3 (Provider Updates): T315-T322 (8 tasks) ⏳ 1/8 complete
- **Phase 3B (Provider Architecture)**: T323-T332 (10 tasks) ⏳ **NEW**
- Phase 4 (Cleanup): T333-T339 (7 tasks) ⏳ 0/7 complete
- Phase 5 (Testing): T340-T347 (8 tasks) ⏳ 0/8 complete

---

## Expected Impact

### Memory Efficiency

**Before**:
- 100 users × 10 tables = **1,000 provider instances**
- Each provider: ~200 bytes
- Total: **~200 KB wasted** on duplicate metadata

**After**:
- 10 provider instances (ONE per table)
- Arc::clone() overhead: 16 bytes per reference
- Total: **~2 KB** (200 bytes × 10 providers)
- **Savings: ~198 KB (99% reduction)**

### Performance

**Provider Creation Cost**:
- Before: Create new provider per query (allocate 200+ bytes, clone metadata, etc.)
- After: Arc::clone() only (increment refcount, ~5 CPU cycles)
- **Speedup: ~100× faster** per query

**Cache Efficiency**:
- Before: Cache metadata only (providers created fresh each time)
- After: Cache metadata + provider instances (zero allocations after registration)
- **Allocation Reduction: 100%** for steady-state queries

### Code Quality

**Reduced Duplication**:
- `TableProviderCore` shared across all provider types
- Common `BaseTableProvider` trait for uniform interface
- Single implementation of `table_id()`, `namespace()`, `table_name()` helpers

**Better Separation of Concerns**:
- User context passed per-request (not baked into struct)
- Providers are stateless, reusable across all users
- Clear ownership: Cache owns providers, queries borrow them

---

## Testing Strategy

### Unit Tests (T330-T331)

**UserTableProvider**:
```rust
#[tokio::test]
async fn test_provider_handles_multiple_users() {
    let provider = create_test_provider(); // ONE instance
    
    // Same provider, different users
    let plan1 = provider.scan_user(&user_id_1, &Role::User, ...).await?;
    let plan2 = provider.scan_user(&user_id_2, &Role::User, ...).await?;
    
    // Verify correct data isolation per user
    assert_eq!(plan1.row_count(), 10);
    assert_eq!(plan2.row_count(), 5);
}
```

**StreamTableProvider**:
```rust
#[test]
fn test_provider_stores_table_id() {
    let provider = create_stream_provider(table_id.clone());
    
    assert_eq!(provider.table_id(), &table_id);
    assert_eq!(provider.namespace(), &namespace_id);
    assert_eq!(provider.table_name(), &table_name);
}
```

### Integration Tests (T332)

**Provider Caching**:
```rust
#[tokio::test]
async fn test_provider_cached_across_users() {
    // CREATE TABLE
    executor.execute("CREATE TABLE chat:messages (...)").await?;
    
    // Get provider for user1
    let provider1 = executor.get_provider(&table_id)?;
    let ptr1 = Arc::as_ptr(&provider1);
    
    // Get provider for user2
    let provider2 = executor.get_provider(&table_id)?;
    let ptr2 = Arc::as_ptr(&provider2);
    
    // Verify SAME instance (not duplicate)
    assert_eq!(ptr1, ptr2);
}
```

**Memory Efficiency**:
```rust
#[tokio::test]
async fn test_n_users_m_tables_equals_m_providers() {
    let num_users = 100;
    let num_tables = 10;
    
    // Create tables
    for i in 0..num_tables {
        executor.execute(&format!("CREATE TABLE chat:table_{} (...)", i)).await?;
    }
    
    // Query from N users
    for user_id in 0..num_users {
        for table_id in 0..num_tables {
            executor.execute_as_user(user_id, "SELECT * FROM ...").await?;
        }
    }
    
    // Verify only M providers exist (not N×M)
    let provider_count = executor.unified_cache.provider_count();
    assert_eq!(provider_count, num_tables);  // 10, NOT 1000!
}
```

---

## Backward Compatibility

**Breaking Changes**: None expected for external APIs

**Internal Changes**:
- `UserTableProvider` constructor signature changed (removed `user_id`, `user_role` parameters)
- New methods: `scan_user()`, `insert_user()` (old `scan()` preserved for system use)
- `StreamTableProvider` constructor adds `Arc<TableId>` parameter

**Migration Path**:
1. Phase 3B implements new architecture
2. Phase 4 removes old cache implementations
3. Phase 5 validates all tests pass with new architecture

---

## Success Criteria

✅ **Memory Efficiency**:
- N users × 1 table = **1 provider** in cache (not N providers)
- Verify via integration test: 100 users × 10 tables = 10 providers

✅ **Performance**:
- Provider retrieval: <10μs avg latency (Arc::clone overhead)
- Query execution: No measurable slowdown vs current implementation

✅ **Code Quality**:
- Zero code duplication between `UserTableProvider` and `StreamTableProvider`
- `TableProviderCore` shared across all provider types
- Clean separation of user context (passed per-request)

✅ **Test Coverage**:
- All existing tests pass after refactoring
- New tests verify single provider instance per table
- Integration tests verify memory efficiency

---

## Related Documents

- **Main Tasks**: `specs/008-schema-consolidation/tasks.md` (Phase 10, T323-T332)
- **Cache Optimization**: `CACHE_CONSOLIDATION_PROPOSAL.md` (unified cache design)
- **Architecture**: `AGENTS.md` (Recent Changes section for Phase 10)

---

## Timeline

**Estimated Effort**: 4-6 hours (10 tasks)
- T323 (base infrastructure): 1 hour
- T324-T326 (provider refactoring): 2 hours
- T327-T329 (cache integration): 1 hour
- T330-T332 (testing): 1-2 hours

**Completion Target**: Following Phase 3 (T315-T322) completion

---

## Notes

- This enhancement was identified during Phase 10 implementation (T315 complete)
- User requested: "one provider only in memory and it's cached in the same cache"
- Aligns with Phase 10 goal: Eliminate memory waste and improve performance
- Complements existing work: LRU timestamp optimization, Arc<TableId> caching
- Expected total Phase 10 impact: ~50% cache memory + ~99% provider memory = **~75% overall memory reduction**

---

**Status**: Tasks added to Phase 10, ready for implementation after T315-T322 completion
