# Phase 14: EntityStore Refactoring - Implementation Summary

**Date**: October 29, 2025  
**Status**: Specification Complete, Ready for Implementation  
**Documents Created**: 
- `PHASE_14_ENTITYSTORE_REFACTORING.md` - Full architectural design
- Updated `spec.md` - Added FR-ARCH-007 (EntityStore Pattern)
- Updated `tasks.md` - Added 36 detailed tasks (T180-T215)

---

## What Was Completed

### 1. ✅ Architectural Design Document

Created comprehensive `PHASE_14_ENTITYSTORE_REFACTORING.md` covering:

- **Motivation**: 600+ lines of duplicated CRUD code across 4 stores
- **Problems**: Code duplication, inconsistent APIs, lack of type safety, no shared behavior
- **Solution**: Unified `EntityStore<K, V>` trait with type-safe keys
- **Architecture**: Before/After diagrams showing 50% code reduction (800→400 lines)
- **Type-Safe Keys**: **5 newtype wrappers** (RowId, UserRowId, TableId, JobId, LiveQueryId)
  - **Note**: Uses existing `NamespaceId` and `StorageId` directly (no wrappers needed)
- **SystemTableStore**: Generic over **both K and V** (`SystemTableStore<K, V>`) since each system table has different key type
- **Implementation Plan**: 8 steps with **33 tasks** (reduced from initial 36)
- **Benefits**: Code reduction, type safety, consistent APIs, index integration
- **Migration Strategy**: 3-phase gradual migration (non-breaking additions, gradual migration, cleanup)
- **Success Criteria**: Measurable goals (400+ lines eliminated, zero compilation errors, all tests pass)
- **Risks & Mitigations**: Documented with impact analysis

### 2. ✅ Updated spec.md

Added new architectural requirement `FR-ARCH-007` (EntityStore Pattern):

```rust
/// Generic entity storage with type-safe keys
pub trait EntityStore<K, V> 
where
    K: AsRef<[u8]> + Clone + Send + Sync,
    V: Serialize + Deserialize + Send + Sync
{
    fn backend(&self) -> &Arc<dyn StorageBackend>;
    fn partition(&self) -> &str;
    fn put(&self, key: &K, value: &V) -> Result<()>;
    fn get(&self, key: &K) -> Result<Option<V>>;
    // ... more methods
}

/// Cross-user table storage (system tables + shared tables)
pub trait CrossUserTableStore<K, V>: EntityStore<K, V> {
    fn table_access(&self) -> Option<TableAccess>;
}
```

Documented:
- Trait definitions with examples
- Type-safe key models (all in `kalamdb-commons/src/models/`)
- Implementation patterns for system/shared/user tables
- Benefits (400+ line elimination, type safety, consistent APIs)
- Reference to detailed migration plan
- **Coexistence with old EntityStore<T>**: New `EntityStore<K,V>` will coexist with old `EntityStore<T>` in `traits.rs` during migration

### 3. ✅ Created Comprehensive Task List

Added **33 detailed tasks (T180-T212)** organized into 8 steps:

**Step 1: Type-Safe Key Models (T180-T186)** - 7 tasks
- `RowId` - for shared/stream tables
- `UserRowId` - composite key for user tables (user_id:row_id)
- `TableId` - composite key for system.tables (namespace:table) ← **Changed from TableMetadataId**
- `JobId` - for system.jobs
- `LiveQueryId` - for system.live_queries
- `UserName` - secondary index key for system.users (username → UserId) ← **New**
- Export all from models/mod.rs
- **Removed**: NamespaceKey, StorageKey (use existing NamespaceId, StorageId directly)
- **Reused**: UserId for system.users primary key

**Step 2: EntityStore Traits (T187-T189)** - 3 tasks
- `EntityStore<K, V>` base trait
- `CrossUserTableStore<K, V>` specialized trait
- Module exports (will coexist with old EntityStore<T>)

**Step 3: SystemTableStore (T190)** - 1 task
- `SystemTableStore<K, V>` generic over **BOTH** key and value ← **Changed from SystemTableStore<V>**
- Each system table uses different key type:
  - `SystemTableStore<UserId, User>` - with SecondaryIndex<User, UserName> ← **Changed from Vec<u8>**
  - `SystemTableStore<TableId, TableMetadata>` - composite key
  - `SystemTableStore<JobId, Job>` - job ID
  - `SystemTableStore<NamespaceId, Namespace>` - existing ID
  - `SystemTableStore<StorageId, Storage>` - existing ID

**Step 4: System Table Migration (T191-T196)** - 6 tasks

**New Folder Structure**: `backend/crates/kalamdb-core/src/tables/system/{table}/`
- Each system table gets its own subfolder with:
  - `{table}_provider.rs` - TableProvider implementation
  - `{table}_store.rs` - SystemTableStore<K, V> wrapper
  - `{table}_table.rs` - Table schema model
  - `{table}_{index}_index.rs` - Secondary indexes (if any)

- Migrate UsersTableProvider → `tables/system/users/` (users_provider.rs, users_store.rs, users_table.rs, users_username_index.rs)
  - Uses `SystemTableStore<UserId, User>` + `SecondaryIndex<User, UserName>`
- Migrate SystemTablesTableProvider → `tables/system/tables/` (tables_provider.rs, tables_store.rs, tables_table.rs)
  - Uses `SystemTableStore<TableId, TableMetadata>`
- Migrate JobsTableProvider → `tables/system/jobs/` (jobs_provider.rs, jobs_store.rs, jobs_table.rs)
  - Uses `SystemTableStore<JobId, Job>`
- Migrate NamespacesTableProvider → `tables/system/namespaces/`
  - Uses `SystemTableStore<NamespaceId, Namespace>`
- Migrate StoragesTableProvider → `tables/system/storages/`
  - Uses `SystemTableStore<StorageId, Storage>`
- Migrate LiveQueriesTableProvider → `tables/system/live_queries/`
  - Uses `SystemTableStore<LiveQueryId, LiveQuery>`

**Step 5: Table Store Refactoring (T197-T199)** - 3 tasks

**New Folder Structure**: `backend/crates/kalamdb-core/src/tables/{type}/`
- `tables/shared/` - shared_table_provider.rs, shared_table_store.rs
- `tables/user/` - user_table_provider.rs, user_table_store.rs  
- `tables/stream/` - stream_table_provider.rs, stream_table_store.rs

- SharedTableStore → `tables/shared/shared_table_store.rs` (SharedTableStoreImpl)
- UserTableStore → `tables/user/user_table_store.rs` (UserTableStoreImpl)
- StreamTableStore → `tables/stream/stream_table_store.rs` (StreamTableStoreImpl)

**Step 6: Update Callers (T200-T205)** - 6 tasks
- Update SharedTableProvider
- Update UserTableProvider
- Update StreamTableProvider
- Update SQL executors
- Update flush service
- Update restore service

**Step 7: Testing (T206-T210)** - 5 tasks
- Update SharedTableStore tests
- Update UserTableStore tests
- Update StreamTableStore tests
- Create EntityStore trait tests
- Run full integration suite

**Step 8: Documentation (T211-T213)** - 3 tasks
- Update AGENTS.md
- Create ENTITYSTORE_PATTERN.md
- Deprecate old EntityStore<T> in traits.rs

### 4. ✅ Updated Task Metadata

- **Total task count**: 377 tasks (was 341) across 17 phases (was 16)
- **Phase 14**: 36 new tasks
- **Dependencies**: Phase 14 depends on Phase 13, blocks all user stories
- **Updated dependency graph**: Shows Phase 0.5 → Phase 13 → Phase 14 → Everything Else

### 5. ✅ Updated Dependency Graph

```
Phase 0.5: Storage Refactoring
    │
    └─> Phase 13: Generic Index Infrastructure
            │
            └─> Phase 14: EntityStore Refactoring ← NEW FOUNDATIONAL
                    │
                    ├─> Setup (Phase 1)
                    │       └─> Foundational (Phase 2)
                    │               ├─ US1-US8 (All User Stories)
                    │               └─> Testing & Migration
                    └─> (All phases depend on Phase 14)
```

**Critical Execution Order**:
1. Phase 0.5 - Storage Backend Abstraction
2. Phase 13 - Generic Index Infrastructure ✅ **COMPLETE**
3. Phase 14 - EntityStore Refactoring ← **NEXT**
4. Everything else

---

## Type-Safe Key Models Summary

All placed in `backend/crates/kalamdb-commons/src/models/`:

| Key Type | Purpose | Format | Example |
|----------|---------|--------|---------|
| `RowId` | Shared/Stream tables | `Vec<u8>` | `b"doc_123"` |
| `UserRowId` | User tables (composite) | `{user_id}:{row_id}` | `"alice:todo_1"` |
| `TableId` | system.tables (composite) | `{namespace}:{table}` | `TableId::new(ns_id, table_name)` |
| `JobId` | system.jobs | `String` | `"job_12345"` |
| `LiveQueryId` | system.live_queries | `String` | `"lq_456"` |
| `UserName` | system.users secondary index | `String` | `UserName::new("alice")` |

**Reused Existing Types** (no new wrappers needed):
- `UserId` - for system.users primary key
- `NamespaceId` - for system.namespaces
- `StorageId` - for system.storages

**Total**: **6 new key types** (was 5)
- Added: `UserName` → secondary index for system.users (username → UserId lookup)
- Reused: `UserId` as primary key for system.users
| `LiveQueryId` | system.live_queries | `String` | `"lq_67890"` |

All implement:
- `AsRef<[u8]>` - for storage compatibility
- `Clone + Send + Sync` - for thread safety
- `Display` - for debugging
- `From<String>`, `From<&str>` - for conversions

---

## Implementation Strategy Summary

### Phase 1: Non-Breaking Additions (Week 1)
- Create 7 key types in `kalamdb-commons/src/models/`
- Create `EntityStore<K,V>` traits in `kalamdb-store/src/entity_store.rs`
- Create `SystemTableStore<V>` in `kalamdb-core/src/stores/system_table.rs`
- **No existing code broken**

### Phase 2: Gradual Migration (Week 2)
- Migrate system table providers one at a time (6-7 providers)
- Refactor shared/user/stream stores one at a time (3 stores)
- Update callers module by module (6 modules)
- **Old code still works during transition**

### Phase 3: Cleanup (Week 3)
- Remove old implementations
- Update all tests
- Verify full integration suite passes
- Update documentation
- **Final state: Clean, unified architecture**

---

## Expected Benefits

### Quantitative
- **400+ lines eliminated** (50% reduction: 800→400 lines)
- **7 newtype wrappers** prevent type confusion bugs
- **2 reusable traits** replace 4 custom implementations
- **10+ stores** all use same pattern

### Qualitative
- ✅ Type safety at compile time
- ✅ Consistent APIs everywhere
- ✅ Easier testing (mock implementations)
- ✅ Better index integration
- ✅ Single source of truth for serialization
- ✅ Clearer ownership and lifetimes

### Examples

**Before** (String Soup):
```rust
// ❌ Easy to make mistakes
shared_store.put("default", "docs", "doc123", data)?;
user_store.put("default", "todos", "alice", "todo1", data)?;
// Which param is which? Compiler can't help!
```

**After** (Type Safety):
```rust
// ✅ Compiler enforces correctness
let row_id = RowId::from_string("doc123");
shared_store.put(&row_id, &row)?;  // Can't mix up parameters!

let user_row = UserRowId::new(user_id, b"todo1");
user_store.put(&user_row, &row)?;  // Type-checked at compile time!
```

---

## Next Steps

### Immediate (This Week)
1. **T180-T185**: Create **6 type-safe key models** in `kalamdb-commons` (RowId, UserRowId, TableId, JobId, LiveQueryId, UserName)
2. **T186**: Export all from `models/mod.rs`
3. **T187-T189**: Define `EntityStore<K,V>` traits in `kalamdb-store` (coexists with old EntityStore<T>)
4. **T190**: Implement `SystemTableStore<K,V>` generic (generic over BOTH key and value)

### Near-Term (Next 2 Weeks)
5. **T191-T196**: Migrate 6 system table providers (each with specific key type, UsersTableProvider gets SecondaryIndex<User, UserName>)
6. **T197-T199**: Refactor 3 table stores (shared/user/stream)
7. **T200-T205**: Update all callers (6 tasks)

### Final (Week 3)
8. **T206-T210**: Update all tests, verify integration (5 tasks)
9. **T211-T213**: Documentation, cleanup, deprecate old EntityStore<T> (3 tasks)

**Total**: **34 tasks** (increased from 33 due to UserName addition)

---

## Success Criteria Checklist

- [ ] **6 key types** created with full test coverage (RowId, UserRowId, TableId, JobId, LiveQueryId, UserName)
- [ ] `EntityStore<K, V>` trait defined with provided methods
- [ ] `SystemTableStore<K, V>` generic implementation complete (generic over BOTH K and V)
- [ ] All 6 system table providers migrated with specific key types:
  - [ ] UsersTableProvider → `SystemTableStore<UserId, User>` with `SecondaryIndex<User, UserName>`
  - [ ] SystemTablesTableProvider → `SystemTableStore<TableId, TableMetadata>`
  - [ ] JobsTableProvider → `SystemTableStore<JobId, Job>`
  - [ ] NamespacesTableProvider → `SystemTableStore<NamespaceId, Namespace>`
  - [ ] StoragesTableProvider → `SystemTableStore<StorageId, Storage>`
  - [ ] LiveQueriesTableProvider → `SystemTableStore<LiveQueryId, LiveQuery>`
- [ ] Shared/User/Stream table stores refactored
- [ ] Zero compilation errors/warnings
- [ ] All existing tests pass
- [ ] New type-safety tests added
- [ ] Code reduction: ≥400 lines eliminated
- [ ] Documentation updated (AGENTS.md, architecture docs)
- [ ] Old `EntityStore<T>` in traits.rs marked as deprecated

---

## References

- **Main Spec**: [PHASE_14_ENTITYSTORE_REFACTORING.md](./PHASE_14_ENTITYSTORE_REFACTORING.md)
- **Architecture**: [spec.md#FR-ARCH-007](./spec.md)
- **Tasks**: [tasks.md#Phase-14](./tasks.md) - **34 tasks (T180-T213)**
- **Related**: [PHASE_13_INDEX_INFRASTRUCTURE.md](./PHASE_13_INDEX_INFRASTRUCTURE.md)
- **Foundation**: [AGENTS.md](../../AGENTS.md)

---

**Status**: ✅ Planning Complete, Ready to Execute  
**Estimated Effort**: 3-5 days (36 tasks)  
**Risk Level**: Medium (breaking changes mitigated by gradual migration)  
**Priority**: P0 - Architectural Foundation (blocks user stories)
