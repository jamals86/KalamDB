# System Tables Scan Implementation - Complete ✅

## Issue Summary

The `/api/sql` endpoint was returning `NotImplemented` errors when querying system tables (storage_locations, jobs, live_queries), despite integration tests passing. This revealed a **critical discrepancy** between the test execution path and the REST API execution path.

### Root Cause

**Integration tests were NOT testing the same code path as the REST API!**

- **Integration Tests Path**: `execute_sql()` → `sql_executor.execute()` → Direct execution (no DataFusion fallback)
- **REST API Path**: `/api/sql` → `sql_executor.execute()` → On `InvalidSql` error → **DataFusion fallback** → `TableProvider.scan()`

The integration tests were passing because they bypassed DataFusion entirely, while the REST API fell back to DataFusion for SELECT queries, which requires fully implemented `TableProvider.scan()` methods.

## Fixes Applied

### 1. Implemented Missing `scan()` Methods

Fixed three system table providers that had `NotImplemented` errors:

#### `storage_locations_provider.rs`
```rust
// Added import
use datafusion::physical_plan::memory::MemoryExec;

// Implemented scan() method
async fn scan(...) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
    let batch = self.scan_all_locations().map_err(|e| {
        DataFusionError::Execution(format!("Failed to scan storage locations: {}", e))
    })?;
    
    let partitions = vec![vec![batch]];
    let exec = MemoryExec::try_new(&partitions, self.schema.clone(), projection.cloned())?;
    Ok(Arc::new(exec))
}
```

#### `jobs_provider.rs`
```rust
// Added import
use datafusion::physical_plan::memory::MemoryExec;

// Implemented scan() method
async fn scan(...) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
    let batch = self.scan_all_jobs().map_err(|e| {
        DataFusionError::Execution(format!("Failed to scan jobs: {}", e))
    })?;
    
    let partitions = vec![vec![batch]];
    let exec = MemoryExec::try_new(&partitions, self.schema.clone(), projection.cloned())?;
    Ok(Arc::new(exec))
}
```

#### `live_queries_provider.rs`
```rust
// Added import
use datafusion::physical_plan::memory::MemoryExec;

// Implemented scan() method
async fn scan(...) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
    let batch = self.scan_all_live_queries().map_err(|e| {
        DataFusionError::Execution(format!("Failed to scan live queries: {}", e))
    })?;
    
    let partitions = vec![vec![batch]];
    let exec = MemoryExec::try_new(&partitions, self.schema.clone(), projection.cloned())?;
    Ok(Arc::new(exec))
}
```

### 2. Fixed Integration Tests to Match REST API Behavior

Updated `backend/tests/integration/common/mod.rs`:

#### Added `session_factory` to TestServer
```rust
pub struct TestServer {
    temp_dir: TempDir,
    pub kalam_sql: Arc<kalamdb_sql::KalamSql>,
    pub sql_executor: Arc<SqlExecutor>,
    pub namespace_service: Arc<NamespaceService>,
    pub session_factory: Arc<DataFusionSessionFactory>,  // ← ADDED
}
```

#### Updated `execute_sql_with_user()` to Fall Back to DataFusion
```rust
pub async fn execute_sql_with_user(&self, sql: &str, user_id: Option<&str>) -> SqlResponse {
    // Try custom DDL/DML execution first (same as REST API)
    match self.sql_executor.execute(sql, user_id_obj.as_ref()).await {
        Ok(result) => { /* ... return result ... */ }
        
        // NEW: Fall back to DataFusion when InvalidSql (SAME AS REST API!)
        Err(kalamdb_core::error::KalamDbError::InvalidSql(_)) => {
            match self.session_factory.create_session().sql(sql).await {
                Ok(df) => match df.collect().await {
                    Ok(batches) => { /* ... convert to SqlResponse ... */ }
                    Err(e) => { /* ... return error ... */ }
                }
                Err(e) => { /* ... return parse error ... */ }
            }
        }
        
        Err(e) => { /* ... return execution error ... */ }
    }
}
```

**This ensures integration tests now use the EXACT same code path as the REST API!**

## System Tables Status

All 7 system table providers now have fully implemented `scan()` methods:

| Table | Provider | scan() Status | Notes |
|-------|----------|---------------|-------|
| `system.users` | `users_provider.rs` | ✅ Already implemented | Reference implementation |
| `system.namespaces` | `namespaces_provider.rs` | ✅ Already implemented | - |
| `system.tables` | `system_tables_provider.rs` | ✅ Already implemented | - |
| `system.storage_locations` | `storage_locations_provider.rs` | ✅ **FIXED** | Was returning NotImplemented |
| `system.jobs` | `jobs_provider.rs` | ✅ **FIXED** | Was returning NotImplemented |
| `system.live_queries` | `live_queries_provider.rs` | ✅ **FIXED** | Was returning NotImplemented |
| `system.table_schemas` | N/A | ✅ Virtual table | No provider needed |

## Verification

### Integration Tests
```bash
cargo test --test test_system_tables -- --test-threads=1
```
**Result**: ✅ **35/35 tests passed**

All tests now execute through DataFusion fallback when needed, matching REST API behavior.

### REST API Queries
All system tables can now be queried via `/api/sql`:

```bash
POST /api/sql
{
  "sql": "SELECT * FROM system.storage_locations"
}
```
**Result**: ✅ **Success** (previously returned NotImplemented error)

```bash
POST /api/sql
{
  "sql": "SELECT * FROM system.jobs"
}
```
**Result**: ✅ **Success** (previously returned NotImplemented error)

```bash
POST /api/sql
{
  "sql": "SELECT * FROM system.live_queries"
}
```
**Result**: ✅ **Success** (previously returned NotImplemented error)

## Architecture Improvement

### Before
```
Integration Tests:
  execute_sql() → sql_executor.execute() → DONE
  ❌ Never exercised DataFusion fallback path
  ❌ Tests passed but API failed

REST API:
  /api/sql → sql_executor.execute() → InvalidSql error
          → DataFusion fallback → TableProvider.scan()
          → NotImplemented error ❌
```

### After
```
Integration Tests:
  execute_sql() → sql_executor.execute() → InvalidSql error
              → DataFusion fallback → TableProvider.scan()
              → MemoryExec → ✅ Success

REST API:
  /api/sql → sql_executor.execute() → InvalidSql error
         → DataFusion fallback → TableProvider.scan()
         → MemoryExec → ✅ Success

✅ Both use IDENTICAL code paths!
```

## Key Takeaways

1. **Integration tests must mimic real execution paths** - Tests that bypass critical code paths give false confidence
2. **All TableProvider implementations must have working scan()** - DataFusion requires this for SELECT queries
3. **Helper methods (scan_all_*) should be leveraged** - All three providers already had helper methods, just needed to be called from scan()
4. **Consistent error handling** - Use the same pattern across all providers (MemoryExec with proper error mapping)

## Files Modified

1. `backend/crates/kalamdb-core/src/tables/system/storage_locations_provider.rs`
   - Added MemoryExec import
   - Implemented scan() method

2. `backend/crates/kalamdb-core/src/tables/system/jobs_provider.rs`
   - Added MemoryExec import
   - Implemented scan() method

3. `backend/crates/kalamdb-core/src/tables/system/live_queries_provider.rs`
   - Added MemoryExec import
   - Implemented scan() method

4. `backend/tests/integration/common/mod.rs`
   - Added session_factory field to TestServer struct
   - Updated execute_sql_with_user() to fall back to DataFusion on InvalidSql

## Conclusion

✅ All system tables now fully functional via REST API
✅ Integration tests now accurately test real-world execution paths
✅ No more false positives from integration tests
✅ All 35 integration tests passing with correct behavior

---

**Date**: 2025-10-21
**Status**: COMPLETE ✅
