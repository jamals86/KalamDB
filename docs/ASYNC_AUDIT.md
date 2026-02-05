# Async/Sync Boundary Audit — End-to-End `/sql` → `object_store`

**Purpose**: Complete call-chain trace for every SQL operation (SELECT, INSERT, UPDATE, DELETE)
from the HTTP endpoint to `object_store` and `RocksDB`, identifying every blocking call in
async contexts and every unnecessary sync duplicate method.

**Last Updated**: 2026-02-05

---

## Architecture Rules

| Storage Layer | Native API | Correct Pattern |
|---|---|---|
| **RocksDB** | Sync (C++ FFI) | `EntityStore` (sync) + `EntityStoreAsync` (spawn_blocking) ✅ |
| **object_store** | Async (Rust native) | Only async methods; **no sync wrappers** needed |
| **Flush jobs** | Inside `spawn_blocking` | Sync methods are correct ✅ |

---

## Complete Call Chains

### SELECT Path ✅ (Fully Async)

```
POST /v1/api/sql                                    [ASYNC] ✅
  → execute_sql_v1()                                [ASYNC] ✅  kalamdb-api/.../execute.rs
    → execute_single_statement()                    [ASYNC] ✅  kalamdb-api/.../executor.rs
      → SqlExecutor::execute_with_metadata()        [ASYNC] ✅  kalamdb-core/.../sql_executor.rs
        → execute_via_datafusion() → session.sql()  [ASYNC] ✅
          → DataFusion calls TableProvider::scan()   [ASYNC] ✅
            → base_scan() → scan_rows()             [ASYNC] ✅
              → scan_with_version_resolution_to_kvs_async()  [ASYNC] ✅

HOT STORAGE:
  EntityStoreAsync::scan_with_raw_prefix_async()    [ASYNC + spawn_blocking] ✅
  EntityStoreAsync::scan_typed_with_prefix_and_start_async() [ASYNC + spawn_blocking] ✅

COLD STORAGE:
  scan_parquet_files_as_batch_async()                [ASYNC] ✅  kalamdb-tables/.../parquet.rs
    → ManifestAccessPlanner::scan_parquet_files_async() [ASYNC] ✅  kalamdb-tables/.../planner.rs
      → StorageCached::list_parquet_files()           [ASYNC] ✅  kalamdb-filestore/.../storage_cached.rs
      → StorageCached::read_parquet_files()           [ASYNC] ✅
        → object_store.get()                          [ASYNC] ✅
```

**Verdict**: SELECT path is clean. No blocking calls in async contexts.

---

### INSERT Path ⚠️ (PK Check Blocks)

```
POST /v1/api/sql                                    [ASYNC] ✅
  → InsertHandler::execute()                        [ASYNC] ✅  kalamdb-core/.../insert.rs
    → execute_native_insert()                       [ASYNC] ✅
      → executor.execute_user_data(cmd).await       [ASYNC] ✅  (routes through Raft)
        → RaftManager::propose_user_data()          [ASYNC] ✅  kalamdb-raft/...
          → Raft consensus commit                   [ASYNC] ✅
            → StateMachine::apply()                 [ASYNC] ✅

=== AFTER RAFT APPLY (sync territory) ===

UserDataApplier::insert()                           [ASYNC] ✅
  → DmlExecutor::insert_user_data()                 [ASYNC] ✅  kalamdb-core/.../dml/executor.rs
    → UserTableProvider::insert_batch()             [SYNC] ⚠️  kalamdb-tables/.../user_table_provider.rs

INSIDE insert_batch() — ALL SYNC:
  1. ManifestService::get_or_load()                 [SYNC] ✅ (RocksDB only, fast)
  2. check columns/validation                      [SYNC] ✅ (CPU only)
  3. PK uniqueness check:
     ├─ small batch (≤2): pk_exists_in_cold()       [SYNC] ❌ BLOCKS! → list_sync() → get_sync()
     │    → StorageCached::list_sync()              [SYNC] ❌ run_blocking(object_store.list())
     │    → StorageCached::get_sync()               [SYNC] ❌ run_blocking(object_store.get())
     └─ large batch (>2): hot-only PK index         [SYNC] ✅ (RocksDB only)
  4. ensure_unique_pk_value()                       [SYNC] ✅ (hot-only index lookup)
  5. EntityStore::insert_batch() → WriteBatch       [SYNC] ✅ (RocksDB)
```

**Issues**:
- `pk_exists_in_cold()` in `base.rs:645` calls `list_sync()` and `get_sync()` — blocks tokio worker
- Same pattern in SharedTableProvider::insert_batch

---

### UPDATE Path ❌ (Multiple Blocking Points)

```
POST /v1/api/sql                                     [ASYNC] ✅
  → UpdateHandler::execute()                         [ASYNC] ✅  kalamdb-core/.../update.rs
    ├─ USER: find_by_pk() on UserTableProvider       [SYNC] ✅ (RocksDB PK index only, no cold)
    ├─ SHARED + needs_current_row:
    │    find_row_by_pk()                            [SYNC] ❌ BLOCKS!
    │      → scan_parquet_files_as_batch()           [SYNC] ❌ kalamdb-tables/.../parquet.rs:13
    │        → planner.scan_parquet_files()          [SYNC] ❌ kalamdb-tables/.../planner.rs:72
    │          → list_parquet_files_sync()           [SYNC] ❌ run_blocking(object_store.list())
    │          → read_parquet_files_sync()           [SYNC] ❌ run_blocking(object_store.get())
    └─ executor.execute_user_data(cmd).await          [ASYNC] ✅ (routes through Raft)

=== AFTER RAFT APPLY ===

DmlExecutor::update_user_data()                      [ASYNC] ✅
  → find_row_by_pk() (for file-ref cleanup)          [SYNC] ❌ BLOCKS! (same chain ↑)
  → UserTableProvider::update_batch()                [SYNC]
    → update_by_pk_value()                           [SYNC]
      ├─ find_by_pk() (hot, RocksDB PK index)       [SYNC] ✅
      ├─ FALLBACK: find_row_by_pk() (cold)           [SYNC] ❌ BLOCKS! (same chain ↑)
      └─ store.insert() (RocksDB write)              [SYNC] ✅

⚠️ DOUBLE COLD LOOKUP: find_row_by_pk() called TWICE per update when row is only in cold storage:
   1. DmlExecutor calls it for file-ref cleanup
   2. update_by_pk_value() calls it again for the actual row
```

---

### DELETE Path ❌ (Multiple Blocking Points)

```
POST /v1/api/sql                                      [ASYNC] ✅
  → DeleteHandler::execute()                          [ASYNC] ✅  kalamdb-core/.../delete.rs
    ├─ extract_pks_from_where() (string parse)        [SYNC] ✅ (CPU only)
    ├─ OR: collect_pks_with_datafusion()              [ASYNC] ✅ (full SELECT scan)
    └─ executor.execute_user_data(cmd).await           [ASYNC] ✅ (Raft)

=== AFTER RAFT APPLY ===

DmlExecutor::delete_user_data()                       [ASYNC] ✅
  → find_row_by_pk() (for file-ref cleanup)           [SYNC] ❌ BLOCKS! (object_store via run_blocking)
  → UserTableProvider::delete_batch()                 [SYNC]
    → delete_by_pk_value()                            [SYNC]
      ├─ find_by_pk() (hot, RocksDB PK index)        [SYNC] ✅
      ├─ FALLBACK: find_row_by_pk() (cold)            [SYNC] ❌ BLOCKS! (same chain)
      └─ store.insert(tombstone)                      [SYNC] ✅ (RocksDB write)

⚠️ Same DOUBLE COLD LOOKUP issue as UPDATE path.
```

---

### Flush Job Path ✅ (Correct — inside spawn_blocking)

```
FlushExecutor::execute()                              [ASYNC] ✅  kalamdb-core/.../flush.rs
  → tokio::task::spawn_blocking()                     [spawn_blocking] ✅ lines 176, 216, 284

INSIDE spawn_blocking:
  flush_shared_table_rows()                           [SYNC] ✅
    → write_parquet_sync()                            [SYNC] ✅ (inside spawn_blocking)
    → rename_sync()                                   [SYNC] ✅ (inside spawn_blocking)
    → flush_manifest() → write_manifest_sync()        [SYNC] ✅ (inside spawn_blocking)
  flush_user_table_rows()                             [SYNC] ✅ (same pattern)
  rebuild_manifest()                                  [SYNC] ✅ (inside spawn_blocking)
    → list_parquet_files_sync()                       [SYNC] ✅ (inside spawn_blocking)
    → head_sync()                                     [SYNC] ✅ (inside spawn_blocking)
    → write_manifest_sync()                           [SYNC] ✅ (inside spawn_blocking)
```

**Verdict**: Flush path is correct. All sync methods run inside spawn_blocking.

---

### CleanupExecutor Path ⚠️

```
CleanupExecutor::execute()                            [ASYNC] ✅  kalamdb-core/.../cleanup.rs
  → drop_partition()                                  [SYNC] ⚠️ RocksDB (no spawn_blocking)
  → delete_table_definition()                         [SYNC] ⚠️ RocksDB (no spawn_blocking)
  → storage_cached.delete_prefix().await              [ASYNC] ✅
```

**Issue**: RocksDB `drop_partition` can take 10-100ms+ and blocks the tokio executor.

---

### FileStorageService Path ❌ (All Sync from Async Handlers)

```
Actix HTTP handler (file upload/download)             [ASYNC] ✅
  → FileStorageService::finalize_file()               [SYNC] ❌
    → storage.put_sync()                              [SYNC] ❌ run_blocking(object_store.put())
  → FileStorageService::delete_file()                 [SYNC] ❌
    → storage.delete_sync()                           [SYNC] ❌ run_blocking(object_store.delete())
  → FileStorageService::get_file()                    [SYNC] ❌
    → storage.get_sync()                              [SYNC] ❌ run_blocking(object_store.get())

Also called from DmlExecutor (UPDATE/DELETE for file-ref cleanup):
  → find_row_by_pk()                                  [SYNC] ❌ (already flagged above)
```

---

## StorageCached `_sync` Method Audit — Caller Analysis

### Methods SAFE TO KEEP (only called from flush jobs inside `spawn_blocking`):

| Method | Callers | Context |
|---|---|---|
| `write_parquet_sync()` | `flush.rs` flush_shared/user_table_rows | ✅ Inside `spawn_blocking` |
| `rename_sync()` | `flush.rs` flush_shared/user_table_rows | ✅ Inside `spawn_blocking` |
| `write_manifest_sync()` | `flush.rs` → `flush_manifest()` | ✅ Inside `spawn_blocking` |
| `head_sync()` | `flush.rs` → `rebuild_manifest()` | ✅ Inside `spawn_blocking` |

### Methods ALREADY REMOVED ✅:

| Method | Status |
|---|---|
| `exists_sync()` | ✅ Removed — tests updated to async |
| `prefix_exists_sync()` | ✅ Removed — tests updated to async |
| `delete_prefix_sync()` | ✅ Removed — tests updated to async |

### Methods TO REMOVE (called from async contexts WITHOUT `spawn_blocking`):

| Method | Callers (blocking async) | Fix |
|---|---|---|
| `list_parquet_files_sync()` | `planner.rs:scan_parquet_files()`, `manifest/service.rs:rebuild_manifest()` | Use `list_parquet_files()` async |
| `read_parquet_files_sync()` | `planner.rs:scan_parquet_files()` | Use `read_parquet_files()` async |
| `read_manifest_sync()` | `manifest/service.rs:read_manifest()` | Use `get()` async |
| `list_sync()` | `base.rs:pk_exists_in_cold()`, `planner.rs` | Use `list()` async |
| `get_sync()` | `base.rs:pk_exists_in_parquet_via_storage_cache()`, `file_service.rs` | Use `get()` async |
| `put_sync()` | `file_service.rs:finalize_file()` | Use `put()` async |
| `delete_sync()` | `file_service.rs:delete_file()` | Use `delete()` async |

### Sync-only `scan_parquet_files_as_batch()` — REMOVE

| Function | File | Line | Status |
|---|---|---|---|
| `scan_parquet_files_as_batch()` [SYNC] | `kalamdb-tables/src/utils/parquet.rs` | ~13 | ❌ Remove |
| `scan_parquet_files_as_batch_async()` [ASYNC] | `kalamdb-tables/src/utils/parquet.rs` | ~60+ | ✅ Keep (rename to `scan_parquet_files_as_batch`) |

### Sync-only `scan_parquet_files()` — REMOVE

| Function | File | Line | Status |
|---|---|---|---|
| `scan_parquet_files()` [SYNC] | `kalamdb-tables/src/manifest/planner.rs` | 72 | ❌ Remove |
| `scan_parquet_files_async()` [ASYNC] | `kalamdb-tables/src/manifest/planner.rs` | 207 | ✅ Keep (rename to `scan_parquet_files`) |

---

## PkExistenceChecker Audit

| Method | Type | Status |
|---|---|---|
| `check_pk_exists()` | ASYNC | ✅ Merged — sync+async unified into single async method |
| `check_cold_storage()` | ASYNC | ✅ Merged — sync version removed |
| `load_manifest_from_storage_async()` | ASYNC | ✅ Only async version remains |
| `pk_exists_in_parquet_async()` | ASYNC | ✅ Only async version remains |

**Verdict**: ✅ DONE — All sync duplicates removed. Only async methods remain. No production callers (used by specs only).

---

## FileStorageService Audit

| Method | Uses sync storage? | Called from | Fix |
|---|---|---|---|
| `finalize_file()` | `put_sync()` | DML executor (async context, no spawn_blocking) | Make async, use `put()` |
| `delete_file()` | `delete_sync()` | DML executor (async context, no spawn_blocking) | Make async, use `delete()` |
| `get_file()` | `get_sync()` | Actix async handler (no spawn_blocking) | Make async, use `get()` |
| `get_file_by_path()` | `get_sync()` | Actix async handler (no spawn_blocking) | Make async, use `get()` |

**All** FileStorageService I/O methods block the tokio executor. They should all be made async.

---

## ManifestService Audit

| Method | Uses sync filestore? | Called from | Fix |
|---|---|---|---|
| `flush_manifest()` | `write_manifest_sync()` | Flush jobs (inside `spawn_blocking`) | ✅ Keep as sync |
| `read_manifest()` | `read_manifest_sync()` | Flush jobs (inside `spawn_blocking`) | ✅ Keep as sync |
| `rebuild_manifest()` | `list_parquet_files_sync()`, `head_sync()`, `write_manifest_sync()` | Flush jobs (inside `spawn_blocking`) | ✅ Keep as sync |
| Most other methods | RocksDB only | Various | ✅ RocksDB is fast |

**Verdict**: ManifestService methods are only called from flush jobs inside `spawn_blocking`. They are **correct as sync**.

---

## Methods to REMOVE (Summary)

### From `StorageCached` (`kalamdb-filestore/src/registry/storage_cached.rs`):

| Method | Why Remove |
|---|---|
| `list_parquet_files_sync()` | Async version `list_parquet_files()` exists; only caller is the sync planner being removed |
| `read_parquet_files_sync()` | Async version `read_parquet_files()` exists; same |
| `read_manifest_sync()` | Only used by ManifestService which is inside spawn_blocking → uses `get_sync` internally anyway. If no callers remain, remove. |
| `list_sync()` | Only caller is `pk_exists_in_cold()` which should be made async |
| ~~`delete_prefix_sync()`~~ | ✅ DONE — Removed |
| ~~`exists_sync()`~~ | ✅ DONE — Removed |
| ~~`prefix_exists_sync()`~~ | ✅ DONE — Removed |

### Keep in `StorageCached` (used by flush jobs inside `spawn_blocking`):

| Method | Why Keep |
|---|---|
| `get_sync()` | Used by `pk_exists_in_parquet_via_storage_cache` — BUT this should be made async. Also used by FileStorageService (should be made async). **KEEP only if flush path needs it. Otherwise REMOVE.** |
| `put_sync()` | Used by FileStorageService only — make FileStorageService async → REMOVE |
| `delete_sync()` | Used by FileStorageService only — make FileStorageService async → REMOVE |
| `write_parquet_sync()` | ✅ KEEP — used by flush jobs inside spawn_blocking |
| `rename_sync()` | ✅ KEEP — used by flush jobs inside spawn_blocking |
| `write_manifest_sync()` | ✅ KEEP — used by flush jobs inside spawn_blocking |
| `head_sync()` | ✅ KEEP — used by rebuild_manifest inside spawn_blocking |

### From `ManifestAccessPlanner` (`kalamdb-tables/src/manifest/planner.rs`):

| Method | Why Remove |
|---|---|
| `scan_parquet_files()` [SYNC] | Async version exists. Rename `scan_parquet_files_async()` → `scan_parquet_files()` |

### From `parquet.rs` (`kalamdb-tables/src/utils/parquet.rs`):

| Method | Why Remove |
|---|---|
| `scan_parquet_files_as_batch()` [SYNC] | Async version exists. Rename `scan_parquet_files_as_batch_async()` → `scan_parquet_files_as_batch()` |

### From `PkExistenceChecker` (`kalamdb-tables/src/utils/pk/existence_checker.rs`):

| Method | Status |
|---|---|
| ~~`check_pk_exists()` [SYNC]~~ | ✅ DONE — Merged into single async method |
| ~~`check_cold_storage()` [SYNC]~~ | ✅ DONE — Merged into single async method |
| ~~`load_manifest_from_storage()` [SYNC]~~ | ✅ DONE — Removed |
| ~~`pk_exists_in_parquet()` [SYNC]~~ | ✅ DONE — Removed |

### From `base.rs` (`kalamdb-tables/src/utils/base.rs`):

| Method | Why |
|---|---|
| `find_row_by_pk()` [SYNC] | Must create async version `find_row_by_pk_async()` to replace it |
| `pk_exists_in_cold()` [SYNC] | Must create async version |
| `pk_exists_batch_in_cold()` [SYNC] | Must create async version |
| `pk_exists_in_parquet_via_storage_cache()` [SYNC] | Must create async version |

---

## Priority Fix Order

### Phase 1: Core DML Cold-Storage Path (Highest Impact)

These changes unblock UPDATE/DELETE on cold-storage rows from blocking the tokio executor:

1. **Create `find_row_by_pk_async()`** in `base.rs` using `scan_parquet_files_as_batch_async()`
2. **Create `pk_exists_in_cold_async()`** in `base.rs` using `list_parquet_files()` + `get()` async
3. **Make DmlExecutor methods truly async** — use the new async versions
4. **Make `update_by_pk_value()` / `delete_by_pk_value()` async** on UserTableProvider/SharedTableProvider

### Phase 2: INSERT PK Check Path

1. **Make `insert_batch()` async** (or at minimum the PK existence check part)
2. **Use `pk_exists_in_cold_async()`** instead of `pk_exists_in_cold()`
3. **Make `ensure_unique_pk_value()` async**

### Phase 3: FileStorageService

1. Make `finalize_file()`, `delete_file()`, `get_file()`, `get_file_by_path()` async
2. Remove `put_sync()`, `get_sync()`, `delete_sync()` from StorageCached (if no other callers)

### Phase 4: Remove Dead Sync Code

1. Remove `scan_parquet_files()` sync from `ManifestAccessPlanner`, rename async
2. Remove `scan_parquet_files_as_batch()` sync from `parquet.rs`, rename async
3. ~~Remove sync methods from `PkExistenceChecker`~~ ✅ DONE
4. Remove `list_parquet_files_sync()`, `read_parquet_files_sync()` from StorageCached
5. ~~Remove `exists_sync()`, `prefix_exists_sync()`, `delete_prefix_sync()` from StorageCached~~ ✅ DONE
6. ~~Update tests to use async versions~~ ✅ DONE

### Phase 5: CleanupExecutor

1. Wrap `drop_partition()`, `delete_table_definition()`, `get_table_if_exists()` in `spawn_blocking`

---

## Testing Strategy

After each phase:
1. `cd backend && cargo check 2>&1 | head -200`
2. `cd backend && cargo nextest run`
3. Look for "Cannot start runtime within runtime" panics
4. Run smoke tests: `cd cli && cargo test --test smoke -- --nocapture`

---

**Status**: Comprehensive audit complete. Phase 4 partial (PkExistenceChecker + StorageCached test-only sync methods removed). Ready for Phase 1 implementation.
**Last Updated**: 2026-02-05
