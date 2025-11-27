# 015: IndexedEntityStore Implementation Plan

## Overview

Add automatic secondary index management to `EntityStore` using RocksDB's atomic `WriteBatch`.
This eliminates manual index management in each provider and ensures index consistency.

## Goals

1. **Centralized Index Management**: Define indexes once, managed automatically on put/update/delete
2. **Atomic Operations**: Entity + all indexes updated in single WriteBatch
3. **Async Support**: spawn_blocking wrapper for async contexts
4. **DataFusion Integration**: Filter pushdown using indexes
5. **Backward Compatible**: Existing code continues to work

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                       TableProvider                             │
│                    (e.g., JobsTableProvider)                    │
│                              │                                  │
│              supports_filters_pushdown()                        │
│                              │                                  │
│                              ▼                                  │
│              ┌─────────────────────────┐                       │
│              │   IndexedEntityStore    │                       │
│              │                         │                       │
│              │  - insert()             │                       │
│              │  - update()             │                       │
│              │  - delete()             │                       │
│              │  - scan_by_index()      │                       │
│              │                         │                       │
│              │  indexes: Vec<IndexDef> │                       │
│              └───────────┬─────────────┘                       │
│                          │                                     │
│           backend.batch([                                      │
│               Put { entity },                                  │
│               Put { index1 },                                  │
│               Put { index2 },                                  │
│           ])                                                   │
│                          │                                     │
│                          ▼                                     │
│              ┌─────────────────────────┐                       │
│              │    StorageBackend       │                       │
│              │    (RocksDB)            │                       │
│              └─────────────────────────┘                       │
└─────────────────────────────────────────────────────────────────┘
```

## Implementation Phases

### Phase 1: Core IndexedEntityStore ✅ COMPLETE
- [x] 1.1 Create `indexed_store.rs` in `kalamdb-store`
- [x] 1.2 Define `IndexDefinition<K, V>` trait
- [x] 1.3 Implement `IndexedEntityStore<K, V>` struct
- [x] 1.4 Implement sync methods: `insert()`, `update()`, `delete()`
- [x] 1.5 Implement `scan_by_index()` method
- [x] 1.6 Export from `kalamdb-store/src/lib.rs`
- [x] 1.7 Add unit tests (5 tests passing)

### Phase 2: Async Support ✅ COMPLETE (included in Phase 1)
- [x] 2.1 Add `insert_async()`, `update_async()`, `delete_async()`
- [x] 2.2 Add `scan_by_index_async()`
- [x] 2.3 Async methods use spawn_blocking internally

### Phase 3: DataFusion Integration ✅ COMPLETE (optional feature)
- [x] 3.1 Add `indexed_columns()` to `IndexDefinition`
- [x] 3.2 Add `filter_to_prefix()` for converting DataFusion Expr to index prefix
- [x] 3.3 Add `supports_filter()` for filter pushdown decisions
- [x] 3.4 Made DataFusion integration optional via `datafusion` feature flag

### Phase 4: Migrate JobsTableProvider ✅ COMPLETE
- [x] 4.1 Create `jobs_indexes.rs` with index definitions:
  - `JobStatusCreatedAtIndex` - queries by status + created_at
  - `JobNamespaceTableIndex` - queries by namespace + table
  - `JobIdempotencyKeyIndex` - lookup by idempotency key
- [x] 4.2 Replace `SystemTableStore` with `IndexedEntityStore` in JobsTableProvider
- [x] 4.3 Remove manual index management code from jobs_provider.rs
- [x] 4.4 Update `list_jobs_filtered()` to use `scan_by_index()`
- [x] 4.5 Update `cleanup_old_jobs()` to use `scan_index_raw()`
- [x] 4.6 Run tests and verify behavior (7 tests passing)

### Phase 5: Cleanup ✅ COMPLETE
- [x] 5.1 Remove `put_index`, `delete_index`, `get_index`, `scan_index` from SystemTableStore
- [x] 5.2 Remove async index methods from SystemTableStore
- [x] 5.3 SystemTableStore is now a thin wrapper for admin-only access control

### Phase 6: Additional Provider Migrations ✅ COMPLETE
- [x] 6.1 Migrate UsersTableProvider to use IndexedEntityStore
  - Created `users_indexes.rs` with UserUsernameIndex and UserRoleIndex
  - Replaced manual username_index with automatic IndexedEntityStore management
  - All 7 users provider tests passing
- [x] 6.2 Reviewed other providers (tables, namespaces, storages, audit_logs, live_queries)
  - These don't need secondary indexes - they use composite keys or simple lookups
  - Tables uses TableId (namespace:table_name) for natural prefix scans
- [x] 6.3 All smoke tests passing (50/50)

## Files to Create

| File | Description |
|------|-------------|
| `backend/crates/kalamdb-store/src/indexed_store.rs` | Core IndexedEntityStore implementation |

## Files to Modify

| File | Changes |
|------|---------|
| `backend/crates/kalamdb-store/src/lib.rs` | Export IndexedEntityStore, IndexDefinition |
| `backend/crates/kalamdb-system/src/providers/jobs/mod.rs` | Add JobStatusCreatedAtIndex |
| `backend/crates/kalamdb-system/src/providers/jobs/jobs_provider.rs` | Use IndexedEntityStore |
| `backend/crates/kalamdb-system/src/system_table_store.rs` | Remove raw index methods (Phase 5) |

## API Design

### IndexDefinition Trait

```rust
pub trait IndexDefinition<K, V>: Send + Sync 
where
    K: StorageKey,
    V: KSerializable,
{
    /// Returns the partition name for this index.
    fn partition(&self) -> &str;
    
    /// Returns column names this index covers (for DataFusion).
    fn indexed_columns(&self) -> Vec<&str>;
    
    /// Extracts the index key from the entity.
    fn extract_key(&self, primary_key: &K, entity: &V) -> Option<Vec<u8>>;
    
    /// Returns the value to store in the index (default: primary key).
    fn index_value(&self, primary_key: &K, _entity: &V) -> Vec<u8> {
        primary_key.storage_key()
    }
    
    /// Converts a DataFusion filter to an index scan prefix.
    fn filter_to_prefix(&self, filter: &Expr) -> Option<Vec<u8>> {
        None
    }
}
```

### IndexedEntityStore API

```rust
impl<K, V> IndexedEntityStore<K, V> {
    // Constructor
    pub fn new(backend, partition, indexes) -> Self;
    
    // Sync CRUD (atomic with indexes)
    pub fn insert(&self, key: &K, entity: &V) -> Result<()>;
    pub fn update(&self, key: &K, entity: &V) -> Result<()>;
    pub fn delete(&self, key: &K) -> Result<()>;
    
    // Index queries
    pub fn scan_by_index(&self, idx: usize, prefix: Option<&[u8]>, limit: Option<usize>) -> Result<Vec<(K, V)>>;
    
    // Async versions
    pub async fn insert_async(&self, key: &K, entity: &V) -> Result<()>;
    pub async fn update_async(&self, key: &K, entity: &V) -> Result<()>;
    pub async fn delete_async(&self, key: &K) -> Result<()>;
    pub async fn scan_by_index_async(&self, ...) -> Result<Vec<(K, V)>>;
    
    // For DataFusion
    pub fn indexes(&self) -> &[Arc<dyn IndexDefinition<K, V>>];
}
```

## Example: JobStatusCreatedAtIndex

```rust
pub struct JobStatusCreatedAtIndex;

impl IndexDefinition<JobId, Job> for JobStatusCreatedAtIndex {
    fn partition(&self) -> &str {
        "system_jobs_status_idx"
    }
    
    fn indexed_columns(&self) -> Vec<&str> {
        vec!["status", "created_at"]
    }
    
    fn extract_key(&self, _pk: &JobId, job: &Job) -> Option<Vec<u8>> {
        // Format: [status_byte][created_at_be_bytes][job_id_bytes]
        let mut key = Vec::with_capacity(1 + 8 + job.job_id.as_bytes().len());
        key.push(status_to_u8(job.status));
        key.extend_from_slice(&job.created_at.to_be_bytes());
        key.extend_from_slice(job.job_id.as_bytes());
        Some(key)
    }
    
    fn filter_to_prefix(&self, filter: &Expr) -> Option<Vec<u8>> {
        // Parse: status = 'Running'
        if let Expr::BinaryExpr(bin) = filter {
            if let (Expr::Column(col), Expr::Literal(ScalarValue::Utf8(Some(val)))) = 
                (bin.left.as_ref(), bin.right.as_ref()) 
            {
                if col.name == "status" && bin.op == Operator::Eq {
                    return Some(vec![status_str_to_u8(val)]);
                }
            }
        }
        None
    }
}
```

## Progress Log

| Date | Phase | Status | Notes |
|------|-------|--------|-------|
| 2025-11-26 | Plan | ✅ Complete | Created implementation plan |
| 2025-11-26 | Phase 1-3 | ✅ Complete | Core implementation with async + DataFusion support |
| 2025-11-26 | Phase 4 | ✅ Complete | Migrated JobsTableProvider to use IndexedEntityStore |
| 2025-11-26 | Phase 5 | ✅ Complete | Removed raw index methods from SystemTableStore |
| 2025-11-26 | Phase 6 | ✅ Complete | Migrated UsersTableProvider, verified other providers don't need indexes |

## Testing Strategy

1. **Unit Tests**: Test IndexedEntityStore in isolation with InMemoryBackend
2. **Integration Tests**: Test with real RocksDB backend
3. **Migration Tests**: Ensure JobsTableProvider works identically before/after
4. **DataFusion Tests**: Verify filter pushdown uses indexes correctly

## Rollback Plan

If issues arise:
1. Keep `SystemTableStore` raw index methods (don't delete in Phase 5)
2. JobsTableProvider can fall back to manual index management
3. IndexedEntityStore is additive - doesn't break existing code
