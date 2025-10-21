# Integration Test Implementation Plan - Summary

**Date**: October 20, 2025
**Status**: Tasks planned and added to tasks.md
**Priority**: P0 (Critical - blocks all further development)

## Quick Summary

The integration tests in `backend/tests/integration/test_shared_tables.rs` are written and waiting for basic CRUD operations (INSERT/SELECT/UPDATE/DELETE) to be implemented. This is the **last blocker** before the system is functional end-to-end.

## What Was Done

✅ **Created comprehensive task breakdown**:
- 12 new tasks (T230-T241) added to `specs/002-simple-kalamdb/tasks.md`
- Detailed implementation plan in `specs/002-simple-kalamdb/integration-test-tasks.md`

✅ **Root cause identified**:
- SqlExecutor correctly delegates DML to DataFusion
- TableProvider implementations missing `insert_into()` and related DML methods
- DataFusion needs these trait methods implemented to execute INSERT/UPDATE/DELETE

✅ **Architecture clarified**:
```
REST API /api/sql → SqlExecutor → DataFusion → TableProvider (needs DML methods)
                                                     ↓
                                            kalamdb-store (already has put/get/delete)
```

## Implementation Tasks Overview

### Phase 18: DML Operations (12 tasks, 2-3 days)

**A. Shared Tables (4 tasks)**:
- T230: Implement `insert_into()` in SharedTableProvider
- T231: Research DataFusion UPDATE/DELETE support
- T232: Implement UPDATE execution
- T233: Implement DELETE execution (soft delete)

**B. User Tables (3 tasks)**:
- T234: Implement `insert_into()` with user_id isolation
- T235: Implement UPDATE with isolation
- T236: Implement DELETE with isolation

**C. Stream Tables (2 tasks)**:
- T237: Implement `insert_into()` (no system columns, timestamp keys)
- T238: Disable UPDATE/DELETE (stream tables are append-only)

**D. Utilities (1 task)**:
- T239: Create Arrow RecordBatch to JSON converter

**E. Validation (2 tasks)**:
- T240: Run and fix shared table integration tests (20 tests)
- T241: Create user table integration tests

## Critical Path

```
T230 + T234 + T237 (insert_into implementations) → Can test INSERT operations
    ↓
T231 (research DataFusion UPDATE/DELETE) → Understand approach
    ↓
T232 + T233 + T235 + T236 (UPDATE/DELETE) → Can test all CRUD
    ↓
T239 (Arrow to JSON utility) → Simplify conversions
    ↓
T240 + T241 (Integration tests) → Verify everything works
```

## Files to Modify

**Core Implementation**:
1. `backend/crates/kalamdb-core/src/tables/shared_table_provider.rs` - Add DML methods
2. `backend/crates/kalamdb-core/src/tables/user_table_provider.rs` - Add DML methods with isolation
3. `backend/crates/kalamdb-core/src/tables/stream_table_provider.rs` - Add INSERT, reject UPDATE/DELETE
4. `backend/crates/kalamdb-core/src/tables/arrow_to_json.rs` - NEW: Conversion utility

**Already Correct** (no changes needed):
- `backend/crates/kalamdb-core/src/sql/executor.rs` - Already delegates to DataFusion ✅
- `backend/crates/kalamdb-store/src/*.rs` - Backend methods already exist ✅
- `backend/tests/integration/test_shared_tables.rs` - Tests already written ✅

## Success Criteria

When implementation is complete:

✅ **20 shared table integration tests pass**
✅ **User table isolation works** (user1 can't see user2's data)
✅ **System columns present** (_updated, _deleted auto-injected)
✅ **SELECT queries work** (WHERE, ORDER BY, filtering)
✅ **UPDATE modifies rows** (updates _updated timestamp)
✅ **DELETE performs soft delete** (sets _deleted=true)
✅ **Stream tables** support INSERT, reject UPDATE/DELETE

Expected output:
```bash
cargo test --test test_shared_tables
# Result: 20/20 tests passing ✅
```

## What This Unlocks

After Phase 18 completion:
- ✅ Full CRUD operations via REST API
- ✅ Can proceed to Phase 14 (Live Query Subscriptions)
- ✅ Can test change tracking with real data
- ✅ Can demonstrate working system to stakeholders
- ✅ Integration tests validate end-to-end functionality

## Technical Notes

### DataFusion TableProvider Trait

Missing methods to implement:
```rust
#[async_trait]
pub trait TableProvider {
    // Already implemented:
    fn schema(&self) -> SchemaRef;
    async fn scan(...) -> Result<Arc<dyn ExecutionPlan>>;
    
    // NEED TO IMPLEMENT:
    async fn insert_into(
        &self,
        state: &SessionState,
        input: Arc<dyn ExecutionPlan>
    ) -> Result<()>;
}
```

### Arrow to JSON Conversion

Key challenge: Converting DataFusion's Arrow RecordBatch to JSON for storage:
```rust
// RecordBatch (Arrow columnar format)
//     ↓
// JSON rows (for kalamdb-store)
//     ↓
// RocksDB storage
```

Need to handle common types:
- Utf8 → String
- Int64/Int32 → Number
- Boolean → Bool
- Float64/Float32 → Number
- Timestamp → Number (milliseconds)

### User Isolation

For user tables, SessionContext must store current user_id:
```rust
let user_id = extract_from_jwt(request);
session_context.register_variable(Arc::new(user_id));

// Later in TableProvider:
let user_id = state.config().extensions.get::<UserId>()?;
```

## References

**Detailed Plan**: `/specs/002-simple-kalamdb/integration-test-tasks.md`
**Task List**: `/specs/002-simple-kalamdb/tasks.md` (Phase 18, T230-T241)
**Integration Tests**: `/backend/tests/integration/test_shared_tables.rs`
**Architecture**: `/specs/002-simple-kalamdb/spec.md` (Data Flow Architecture section)

## Next Steps

1. **Developer reviews** integration-test-tasks.md for implementation details
2. **Implement T230-T241** following the technical guide
3. **Run integration tests** after each group (A, B, C)
4. **Validate all 20 tests pass** before marking Phase 18 complete
5. **Proceed to Phase 14** (Live Query Subscriptions)

---

**Document Created By**: AI Assistant (GitHub Copilot)
**Based On**: Integration test analysis + spec.md + tasks.md review
**Purpose**: Provide clear implementation path to unblock development
