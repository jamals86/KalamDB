# Quick Start: Complete Phase 9.5 Steps D & E

## Current Status
✅ Step C: Complete (8/8 tasks)  
🔄 Step D: 20% complete (1/5 tasks - T156 done)  
📋 Step E: Ready (documentation prepared)  

## Next Task: T157 - Refactor user_table_update.rs

### File Location
```bash
code backend/crates/kalamdb-core/src/tables/user_table_update.rs
```

### Changes Required (Same Pattern as T156)

#### 1. Update Imports
```rust
// REMOVE these lines:
use crate::catalog::TableType;
use crate::storage::column_family_manager::ColumnFamilyManager;
use chrono::Utc;
use rocksdb::DB;

// ADD these lines:
use kalamdb_store::UserTableStore;
use std::time::{SystemTime, UNIX_EPOCH};  // If needed for timestamps
```

#### 2. Update Struct
```rust
// OLD:
pub struct UserTableUpdateHandler {
    db: Arc<DB>,
}

// NEW:
pub struct UserTableUpdateHandler {
    store: Arc<UserTableStore>,
}
```

#### 3. Update Constructor
```rust
// OLD:
pub fn new(db: Arc<DB>) -> Self {
    Self { db }
}

// NEW:
pub fn new(store: Arc<UserTableStore>) -> Self {
    Self { store }
}
```

#### 4. Update update_row() Method
```rust
// OLD LOGIC:
// 1. Get column family
// 2. Build RocksDB key: {user_id}:{row_id}
// 3. Get existing row from RocksDB
// 4. Merge updates
// 5. Inject _updated timestamp
// 6. Write back to RocksDB

// NEW LOGIC:
pub fn update_row(
    &self,
    namespace_id: &NamespaceId,
    table_name: &TableName,
    user_id: &UserId,
    row_id: &str,
    updates: JsonValue,
) -> Result<(), KalamDbError> {
    // Get existing row
    let mut row = self.store
        .get(
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            row_id,
        )
        .map_err(|e| KalamDbError::Other(format!("Failed to get row: {}", e)))?
        .ok_or_else(|| KalamDbError::NotFound(format!("Row not found: {}", row_id)))?;
    
    // Merge updates into existing row
    if let Some(row_obj) = row.as_object_mut() {
        if let Some(updates_obj) = updates.as_object() {
            for (key, value) in updates_obj {
                row_obj.insert(key.clone(), value.clone());
            }
        }
    }
    
    // Write back (store.put automatically updates _updated timestamp)
    self.store
        .put(
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            row_id,
            row,
        )
        .map_err(|e| KalamDbError::Other(format!("Failed to update row: {}", e)))?;
    
    Ok(())
}
```

#### 5. Update update_batch() Method
Similar simplification - iterate and call `update_row()` for each

#### 6. Update Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::RocksDbInit;
    use kalamdb_store::UserTableStore;
    use tempfile::TempDir;

    fn setup_test_handler() -> (UserTableUpdateHandler, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let init = RocksDbInit::new(temp_dir.path().to_str().unwrap());
        let db = init.open().unwrap();
        let store = Arc::new(UserTableStore::new(db).unwrap());
        let handler = UserTableUpdateHandler::new(store);
        (handler, temp_dir)
    }
    
    // Simplify tests - remove direct RocksDB verification
    // Keep functional tests that verify update behavior
}
```

### Verification
```bash
# Compile check
cd backend && cargo check -p kalamdb-core --lib

# Run tests
cargo test -p kalamdb-core --lib user_table_update

# Should see: All tests passing
```

---

## Next Task: T158 - Refactor user_table_delete.rs

### Changes Required

#### Update delete_row() Method
```rust
pub fn delete_row(
    &self,
    namespace_id: &NamespaceId,
    table_name: &TableName,
    user_id: &UserId,
    row_id: &str,
) -> Result<(), KalamDbError> {
    // Soft delete (default behavior)
    self.store
        .delete(
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            row_id,
            false,  // hard = false for soft delete
        )
        .map_err(|e| KalamDbError::Other(format!("Failed to delete row: {}", e)))?;
    
    Ok(())
}
```

#### Update hard_delete_row() Method
```rust
pub fn hard_delete_row(
    &self,
    namespace_id: &NamespaceId,
    table_name: &TableName,
    user_id: &UserId,
    row_id: &str,
) -> Result<(), KalamDbError> {
    // Hard delete (physical removal)
    self.store
        .delete(
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            row_id,
            true,  // hard = true for physical delete
        )
        .map_err(|e| KalamDbError::Other(format!("Failed to hard delete row: {}", e)))?;
    
    Ok(())
}
```

---

## Task T159 - Update user_table_service.rs

### File Location
```bash
code backend/crates/kalamdb-core/src/services/user_table_service.rs
```

### Changes Required

```rust
use kalamdb_store::UserTableStore;

pub struct UserTableService {
    kalam_sql: Arc<KalamSql>,
    db: Arc<DB>,
    user_table_store: Arc<UserTableStore>,  // ADD THIS
}

impl UserTableService {
    pub fn new(kalam_sql: Arc<KalamSql>, db: Arc<DB>) -> Result<Self, KalamDbError> {
        // Initialize UserTableStore
        let user_table_store = Arc::new(
            UserTableStore::new(db.clone())
                .map_err(|e| KalamDbError::Other(format!("Failed to create UserTableStore: {}", e)))?
        );
        
        Ok(Self {
            kalam_sql,
            db,
            user_table_store,
        })
    }
    
    // When initializing handlers, pass user_table_store:
    fn create_insert_handler(&self) -> UserTableInsertHandler {
        UserTableInsertHandler::new(self.user_table_store.clone())
    }
    
    fn create_update_handler(&self) -> UserTableUpdateHandler {
        UserTableUpdateHandler::new(self.user_table_store.clone())
    }
    
    fn create_delete_handler(&self) -> UserTableDeleteHandler {
        UserTableDeleteHandler::new(self.user_table_store.clone())
    }
}
```

---

## Task T160 - Update main.rs

### File Location
```bash
code backend/crates/kalamdb-server/src/main.rs
```

### Changes Required

```rust
// No changes needed if UserTableService handles it internally!
// The UserTableService constructor now creates its own UserTableStore

// If handlers are used outside of UserTableService:
let user_table_store = Arc::new(
    UserTableStore::new(db.clone())?
);

// Pass to any standalone handlers if needed
```

---

## Step E Quick Checklist

### T161: Verify Zero RocksDB Imports
```bash
cd backend/crates/kalamdb-core
grep -r "use rocksdb" src/ --exclude-dir=tests

# Expected output:
# src/storage/rocksdb_init.rs:use rocksdb::{DB, Options};
# src/storage/column_family_manager.rs:use rocksdb::{ColumnFamilyDescriptor, Options, DB};
# src/catalog/catalog_store.rs:use rocksdb::DB;  # DEPRECATED, OK to keep

# Any other imports = FAIL, need to refactor
```

### T162: Run Full Test Suite
```bash
cd backend
cargo test -p kalamdb-core --lib

# Verify: 258+ tests passing (allow pre-existing failures)
```

### T163: Mark CatalogStore as Deprecated
Add to `backend/crates/kalamdb-core/src/catalog/catalog_store.rs`:
```rust
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

### T164: Create Migration Guide
```bash
code ARCHITECTURE_REFACTOR_PLAN.md
# Copy content from STEP_D_E_IMPLEMENTATION_PLAN.md section "T164"
```

### T165: Update spec.md
Add to `specs/002-simple-kalamdb/spec.md`:
```markdown
## Architecture Implementation Status

### Three-Layer Data Access ✅ IMPLEMENTED (Phase 9.5)

- **kalamdb-core**: Business logic layer (NO RocksDB dependencies)
- **kalamdb-sql**: System table operations (users, namespaces, jobs, etc.)
- **kalamdb-store**: User/shared/stream table operations
- **RocksDB**: Storage engine (isolated to sql + store crates only)

Implementation completed: October 19, 2025
```

---

## Final Commands

### After All Tasks Complete

```bash
# 1. Run full test suite
cd backend && cargo test --all

# 2. Check for warnings
cargo check --all

# 3. Format code
cargo fmt --all

# 4. Stage changes
git add -A

# 5. Commit
git commit -m "Phase 9.5: Three-layer architecture refactoring complete

- Refactored all system table providers to use kalamdb-sql
- Refactored all user table handlers to use kalamdb-store
- Eliminated direct RocksDB dependencies from kalamdb-core
- Marked CatalogStore as deprecated
- Added migration guide

Tasks completed: T133-T165 (33 tasks)
Tests: 258+ passing
Architecture: Clean 3-layer separation achieved"

# 6. Push
git push origin 002-simple-kalamdb
```

---

## Time Estimate

- T157: 30 min (user_table_update.rs)
- T158: 30 min (user_table_delete.rs)
- T159: 20 min (user_table_service.rs)
- T160: 10 min (main.rs check)
- T161-T165: 30 min (verification & docs)

**Total**: ~2 hours to complete Phase 9.5

---

## Reference Files

- **Pattern Example**: `backend/crates/kalamdb-core/src/tables/user_table_insert.rs`
- **Store API**: `backend/crates/kalamdb-store/src/user_table_store.rs`
- **Detailed Plan**: `STEP_D_E_IMPLEMENTATION_PLAN.md`
- **Progress**: `PHASE_9.5_PROGRESS_REPORT.md`

## Questions?

Check `STEP_D_E_IMPLEMENTATION_PLAN.md` for detailed explanations and code examples.
