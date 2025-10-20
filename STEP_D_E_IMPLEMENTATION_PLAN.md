# Phase 9.5 Steps D & E - Implementation Plan

## Current Status

✅ **T156 COMPLETE** - `user_table_insert.rs` refactored to use `Arc<UserTableStore>`
- Constructor updated to `new(store: Arc<UserTableStore>)`
- All RocksDB operations replaced with `store.put()`
- Tests updated and passing
- File compiles successfully

## Remaining Tasks

### Step D: Refactor User Table Handlers (T157-T160)

**T157** - Update `user_table_update.rs`:
```rust
// Change from:
pub struct UserTableUpdateHandler {
    db: Arc<DB>,
}

impl UserTableUpdateHandler {
    pub fn new(db: Arc<DB>) -> Self

// Change to:
pub struct UserTableUpdateHandler {
    store: Arc<UserTableStore>,
}

impl UserTableUpdateHandler {
    pub fn new(store: Arc<UserTableStore>) -> Self
    
// Replace update logic:
// OLD: Manual RocksDB get + put with system column updates
// NEW: store.get() + store.put() (system columns auto-handled)
```

**T158** - Update `user_table_delete.rs`:
```rust
// Change from:
pub struct UserTableDeleteHandler {
    db: Arc<DB>,
}

// Change to:
pub struct UserTableDeleteHandler {
    store: Arc<UserTableStore>,
}

// Replace delete logic:
// Soft delete: store.delete(namespace_id, table_name, user_id, row_id, false)
// Hard delete: store.delete(namespace_id, table_name, user_id, row_id, true)
```

**T159** - Update `user_table_service.rs`:
```rust
// Add UserTableStore to constructor
pub struct UserTableService {
    kalam_sql: Arc<KalamSql>,
    db: Arc<DB>,
    user_table_store: Arc<UserTableStore>,  // ADD THIS
}

impl UserTableService {
    pub fn new(kalam_sql: Arc<KalamSql>, db: Arc<DB>) -> Self {
        let user_table_store = Arc::new(UserTableStore::new(db.clone()).unwrap());
        Self { kalam_sql, db, user_table_store }
    }
    
// Pass user_table_store to handlers when initializing them
```

**T160** - Update `main.rs`:
```rust
// In main() function, after creating db:
let user_table_store = Arc::new(UserTableStore::new(db.clone())?);

// Pass to handlers/services that need it
```

### Step E: Verify and Document (T161-T165)

**T161** - Verify zero rocksdb imports:
```bash
cd /Users/jamal/git/KalamDB/backend/crates/kalamdb-core
grep -r "use rocksdb" src/ --exclude-dir=tests
# Expected: Only in storage/ and catalog/ modules (deprecated)
```

**T162** - Run test suite:
```bash
cargo test -p kalamdb-core --lib
# Verify: All tests pass (258+ tests)
```

**T163** - Mark CatalogStore as deprecated:
```rust
// In backend/crates/kalamdb-core/src/catalog/catalog_store.rs:

#[deprecated(
    since = "0.2.0",
    note = "Use kalamdb-sql for system tables and kalamdb-store for user tables. \
            This struct will be removed in 0.3.0. \
            See ARCHITECTURE_REFACTOR_PLAN.md for migration guide."
)]
pub struct CatalogStore {
    // ... existing code
}
```

**T164** - Create migration guide:
```markdown
# File: ARCHITECTURE_REFACTOR_PLAN.md

## CatalogStore Deprecation

### What Changed

The `CatalogStore` has been deprecated in favor of:
- **kalamdb-sql**: For system table operations
- **kalamdb-store**: For user/shared/stream table operations

### Migration Path

#### Before (deprecated):
```rust
let catalog_store = Arc::new(CatalogStore::new(db));
let provider = SystemTableProvider::new(catalog_store);
```

#### After (recommended):
```rust
// For system tables:
let kalam_sql = Arc::new(KalamSql::new(db)?);
let provider = SystemTableProvider::new(kalam_sql);

// For user tables:
let user_store = Arc::new(UserTableStore::new(db)?);
let handler = UserTableInsertHandler::new(user_store);
```

### Timeline

- **0.2.0**: CatalogStore marked as deprecated (warnings issued)
- **0.3.0**: CatalogStore will be removed entirely
```

**T165** - Update spec.md:
```markdown
## Architecture

### Three-Layer Data Access ✅ IMPLEMENTED

```
Application Layer (kalamdb-core)
    ↓ uses
Abstraction Layer (kalamdb-sql + kalamdb-store)
    ↓ uses  
Storage Layer (RocksDB)
```

**kalamdb-core**: Business logic, NO direct RocksDB dependencies
**kalamdb-sql**: System table operations (namespaces, users, jobs, etc.)
**kalamdb-store**: User/shared/stream table operations
**RocksDB**: Low-level storage engine (isolated to 2 crates)

Status: ✅ Fully implemented as of Phase 9.5
```

## Quick Implementation Script

Given the repetitive nature of T157-T158, here's the pattern:

### For each handler file:

1. **Update imports**:
   - Remove: `use rocksdb::DB;`
   - Remove: `use chrono::Utc;` (if using timestamps directly)
   - Remove: `use crate::storage::column_family_manager::ColumnFamilyManager;`
   - Remove: `use crate::catalog::TableType;`
   - Add: `use kalamdb_store::UserTableStore;`

2. **Update struct**:
   ```rust
   pub struct HandlerName {
       store: Arc<UserTableStore>,  // Was: db: Arc<DB>
   }
   ```

3. **Update constructor**:
   ```rust
   pub fn new(store: Arc<UserTableStore>) -> Self {
       Self { store }  // Was: Self { db }
   }
   ```

4. **Update methods**:
   - Replace all `self.db.*` with `self.store.*`
   - Use `store.put()`, `store.get()`, `store.delete()`
   - Remove manual system column injection
   - Remove manual CF lookups

5. **Update tests**:
   ```rust
   fn setup_test_handler() -> (HandlerName, TempDir) {
       let temp_dir = TempDir::new().unwrap();
       let init = RocksDbInit::new(temp_dir.path().to_str().unwrap());
       let db = init.open().unwrap();
       let store = Arc::new(UserTableStore::new(db).unwrap());
       let handler = HandlerName::new(store);
       (handler, temp_dir)
   }
   ```

## Success Criteria Checklist

- [ ] T157: user_table_update.rs refactored
- [ ] T158: user_table_delete.rs refactored  
- [ ] T159: user_table_service.rs updated
- [ ] T160: main.rs updated
- [ ] T161: Zero rocksdb imports in kalamdb-core (except storage/catalog deprecated modules)
- [ ] T162: All tests passing
- [ ] T163: CatalogStore marked deprecated
- [ ] T164: Migration guide created
- [ ] T165: spec.md updated with ✅ IMPLEMENTED markers

## Estimated Time

- T157-T158: 30 minutes each (1 hour total)
- T159-T160: 20 minutes each (40 minutes total)
- T161-T165: 30 minutes total
- **Total**: ~2.5 hours for complete Phase 9.5 Steps D & E

## Next Session Commands

```bash
# Start with T157
code backend/crates/kalamdb-core/src/tables/user_table_update.rs

# Apply the same refactoring pattern as user_table_insert.rs
# Then move to T158, T159, T160, and finally T161-T165
```
