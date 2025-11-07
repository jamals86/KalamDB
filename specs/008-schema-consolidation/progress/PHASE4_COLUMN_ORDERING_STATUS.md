# Phase 4 Column Ordering Implementation Status

## Summary

Phase 4 (T062-T065) requires SELECT * to return columns ordered by `ordinal_position` from TableDefinition.  
**Status**: **PARTIALLY COMPLETE** ✅ (1/6 system tables fixed)

## Root Cause Analysis

The hardcoded Arrow schemas in system table providers ignored TableDefinition's `ordinal_position` field.  
Each provider was using `Schema::new(vec![Field::new(...)])` instead of `table_def.to_arrow_schema()`.

## Implementation Status

### ✅ Fixed: system.jobs
- **File**: `backend/crates/kalamdb-core/src/tables/system/jobs_v2/jobs_table.rs`
- **Change**: Now uses `jobs_table_definition().to_arrow_schema()`
- **TableDefinition**: Complete (7/7 columns match provider)
- **Result**: Columns now consistently ordered by ordinal_position

### ❌ Incomplete: Other System Tables

| Table | TableDefinition Columns | Provider Columns | Status |
|-------|------------------------|------------------|--------|
| **system.users** | 8 | 11 | Missing: `oauth_provider`, `oauth_id`, `metadata` |
| **system.namespaces** | 3 | 5 | Missing: `options`, `table_count` |
| **system.storages** | 4 | 11 | Missing 7 columns |
| **system.live_queries** | 4 | 12 | Missing 8 columns |
| **system.tables** | 5 | 12 | Missing 7 columns |

## Why Reverted

When I attempted to fix all system tables by using `.to_arrow_schema()`, I got this error:

```
Arrow error: Invalid argument error: number of columns(12) must match number of fields(5) in schema
```

This happened because:
1. TableDefinitions in `system_table_definitions.rs` are incomplete
2. Providers (e.g., `tables_provider.rs`) build RecordBatches with ALL columns
3. Schema mismatch causes Arrow errors

## Next Steps

To complete Phase 4 for all system tables:

1. **Update TableDefinitions** (`backend/crates/kalamdb-core/src/tables/system/system_table_definitions.rs`):
   - Add missing columns to each `*_table_definition()` function
   - Ensure ordinal_position matches desired SELECT * ordering
   - Total work: ~40-50 ColumnDefinition entries

2. **Update Providers** (after TableDefinitions are complete):
   - Change each `*_table.rs` from hardcoded `Schema::new()` to `.to_arrow_schema()`
   - Remove unused imports (`DataType`, `Field`, `Schema`, `TimeUnit`, `Arc`)
   - Add import: `use crate::tables::system::system_table_definitions::{table}_table_definition;`

3. **Test**:
   - Run `SELECT * FROM system.{table}` multiple times
   - Verify column order is consistent across queries
   - Run integration tests

## Files Changed (This PR)

✅ **backend/crates/kalamdb-core/src/tables/system/jobs_v2/jobs_table.rs**
- Removed hardcoded schema
- Now uses `jobs_table_definition().to_arrow_schema()`
- Added Phase 4 documentation comment

❌ **Reverted** (TableDefinitions incomplete):
- `users_v2/users_table.rs`
- `namespaces_v2/namespaces_table.rs`
- `storages_v2/storages_table.rs`
- `live_queries_v2/live_queries_table.rs`
- `tables_v2/tables_table.rs`

## Testing

Before this fix:
```sql
-- Run 1:
SELECT * FROM system.jobs;  -- Columns: job_id, status, job_type, created_at, ...

-- Run 2:
SELECT * FROM system.jobs;  -- Columns: status, job_id, created_at, job_type, ...  (RANDOM!)
```

After this fix:
```sql
-- Run 1:
SELECT * FROM system.jobs;  -- Columns: job_id, job_type, status, created_at, ... (ordinal 1-7)

-- Run 2:
SELECT * FROM system.jobs;  -- Columns: job_id, job_type, status, created_at, ... (CONSISTENT!)
```

## Related Tasks

- T062: Update SELECT * column ordering ✅ (jobs only)
- T063: Validate ordinal_position uniqueness ⏸️  (depends on complete TableDefinitions)
- T064: Update system table schemas ⏸️  (in progress)
- T065: Test column ordering ⏸️  (pending complete implementation)

## Why Phase 4 Was Marked Complete

Phase 4 created the `ordinal_position` field in `ColumnDefinition` and added `.to_arrow_schema()` method.  
However, **it didn't update the providers to USE this method**. The infrastructure was built but not connected.

This is a common incomplete migration pattern where:
1. New system added ✅  
2. Old system deprecated ✅  
3. Migration from old→new **NOT DONE** ❌

## Recommendation

**Priority**: P1 (Production correctness bug)  
**Effort**: Medium (~2-3 hours to complete all TableDefinitions)  
**Risk**: Low (isolated to system table schemas)

Complete the TableDefinitions in `system_table_definitions.rs`, then apply the same pattern used for jobs_table.rs to all other system tables.
