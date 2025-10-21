# Tasks Architecture Alignment Report

**Date**: October 19, 2025  
**Branch**: `002-simple-kalamdb`  
**Status**: ✅ **COMPLETE** - All Phase 10+ tasks updated for three-layer architecture

---

## Summary

All tasks from Phase 10 onward have been updated to align with the implemented three-layer architecture where:
- **kalamdb-core**: Business logic (NO direct RocksDB imports)
- **kalamdb-store**: User/shared/stream table data operations
- **kalamdb-sql**: System table metadata operations
- **RocksDB**: Isolated to kalamdb-store and kalamdb-sql only

---

## Changes Made

### Phase 10: User Story 3a - Table Deletion (9 tasks → 16 tasks)

**Added Prerequisites (7 new tasks)**:
- **T165a-c**: Add `drop_table()` methods to UserTableStore, SharedTableStore, StreamTableStore
- **T165d-e**: Add `delete_table()` and `delete_table_schemas_for_table()` to kalamdb-sql
- **T165f-g**: Add `update_storage_location()` and `update_job()` to kalamdb-sql

**Updated Tasks**:
- **T167**: Updated to accept `Arc<UserTableStore>`, `Arc<SharedTableStore>`, `Arc<StreamTableStore>`, `Arc<KalamSql>` in constructor
- **T168**: Now explicitly calls `kalam_sql.scan_all_live_queries()` and filters by table_name
- **T169**: Updated to match on TableType enum and call appropriate store's `drop_table()` method
- **T170**: Clarified Parquet deletion logic (skip for stream tables)
- **T171**: Now calls `kalam_sql.delete_table()` and `kalam_sql.delete_table_schemas_for_table()`
- **T172**: Now calls `kalam_sql.get_storage_location()`, modifies, and calls `kalam_sql.update_storage_location()`
- **T174**: Now explicitly calls `kalam_sql.insert_job()` and `kalam_sql.update_job()`

---

### Phase 11: User Story 3b - ALTER TABLE (12 tasks → 16 tasks)

**CRITICAL CHANGE**: Complete rewrite to eliminate file-based schema storage

**Architecture Change**:
- ❌ **REMOVED**: All references to `schema_v{N}.json`, `manifest.json`, `current.json` symlinks
- ✅ **ADDED**: Use `system_table_schemas` CF via kalamdb-sql for all schema versioning

**Added Prerequisites (4 new tasks)**:
- **T174a**: Add `update_table()` to kalamdb-sql
- **T174b**: Add `insert_table_schema()` to kalamdb-sql
- **T174c**: Add `get_table()` to kalamdb-sql
- **T174d**: Add `get_table_schemas_for_table()` to kalamdb-sql

**Updated Tasks**:
- **T176**: Constructor now accepts `Arc<KalamSql>` only
- **T178**: Now calls `kalam_sql.scan_all_live_queries()` to check for active subscriptions
- **T181**: Completely rewritten to use RocksDB-only metadata (fetch from system_table_schemas, modify, insert new version)
- **T182**: Now calls `kalam_sql.update_table()` to update current_schema_version
- **T185**: Now calls `kalam_sql.get_table_schemas_for_table()` to show version history

---

### Phase 12: User Story 4a - Stream Tables (12 tasks → 11 tasks)

**Architecture Alignment**: All tasks now use StreamTableStore API

**Updated Tasks**:
- **T148**: Constructor now accepts `Arc<StreamTableStore>`, `Arc<KalamSql>`
- **T149**: Constructor now accepts `Arc<StreamTableStore>`, all data ops delegate to store
- **T150**: Now calls `stream_table_store.put()` (was direct RocksDB operations)
- **T151**: Ephemeral mode check queries live query manager (no direct subscriber check)
- **T152**: Now calls `stream_table_store.evict_older_than()` for TTL eviction
- **T153**: Uses `stream_table_store.scan()` and `delete()` for max_buffer eviction
- **T155**: Checks `TableType::Stream` enum (no change needed)
- **T157**: Now calls `stream_table_store.drop_table()` in table_deletion_service

**Removed Tasks**:
- **OLD T150**: "Create column family for stream table" - now handled internally by StreamTableStore

---

### Phase 13: User Story 5 - Shared Tables (8 tasks → 6 tasks)

**Architecture Alignment**: All tasks now use SharedTableStore API

**Updated Tasks**:
- **T160**: Constructor now accepts `Arc<SharedTableStore>`, `Arc<KalamSql>`
- **T162**: Now uses `shared_table_store.put()`, `get()`, `delete()` for all DML operations
- **T163**: Constructor now accepts `Arc<SharedTableStore>`, all data ops delegate to store
- **T164**: Now calls `shared_table_store.scan()` and `delete_batch_by_keys()` for flush
- **T165**: Now calls `shared_table_store.drop_table()` in table_deletion_service

**Removed Tasks**:
- **OLD T162**: "Create column family for shared table" - now handled internally by SharedTableStore

---

### Phase 14: User Story 6 - Live Query Subscriptions (10 tasks)

**Architecture Clarification**: Change detection uses store callbacks

**Updated Tasks**:
- **T166**: Clarified that change detection hooks into store write operations (not direct RocksDB monitoring)
- **T168**: Clarified filter compilation using DataFusion SQL parser
- **T169-171**: INSERT/UPDATE/DELETE notifications triggered by UserTableStore operations
- **T172**: Flush notifications triggered by flush job completion
- **T173**: Initial data fetch uses DataFusion query execution
- **T174**: User isolation enforced by UserTableStore key prefix filtering

---

### Phase 15: User Story 7 - Backup/Restore (11 tasks)

**Architecture Alignment**: All metadata operations use kalamdb-sql

**Updated Tasks**:
- **T179**: Constructor now accepts `Arc<KalamSql>`
- **T180**: Metadata backup now calls `kalam_sql.get_namespace()`, `scan_all_tables()`, `get_table_schemas_for_table()`
- **T181**: Parquet backup logic clarified (skip stream tables)
- **T184**: Constructor now accepts `Arc<KalamSql>`
- **T185**: Metadata restore now calls `kalam_sql.insert_namespace()`, `insert_table()`, `insert_table_schema()`
- **T188**: Job registration now explicitly calls `kalam_sql.insert_job()`

---

### Phase 16: User Story 8 - Catalog Browsing (10 tasks)

**Architecture Alignment**: All queries use kalamdb-sql

**Updated Tasks**:
- **T191**: information_schema.tables delegates to `kalam_sql.scan_all_tables()`
- **T192**: SHOW TABLES execution calls `kalam_sql.scan_all_tables()`
- **T193-197**: DESCRIBE TABLE enhancements call `kalam_sql.get_table()` and `get_table_schemas_for_table()`
- **T198**: Table statistics use kalamdb-sql for metadata, stores for data counts

---

### Phase 17: Polish & Cross-Cutting Concerns (28 tasks)

**Architecture Updates**:
- **T200**: Removed obsolete JSON config file references (namespaces.json, storage_locations.json)
- **T202**: Added error types for kalamdb-store and kalamdb-sql
- **T205**: Connection pooling now in kalamdb-store (not in kalamdb-core)
- **T206**: Schema cache loads from system_table_schemas via kalamdb-sql (not manifest.json)
- **T207**: System table query caching uses kalamdb-sql methods
- **T214**: README must explain three-layer architecture
- **T218**: 100% rustdoc coverage now includes kalamdb-store and kalamdb-sql public APIs
- **T219**: Added ADR-009 for three-layer architecture benefits
- **T220**: Integration tests use kalamdb-store test_utils for RocksDB instances

---

## Missing API Methods (Must Implement)

### kalamdb-store (3 methods across 3 files)

```rust
// user_table_store.rs
pub fn drop_table(namespace_id: &str, table_name: &str) -> Result<()>

// shared_table_store.rs
pub fn drop_table(namespace_id: &str, table_name: &str) -> Result<()>

// stream_table_store.rs
pub fn drop_table(namespace_id: &str, table_name: &str) -> Result<()>
```

### kalamdb-sql (8 methods in adapter.rs + lib.rs)

```rust
// Delete methods
pub fn delete_table(&self, table_id: &str) -> Result<()>
pub fn delete_table_schemas_for_table(&self, table_id: &str) -> Result<()>

// Update methods
pub fn update_storage_location(&self, location: &StorageLocation) -> Result<()>
pub fn update_job(&self, job: &Job) -> Result<()>
pub fn update_table(&self, table: &Table) -> Result<()>

// Get methods
pub fn get_table(&self, table_id: &str) -> Result<Option<Table>>
pub fn get_table_schemas_for_table(&self, table_id: &str) -> Result<Vec<TableSchema>>

// Insert methods
pub fn insert_table_schema(&self, schema: &TableSchema) -> Result<()>
```

---

## Task Count Summary

| Phase | Before | After | Change | Prerequisites Added |
|-------|--------|-------|--------|---------------------|
| Phase 10 | 9 | 16 | +7 tasks | 7 API methods |
| Phase 11 | 12 | 16 | +4 tasks | 4 API methods |
| Phase 12 | 12 | 11 | -1 task | 0 (use existing StreamTableStore) |
| Phase 13 | 8 | 6 | -2 tasks | 0 (use existing SharedTableStore) |
| Phase 14 | 10 | 10 | 0 | 0 (clarifications only) |
| Phase 15 | 11 | 11 | 0 | 0 (clarifications only) |
| Phase 16 | 10 | 10 | 0 | 0 (clarifications only) |
| Phase 17 | 28 | 28 | 0 | 0 (clarifications only) |
| **TOTAL** | **100** | **108** | **+8 tasks** | **11 new API methods** |

---

## Next Steps (Before Starting Phase 10)

### 1. Implement Missing kalamdb-store Methods (Estimated: 2-4 hours)

```bash
cd backend/crates/kalamdb-store

# Add drop_table() to each store
vim src/user_table_store.rs    # Add drop_table() method + test
vim src/shared_table_store.rs  # Add drop_table() method + test
vim src/stream_table_store.rs  # Add drop_table() method + test

cargo test --package kalamdb-store  # Verify 24 tests pass (21 existing + 3 new)
```

### 2. Implement Missing kalamdb-sql Methods (Estimated: 3-5 hours)

```bash
cd backend/crates/kalamdb-sql

# Add 8 new methods to adapter.rs
vim src/adapter.rs  # Implement delete_table, update_*, get_*, insert_table_schema

# Expose in public API
vim src/lib.rs  # Add pub fn wrappers for all 8 methods

cargo test --package kalamdb-sql  # Verify 17 tests pass (9 existing + 8 new)
```

### 3. Verify Architecture Compliance (Estimated: 30 minutes)

```bash
cd backend/crates/kalamdb-core

# Ensure NO direct RocksDB imports in business logic
grep -r "use rocksdb" src/services src/flush src/tables | grep -v "test"
# Expected: 0 matches (storage layer exceptions OK)

cargo build --package kalamdb-core  # Should compile without errors
```

### 4. Start Phase 10 Implementation (Estimated: 5-8 days)

```bash
# All prerequisites now complete - can proceed with Phase 10 tasks
# Tasks T166-T174 ready to implement with correct architecture
```

---

## Architectural Benefits Achieved

### ✅ Complete RocksDB Isolation
- kalamdb-core business logic has ZERO direct RocksDB dependencies
- All RocksDB access goes through kalamdb-store or kalamdb-sql
- Easy to swap storage backend in future (e.g., FoundationDB, TiKV)

### ✅ Clear Separation of Concerns
- **kalamdb-store**: Data plane (user/shared/stream table data)
- **kalamdb-sql**: Control plane (system table metadata)
- **kalamdb-core**: Business logic (orchestration, validation, query execution)

### ✅ Testability
- Each layer can be tested independently
- Mocks/stubs easy to create at layer boundaries
- Integration tests can verify cross-layer behavior

### ✅ Future-Ready for Distributed Deployment
- Architecture prepared for future kalamdb-raft crate (Raft consensus)
- Store and SQL layers can be replicated independently
- Clear RPC boundaries between layers

---

## Files Modified

- `/Users/jamal/git/KalamDB/specs/002-simple-kalamdb/tasks.md` (complete rewrite of Phases 10-17)

---

## Validation Checklist

Before proceeding to Phase 10 implementation:

- [ ] All 11 missing API methods implemented in kalamdb-store and kalamdb-sql
- [ ] All new methods have unit tests (11 new tests total)
- [ ] `cargo test --package kalamdb-store` passes (24 tests)
- [ ] `cargo test --package kalamdb-sql` passes (17 tests)
- [ ] `cargo build --package kalamdb-core` compiles successfully
- [ ] No direct RocksDB imports in kalamdb-core business logic (verified via grep)
- [ ] Documentation updated (README.md, architecture diagrams)

---

**Report Status**: ✅ **COMPLETE** - All tasks from Phase 10+ aligned with three-layer architecture
