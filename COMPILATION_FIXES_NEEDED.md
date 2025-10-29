# Comprehensive Compilation Fixes (58 Errors)

## Summary
The main issue is that user table handlers are calling `SharedTableStoreExt` methods when they should call `UserTableStoreExt` methods. Additionally, all Arc<SystemTableStore> calls need `.as_ref()`.

## Critical Root Cause
**UserTableStore** is a type alias for `SystemTableStore<UserTableRowId, UserTableRow>`.
- It implements BOTH `UserTableStoreExt` AND `SharedTableStoreExt` 
- When calling methods, the compiler finds multiple matches
- The user table code should ONLY use `UserTableStoreExt`

## Required Changes (By File)

### 1. `backend/crates/kalamdb-core/src/tables/user_tables/user_table_insert.rs`

**Lines 106-113:** Change from `SharedTableStoreExt::put` to `UserTableStoreExt::put`
```rust
// BEFORE:
SharedTableStoreExt::put(
    &self.store,
    namespace_id.as_str(),
    table_name.as_str(),
    &row_id,
    &entity,
)

// AFTER:
UserTableStoreExt::put(
    self.store.as_ref(),
    namespace_id.as_str(),
    table_name.as_str(),
    user_id.as_str(),
    &row_id,
    &entity,
)
```

**Lines 223-230:** Same change for batch insert

### 2. `backend/crates/kalamdb-core/src/tables/user_tables/user_table_update.rs`

**Lines 84-91:** Change from `SharedTableStoreExt::get` to `UserTableStoreExt::get`
```rust
// BEFORE:
SharedTableStoreExt::get(
    &self.store,
    namespace_id.as_str(),
    table_name.as_str(),
    row_id,
)

// AFTER:
UserTableStoreExt::get(
    self.store.as_ref(),
    namespace_id.as_str(),
    table_name.as_str(),
    user_id.as_str(),
    row_id,
)
```

**Lines 122-128:** Change put call to use EntityStore directly (it only has 2 params: key, value)
```rust
// BEFORE:
self.store
    .put(
        namespace_id.as_str(),
        table_name.as_str(),
        row_id,
        &updated_row,
    )

// AFTER:
UserTableStoreExt::put(
    self.store.as_ref(),
    namespace_id.as_str(),
    table_name.as_str(),
    user_id.as_str(),
    row_id,
    &updated_row,
)
```

### 3. `backend/crates/kalamdb-core/src/tables/user_tables/user_table_delete.rs`

**Lines 78-83:** Change from `SharedTableStoreExt::get` to `UserTableStoreExt::get`
**Lines 86-92:** Change from `self.store.delete` to `UserTableStoreExt::delete`
**Lines 203-209:** Change from `self.store.delete` to `UserTableStoreExt::delete` for hard delete

### 4. `backend/crates/kalamdb-core/src/tables/user_tables/user_table_provider.rs`

**Line 426:** Change `row_data.get("_deleted")` to `row_data._deleted`
```rust
// BEFORE:
row_data
    .get("_deleted")

// AFTER:
row_data._deleted
```

### 5. `backend/crates/kalamdb-core/src/sql/executor.rs`

**Line 3319:** Change `row_data.as_object()` to `row_data.fields.as_object()`
**Line 3462:** Same change
**Line 3504:** Change `for (row_id, row_data)` to `for (row_id_str, row_data): (String, _)`

### 6. `backend/crates/kalamdb-core/src/live_query/change_detector.rs`

Need to convert `UserTableRow` to `JsonValue` when creating notifications:
```rust
// BEFORE:
ChangeNotification::update(table_name.to_string(), old_data, new_value)

// AFTER:  
ChangeNotification::update(
    table_name.to_string(),
    serde_json::to_value(old_data).unwrap(),
    serde_json::to_value(new_value).unwrap()
)
```

### 7. `backend/crates/kalamdb-core/src/tables/stream_tables/stream_table_provider.rs`

**Add import at top:**
```rust
use crate::stores::system_table::SharedTableStoreExt;
```

**Lines 292-293, 353-357:** Add `.map_err()` and collect() to scan calls
```rust
// BEFORE:
self.store
    .scan(self.namespace_id().as_str(), self.table_name().as_str())

// AFTER:
SharedTableStoreExt::scan(
    self.store.as_ref(),
    self.namespace_id().as_str(),
    self.table_name().as_str(),
).map_err(|e| datafusion::error::DataFusionError::Execution(e.to_string()))?
```

### 8. Add SharedTableStoreExt imports to:
- `backend/crates/kalamdb-core/src/services/stream_table_service.rs`
- `backend/crates/kalamdb-core/src/jobs/stream_eviction.rs`  
- `backend/crates/kalamdb-core/src/services/table_deletion_service.rs`

### 9. `backend/crates/kalamdb-core/src/services/table_deletion_service.rs`

**Line 214:** Disambiguate drop_table call:
```rust
// BEFORE:
.drop_table(namespace_id.as_str(), table_name.as_str())

// AFTER:
UserTableStoreExt::drop_table(&self.user_table_store, namespace_id.as_str(), table_name.as_str())
```

### 10. `backend/crates/kalamdb-core/src/stores/system_table.rs`

**Line 129:** Fix load_batch return type:
```rust
// BEFORE:
fn load_batch(&self) -> Result<arrow::record_batch::RecordBatch, KalamDbError> {
    Err(kalamdb_store::StorageError::Other(Box::new(...)))
}

// AFTER:
fn load_batch(&self) -> std::result::Result<arrow::record_batch::RecordBatch, KalamDbError> {
    Err(KalamDbError::InvalidOperation(
        "System tables do not support RecordBatch loading".to_string(),
    ))
}
```

## Testing Plan
After all fixes:
1. Run `cargo check` - should have 0 errors
2. Run `cargo test` - verify functionality
3. Run `cargo clippy` - check for warnings

