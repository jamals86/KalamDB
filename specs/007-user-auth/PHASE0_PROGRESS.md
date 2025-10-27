# Phase 0: System Model Consolidation - Progress Report

**Date**: 2025-01-27  
**Status**: 🚧 IN PROGRESS (26.3% complete - 5/19 tasks)  
**Branch**: 007-user-auth  
**Priority**: P0 (BLOCKING - prerequisite for all authentication work)

## Executive Summary

Phase 0 aims to consolidate all system table models into `kalamdb-commons/src/models/system.rs` as the single source of truth, eliminating duplicate definitions and type mismatches across the codebase.

**Current State**:
- ✅ Canonical models exist in commons
- ✅ kalamdb-sql cleaned up and re-exports from commons
- ✅ users_provider.rs updated to use canonical User model
- ✅ All `kalamdb_sql::models::*` imports replaced
- ❌ 154 compilation errors remain (down from 159)
- ❌ Job/Namespace field name mismatches discovered
- ❌ Type conversion issues with TableName/NamespaceId/StorageId

## Completed Tasks (5/19)

### ✅ T001: Verify Canonical Models in Commons
**Status**: COMPLETED  
**Files**: `backend/crates/kalamdb-commons/src/models/system.rs`  
**Result**: All 9 system models present with correct field types:
- User (with UserId, Role enum, AuthType enum, password_hash, api_key)
- Job (with JobType/JobStatus enums, started_at/completed_at timestamps)
- LiveQuery, Namespace, SystemTable, Storage, TableSchema, InformationSchemaTable, UserTableCounter

### ✅ T002: Verify kalamdb-sql Cleanup
**Status**: COMPLETED  
**Files**: 
- `backend/crates/kalamdb-sql/src/models.rs` - DELETED ✓
- `backend/crates/kalamdb-sql/src/lib.rs` - Re-exports added ✓

**Result**: kalamdb-sql now re-exports `SystemTable as Table` and all other models from commons (line 43-47 in lib.rs)

### ✅ T003: Verify Catalog Models Documentation
**Status**: COMPLETED  
**Files**: `docs/architecture/CATALOG_VS_SYSTEM_MODELS.md`  
**Result**: Comprehensive guide created explaining:
- Catalog models (runtime entities with business logic) ≠ System models (persistence DTOs)
- Storage model already in commons (no duplication)
- Clear separation of concerns

### ✅ T004-T005: Fix users_provider.rs
**Status**: COMPLETED  
**Files**: 
- `backend/crates/kalamdb-core/src/tables/system/users_provider.rs`
- `backend/crates/kalamdb-core/src/tables/system/users.rs`

**Changes**:
1. Removed `storage_mode` and `storage_id` field access (non-existent in canonical User)
2. Converted `role: "user".to_string()` → `role: Role::User`
3. Updated schema to include: password_hash, role, auth_type, auth_data, api_key, last_seen, deleted_at
4. Fixed RecordBatch construction to append 12 columns (not 7)
5. Added imports: `use kalamdb_commons::{AuthType, Role, UserId};`

### ✅ T010: Replace kalamdb_sql::models Imports
**Status**: COMPLETED  
**Files Modified** (via sed):
- `backend/crates/kalamdb-core/src/sql/executor.rs`
- `backend/crates/kalamdb-core/src/services/restore_service.rs`
- `backend/crates/kalamdb-core/src/services/stream_table_service.rs`
- `backend/crates/kalamdb-core/src/services/shared_table_service.rs`
- `backend/crates/kalamdb-core/src/services/user_table_service.rs`
- `backend/src/commands/create_user.rs`
- `backend/src/lifecycle.rs`

**Pattern**: `kalamdb_sql::models::*` → `kalamdb_sql::*` (which re-exports from commons)

## In-Progress Tasks (0/19)

None currently active.

## Blocked/Pending Tasks (14/19)

### 🚧 T006-T009: Migrate live_queries_provider.rs
**Status**: PENDING  
**Blocker**: Should complete after Job fixes to ensure consistent pattern  
**Scope**: Remove LiveQueryRecord struct, use kalamdb_commons::system::LiveQuery

### 🚧 T011-T012: Verification Tasks
**Status**: BLOCKED  
**Blocker**: 154 compilation errors prevent verification  
**Dependencies**: T020-T026 must complete first

### 🚧 T013-T014: Build & Test Validation
**Status**: BLOCKED  
**Current State**: `cargo build` fails with 154 errors  
**Dependencies**: All fixes below must complete

## Newly Discovered Work (T020-T026)

During T004-T010 implementation, significant field name mismatches were discovered in Job and Namespace usage:

### T020-T023: Job Struct Field Migrations

**Issue**: Code uses old field names that don't exist in canonical Job model

| Old Field      | New Field       | Occurrences | Impact |
|----------------|-----------------|-------------|--------|
| `start_time`   | `started_at`    | ~8          | restore_service.rs, backup_service.rs, table_deletion_service.rs |
| `end_time`     | `completed_at`  | ~8          | Same files |
| `job_type: "restore"` | `job_type: JobType::Restore` | ~10 | Type safety violation |
| `status: "running"` | `status: JobStatus::Running` | ~10 | Type safety violation |

**Example Fix Needed**:
```rust
// CURRENT (BROKEN):
let job = Job {
    job_type: "restore".to_string(),    // ❌ Should be JobType enum
    status: "running".to_string(),       // ❌ Should be JobStatus enum
    start_time: now_ms,                  // ❌ Field doesn't exist
    end_time: None,                      // ❌ Field doesn't exist
    // ...
};

// REQUIRED (FIX):
let job = Job {
    job_type: JobType::Restore,          // ✅ Use enum
    status: JobStatus::Running,          // ✅ Use enum
    started_at: Some(now_ms),            // ✅ Correct field name
    completed_at: None,                  // ✅ Correct field name
    // ...
};
```

### T024: Namespace Struct Field Migration

**Issue**: Missing `owner_id` field in Namespace instantiation  
**Occurrences**: ~3 in sql/executor.rs  
**Fix**: Add `owner_id: UserId::new("system")` or similar

### T025: Type Conversion Issues

**Issue**: 98 type mismatch errors for TableName/NamespaceId/StorageId  
**Root Cause**: Code passes `&TableName` where `String` expected or vice versa  
**Solution Pattern**:
- `TableName::new(str)` to create from string
- `table_name.as_str()` to get &str reference
- `table_name.to_string()` to get owned String

### T026: User Schema Update

**Status**: COMPLETED in T004-T005  
**Note**: This task can be marked complete

## Error Summary

**Total Errors**: 154 (down from 159 initial)  
**Error Distribution**:
- 98 × Type mismatches (TableName/NamespaceId/StorageId conversions)
- 9 × Job.end_time field missing
- 5 × Job struct instantiation errors
- 4 × Job.start_time field missing
- 3 × Namespace.owner_id missing
- 35 × Various other type/field errors

**Critical Files Affected**:
1. `backend/crates/kalamdb-core/src/services/restore_service.rs` (Job usage)
2. `backend/crates/kalamdb-core/src/services/backup_service.rs` (Job usage)
3. `backend/crates/kalamdb-core/src/services/table_deletion_service.rs` (Job usage)
4. `backend/crates/kalamdb-core/src/sql/executor.rs` (Namespace, type conversions)
5. `backend/crates/kalamdb-core/src/tables/system/storages_provider.rs` (fixed - lifetime issue)

## Next Steps

### Immediate Priority (T020-T023)

1. **Fix Job field names in restore_service.rs**:
   - Lines 550-562: Update job creation
   - Replace start_time → started_at
   - Replace end_time → completed_at
   - Convert job_type string → JobType enum
   - Convert status string → JobStatus enum

2. **Fix Job field names in backup_service.rs**:
   - Line 464: Update job creation
   - Same pattern as restore_service.rs

3. **Fix Job field names in table_deletion_service.rs**:
   - Lines 444, 474: Update job creation and duration calculation
   - Same pattern as above

4. **Add JobType and JobStatus enum imports**:
   ```rust
   use kalamdb_commons::system::{Job, JobType, JobStatus};
   ```

### Secondary Priority (T024-T025)

5. **Fix Namespace owner_id in sql/executor.rs**:
   - Add `owner_id: UserId::new("system")` to Namespace instantiations

6. **Systematic Type Conversion Fixes**:
   - Create helper functions if needed
   - Apply `.to_string()`, `.as_str()`, `::new()` consistently

### Final Validation (T013-T014)

7. **Build Validation**:
   ```bash
   cd /Users/jamal/git/KalamDB
   cargo build
   ```

8. **Test Validation**:
   ```bash
   cargo test
   backend/tests/test_api_key_auth.rs
   backend/tests/test_combined_data_integrity.rs
   ```

## Success Criteria

Phase 0 is complete when:

1. ✅ All system models in kalamdb-commons/src/models/system.rs
2. ✅ No kalamdb_sql::models::* imports remain
3. ❌ `cargo build` succeeds with 0 errors (currently 154)
4. ❌ `cargo test` passes all tests
5. ❌ Integration tests pass
6. ❌ Documentation updated

**Current Progress**: 2/6 criteria met (33%)

## Files Modified Summary

### Created
- `docs/architecture/CATALOG_VS_SYSTEM_MODELS.md` (comprehensive architecture guide)
- `docs/architecture/SYSTEM_MODEL_CONSOLIDATION_STATUS.md` (status tracking)
- `specs/007-user-auth/US9_TASKS_SUMMARY.md` (task breakdown)
- `specs/007-user-auth/PHASE0_PROGRESS.md` (this document)

### Modified
- `backend/crates/kalamdb-core/src/tables/system/users_provider.rs` (User model migration)
- `backend/crates/kalamdb-core/src/tables/system/users.rs` (schema update)
- `backend/crates/kalamdb-core/src/sql/executor.rs` (import fixes)
- `backend/crates/kalamdb-core/src/services/*.rs` (7 files - import fixes)
- `backend/crates/kalamdb-core/src/tables/system/storages_provider.rs` (lifetime fix)
- `backend/src/commands/create_user.rs` (import fix)
- `backend/src/lifecycle.rs` (import fix)
- `specs/007-user-auth/spec.md` (added US9, US10)
- `specs/007-user-auth/tasks.md` (added Phase 0, updated T004-T010, added T020-T026)

### Deleted
- `backend/crates/kalamdb-sql/src/models.rs` (verified already deleted)

## Estimated Remaining Effort

- **T020-T023** (Job fixes): 2-3 hours
- **T024** (Namespace fixes): 30 minutes  
- **T025** (Type conversions): 3-4 hours
- **T006-T009** (live_queries_provider): 1-2 hours
- **T013-T017** (Testing): 2-3 hours
- **T018-T019** (Documentation): 1 hour

**Total**: 10-14 hours to complete Phase 0

## Risks & Blockers

1. **Widespread Type Mismatches**: 98 type errors suggest need for systematic refactoring strategy
2. **Test Compatibility**: Serialization changes may break existing tests
3. **Integration Impact**: Changes to system models may affect WebSocket protocol, API responses
4. **Scope Creep**: Additional field mismatches may be discovered during fixes

## Recommendations

1. **Batch Type Conversion Fixes**: Create helper PR with just type conversion utilities, then apply systematically
2. **Job Struct Priority**: Fix all Job-related errors first (highest impact - 18 errors)
3. **Incremental Testing**: Run `cargo check` after each file to catch regressions early
4. **Documentation**: Update SYSTEM_MODEL_CONSOLIDATION.md after each major fix with lessons learned

---

**Last Updated**: 2025-01-27 (after T001-T005, T010 completion)  
**Next Review**: After T020-T023 completion (Job fixes)
