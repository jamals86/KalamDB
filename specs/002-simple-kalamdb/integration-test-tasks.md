# Integration Test Coverage - DML Operations Implementation

**Branch**: `002-simple-kalamdb` | **Date**: 2025-10-20 | **Priority**: P0 (Critical)
**Context**: Integration tests are written and waiting for basic CRUD operations to be implemented

## Current Status

### What's Working ✅
- DDL operations (CREATE TABLE, DROP TABLE, ALTER TABLE)
- Namespace management (CREATE/DROP NAMESPACE)
- REST API `/api/sql` endpoint
- WebSocket `/ws` endpoint infrastructure
- Table metadata storage (system.tables, system.table_schemas)
- RocksDB column families setup
- kalamdb-store backend (UserTableStore, SharedTableStore, StreamTableStore with put/get/delete methods)

### What's Missing ❌
The integration tests expect basic CRUD operations to work through the `/api/sql` endpoint:
- **INSERT INTO** (user tables, shared tables)
- **SELECT FROM** (with WHERE, ORDER BY)
- **UPDATE** (SET columns WHERE condition)
- **DELETE FROM** (soft delete with _deleted column)

### Root Cause Analysis

**The Issue**: SQL executor delegates DML to DataFusion (`execute_datafusion_query()`), but DataFusion TableProvider trait needs implementation of write operations for DML to work.

**Architecture Flow**:
```
REST API POST /api/sql
  ↓
SqlExecutor.execute(sql)
  ↓
execute_datafusion_query(sql)  [for INSERT/UPDATE/DELETE/SELECT]
  ↓
DataFusion SessionContext.sql(sql).collect()
  ↓
Needs: TableProvider with insert_into() trait method ❌ NOT IMPLEMENTED
```

**DataFusion TableProvider Interface**:
```rust
#[async_trait]
pub trait TableProvider: Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn schema(&self) -> SchemaRef;
    async fn scan(...) -> Result<Arc<dyn ExecutionPlan>>;
    
    // DML operations (optional, but required for INSERT/UPDATE/DELETE):
    async fn insert_into(...) -> Result<()> {  // ❌ Not implemented
        Err(DataFusionError::NotImplemented(...))
    }
}
```

**Current Implementation Status**:
- ✅ `UserTableProvider` - scan() implemented, DML methods missing
- ✅ `SharedTableProvider` - scan() implemented, DML methods missing
- ✅ `StreamTableProvider` - scan() implemented, DML methods missing

## Phase 18: DML Operations for Integration Tests (Priority: P0)

**Goal**: Implement INSERT/UPDATE/DELETE operations so integration tests pass

**Estimated Effort**: 8-12 tasks (2-3 days for experienced Rust developer)

### Tasks

#### A. DataFusion DML Support for Shared Tables

- [ ] **T230** [P] [IntegrationTest] Implement `insert_into()` in `SharedTableProvider`:
  - File: `backend/crates/kalamdb-core/src/tables/shared_table_provider.rs`
  - Add `#[async_trait]` method `insert_into(&self, state: &SessionState, input: Arc<dyn ExecutionPlan>) -> Result<()>`
  - Extract rows from input ExecutionPlan by calling `input.execute(0, context)?`
  - Convert Arrow RecordBatch to JSON rows
  - For each row: generate row_id (snowflake ID), inject system columns (_updated=NOW(), _deleted=false)
  - Call `self.shared_table_store.put(namespace_id, table_name, row_id, row_data)` for each row
  - Return Ok(()) on success
  - Add unit test: test_shared_table_insert_via_datafusion

- [ ] **T231** [P] [IntegrationTest] Research DataFusion UPDATE/DELETE execution:
  - DataFusion 35.0+ may not have built-in UPDATE/DELETE TableProvider methods
  - Check if DataFusion converts UPDATE to "scan + filter + modify + write" plan
  - Check if DELETE converts to "scan + filter + mark deleted"
  - Document findings in `docs/backend/DATAFUSION_DML_ARCHITECTURE.md`
  - Determine if custom LogicalPlan extensions needed

- [ ] **T232** [IntegrationTest] Implement UPDATE execution for SharedTableProvider:
  - Option 1: If DataFusion supports it, implement `update()` trait method
  - Option 2: If not, intercept LogicalPlan::Dml in executor and handle manually:
    - Parse UPDATE statement to extract SET columns and WHERE condition
    - Call `shared_table_store.scan()` to get matching rows
    - For each row: apply SET column updates, update _updated=NOW()
    - Call `shared_table_store.put()` to write updated rows
  - Add unit test: test_shared_table_update

- [ ] **T233** [IntegrationTest] Implement DELETE execution for SharedTableProvider:
  - Similar to UPDATE, determine if trait method exists or custom handling needed
  - For soft delete: set _deleted=true, _updated=NOW(), call put()
  - For hard delete (not used by integration tests): call `shared_table_store.delete(hard=true)`
  - Add unit test: test_shared_table_soft_delete

#### B. DataFusion DML Support for User Tables

- [ ] **T234** [P] [IntegrationTest] Implement `insert_into()` in `UserTableProvider`:
  - File: `backend/crates/kalamdb-core/src/tables/user_table_provider.rs`
  - Same logic as SharedTableProvider but with user_id isolation
  - Extract user_id from SessionState (stored in SessionContext variables)
  - Key format: `{user_id}:{row_id}`
  - Call `self.user_table_store.put(namespace_id, table_name, user_id, row_id, row_data)`
  - Add unit test: test_user_table_insert_via_datafusion

- [ ] **T235** [IntegrationTest] Implement UPDATE execution for UserTableProvider:
  - Same approach as SharedTableProvider but with user_id scoping
  - Only update rows where key starts with `{user_id}:`
  - Add unit test: test_user_table_update_with_isolation

- [ ] **T236** [IntegrationTest] Implement DELETE execution for UserTableProvider:
  - Soft delete only (set _deleted=true for user's rows)
  - Add unit test: test_user_table_soft_delete_with_isolation

#### C. Stream Table DML Support

- [ ] **T237** [P] [IntegrationTest] Implement `insert_into()` in `StreamTableProvider`:
  - File: `backend/crates/kalamdb-core/src/tables/stream_table_provider.rs`
  - NO system columns (_updated, _deleted) for stream tables
  - Key format: `{timestamp_ms}:{row_id}` for TTL eviction
  - Check ephemeral mode before write (if ephemeral=true and no subscribers, discard)
  - Call `self.stream_table_store.put(namespace_id, table_name, row_id, row_data)`
  - Add unit test: test_stream_table_insert

- [ ] **T238** [IntegrationTest] Disable UPDATE/DELETE for stream tables:
  - Stream tables are append-only (no UPDATE/DELETE support)
  - Return error: "UPDATE not supported on stream tables"
  - Return error: "DELETE not supported on stream tables"
  - Add unit tests: test_stream_table_update_error, test_stream_table_delete_error

#### D. Integration Test Validation

- [ ] **T239** [IntegrationTest] Run shared table integration tests:
  - Execute: `cargo test --test test_shared_tables`
  - Verify all tests pass:
    - test_shared_table_create_and_drop
    - test_shared_table_insert_and_select
    - test_shared_table_multiple_inserts
    - test_shared_table_update
    - test_shared_table_select
    - test_shared_table_delete
    - test_shared_table_system_columns
    - (and 13 more tests in the file)
  - Fix any failures, document in INTEGRATION_TEST_RESULTS.md

- [ ] **T240** [IntegrationTest] Create minimal integration test for user tables:
  - File: `backend/tests/integration/test_user_tables_basic.rs`
  - Test: CREATE USER TABLE, INSERT, SELECT, UPDATE, DELETE
  - Verify user isolation (user1 can't see user2's data)
  - Verify system columns (_updated, _deleted)
  - Execute: `cargo test --test test_user_tables_basic`

## Technical Implementation Guide

### 1. Understanding DataFusion's DML Model

**Key Research Areas**:
- Does DataFusion 35.0 have `LogicalPlan::Dml` variant?
- Check `TableProvider` trait for insert/update/delete methods
- Look at examples in DataFusion repo: `datafusion-examples/examples/insert_into.rs`
- Check if SessionState has DML configuration

**Recommended Approach**:
```rust
// Check if DataFusion has built-in DML support:
use datafusion::logical_expr::LogicalPlan;

match logical_plan {
    LogicalPlan::Dml(dml_statement) => {
        // Handle INSERT/UPDATE/DELETE
    }
    _ => // ...
}
```

### 2. Converting Arrow RecordBatch to JSON

**Helper Function** (add to `backend/crates/kalamdb-core/src/tables/arrow_to_json.rs`):
```rust
use datafusion::arrow::record_batch::RecordBatch;
use serde_json::{Map, Value as JsonValue};

pub fn record_batch_to_json_rows(batch: RecordBatch) -> Result<Vec<JsonValue>, KalamDbError> {
    let schema = batch.schema();
    let mut rows = Vec::new();
    
    for row_idx in 0..batch.num_rows() {
        let mut row_map = Map::new();
        
        for (col_idx, field) in schema.fields().iter().enumerate() {
            let column = batch.column(col_idx);
            let value = array_value_to_json(column, row_idx)?;
            row_map.insert(field.name().clone(), value);
        }
        
        rows.push(JsonValue::Object(row_map));
    }
    
    Ok(rows)
}

fn array_value_to_json(array: &ArrayRef, row_idx: usize) -> Result<JsonValue, KalamDbError> {
    // Handle different Arrow array types
    use datafusion::arrow::array::*;
    use datafusion::arrow::datatypes::DataType;
    
    match array.data_type() {
        DataType::Utf8 => {
            let string_array = array.as_any().downcast_ref::<StringArray>().unwrap();
            Ok(JsonValue::String(string_array.value(row_idx).to_string()))
        }
        DataType::Int64 => {
            let int_array = array.as_any().downcast_ref::<Int64Array>().unwrap();
            Ok(JsonValue::Number(int_array.value(row_idx).into()))
        }
        DataType::Boolean => {
            let bool_array = array.as_any().downcast_ref::<BooleanArray>().unwrap();
            Ok(JsonValue::Bool(bool_array.value(row_idx)))
        }
        // Add more types as needed
        _ => Err(KalamDbError::Other(format!("Unsupported Arrow type: {:?}", array.data_type())))
    }
}
```

### 3. SessionState User ID Extraction

**Current User Tracking**:
```rust
// In SqlExecutor.execute():
let user_id = UserId::new("user123".to_string()); // TODO: Extract from JWT/auth context

// Store in SessionState:
let session_config = self.session_context.state().config();
session_config.options_mut().extensions.insert(user_id);

// Retrieve in TableProvider:
let user_id = state.config().options().extensions.get::<UserId>()
    .ok_or_else(|| DataFusionError::Internal("User ID not set in session".to_string()))?;
```

### 4. System Column Injection

**Add to row data** (for user/shared tables only, NOT stream tables):
```rust
if let Some(obj) = row_data.as_object_mut() {
    obj.insert("_updated".to_string(), JsonValue::Number(Utc::now().timestamp_millis().into()));
    obj.insert("_deleted".to_string(), JsonValue::Bool(false));
}
```

## Success Criteria

✅ **All 20 shared table integration tests pass** without modifications to test code
✅ **User table isolation verified** (users can't access each other's data)
✅ **System columns work** (_updated, _deleted present and correct)
✅ **SELECT queries work** with WHERE, ORDER BY, filtering
✅ **UPDATE operations** modify existing rows and update _updated timestamp
✅ **DELETE operations** perform soft delete (set _deleted=true)
✅ **Stream tables** support INSERT but reject UPDATE/DELETE

## Integration Test Coverage

**File**: `backend/tests/integration/test_shared_tables.rs`

**Test Count**: 20 tests covering:
1. CREATE and DROP table lifecycle
2. INSERT single row
3. INSERT multiple rows
4. SELECT with WHERE filter
5. SELECT with ORDER BY
6. UPDATE row values
7. DELETE row (soft delete)
8. System columns (_updated, _deleted)
9. IF NOT EXISTS syntax
10. FLUSH POLICY configuration
11. Query filtering
12. Ordering
13. DROP TABLE with data
14. Multiple tables in same namespace
15. Complete table lifecycle (create → insert → query → update → delete → drop)

**Expected Message** (current state):
```
No further action required on our part. The integration tests will pass once 
the team implements basic CRUD operations (INSERT/SELECT/UPDATE/DELETE).
```

**Target Message** (after implementation):
```
✅ All 20 shared table integration tests passing
✅ DML operations (INSERT/UPDATE/DELETE) functional via REST API
✅ Ready for Phase 14 (Live Query Subscriptions)
```

## Related Files

**Implementation Files**:
- `backend/crates/kalamdb-core/src/tables/shared_table_provider.rs`
- `backend/crates/kalamdb-core/src/tables/user_table_provider.rs`
- `backend/crates/kalamdb-core/src/tables/stream_table_provider.rs`
- `backend/crates/kalamdb-core/src/sql/executor.rs` (already delegates to DataFusion)
- `backend/crates/kalamdb-store/src/shared_table_store.rs` (backend already exists)
- `backend/crates/kalamdb-store/src/user_table_store.rs` (backend already exists)

**Test Files**:
- `backend/tests/integration/test_shared_tables.rs` (20 tests)
- `backend/tests/integration/common/mod.rs` (TestServer helper)
- `backend/tests/integration/common/fixtures.rs` (test utilities)

**Documentation**:
- `docs/backend/DATAFUSION_DML_ARCHITECTURE.md` (to be created in T231)
- `INTEGRATION_TEST_RESULTS.md` (to be created in T239)

## Notes

### Why This Wasn't Done Earlier

Phase 13 focused on DDL (CREATE TABLE, DROP TABLE) and the storage backend (SharedTableStore). The assumption was that DataFusion would "just work" for DML operations once tables were registered. However:

1. **DataFusion requires TableProvider implementation** of insert/update/delete methods
2. **These methods are optional** in the trait (default to NotImplemented error)
3. **Integration tests were written ahead** expecting DML to work

### Why This Is Critical (P0)

Without DML operations:
- ❌ No data can be inserted into tables
- ❌ No queries can be executed (SELECT returns empty)
- ❌ Integration tests remain failing
- ❌ Cannot proceed to Phase 14 (Live Query Subscriptions)
- ❌ Cannot demonstrate working system to stakeholders

This is the **last blocker** before the system is functional end-to-end.

### Estimated Timeline

- **T230-T233** (Shared tables): 1 day
- **T234-T236** (User tables): 0.5 day
- **T237-T238** (Stream tables): 0.5 day
- **T239-T240** (Validation): 0.5 day
- **Buffer for fixes**: 0.5 day

**Total**: 2-3 days for experienced Rust + DataFusion developer

## Next Steps After Completion

Once all integration tests pass:
1. ✅ Mark Phase 13 as "TRULY COMPLETE" 
2. ✅ Update tasks.md with Phase 18 completion
3. → Proceed to Phase 14 (Live Query Subscriptions with Change Tracking)
4. → Add live query change detection integration
5. → Test real-time notifications via WebSocket

---

**Document Status**: PLANNING - Ready for implementation
**Last Updated**: 2025-10-20
**Author**: AI Assistant (based on integration test analysis)
