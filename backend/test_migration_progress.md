# Test Migration Progress - Phase 10 AppContext Integration

## Summary
Migrating backend tests from old TestServer (with individual services) to new AppContext-based pattern.

## Completed ✅

1. **TestServer struct** - Updated to use `Arc<AppContext>` instead of individual services
   - Removed: `kalam_sql`, `namespace_service`, `session_factory`, `live_query_manager`  
   - Added: `app_context: Arc<AppContext>`

2. **TestServer::new()** - Rewritten to use `AppContext::init(backend, node_id, storage_path)`
   - Bootstrap system user via `app_context.system_tables().users()`
   - Bootstrap local storage via `app_context.system_tables().storages()`
   - SqlExecutor now takes `(app_context, enforce_password_complexity)`

3. **auth_helper.rs** - Fixed SqlExecutor.execute() calls
   - Changed from `execute(session, sql, Some(&user_id))` 
   - To: `execute(session, sql, &ExecutionContext)`
   - Fixed import: `kalamdb_commons::types::User` (not `system::User`)

4. **flush_helpers.rs** - Removed all kalam_sql references
   - Use `server.app_context.schema_registry().get_table_definition()`
   - Use `server.app_context.user_table_store()` and `.shared_table_store()`
   - Fixed catalog imports: `kalamdb_commons::models::{NamespaceId, TableName, TableId}`

5. **common/mod.rs helper methods**
   - `cleanup()`, `cleanup_namespace()`, `namespace_exists()`, `table_exists()`
   - All now use `app_context.system_tables().namespaces()` and `.tables()`

## Remaining Issues 🚧

### High Priority

1. **TableId duplicate imports** (4 files)
   - Error: `the name 'TableId' is defined multiple times`
   - Need to remove redundant imports in flush_helpers.rs

2. **SystemTableProvider methods missing** (3 errors)
   - `UsersTableProvider::insert_user()` - method not found
   - `StoragesTableProvider::get()` - method not found  
   - `StoragesTableProvider::insert()` - method not found
   - **Fix**: These providers need CRUD methods added

3. **kalamdb_core::services module removed** (2 files)
   - test_password_complexity.rs
   - test_flush_job_persistence.rs
   - **Fix**: Replace with AppContext pattern

4. **kalamdb_core::catalog module removed** (multiple files)
   - Use `kalamdb_commons::models::{NamespaceId, TableName, TableId, UserId}`
   - Use `kalamdb_core::schema_registry::SchemaRegistry`

5. **Job-related imports broken** (test_user_cleanup.rs)
   - `kalamdb_core::jobs::UserCleanupJob` - needs update to UnifiedJobManager
   - `JobExecutor`, `JobResult` - deprecated interfaces

6. **test_column_ordering.rs**
   - `register_system_tables()` now returns `SystemTablesRegistry`, not tuple
   - Needs destructuring fix

### Medium Priority

7. **KalamDataType import path** 
   - `kalamdb_commons::models::types::KalamDataType` doesn't exist
   - Should be `kalamdb_commons::models::datatypes::KalamDataType`

8. **test_system_users.rs, test_user_cleanup.rs**
   - Multiple storage/catalog related errors
   - Need AppContext migration

## Next Steps

1. Fix duplicate TableId imports in flush_helpers.rs
2. Add CRUD methods to SystemTableProviders (insert_user, get, insert)
3. Fix test_password_complexity.rs and test_flush_job_persistence.rs (remove services imports)
4. Update test_column_ordering.rs for new register_system_tables API
5. Fix test_user_cleanup.rs job-related imports
6. Fix remaining catalog import references

## Files Modified So Far

- `/Users/jamal/git/KalamDB/backend/tests/integration/common/mod.rs` ✅
- `/Users/jamal/git/KalamDB/backend/tests/integration/common/auth_helper.rs` ✅  
- `/Users/jamal/git/KalamDB/backend/tests/integration/common/flush_helpers.rs` ✅

## Test Compilation Status

```bash
# Current status
cargo test --no-run 2>&1 | grep -E "^error\[" | wc -l
# ~40 unique error types remaining (down from 100+)
```

## Architecture Notes

**Old Pattern (Phase 9)**:
```rust
let server = TestServer {
    kalam_sql: Arc<KalamSql>,
    namespace_service: Arc<NamespaceService>,
    user_table_service: Arc<UserTableService>,
    // ... 8 more fields
};
```

**New Pattern (Phase 10)**:
```rust
let app_context = AppContext::init(backend, node_id, storage_path);
let server = TestServer {
    app_context: Arc<AppContext>, // Single source of truth!
    session_context: Arc<SessionContext>,
    sql_executor: Arc<SqlExecutor>,
};

// Access everything via app_context
app_context.system_tables().users().insert_user(&user);
app_context.schema_registry().get_table_definition(&table_id);
```

**ExecutionContext Pattern**:
```rust
// Old
executor.execute(session, sql, Some(&user_id)).await

// New  
let exec_ctx = ExecutionContext::new(user_id, role);
executor.execute(session, sql, &exec_ctx).await
```
