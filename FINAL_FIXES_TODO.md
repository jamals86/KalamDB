# Final 37 Compilation Errors - Systematic Fixes

## Status: Down from 58 to 37 errors after initial fixes

## Completed Fixes
✅ Added UserTableStoreExt import to user_table_insert.rs
✅ Added SharedTableStoreExt import to stream_table_provider.rs  
✅ Added StreamTableRowId import to system_table.rs
✅ Fixed Result type error in system_table.rs::load_batch
✅ Fixed UserTableRow JsonValue conversion in change_detector.rs (used .fields property)
✅ Fixed unclosed delimiter in sql/executor.rs

## Remaining Error Categories (37 errors)

### 1. SharedTableRow/UserTableRow field access (7 errors)
**Pattern**: `.as_object()` and `.get()` called on struct instead of `.fields`
**Files**: 
- executor.rs:3319, 3373, 3462, 3506 (UserTableRow)
- executor.rs (SharedTableRow)
- user_table_provider.rs:426
- services/shared_table_service.rs

**Fix**: Change `row_data.as_object()` → `row_data.fields.as_object()`
**Fix**: Change `row_data.get("key")` → `row_data.fields.get("key")`

### 2. Arc<SystemTableStore> trait method calls (10+ errors)
**Pattern**: Arc doesn't auto-implement traits, need `.as_ref()` or explicit trait call
**Files**:
- user_table_insert.rs:225
- user_table_update.rs:84, 123
- user_table_delete.rs:79, 87, 204
- stream_table_provider.rs:293, 310, 355
- stream_eviction.rs:177

**Fix pattern**:
```rust
// OLD: self.store.put(...)
// NEW: UserTableStoreExt::put(self.store.as_ref(), ...)

// OLD: self.stream_store.cleanup_expired_rows(...)
// NEW: SharedTableStoreExt::cleanup_expired_rows(self.stream_store.as_ref(), ...)
```

### 3. [u8] Sized errors (4 errors)
**Pattern**: Scan returns Vec<(K, V)> where K: AsRef<[u8]>, compiler thinks it's [u8]
**Files**: system_table.rs:692, 756

**Fix**: The scan method already returns proper types, issue is in how we destructure
```rust
// The EntityStore::scan_all() returns Vec<(K, V)>
// K is already UserTableRowId/SharedTableRowId, not [u8]
```

### 4. Missing StreamTableRow import (6 errors)
**Files**: stores/system_table.rs lines with SharedTableStoreExt implementation

**Fix**: Import StreamTableRow type
```rust
use crate::tables::stream_tables::stream_table_store::StreamTableRow;
```

### 5. Missing variables/undefined values (2 errors)
- user_table_delete.rs:117 - `row_data` not found
- stream_table_provider.rs:217 - `row` not found

**Fix**: Define variables before use (get from store)

### 6. Ambiguous trait method calls (5+ errors)
**Pattern**: Both UserTableStoreExt and SharedTableStoreExt implement same methods
**Files**:
- table_deletion_service.rs:214 (drop_table)
- user_table_update.rs:123 (put)
- user_table_delete.rs:87, 204 (delete)

**Fix**: Disambiguate with trait prefix:
```rust
UserTableStoreExt::method(self.store.as_ref(), ...)
// or
SharedTableStoreExt::method(self.store.as_ref(), ...)
```

### 7. cleanup_expired_rows trait mismatch (2 errors)
**Pattern**: Method defined but not in trait
**Files**: system_table.rs:794, stream_eviction.rs:177

**Fix**: Either add to SharedTableStoreExt trait OR call directly on store type

### 8. StreamTableStore::new() signature (6 errors in tests)
**Pattern**: Tests call ::new() with 1 arg, expects 2
**Files**: executor.rs tests (multiple)

**Fix**: Use proper constructor `new_stream_table_store(backend, namespace, table)`

## Application Strategy
1. Fix field access (Category 1) - 7 fixes
2. Add StreamTableRow import (Category 4) - 1 import
3. Fix Arc trait calls with .as_ref() (Category 2) - 10+ fixes
4. Disambiguate ambiguous trait calls (Category 6) - 5 fixes
5. Fix Sized issues (Category 3) - 2 fixes
6. Fix missing variables (Category 5) - 2 fixes
7. Fix cleanup_expired_rows (Category 7) - 2 fixes
8. Fix test constructors (Category 8) - 6 fixes

Total: 35+ individual fixes needed
