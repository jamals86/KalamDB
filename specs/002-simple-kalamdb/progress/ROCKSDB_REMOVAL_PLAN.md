# RocksDB Removal Plan for kalamdb-core

**Date**: 2025-10-19  
**Branch**: `002-simple-kalamdb`  
**Goal**: Eliminate all direct RocksDB imports from `kalamdb-core` to achieve proper architectural separation

## Current State Analysis

### ❌ Current Reality
`kalamdb-core` has **22 direct RocksDB imports** across multiple files, violating the architectural design.

### ✅ Target Architecture
```
kalamdb-core (business logic - ZERO RocksDB imports)
    ↓
kalamdb-sql (system tables) + kalamdb-store (user/shared/stream tables)
    ↓
RocksDB (storage engine)
```

### Architecture Principle (from plan.md)
> "kalamdb-core has ZERO direct rocksdb imports" - Success criteria line 552

---

## Files with Direct RocksDB Imports

### Category 1: Storage Layer (9 files) - **ACCEPTABLE**
These files are in `storage/` and can remain in `kalamdb-core` but should use abstractions:

1. ✅ `storage/mod.rs` - Re-exports only (acceptable)
2. ⚠️ `storage/rocksdb_store.rs` - Direct imports: `DB, Options, WriteOptions, IteratorMode, DBCompressionType`
3. ⚠️ `storage/rocksdb_init.rs` - Direct imports: `ColumnFamilyDescriptor, DB`
4. ⚠️ `storage/rocksdb_config.rs` - Direct imports: `BlockBasedOptions, Cache, Options, SliceTransform`
5. ⚠️ `storage/column_family_manager.rs` - Direct imports: `ColumnFamilyDescriptor, Options, DB`

**Strategy**: Keep these files but wrap `Arc<DB>` in a newtype pattern to hide RocksDB types from the rest of `kalamdb-core`.

---

### Category 2: Catalog Layer (1 file) - **MIGRATE TO kalamdb-sql**
6. ❌ `catalog/catalog_store.rs` - Direct imports: `ColumnFamily, DB`

**Strategy**: This should use `kalamdb-sql` crate's system table abstraction instead of direct RocksDB access.

---

### Category 3: Tables Layer (6 files) - **MIGRATE TO kalamdb-store**
These should delegate to `kalamdb-store` crate:

7. ❌ `tables/user_table_insert.rs` - Test imports: `Options, DB`
8. ❌ `tables/rocksdb_scan.rs` - Direct import: `DB`
9. ❌ `tables/hybrid_table_provider.rs` - Direct import: `DB`
10. ❌ `tables/user_table_update.rs` - Test imports: `Options, DB`
11. ❌ `tables/user_table_delete.rs` - Test imports: `Options, DB`
12. ❌ `tables/user_table_provider.rs` - Direct/test imports: `DB, Options`

**Strategy**: Replace direct RocksDB operations with `UserTableStore` from `kalamdb-store` crate.

---

### Category 4: Flush Layer (2 files) - **MIGRATE TO kalamdb-store**
13. ❌ `flush/trigger.rs` - Direct import: `DB`
14. ❌ `flush/user_table_flush.rs` - Direct imports: `IteratorMode, WriteBatch, DB, Options`

**Strategy**: Use `kalamdb-store` API for iteration and batch operations.

---

### Category 5: Services Layer (2 files) - **REMOVE TEST IMPORTS**
15. ❌ `services/namespace_service.rs` - Test imports: `DB, Options`
16. ❌ `services/user_table_service.rs` - Direct/test imports: `DB, Options`

**Strategy**: Remove test imports, use mock/test utilities from `kalamdb-store` instead.

---

### Category 6: SQL Layer (1 file) - **REMOVE TEST IMPORTS**
17. ❌ `sql/executor.rs` - Test imports: `Options, DB`

**Strategy**: Use integration test helpers instead of direct RocksDB in tests.

---

## Refactoring Steps

### Phase 1: Create Storage Abstraction Layer (NEW)
**Goal**: Hide RocksDB types behind a clean interface

#### Task 1.1: Create `StorageBackend` Trait
**File**: `backend/crates/kalamdb-core/src/storage/backend.rs`

```rust
//! Storage backend abstraction to isolate RocksDB types

use std::sync::Arc;
use anyhow::Result;

/// Storage backend interface that hides RocksDB implementation details
pub trait StorageBackend: Send + Sync {
    /// Get a reference to the underlying database (internal use only)
    fn db(&self) -> &Arc<rocksdb::DB>;
    
    /// Check if a column family exists
    fn has_cf(&self, name: &str) -> bool;
    
    /// Get column family handle by name
    fn get_cf(&self, name: &str) -> Result<rocksdb::ColumnFamily>;
}

/// RocksDB implementation of StorageBackend
pub struct RocksDbBackend {
    db: Arc<rocksdb::DB>,
}

impl RocksDbBackend {
    pub fn new(db: Arc<rocksdb::DB>) -> Self {
        Self { db }
    }
}

impl StorageBackend for RocksDbBackend {
    fn db(&self) -> &Arc<rocksdb::DB> {
        &self.db
    }
    
    fn has_cf(&self, name: &str) -> bool {
        self.db.cf_handle(name).is_some()
    }
    
    fn get_cf(&self, name: &str) -> Result<rocksdb::ColumnFamily> {
        self.db.cf_handle(name)
            .ok_or_else(|| anyhow::anyhow!("Column family not found: {}", name))
    }
}
```

**Impact**: Files in `storage/` can keep RocksDB imports but only expose `Arc<dyn StorageBackend>`.

---

### Phase 2: Migrate Tables Layer to kalamdb-store
**Goal**: Remove all RocksDB imports from `tables/` directory

#### Task 2.1: Update `user_table_insert.rs`
**Current**: Direct RocksDB operations
```rust
use rocksdb::DB;
let cf = db.cf_handle(&cf_name)?;
db.put_cf(cf, key.as_bytes(), &value)?;
```

**After**: Use `kalamdb-store`
```rust
use kalamdb_store::UserTableStore;
let store = UserTableStore::new(db)?;
store.put(namespace_id, table_name, user_id, row_id, row_data)?;
```

**Files to update**:
- `tables/user_table_insert.rs`
- `tables/user_table_update.rs`
- `tables/user_table_delete.rs`
- `tables/user_table_provider.rs`

#### Task 2.2: Update `rocksdb_scan.rs`
**Current**: Direct iteration over RocksDB
```rust
use rocksdb::DB;
let iter = db.iterator_cf(cf, IteratorMode::Start);
```

**After**: Use `kalamdb-store` scan API
```rust
use kalamdb_store::UserTableStore;
let store = UserTableStore::new(db)?;
let rows = store.scan(namespace_id, table_name, user_id)?;
```

**Required**: Add `scan()` method to `UserTableStore` in `kalamdb-store` crate.

#### Task 2.3: Update `hybrid_table_provider.rs`
**Current**: Passes `Arc<DB>` directly
```rust
use rocksdb::DB;
pub struct HybridTableProvider {
    db: Arc<DB>,
}
```

**After**: Use storage abstraction
```rust
use crate::storage::StorageBackend;
pub struct HybridTableProvider {
    backend: Arc<dyn StorageBackend>,
}
```

---

### Phase 3: Migrate Flush Layer to kalamdb-store
**Goal**: Remove RocksDB imports from flush operations

#### Task 3.1: Update `flush/user_table_flush.rs`
**Current**: Direct RocksDB iteration and WriteBatch
```rust
use rocksdb::{IteratorMode, WriteBatch, DB};
let iter = db.iterator_cf(cf, IteratorMode::Start);
let batch = WriteBatch::default();
```

**After**: Use `kalamdb-store` batch operations
```rust
use kalamdb_store::UserTableStore;
let store = UserTableStore::new(db)?;
let (rows, delete_keys) = store.flush_batch(namespace_id, table_name)?;
```

**Required**: Add `flush_batch()` method to `UserTableStore`:
```rust
impl UserTableStore {
    /// Get all rows and prepare delete keys for flush operation
    pub fn flush_batch(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> Result<(Vec<(String, JsonValue)>, Vec<Vec<u8>>)> {
        // Returns (rows to flush, keys to delete after successful write)
    }
    
    /// Delete flushed rows in batch
    pub fn delete_flushed(
        &self,
        namespace_id: &str,
        table_name: &str,
        keys: Vec<Vec<u8>>,
    ) -> Result<()> {
        // Batch delete operation
    }
}
```

#### Task 3.2: Update `flush/trigger.rs`
**Current**: Direct DB reference
```rust
use rocksdb::DB;
pub fn schedule_flush(db: Arc<DB>, ...) { }
```

**After**: Use storage backend
```rust
use crate::storage::StorageBackend;
pub fn schedule_flush(backend: Arc<dyn StorageBackend>, ...) { }
```

---

### Phase 4: Migrate Services Layer
**Goal**: Remove RocksDB imports from service tests

#### Task 4.1: Create Test Utilities in kalamdb-store
**File**: `backend/crates/kalamdb-store/src/test_utils.rs`

```rust
#[cfg(test)]
pub mod test_utils {
    use rocksdb::{DB, Options};
    use tempfile::TempDir;
    use std::sync::Arc;
    
    pub struct TestDb {
        pub db: Arc<DB>,
        _dir: TempDir,
    }
    
    impl TestDb {
        pub fn new() -> anyhow::Result<Self> {
            let dir = tempfile::tempdir()?;
            let mut opts = Options::default();
            opts.create_if_missing(true);
            let db = Arc::new(DB::open(&opts, dir.path())?);
            Ok(Self { db, _dir: dir })
        }
    }
}
```

#### Task 4.2: Update Service Tests
**Files**:
- `services/namespace_service.rs`
- `services/user_table_service.rs`

**Before**:
```rust
#[cfg(test)]
mod tests {
    use rocksdb::{DB, Options};
    
    #[test]
    fn test_something() {
        let dir = tempfile::tempdir().unwrap();
        let db = DB::open(&Options::default(), dir.path()).unwrap();
        // ...
    }
}
```

**After**:
```rust
#[cfg(test)]
mod tests {
    use kalamdb_store::test_utils::TestDb;
    
    #[test]
    fn test_something() {
        let test_db = TestDb::new().unwrap();
        // Use test_db.db as Arc<DB>
    }
}
```

---

### Phase 5: Migrate SQL Layer Tests
**Goal**: Remove RocksDB test imports from SQL executor

#### Task 5.1: Update `sql/executor.rs` Tests
Same pattern as Phase 4 - use `kalamdb_store::test_utils::TestDb`.

---

### Phase 6: Migrate Catalog Layer to kalamdb-sql
**Goal**: Use system table abstraction instead of direct RocksDB

#### Task 6.1: Update `catalog/catalog_store.rs`
**Current**: Direct RocksDB access
```rust
use rocksdb::{ColumnFamily, DB};
let cf = db.cf_handle("system_table:tables")?;
```

**After**: Use `kalamdb-sql` crate
```rust
// Use SELECT * FROM system.tables instead
// This requires rethinking catalog access pattern
```

**Note**: This is more complex and may require architectural discussion. The catalog layer sits between SQL and storage, so it may legitimately need some storage access. Consider if this is an exception to the "zero RocksDB imports" rule.

---

## Enhanced kalamdb-store API

### Required New Methods

#### 1. Scan Operations
```rust
impl UserTableStore {
    /// Scan all rows for a specific user
    pub fn scan(
        &self,
        namespace_id: &str,
        table_name: &str,
        user_id: &str,
    ) -> Result<Vec<JsonValue>>;
    
    /// Scan all rows across all users (for DataFusion integration)
    pub fn scan_all(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> Result<Vec<(String, JsonValue)>>; // Returns (user_id, row_data)
}
```

#### 2. Batch Operations
```rust
impl UserTableStore {
    /// Prepare batch for flush operation
    pub fn flush_batch(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> Result<FlushBatch>;
    
    /// Delete keys after successful flush
    pub fn delete_flushed(&self, batch: FlushBatch) -> Result<()>;
}

pub struct FlushBatch {
    pub rows: Vec<(String, String, JsonValue)>, // (user_id, row_id, data)
    pub keys: Vec<Vec<u8>>,
}
```

#### 3. Iteration Support
```rust
impl UserTableStore {
    /// Create an iterator over user rows
    pub fn iter(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> Result<UserTableIterator>;
}

pub struct UserTableIterator {
    // Internal RocksDB iterator (private)
}

impl Iterator for UserTableIterator {
    type Item = Result<(String, String, JsonValue)>; // (user_id, row_id, data)
    fn next(&mut self) -> Option<Self::Item>;
}
```

---

## Implementation Checklist

### Phase 1: Storage Abstraction (5 tasks)
- [ ] **T_REFACTOR_1**: Create `storage/backend.rs` with `StorageBackend` trait
- [ ] **T_REFACTOR_2**: Implement `RocksDbBackend` struct
- [ ] **T_REFACTOR_3**: Update `storage/mod.rs` to export backend types
- [ ] **T_REFACTOR_4**: Update `hybrid_table_provider.rs` to use `Arc<dyn StorageBackend>`
- [ ] **T_REFACTOR_5**: Run tests to verify storage abstraction works

### Phase 2: Tables Layer Migration (8 tasks)
- [ ] **T_REFACTOR_6**: Add `scan()` method to `UserTableStore` in kalamdb-store
- [ ] **T_REFACTOR_7**: Add `scan_all()` method to `UserTableStore` in kalamdb-store
- [ ] **T_REFACTOR_8**: Update `rocksdb_scan.rs` to use `UserTableStore::scan_all()`
- [ ] **T_REFACTOR_9**: Update `user_table_insert.rs` to use `UserTableStore` (remove RocksDB imports)
- [ ] **T_REFACTOR_10**: Update `user_table_update.rs` to use `UserTableStore` (remove RocksDB imports)
- [ ] **T_REFACTOR_11**: Update `user_table_delete.rs` to use `UserTableStore` (remove RocksDB imports)
- [ ] **T_REFACTOR_12**: Update `user_table_provider.rs` to use storage abstraction
- [ ] **T_REFACTOR_13**: Run all table layer tests to verify migration

### Phase 3: Flush Layer Migration (5 tasks)
- [ ] **T_REFACTOR_14**: Add `flush_batch()` method to `UserTableStore` in kalamdb-store
- [ ] **T_REFACTOR_15**: Add `delete_flushed()` method to `UserTableStore` in kalamdb-store
- [ ] **T_REFACTOR_16**: Add iterator support to `UserTableStore` in kalamdb-store
- [ ] **T_REFACTOR_17**: Update `flush/user_table_flush.rs` to use `UserTableStore` API
- [ ] **T_REFACTOR_18**: Update `flush/trigger.rs` to use storage abstraction

### Phase 4: Services Layer Migration (3 tasks)
- [ ] **T_REFACTOR_19**: Create `test_utils.rs` in kalamdb-store crate with `TestDb` helper
- [ ] **T_REFACTOR_20**: Update `services/namespace_service.rs` tests to use `TestDb`
- [ ] **T_REFACTOR_21**: Update `services/user_table_service.rs` tests to use `TestDb`

### Phase 5: SQL Layer Migration (1 task)
- [ ] **T_REFACTOR_22**: Update `sql/executor.rs` tests to use `TestDb` from kalamdb-store

### Phase 6: Catalog Layer Migration (2 tasks)
- [ ] **T_REFACTOR_23**: Review catalog access pattern (may need architectural exception)
- [ ] **T_REFACTOR_24**: Either migrate to kalamdb-sql or justify as storage layer exception

### Phase 7: Cleanup (3 tasks)
- [ ] **T_REFACTOR_25**: Remove `rocksdb` dependency from `kalamdb-core/Cargo.toml`
- [ ] **T_REFACTOR_26**: Run `cargo build --package kalamdb-core` to verify zero RocksDB imports
- [ ] **T_REFACTOR_27**: Run full test suite to ensure all 47 Phase 9 tests still pass

---

## Verification Strategy

### Step 1: Grep Check
```bash
# Should return ZERO matches after refactoring
grep -r "use rocksdb" backend/crates/kalamdb-core/src/
grep -r "extern crate rocksdb" backend/crates/kalamdb-core/src/
```

### Step 2: Cargo Check
```bash
# Remove rocksdb from kalamdb-core/Cargo.toml
# Then verify it compiles
cd backend/crates/kalamdb-core
cargo build
```

### Step 3: Test Verification
```bash
# All 47 Phase 9 tests must pass
cargo test --package kalamdb-core
cargo test --workspace
```

---

## Deprecated/Unused Import Analysis

### Test-Only Imports (Safe to Remove)
These imports only appear in `#[cfg(test)]` blocks:
- `services/namespace_service.rs` - test imports
- `services/user_table_service.rs` - test imports
- `sql/executor.rs` - test imports
- `tables/user_table_insert.rs` - test imports
- `tables/user_table_update.rs` - test imports
- `tables/user_table_delete.rs` - test imports
- `tables/user_table_provider.rs` - test imports
- `flush/user_table_flush.rs` - test imports

**Action**: Replace with `kalamdb_store::test_utils::TestDb` pattern.

---

## Success Criteria

✅ **Zero RocksDB Imports**: `grep -r "use rocksdb" backend/crates/kalamdb-core/src/` returns 0 matches  
✅ **Cargo Verification**: `kalamdb-core/Cargo.toml` has NO `rocksdb` dependency  
✅ **Build Success**: `cargo build --package kalamdb-core` compiles without errors  
✅ **Test Preservation**: All 47 Phase 9 tests still pass  
✅ **Architecture Compliance**: Clean separation: `kalamdb-core` → `kalamdb-store` → RocksDB

---

## Timeline Estimate

- **Phase 1** (Storage Abstraction): 2-3 hours
- **Phase 2** (Tables Layer): 4-6 hours
- **Phase 3** (Flush Layer): 3-4 hours
- **Phase 4** (Services Tests): 1-2 hours
- **Phase 5** (SQL Tests): 1 hour
- **Phase 6** (Catalog Review): 2-3 hours (requires design decision)
- **Phase 7** (Cleanup & Verification): 1-2 hours

**Total**: 14-21 hours of development work

---

## Open Questions

1. **Catalog Layer Exception**: Should `catalog/catalog_store.rs` be allowed to keep RocksDB imports since it's part of the storage layer infrastructure? Or should it exclusively use `kalamdb-sql` system tables?

2. **Storage Abstraction Scope**: Should the `StorageBackend` trait expose RocksDB types at all, or should we create a fully generic K/V abstraction?

3. **Test Strategy**: Should test utilities live in `kalamdb-store` or should we create a separate `kalamdb-test-utils` crate?

4. **Performance Impact**: Will the additional abstraction layers impact write/query performance? Should we benchmark before/after?

---

## Notes

- This refactoring maintains backward compatibility at the API level
- No changes to REST API or WebSocket protocol
- All user-facing SQL behavior remains identical
- Internal architecture becomes cleaner and more maintainable
- Easier to swap storage backends in the future (e.g., LMDB, Sled)
