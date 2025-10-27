# User Story 9 (System Model Consolidation) - Tasks Summary

**Date**: October 27, 2025  
**Feature**: 007-user-auth  
**User Story**: US9 - System Model Consolidation (Priority: P0 - CRITICAL PREREQUISITE)

## Task Summary

Added **Phase 0** with **19 tasks** (T001-T019) to `specs/007-user-auth/tasks.md`

### Phase 0: System Model Consolidation

**Purpose**: Consolidate ALL duplicate system table models into single source of truth in `kalamdb-commons/src/models/system.rs` before any authentication work begins

**Total Tasks**: 19 tasks organized into 5 categories:
1. **Verification Tasks** (3 tasks): T001-T003
2. **Cleanup & Migration Tasks** (6 tasks): T004-T009
3. **Import Cleanup Tasks** (3 tasks): T010-T012
4. **Build & Test Validation** (5 tasks): T013-T017
5. **Documentation Updates** (2 tasks): T018-T019

### Task Breakdown

#### Verification Tasks (T001-T003)
- T001: Verify all canonical models exist in kalamdb-commons/src/models/system.rs
- T002: Verify kalamdb-sql/src/models.rs is deleted and lib.rs re-exports from commons
- T003: Verify catalog models are documented as different from system models

#### Cleanup & Migration Tasks (T004-T009)
- T004: Fix users_provider.rs field mismatches (storage_mode/storage_id)
- T005: Update all Role assignments to use Role enum (not strings)
- T006: Migrate live_queries_provider.rs (remove LiveQueryRecord)
- T007: Replace all LiveQueryRecord → LiveQuery references
- T008: Update method signatures in live_queries_provider.rs
- T009: Update exports in tables/system/mod.rs

#### Import Cleanup Tasks (T010-T012)
- T010: Search and replace remaining kalamdb_sql::models::* imports
- T011: Verify no old User/Job/LiveQuery instantiations in kalamdb-core
- T012: Verify no local struct definitions in kalamdb-sql

#### Build & Test Validation (T013-T017)
- T013: Run `cargo build` and fix compilation errors
- T014: Run `cargo test` and fix test failures
- T015: Run test_api_key_auth.rs integration test
- T016: Run test_combined_data_integrity.rs integration test
- T017: Run table provider unit tests

#### Documentation Updates (T018-T019)
- T018: Update .github/copilot-instructions.md
- T019: Mark consolidation complete in SYSTEM_MODEL_CONSOLIDATION.md

## Impact on Task Numbering

**Before**: Tasks started at T001 (Phase 1: Setup)  
**After**: Tasks now start at T001 (Phase 0: System Model Consolidation)

**Renumbering Required**:
- Phase 1 (Setup): T001-T010 → T020-T029
- Phase 2 (Foundational): T011-T044 → T030-T064
- All subsequent phases: Add +19 to all task IDs

**Total Task Count**:
- **Before**: 322 tasks across 15 phases
- **After**: 341 tasks across 16 phases

## Implementation Status (as of 2025-10-27)

Based on the current state documented in spec.md:

✅ **Already Completed** (don't need tasks):
- T001: ✅ kalamdb-commons/src/models/system.rs has all models
- T002: ✅ kalamdb-sql/src/models.rs deleted, lib.rs re-exports
- T003: ✅ Catalog models documented in CATALOG_VS_SYSTEM_MODELS.md

❌ **Remaining Work**:
- T004: ❌ Fix users_provider.rs (CRITICAL - blocking compilation)
- T005: ❌ Update Role enum usage (CRITICAL - blocking compilation)
- T006-T009: ❌ Migrate live_queries_provider.rs
- T010-T012: ❌ Import cleanup verification
- T013-T014: ❌ Build and test validation
- T015-T017: ❌ Integration/unit test verification
- T018-T019: ❌ Documentation updates

## Execution Order

**Sequential (MUST complete in order)**:
1. T004-T005: Fix users_provider.rs (CRITICAL - unblocks build)
2. T006-T009: Migrate live_queries_provider.rs
3. T010-T012: Import cleanup
4. T013-T014: Build validation
5. T015-T017: Test validation
6. T018-T019: Documentation

**Parallel Opportunities**:
- T001-T003 can run in parallel (verification only)
- T006-T008 can run in parallel (same file, different methods)
- T015-T017 can run in parallel (different test files)
- T018-T019 can run in parallel (different docs)

## Files Affected

### Modified Files
- `backend/crates/kalamdb-core/src/tables/system/users_provider.rs` (T004-T005)
- `backend/crates/kalamdb-core/src/tables/system/live_queries_provider.rs` (T006-T008)
- `backend/crates/kalamdb-core/src/tables/system/mod.rs` (T009)
- `.github/copilot-instructions.md` (T018)
- `docs/architecture/SYSTEM_MODEL_CONSOLIDATION.md` (T019)

### Already Modified
- ✅ `kalamdb-commons/src/models/system.rs`
- ✅ `kalamdb-sql/src/lib.rs`
- ✅ `kalamdb-sql/src/adapter.rs`
- ✅ `kalamdb-core/src/services/stream_table_service.rs`
- ✅ `kalamdb-core/src/services/shared_table_service.rs`
- ✅ `kalamdb-core/src/tables/system/jobs_provider.rs`

## Success Criteria

**Phase 0 is complete when**:
1. ✅ All canonical models in kalamdb-commons/src/models/system.rs
2. ✅ kalamdb-sql/src/models.rs deleted
3. ✅ users_provider.rs compiles without errors
4. ✅ live_queries_provider.rs uses LiveQuery (not LiveQueryRecord)
5. ✅ No `use kalamdb_sql::models::` imports remain
6. ✅ `cargo build` succeeds in backend/
7. ✅ `cargo test` passes all tests
8. ✅ Integration tests pass (test_api_key_auth, test_combined_data_integrity)
9. ✅ Unit tests pass (jobs_provider::tests, live_queries_provider::tests)
10. ✅ Documentation updated

## Next Phase

After Phase 0 completion, proceed to **Phase 0.5: Storage Backend Abstraction** (User Story 10), which is the second P0 critical prerequisite.

---

**Note**: This phase is NON-NEGOTIABLE and MUST be completed before any authentication implementation work begins. The auth code depends on the canonical User model from kalamdb-commons.
