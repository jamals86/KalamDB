# Tasks: Production Readiness & Full DML/DDL

**Feature**: `013-production-readiness`
**Status**: In Progress

## Phase 1: Setup
*Goal: Initialize project structure and verify environment.*

- [x] T001 [P] Verify development environment and dependencies in `backend/crates/kalamdb-core`
- [x] T002 [P] Create integration test file `tests/test_manifest_persistence.rs`
- [x] T003 [P] Create integration test file `tests/test_alter_table.rs`
- [x] T004 [P] Create integration test file `tests/test_dml_complex.rs`

## Phase 2: Foundational (Data Durability & Provider Refactoring)
*Goal: Ensure data durability via manifest persistence (User Story 4) and unify table provider logic.*
*Independent Test: Perform writes, trigger a flush, verify manifest file exists and contains correct segment info.*

- [x] T005 [US4] Refactor `Manifest` struct in `backend/crates/kalamdb-commons/src/models/manifest.rs` to match data model
- [x] T006 [US4] Modify `ManifestService` in `backend/crates/kalamdb-core/src/manifest/service.rs` to implement Hot/Cold split (update in-memory/RocksDB only)
- [x] T007 [US4] Implement `flush_manifest` method in `backend/crates/kalamdb-core/src/manifest/service.rs` to write `manifest.json` to disk
- [x] T008 [US4] Update `ensure_manifest_initialized` in `backend/crates/kalamdb-core/src/manifest/service.rs` to load from Cold Store if Hot Store is empty
- [x] T009 [US4] Create `BaseTableProvider` abstraction in `backend/crates/kalamdb-core/src/tables/base_table_provider.rs` to unify logic for User/Shared/Stream tables
- [x] T010 [US4] Refactor `UserTableProvider` in `backend/crates/kalamdb-core/src/tables/user_tables/provider.rs` to use `BaseTableProvider` and integrate `ManifestService`
- [x] T011 [US4] Refactor `SharedTableProvider` in `backend/crates/kalamdb-core/src/tables/shared_tables/provider.rs` to use `BaseTableProvider` and integrate `ManifestService`
- [x] T012 [US4] Refactor `StreamTableProvider` in `backend/crates/kalamdb-core/src/tables/stream_tables/provider.rs` to use `BaseTableProvider` (ensure clean separation)
- [x] T013 [US4] Ensure `insert` and `scan` logic in `BaseTableProvider` correctly handles Manifest updates and RocksDB/Parquet reads
- [x] T014 [US4] Implement integration test for manifest persistence in `tests/test_manifest_persistence.rs`

## Phase 3: User Story 1 (Full DML Support)
*Goal: Finalize DML support (UPDATE/DELETE) with DataFusion integration.*
*Independent Test: Insert data, update it, delete it, restart server, verify state.*

- [x] T015 [US1] Refactor `UpdateHandler` in `backend/crates/kalamdb-core/src/sql/executor/handlers/dml/update.rs` to use DataFusion `Scan` -> `Filter` -> `Write` pattern
- [x] T016 [US1] Refactor `DeleteHandler` in `backend/crates/kalamdb-core/src/sql/executor/handlers/dml/delete.rs` to use DataFusion `Scan` -> `Filter` -> `Write` pattern
- [x] T017 [US1] Implement integration test for complex DML in `tests/test_dml_complex.rs`

## Phase 4: User Story 2 (Schema Evolution)
*Goal: Add full DDL support (ALTER TABLE).*
*Independent Test: Create table, insert data, alter schema, verify data accessibility.*

- [x] T018 [US2] Enhance `AlterTableHandler` in `backend/crates/kalamdb-core/src/sql/executor/handlers/table/alter.rs` to add `RENAME COLUMN` support (Metadata only)
- [x] T019 [US2] Verify and refine `ADD COLUMN` logic in `AlterTableHandler` to handle edge cases
- [x] T020 [US2] Verify and refine `DROP COLUMN` logic in `AlterTableHandler` to handle edge cases
- [x] T021 [US2] Update `SchemaRegistry` in `backend/crates/kalamdb-core/src/schema_registry/registry.rs` to ensure `alter_table` invalidates cached `TableProvider`
- [x] T022 [US2] Implement integration test for ALTER TABLE in `tests/test_alter_table.rs`

## Phase 5: User Story 3 (System Observability)
*Goal: Enhance system tables for better observability.*
*Independent Test: Query `system.live_queries` and `system.tables`.*

- [x] T023 [US3] Update `TableDefinition` in `backend/crates/kalamdb-commons/src/models/schemas/table.rs` to include `options` field
- [x] T024 [US3] Enhance `SystemTableProvider` for `system.tables` in `backend/crates/kalamdb-system/src/providers/tables/tables_provider.rs` to include `options` column
- [x] T025 [US3] Verify `LiveQueryRegistry` integration in `backend/crates/kalamdb-core/src/live/manager/core.rs`
- [x] T026 [US3] Verify `system.live_queries` provider in `backend/crates/kalamdb-system/src/providers/live_queries/live_queries_provider.rs` exposes all necessary fields

## Phase 6: Polish & Cross-Cutting Concerns
*Goal: Final validation and performance tuning.*

- [x] T027 Run full smoke tests suite `run_smoke_tests.sh` (48/48 passing - 100% pass rate)
- [ ] T028 Perform manual verification using `specs/013-production-readiness/quickstart.md`
- [ ] T029 [P] Benchmark Manifest overhead (optional)

## Dependencies

- **US4 (Durability)** must be completed before **US1 (DML)** to ensure persistence of updates/deletes.
- **US1 (DML)** and **US2 (Schema Evolution)** can be implemented in parallel after US4.
- **US3 (Observability)** is independent but lower priority.

## Parallel Execution Examples

- **US1 & US2**: One developer can work on `UpdateHandler` (T013) while another works on `AlterTableHandler` (T016).
- **US3**: Can be implemented in parallel with any other phase.

## Implementation Strategy

1.  **Foundation First**: Secure the storage layer with Manifest persistence (US4).
2.  **Core Functionality**: Enable full DML (US1) and DDL (US2) to unblock application development.
3.  **Observability**: Enhance system tables (US3) for debugging and monitoring.
4.  **Validation**: rigorous testing and benchmarking.
