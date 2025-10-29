# Comprehensive Compilation Fix Plan

## Summary of 58 Errors + 40 Warnings

### Error Categories:
1. **Arc wrapper issues (20)** - Traits implemented for `SystemTableStore<K,V>` but code has `Arc<SystemTableStore<K,V>>`
2. **Ambiguous method calls (8)** - Multiple traits provide same method  
3. **UserTableRow.as_object() (3)** - Need `.fields.as_object()`
4. **[u8] Sized errors (6)** - Iterator type mismatches
5. **Missing trait imports (4)** - SharedTableStoreExt not in scope
6. **Type mismatches (3)** - Return types, UserTableRow vs JsonValue
7. **Missing methods (2)** - cleanup_expired_rows, create_column_family for StreamTableStore
8. **Iterator issues (2)** - scan() returns Vec not Iterator
9. **Other (10)** - Various type and signature issues

### Fix Strategy:
All user table insert/update/delete operations incorrectly try to call SharedTableStoreExt methods.
The correct pattern is UserTableStoreExt for user tables.

The change_detector should convert UserTableRow to/from JsonValue for compatibility.

All extension trait calls on Arc must use `.as_ref()` to dereference.

## Files to Fix (in order):

1. `stores/system_table.rs` - Fix load_batch signature, [u8] iterator issues, StreamTableRow impl
2. `live_query/change_detector.rs` - Fix all arc.as_ref(), convert UserTableRow<->JsonValue
3. `tables/user_tables/user_table_insert.rs` - Use UserTableStoreExt instead of SharedTableStoreExt
4. `tables/user_tables/user_table_update.rs` - Use UserTableStoreExt instead of SharedTableStoreExt  
5. `tables/user_tables/user_table_delete.rs` - Use UserTableStoreExt instead of SharedTableStoreExt
6. `tables/user_tables/user_table_provider.rs` - Fix .as_object() to .fields.as_object(), ._deleted
7. `sql/executor.rs` - Fix .as_object() calls, String types for row_id
8. `tables/stream_tables/stream_table_provider.rs` - Add SharedTableStoreExt import, fix scan calls
9. `services/stream_table_service.rs` - Add SharedTableStoreExt import
10. `jobs/stream_eviction.rs` - Add SharedTableStoreExt import
11. `services/table_deletion_service.rs` - Disambiguate drop_table call

