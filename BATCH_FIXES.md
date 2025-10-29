# Systematic Batch Fixes for 54 Compilation Errors

## Category 1: Import UserTableStoreExt (1 error)
### File: user_table_insert.rs:106
**Error**: use of undeclared type `UserTableStoreExt`
**Fix**: Add import at top of file
```rust
use crate::stores::system_table::UserTableStoreExt;
```

## Category 2: Import SharedTableStoreExt (3 errors)
### Files:
- stream_table_provider.rs:310
- stream_table_provider.rs:293
- stream_table_provider.rs:355

**Fix**: Add import at top of each file
```rust
use crate::stores::system_table::SharedTableStoreExt;
```

## Category 3: UserTableRow JsonValue Conversion (5 errors)
### Files:
- change_detector.rs:111, 129
- executor.rs:3319, 3462  
- user_table_provider.rs:426

**Pattern**: UserTableRow doesn't have `.get()` or `.as_object()` methods
**Fix**: Access `.fields` property which is a JsonValue:
```rust
// OLD: row_data.get("_deleted")
// NEW: row_data.fields.get("_deleted")

// OLD: row_data.as_object()
// NEW: row_data.fields.as_object()
```

## Category 4: Arc Wrapper + Trait Disambiguation (15+ errors)
### Pattern: Arc<SystemTableStore> needs .as_ref() for trait methods

**UserTableStoreExt calls**:
- change_detector.rs:105, 198, 203
- user_table_update.rs:123
- user_table_delete.rs:87, 204

**Fix pattern**:
```rust
// OLD: store.put(...)
// NEW: UserTableStoreExt::put(store.as_ref(), ...)

// OLD: self.store.get(...)  
// NEW: UserTableStoreExt::get(self.store.as_ref(), ...)

// OLD: self.store.delete(...)
// NEW: UserTableStoreExt::delete(self.store.as_ref(), ...)
```

**SharedTableStoreExt calls**:
- user_table_insert.rs:225
- user_table_update.rs:84
- user_table_delete.rs:79
- stream_table_provider.rs:293, 310, 355
- stream_eviction.rs:177

**Fix pattern**:
```rust
// OLD: self.store.scan(...)
// NEW: SharedTableStoreExt::scan(self.store.as_ref(), ...)

// OLD: self.stream_store.cleanup_expired_rows(...)
// NEW: SharedTableStoreExt::cleanup_expired_rows(self.stream_store.as_ref(), ...)
```

## Category 5: StreamTableRowId import (4 errors)
### Files:
- system_table.rs:758, 789
- stream_table_provider.rs:217, 282

**Fix**: Add import
```rust
use crate::tables::StreamTableRowId;
```

## Category 6: Result<T, E> Type Errors (2 errors)
### system_table.rs:126, 129
**Error**: Result expects 1 generic, got 2
**Fix**: Use std::result::Result or KalamDbError return type
```rust
// OLD: Result<RecordBatch, KalamDbError>
// NEW: std::result::Result<RecordBatch, KalamDbError>

// OR change error to KalamDbError directly:
Err(KalamDbError::InvalidOperation("...".to_string()))
```

## Category 7: [u8] Sized Issues (6 errors)
### system_table.rs:692, 756
**Pattern**: Scan returns Vec<(K, V)> where K implements AsRef<[u8]>
**Fix**: Convert key to String immediately
```rust
// The scan method returns Vec<(K, V)>
// OLD: for (key_bytes, row) in all_results
// NEW: for (key, row) in all_results  // K already implements AsRef<[u8]>, use directly
```

## Category 8: Missing row_data variable (1 error)
### user_table_delete.rs:117
**Error**: cannot find value `row_data`
**Fix**: Get from store first, then check
```rust
let row_data = UserTableStoreExt::get(
    self.store.as_ref(),
    namespace_id.as_str(),
    table_name.as_str(),
    user_id,
    row_id
)?;
if let Some(mut data) = row_data {
    // process...
}
```

## Category 9: Missing row variable (1 error)
### stream_table_provider.rs:217
**Error**: cannot find value `row`
**Context**: Need to create row from data before put
**Fix**: Create SharedTableRow from input data

## Category 10: table_deletion_service ambiguity (1 error)
### table_deletion_service.rs:214
**Error**: Ambiguous drop_table (UserTableStoreExt vs SharedTableStoreExt)
**Fix**: Disambiguate based on TableType
```rust
match table_type {
    TableType::User => UserTableStoreExt::drop_table(
        self.user_table_store.as_ref(),
        namespace_id.as_str(),
        table_name.as_str()
    ),
    _ => SharedTableStoreExt::drop_table(...)
}
```

## Application Order
1. Add missing imports (Categories 1, 2, 5)
2. Fix JsonValue access patterns (Category 3)
3. Fix Result types (Category 6)
4. Disambiguate trait calls with .as_ref() (Category 4)
5. Fix Sized issues (Category 7)
6. Fix missing variables (Categories 8, 9)
7. Fix ambiguous calls (Category 10)
