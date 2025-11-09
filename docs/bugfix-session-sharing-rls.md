# Session Sharing Bug Fix - USER Table RLS Isolation

**Date**: 2025-01-15  
**Severity**: CRITICAL  
**Status**: FIXED  

## Summary

Fixed critical RLS bug where concurrent users were getting 0 rows when querying their USER tables. Root cause was session table registration caching preventing per-user isolation.

## Problem

When using a **shared global SessionContext** (`AppContext.base_session_context`), DataFusion tables persist across HTTP requests. The original code had a "skip if exists" optimization:

```rust
// ❌ BUG: Skip if table already registered in this session
if schema_provider.table_exist(tbl) {
    continue;
}
```

### Attack Scenario

1. **User1** makes query → `register_query_tables()` creates `UserTableAccess(user1_id)` → registers in shared session
2. **User2** makes query → Session already has table registered → **skips registration**
3. **User2 uses User1's UserTableAccess** → Scan uses `user1_id` prefix → **Returns 0 rows** ❌

### Test Failure

```
✅ 1000 total rows inserted (200 per user) in 1.00s
❌ User user1_wslpJ got 0 rows, expected 200
❌ User user2_XkRtP got 0 rows, expected 200
```

INSERT succeeded (1000 rows in 1.00s), but SELECT returned 0 rows for all users.

## Root Cause

**File**: `backend/crates/kalamdb-core/src/sql/executor/mod.rs`  
**Function**: `register_query_tables()`  
**Lines**: 244-246 (original), 233-267 (fixed)

The "skip if exists" check prevented re-registration of USER tables. Since `UserTableAccess` contains the `user_id` for RLS filtering, reusing the old provider meant using the **wrong user_id**.

### Architecture Context

KalamDB uses a **shared global SessionContext** for 99% memory reduction:
- Before: 1,000 queries/sec = 1,000 SessionContext allocations = 50-200MB/sec leak
- After: 1,000 queries/sec = 1 SessionContext (shared) = <1MB total

User isolation is enforced at the **TableProvider level**:
- `UserTableAccess.user_id` (per-request wrapper around `UserTableShared`)
- `TableProvider.scan()` filters by key prefix: `{userId}:{rowId}`
- `ExecutionContext.role()` checks (admin vs user)

## Solution

**ALWAYS deregister and re-register USER tables** to ensure the correct `user_id` is used:

```rust
match table_type {
    TableType::User => {
        // CRITICAL: For USER tables, ALWAYS unregister and re-register to ensure correct user_id
        // When using a shared global session (base_session_context), tables persist across requests.
        // If we skip re-registration, User2 would use User1's UserTableAccess → wrong user_id → 0 rows.
        // Solution: Unconditionally unregister and create fresh UserTableAccess with current exec_ctx.user_id
        if schema_provider.table_exist(tbl) {
            schema_provider
                .deregister_table(tbl)
                .map_err(|e| KalamDbError::Other(format!("Failed to deregister user table {}.{}: {}", ns, tbl, e)))?;
        }
        
        // Create per-request UserTableAccess wrapper with CURRENT user_id
        let shared = schema_registry.get_user_table_shared(&table_id).ok_or_else(|| {
            KalamDbError::InvalidOperation(format!(
                "User table provider not found for: {}.{}",
                ns, tbl
            ))
        })?;
        let access = UserTableAccess::new(
            shared,
            exec_ctx.user_id.clone(),  // ✅ Uses CURRENT user from request
            exec_ctx.user_role.clone(),
        );
        schema_provider
            .register_table(tbl.to_string(), Arc::new(access))
            .map_err(|e| KalamDbError::Other(format!("Failed to register user table {}.{}: {}", ns, tbl, e)))?;
    }
    TableType::Shared | TableType::Stream => {
        // Skip if table already registered (SHARED/STREAM tables are user-agnostic)
        if schema_provider.table_exist(tbl) {
            continue;
        }
        // ... register from cache
    }
}
```

### Key Changes

1. **USER tables**: Unconditionally deregister → create fresh `UserTableAccess` with `exec_ctx.user_id`
2. **SHARED/STREAM tables**: Keep "skip if exists" optimization (no user_id dependency)
3. **Updated AppContext.session() docs**: Added "USER Table Registration" section explaining the fix

## Test Results

```
🎉 Test PASSED - All 5 users correctly isolated
   Total time: 10.90s
   Breakdown:
     ├─ Setup: 10.90s (100.0%)
     ├─ User creation: 10.43s (95.6%)
     ├─ Inserts: 1.05s (9.6%)
     └─ Verification: 4.73s (43.4%)
```

**Performance**:
- INSERT: 1000 rows in 1.05s (944.5 inserts/sec)
- SELECT: 5 users verified in 4.73s (avg 926ms per user)
- CLI overhead: 18.2ms (2.0% of total time)
- **Each user gets their correct 200 rows** ✅

## Files Modified

1. **backend/crates/kalamdb-core/src/sql/executor/mod.rs**:
   - Lines 233-267: Added deregister logic for USER tables
   - Lines 268-282: Added "skip if exists" for SHARED/STREAM tables

2. **backend/crates/kalamdb-core/src/app_context.rs**:
   - Lines 330-355: Updated `session()` documentation with "USER Table Registration" section

## Prevention

**Code Review Checklist**:
- [ ] When using shared SessionContext, verify table registration happens per-request
- [ ] For USER tables, ensure `UserTableAccess` is created fresh with current `exec_ctx.user_id`
- [ ] For SHARED/STREAM tables, caching is safe (no user_id dependency)
- [ ] Test with concurrent users (5+ users, 200+ rows each)

**Test Coverage**:
- `cli/tests/test_concurrent_users.rs`: 5 users × 200 rows = 1000 rows (verifies RLS isolation)

## Lessons Learned

1. **Session sharing is safe** when tables are **stateless** (SHARED/STREAM)
2. **Session sharing is dangerous** when tables are **stateful** (USER tables with per-user isolation)
3. **"Skip if exists" optimizations** can break correctness when state changes per-request
4. **Always deregister before re-registering** when provider state depends on request context

## Related

- **Performance optimization**: Batch INSERT (38× faster - see test output)
- **CliTiming infrastructure**: Server vs CLI timing breakdown (1.8-2.0% overhead)
- **RLS debugging**: Added `log::info!("🔍 RLS SCAN: ...")` in `user_table_provider.rs`
