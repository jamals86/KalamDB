# Tasks: SQL Handlers Prep

**Input**: Design documents from `/specs/011-sql-handlers-prep/`  
**Branch**: `011-sql-handlers-prep`  
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `- [ ] [ID] [P?] [Story?] Description`

- **Checkbox**: ALWAYS start with `- [ ]` (markdown checkbox)
- **[ID]**: Sequential task number (T001, T002, etc.) in execution order
- **[P]**: OPTIONAL - Include ONLY if task is parallelizable (different files, no dependencies)
- **[Story]**: REQUIRED for user story phase tasks (e.g., [US1], [US2], [US3])
  - Setup/Foundational/Polish phases: NO story label
  - User Story phases: MUST have story label
- **Description**: Clear action with exact file path

## Path Conventions

This is a Rust workspace project:
- Backend library: `backend/crates/kalamdb-core/src/`
- REST API: `backend/crates/kalamdb-api/src/`
- Tests: `backend/tests/`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and module restructuring

- [X] T001 Create `sql/executor/models/` directory structure
- [X] T002 [P] Create `sql/executor/helpers/` directory structure
- [X] T003 [P] Create `sql/executor/handlers/ddl/` directory structure
- [X] T004 [P] Create `sql/executor/handlers/dml/` directory structure
- [X] T005 [P] Create `sql/executor/handlers/flush/` directory structure
- [X] T006 [P] Create `sql/executor/handlers/jobs/` directory structure
- [X] T007 [P] Create `sql/executor/handlers/subscription/` directory structure
- [X] T008 [P] Create `sql/executor/handlers/user/` directory structure
- [X] T009 [P] Create `sql/executor/handlers/transaction/` directory structure

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core execution infrastructure that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

### Execution Models Refactoring

- [X] T010 [P] Create ExecutionContext in `backend/crates/kalamdb-core/src/sql/executor/models/context.rs`
- [X] T011 [P] Create ExecutionResult in `backend/crates/kalamdb-core/src/sql/executor/models/result.rs`
- [X] T012 [P] Create ExecutionMetadata in `backend/crates/kalamdb-core/src/sql/executor/models/metadata.rs`
- [X] T013 Create models module re-exports in `backend/crates/kalamdb-core/src/sql/executor/models/mod.rs`

### Helper Utilities Migration

- [X] T014 [P] Move audit.rs to `backend/crates/kalamdb-core/src/sql/executor/helpers/audit.rs`
- [X] T015 [P] Move helpers.rs to `backend/crates/kalamdb-core/src/sql/executor/helpers/helpers.rs`
- [X] T016 Create helpers module re-exports in `backend/crates/kalamdb-core/src/sql/executor/helpers/mod.rs`
- [X] T017 Remove legacy `handlers/table_registry.rs` file

### Handler Infrastructure

- [X] T018 Create TypedStatementHandler trait in `backend/crates/kalamdb-core/src/sql/executor/handlers/typed.rs`
- [X] T019 Create TypedHandlerAdapter generic adapter in `backend/crates/kalamdb-core/src/sql/executor/handlers/handler_adapter.rs`
- [X] T020 Create HandlerRegistry with DashMap in `backend/crates/kalamdb-core/src/sql/executor/handlers/handler_registry.rs`
- [ ] T021 Update SqlExecutor to use HandlerRegistry in `backend/crates/kalamdb-core/src/sql/executor/mod.rs`

### Parameter Binding Infrastructure

- [X] T022 Add ScalarValue parameter support to ExecutionContext (see T010)
- [X] T023 Implement DataFusion LogicalPlan placeholder replacement in `backend/crates/kalamdb-core/src/sql/executor/parameter_binding.rs`
- [X] T024 Add parameter validation (max 50 params, 512KB per param) in parameter_binding.rs
- [ ] T025 Add handler execution timeout (30s default) to ExecutionContext

### Configuration Updates

- [X] T026 Add `[execution]` section to config.toml with handler_timeout_seconds, max_parameters, max_parameter_size_bytes

### Error Handling

- [X] T027 [P] Add structured error codes to KalamDbError in `backend/crates/kalamdb-commons/src/error.rs`
- [X] T028 [P] Implement ErrorResponse with code/message/details in `backend/crates/kalamdb-core/src/sql/executor/models/error.rs`

### Import Path Updates

- [ ] T029 Update all imports of ExecutionContext to new models/ path across workspace
- [ ] T030 Update all imports of audit helpers to new helpers/ path across workspace
- [ ] T031 [P] Run `cargo build` to verify workspace compilation
- [ ] T032 [P] Run existing unit tests to verify no behavioral regressions

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 1 - Execute SQL with Request Context (Priority: P1) 🎯 MVP

**Goal**: Enable authenticated users to execute SQL statements via REST API with proper authorization enforcement and consistent response format

**Independent Test**: Call POST /v1/api/sql with valid JWT, verify ExecutionContext created from auth and response includes row_count/rows_affected

### Implementation for User Story 1

- [ ] T033 [US1] Update REST API route to construct ExecutionContext in `backend/crates/kalamdb-api/src/routes/sql.rs`
- [ ] T034 [US1] Implement execute_via_datafusion with parameter binding in `backend/crates/kalamdb-core/src/sql/executor/mod.rs`
- [ ] T035 [US1] Add authorization check dispatcher in SqlExecutor before handler execution
- [ ] T036 [US1] Implement row_count/rows_affected computation for all ExecutionResult variants
- [ ] T037 [US1] Update response serialization with row counts in kalamdb-api routes
- [ ] T038 [US1] Add request_id and elapsed_ms to all API responses

**Checkpoint**: User Story 1 complete - simple SELECT with auth context works end-to-end

---

## Phase 4: User Story 2 - Parameterized Execution Support (Priority: P2)

**Goal**: Enable secure parameter binding via DataFusion's native ScalarValue validation for SELECT, INSERT, UPDATE, DELETE

**Independent Test**: Call POST /v1/api/sql with `{"sql": "SELECT * FROM t WHERE id = $1", "params": [123]}` and verify parameter substitution and type validation

### Implementation for User Story 2

- [ ] T039 [P] [US2] Add params field to API request schema in `backend/crates/kalamdb-api/src/routes/sql.rs`
- [ ] T040 [P] [US2] Implement JSON → ScalarValue deserialization in kalamdb-api
- [ ] T041 [US2] Implement LogicalPlan traversal for placeholder replacement (TreeNode::rewrite) in parameter_binding.rs
- [ ] T042 [US2] Add ExprRewriter for Expr::Placeholder → Expr::Literal conversion in parameter_binding.rs
- [ ] T043 [US2] Integrate parameter binding into execute_via_datafusion function
- [ ] T044 [US2] Add validation for unsupported statement types (DDL, system commands) with parameters
- [ ] T045 [US2] Implement parameter count validation error (PARAM_COUNT_MISMATCH)
- [ ] T046 [US2] Implement parameter size validation error (PARAM_SIZE_EXCEEDED)
- [ ] T047 [US2] Add unit tests for parameter binding in `backend/crates/kalamdb-core/src/sql/executor/tests/test_parameter_binding.rs`

**Checkpoint**: User Story 2 complete - parameterized queries work with validation

---

## Phase 5: User Story 3 - Auditable Operations (Priority: P3)

**Goal**: Provide audit logging helpers for consistent recording of DDL and DML operations

**Independent Test**: Unit test calling log_ddl_operation produces AuditLogEntry with expected fields from ExecutionContext

### Implementation for User Story 3

- [ ] T048 [P] [US3] Implement log_ddl_operation helper in `backend/crates/kalamdb-core/src/sql/executor/helpers/audit.rs`
- [ ] T049 [P] [US3] Implement log_dml_operation helper with rows_affected in audit.rs
- [ ] T050 [US3] Add audit log entry creation from ExecutionContext (user_id, request_id, ip_address, timestamp)
- [ ] T051 [US3] Add unit tests for audit helpers in `backend/tests/unit/test_audit_helpers.rs`

**Checkpoint**: User Story 3 complete - audit logging API available

---

## Phase 6: User Story 4 - Modular DML Handlers (Priority: P2)

**Goal**: Split DML handling into focused modules (INSERT, DELETE, UPDATE) that delegate to DataFusion with parameter binding

**Independent Test**: Call POST /v1/api/sql with INSERT/UPDATE/DELETE and verify correct handler routing and rows_affected response

### Implementation for User Story 4

- [X] T052 [P] [US4] Create InsertHandler in `backend/crates/kalamdb-core/src/sql/executor/handlers/dml/insert.rs`
- [X] T053 [P] [US4] Create UpdateHandler in `backend/crates/kalamdb-core/src/sql/executor/handlers/dml/update.rs`
- [X] T054 [P] [US4] Create DeleteHandler in `backend/crates/kalamdb-core/src/sql/executor/handlers/dml/delete.rs`
- [X] T055 [US4] Create DML module re-exports in `backend/crates/kalamdb-core/src/sql/executor/handlers/dml/mod.rs`
- [X] T056 [US4] Register InsertHandler in HandlerRegistry via register_dynamic()
- [X] T057 [US4] Register UpdateHandler in HandlerRegistry via register_dynamic()
- [X] T058 [US4] Register DeleteHandler in HandlerRegistry via register_dynamic()
- [X] T059 [P] [US4] Add unit tests for InsertHandler in insert.rs (success + authorization)
- [X] T060 [P] [US4] Add unit tests for UpdateHandler in update.rs (success + authorization)
- [X] T061 [P] [US4] Add unit tests for DeleteHandler in delete.rs (success + authorization)

**Checkpoint**: User Story 4 complete - DML operations route to dedicated handlers

---

## Phase 7: User Story 5 - Full DML with Native Write Paths (Priority: P1)

**Goal**: Enable INSERT/DELETE/UPDATE to execute using native storage providers with DataFusion parameter binding

**Independent Test**: Execute INSERT with parameters, verify rows written to storage; DELETE with predicate, verify rows removed; UPDATE with SET/WHERE, verify rows_affected only counts actual changes

### Implementation for User Story 5

- [ ] T062 [US5] Implement parameter validation in InsertHandler before write
- [ ] T063 [US5] Implement parameter validation in UpdateHandler before write
- [ ] T064 [US5] Implement parameter validation in DeleteHandler before write
- [ ] T065 [US5] Add rows_affected computation to InsertHandler (sum of RecordBatch.num_rows())
- [ ] T066 [US5] Add rows_affected computation to UpdateHandler (only rows with actual changes)
- [ ] T067 [US5] Add rows_affected computation to DeleteHandler
- [ ] T068 [US5] Add integration tests for INSERT with parameters in `backend/tests/integration/test_dml_parameters.rs`
- [ ] T069 [US5] Add integration tests for UPDATE with parameters in test_dml_parameters.rs
- [ ] T070 [US5] Add integration tests for DELETE with parameters in test_dml_parameters.rs

**Checkpoint**: User Story 5 complete - DML write paths fully functional with parameters

---

## Phase 8: User Story 6 - Complete Handler Implementation (Priority: P0)

**Goal**: Implement all 28 SQL statement handlers (excluding SELECT) with typed handler pattern

**Independent Test**: Execute one statement from each category (DDL, DML, Flush, Jobs, Subscription, User, Transaction) via REST API, verify correct routing and authorization

### Namespace Handlers (4 handlers) - Migrate from ddl_legacy.rs

**Note**: CreateNamespaceHandler already implemented as reference (✅ COMPLETE)

- [X] T071 [US6] CreateNamespaceHandler - ✅ COMPLETE (reference implementation in `handlers/namespace/create.rs`)
- [ ] T072 [P] [US6] Migrate AlterNamespaceHandler logic from ddl_legacy.rs to `handlers/namespace/alter.rs` (impl TypedStatementHandler)
- [ ] T073 [P] [US6] Migrate DropNamespaceHandler logic from ddl_legacy.rs (execute_drop_namespace) to `handlers/namespace/drop.rs` (impl TypedStatementHandler)
- [ ] T074 [P] [US6] Migrate ShowNamespacesHandler logic to `handlers/namespace/show.rs` (impl TypedStatementHandler, use AppContext.system_tables().namespaces())

### Storage Handlers (4 handlers) - Migrate from ddl_legacy.rs

- [ ] T075 [P] [US6] Migrate CreateStorageHandler logic from ddl_legacy.rs (execute_create_storage) to `handlers/storage/create.rs` (impl TypedStatementHandler)
- [ ] T076 [P] [US6] Migrate AlterStorageHandler logic to `handlers/storage/alter.rs` (impl TypedStatementHandler)
- [ ] T077 [P] [US6] Migrate DropStorageHandler logic to `handlers/storage/drop.rs` (impl TypedStatementHandler)
- [ ] T078 [P] [US6] Migrate ShowStoragesHandler logic to `handlers/storage/show.rs` (impl TypedStatementHandler, use AppContext.system_tables().storages())

### Table Handlers (7 handlers) - Migrate from ddl_legacy.rs

**Note**: Table handlers are complex (445+ lines in ddl_legacy.rs) - split by table type (USER/SHARED/STREAM)

- [ ] T079 [US6] Migrate CreateTableHandler logic from ddl_legacy.rs (execute_create_table + create_user_table/create_shared_table/create_stream_table) to `handlers/table/create.rs` (~500 lines with helper methods)
- [ ] T080 [US6] Migrate AlterTableHandler logic from ddl_legacy.rs (execute_alter_table, Phase 10.2 SchemaRegistry pattern) to `handlers/table/alter.rs` (impl TypedStatementHandler)
- [ ] T081 [US6] Migrate DropTableHandler logic from ddl_legacy.rs (execute_drop_table + 6 helper methods) to `handlers/table/drop.rs` (~400 lines, Phase 9 JobsManager pattern)
- [ ] T082 [P] [US6] Migrate ShowTablesHandler logic to `handlers/table/show.rs` (impl TypedStatementHandler, use SchemaRegistry.scan_namespace())
- [ ] T083 [P] [US6] Migrate DescribeTableHandler logic to `handlers/table/describe.rs` (impl TypedStatementHandler, use SchemaRegistry.get_table_definition())
- [ ] T084 [P] [US6] Migrate ShowStatsHandler logic to `handlers/table/show_stats.rs` (impl TypedStatementHandler)

### Handler Registration & Module Exports

- [ ] T085 [US6] Register all namespace/storage/table handlers in HandlerRegistry::new() with extractor closures
- [ ] T086 [US6] Update mod.rs files in namespace/, storage/, table/ directories to re-export all handlers

### Handler Unit Tests (run after migration complete)

- [ ] T087 [P] [US6] Add unit tests for AlterNamespaceHandler in alter.rs (success + authorization)
- [ ] T088 [P] [US6] Add unit tests for DropNamespaceHandler in drop.rs (success + authorization + can_delete check)
- [ ] T089 [P] [US6] Add unit tests for ShowNamespacesHandler in show.rs (success + authorization)
- [ ] T090 [P] [US6] Add unit tests for CreateStorageHandler in create.rs (success + authorization + template validation)
- [ ] T091 [P] [US6] Add unit tests for AlterStorageHandler in alter.rs (success + authorization)
- [ ] T092 [P] [US6] Add unit tests for DropStorageHandler in drop.rs (success + authorization)
- [ ] T093 [P] [US6] Add unit tests for ShowStoragesHandler in show.rs (success + authorization)
- [ ] T094 [P] [US6] Add unit tests for CreateTableHandler in create.rs (success + USER/SHARED/STREAM table types + auto-increment injection)
- [ ] T095 [P] [US6] Add unit tests for AlterTableHandler in alter.rs (success + authorization + SchemaRegistry usage)
- [ ] T096 [P] [US6] Add unit tests for DropTableHandler in drop.rs (success + authorization + JobsManager integration + subscription check)
- [ ] T097 [P] [US6] Add unit tests for ShowTablesHandler in show.rs (success + authorization)
- [ ] T098 [P] [US6] Add unit tests for DescribeTableHandler in describe.rs (success + authorization)
- [ ] T099 [P] [US6] Add unit tests for ShowStatsHandler in show_stats.rs (success + authorization)

### Flush Handlers (2 handlers) - Implement with JobsManager pattern

**Note**: These are NOT in ddl_legacy.rs - implement from scratch following Phase 9 patterns

- [ ] T100 [P] [US6] Implement FlushTableHandler in `handlers/flush/flush_table.rs` (impl TypedStatementHandler, use JobsManager.create_job with JobType::Flush)
- [ ] T101 [P] [US6] Implement FlushAllTablesHandler in `handlers/flush/flush_all_tables.rs` (impl TypedStatementHandler, use SchemaRegistry.scan_namespace + JobsManager)
- [ ] T102 [US6] Register flush handlers in HandlerRegistry::new()
- [ ] T103 [US6] Add flush module re-exports in `handlers/flush/mod.rs`
- [ ] T104 [P] [US6] Add unit tests for FlushTableHandler (success + authorization + job creation)
- [ ] T105 [P] [US6] Add unit tests for FlushAllTablesHandler (success + authorization + rows_affected count)

### Job Handlers (2 handlers) - Implement with JobsManager pattern

**Note**: These are NOT in ddl_legacy.rs - implement from scratch

- [ ] T106 [P] [US6] Implement KillJobHandler in `handlers/jobs/kill_job.rs` (impl TypedStatementHandler, use JobsManager.cancel_job)
- [ ] T107 [P] [US6] Implement KillLiveQueryHandler in `handlers/jobs/kill_live_query.rs` (impl TypedStatementHandler, use LiveQueryManager)
- [ ] T108 [US6] Register job handlers in HandlerRegistry::new()
- [ ] T109 [US6] Add jobs module re-exports in `handlers/jobs/mod.rs`
- [ ] T110 [P] [US6] Add unit tests for KillJobHandler (success + authorization + self-service check)
- [ ] T111 [P] [US6] Add unit tests for KillLiveQueryHandler (success + authorization)

### Subscription Handler (1 handler) - Implement with LiveQueryManager

**Note**: This is NOT in ddl_legacy.rs - implement from scratch

- [ ] T112 [US6] Implement SubscribeHandler in `handlers/subscription/subscribe.rs` (impl TypedStatementHandler, use LiveQueryManager)
- [ ] T113 [US6] Register SubscribeHandler in HandlerRegistry::new()
- [ ] T114 [US6] Add subscription module re-exports in `handlers/subscription/mod.rs`
- [ ] T115 [US6] Add unit tests for SubscribeHandler (success + authorization + subscription_id generation)

### User Management Handlers (3 handlers) - Implement from scratch

**Note**: These are NOT in ddl_legacy.rs - implement from scratch with bcrypt/JWT patterns

- [ ] T116 [P] [US6] Implement CreateUserHandler in `handlers/user/create.rs` (impl TypedStatementHandler, use AppContext.system_tables().users(), bcrypt password hashing)
- [ ] T117 [P] [US6] Implement AlterUserHandler in `handlers/user/alter.rs` (impl TypedStatementHandler, self-service password change + admin-only role change)
- [ ] T118 [P] [US6] Implement DropUserHandler in `handlers/user/drop.rs` (impl TypedStatementHandler, soft delete with deleted_at timestamp)
- [ ] T119 [US6] Register user handlers in HandlerRegistry::new()
- [ ] T120 [US6] Add user module re-exports in `handlers/user/mod.rs`
- [ ] T121 [P] [US6] Add unit tests for CreateUserHandler (success + authorization + password validation)
- [ ] T122 [P] [US6] Add unit tests for AlterUserHandler (success + self-service password + admin-only role change)
- [ ] T123 [P] [US6] Add unit tests for DropUserHandler (success + authorization + soft delete)

### Transaction Handlers (3 handlers - placeholders)

**Note**: Return NotImplemented with "Transaction support planned for Phase 11" message

- [ ] T124 [P] [US6] Implement BeginTransactionHandler (returns KalamDbError::NotImplemented) in `handlers/transaction/begin.rs`
- [ ] T125 [P] [US6] Implement CommitTransactionHandler (returns KalamDbError::NotImplemented) in `handlers/transaction/commit.rs`
- [ ] T126 [P] [US6] Implement RollbackTransactionHandler (returns KalamDbError::NotImplemented) in `handlers/transaction/rollback.rs`
- [ ] T127 [US6] Register transaction handlers in HandlerRegistry::new()
- [ ] T128 [US6] Add transaction module re-exports in `handlers/transaction/mod.rs`
- [ ] T129 [P] [US6] Add unit tests for transaction handlers (verify NotImplemented error with "Phase 11" message)

### Legacy Code Cleanup

- [ ] T130 [US6] Delete ddl_legacy.rs after all handler migrations verified complete (⚠️ DO THIS LAST - only after T072-T084 complete)
- [ ] T131 [US6] Remove any remaining references to ddl_legacy.rs from imports/mod.rs files

### Integration Tests

- [ ] T132 [US6] Add integration test for namespace handlers in `backend/tests/integration/test_namespace_handlers.rs` (CREATE/ALTER/DROP/SHOW)
- [ ] T133 [US6] Add integration test for storage handlers in `backend/tests/integration/test_storage_handlers.rs` (CREATE/ALTER/DROP/SHOW)
- [ ] T134 [US6] Add integration test for table handlers in `backend/tests/integration/test_table_handlers.rs` (CREATE/ALTER/DROP/SHOW/DESCRIBE all 3 types)
- [ ] T135 [US6] Add integration test for flush handlers in `backend/tests/integration/test_flush_handlers.rs` (FLUSH TABLE/ALL TABLES)
- [ ] T136 [US6] Add integration test for job handlers in `backend/tests/integration/test_job_handlers.rs` (KILL JOB/LIVE QUERY)
- [ ] T137 [US6] Add integration test for subscription handler in `backend/tests/integration/test_subscription_handler.rs` (SUBSCRIBE)
- [ ] T138 [US6] Add integration test for user handlers in `backend/tests/integration/test_user_handlers.rs` (CREATE/ALTER/DROP USER)
- [ ] T139 [US6] Add integration test for transaction handlers (verify 501 NotImplemented) in `backend/tests/integration/test_transaction_handlers.rs`

**Checkpoint**: User Story 6 complete - all 28 handlers implemented and tested (⚠️ Verify ddl_legacy.rs deleted before marking complete)

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: Final improvements affecting multiple handlers

- [ ] T135 [P] Update AGENTS.md with new handler architecture patterns
- [ ] T136 [P] Update docs/how-to-add-sql-statement.md guide with quickstart examples
- [ ] T137 [P] Add row_count/rows_affected to all handler responses (verify consistency)
- [ ] T138 Code review and refactoring for consistency across handlers
- [ ] T139 [P] Performance profiling of handler dispatch (<2μs overhead target)
- [ ] T140 [P] Performance profiling of parameter validation (<10μs target)
- [ ] T141 [P] Performance profiling of SchemaRegistry lookups (1-2μs target)
- [ ] T142 Security review of parameter validation (injection attack prevention)
- [ ] T143 Run quickstart.md validation scenarios
- [ ] T144 Final workspace compilation check (`cargo build`)
- [ ] T145 Final test suite run (`cargo test`)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup (Phase 1) completion - BLOCKS all user stories
- **User Stories (Phase 3-8)**: All depend on Foundational (Phase 2) completion
  - US1 (P1): Request context - foundation for all handlers
  - US2 (P2): Parameter binding - needed by US4, US5, US6
  - US3 (P3): Audit logging - optional, can run in parallel
  - US4 (P2): DML handler structure - needed by US5
  - US5 (P1): DML implementation - high priority for writes
  - US6 (P0): All handlers - largest effort, depends on US1, US2, US4
- **Polish (Phase 9)**: Depends on US6 completion

### User Story Dependencies

- **US1 (P1)**: BLOCKING for all other stories (request context foundation)
- **US2 (P2)**: BLOCKING for US4, US5 (parameter binding infrastructure)
- **US3 (P3)**: Independent, can run in parallel with others
- **US4 (P2)**: Depends on US1, US2; BLOCKING for US5
- **US5 (P1)**: Depends on US1, US2, US4
- **US6 (P0)**: Depends on US1, US2, US4 (uses patterns from all previous stories)

### Parallel Opportunities

**Phase 1 (Setup)**: Tasks T002-T009 can run in parallel (different directories)

**Phase 2 (Foundational)**: 
- Models (T010-T012) can run in parallel
- Helpers (T014-T015) can run in parallel
- Error types (T027-T028) can run in parallel
- Build/test validation (T031-T032) can run in parallel

**Phase 3 (US1)**: Most tasks sequential (single executor file)

**Phase 4 (US2)**: API schema and deserialization (T039-T040) can run in parallel

**Phase 5 (US3)**: Both audit helpers (T048-T049) can run in parallel

**Phase 6 (US4)**: All 3 DML handlers (T052-T054) can run in parallel, unit tests (T059-T061) can run in parallel

**Phase 7 (US5)**: Validation tasks (T062-T064) can run in parallel, rows_affected (T065-T067) can run in parallel, integration tests (T068-T070) can run in parallel

**Phase 8 (US6)**: 
- All DDL handler implementations (T071-T083) can run in parallel
- All DDL handler tests (T086-T098) can run in parallel
- All flush/job/subscription/user handlers can run in parallel
- All transaction handlers (T123-T125) can run in parallel
- Integration tests (T129-T134) can run in parallel

**Phase 9 (Polish)**: Documentation (T135-T136) and profiling (T139-T141) can run in parallel

---

## Parallel Example: User Story 6 (DDL Handlers)

```bash
# Launch all 13 DDL handler implementations together:
Task T071: "AlterNamespaceHandler in alter_namespace.rs"
Task T072: "DropNamespaceHandler in drop_namespace.rs"
Task T073: "ShowNamespacesHandler in show_namespaces.rs"
Task T074: "CreateStorageHandler in create_storage.rs"
Task T075: "AlterStorageHandler in alter_storage.rs"
Task T076: "DropStorageHandler in drop_storage.rs"
Task T077: "ShowStoragesHandler in show_storages.rs"
Task T078: "CreateTableHandler in create_table.rs"
Task T079: "AlterTableHandler in alter_table.rs"
Task T080: "DropTableHandler in drop_table.rs"
Task T081: "ShowTablesHandler in show_tables.rs"
Task T082: "DescribeTableHandler in describe_table.rs"
Task T083: "ShowStatsHandler in show_stats.rs"

# After implementations done, launch all 13 DDL handler tests together:
Task T086: "Unit tests for AlterNamespaceHandler"
Task T087: "Unit tests for DropNamespaceHandler"
# ... etc
```

---

## Implementation Strategy

### MVP First (US1 + US2 Only)

1. Complete Phase 1: Setup → Directory structure ready
2. Complete Phase 2: Foundational → Core infrastructure ready
3. Complete Phase 3: US1 → Basic SQL execution with auth works
4. Complete Phase 4: US2 → Parameter binding works
5. **STOP and VALIDATE**: Test US1+US2 independently with parameterized SELECT/INSERT
6. Deploy/demo if ready

### Incremental Delivery

1. Complete Setup + Foundational → 32 tasks → Foundation ready
2. Add US1 (6 tasks) → Basic execution → Test independently → Deploy/Demo (minimal MVP!)
3. Add US2 (9 tasks) → Parameters → Test independently → Deploy/Demo
4. Add US3 (4 tasks) → Audit logging → Test independently
5. Add US4 (10 tasks) → DML handlers → Test independently → Deploy/Demo
6. Add US5 (9 tasks) → Full DML writes → Test independently → Deploy/Demo
7. Add US6 (64 tasks) → All handlers → Test independently → Deploy/Demo (complete feature!)
8. Polish (11 tasks) → Production-ready

### Parallel Team Strategy

With multiple developers after Foundational phase complete:

**Team A**: US1 → US5 (core execution path, 38 tasks sequential)
**Team B**: US3 (audit helpers, 4 tasks, independent)
**Team C**: US6 DDL handlers (13 tasks parallel) after US1+US2 done
**Team D**: US6 Other handlers (flush/job/subscription/user/transaction, 18 tasks parallel) after US1+US2 done

---

## Task Count Summary

- **Phase 1 (Setup)**: 9 tasks (✅ COMPLETE - directory structure exists)
- **Phase 2 (Foundational)**: 23 tasks (BLOCKING)
- **Phase 3 (US1)**: 6 tasks
- **Phase 4 (US2)**: 9 tasks
- **Phase 5 (US3)**: 4 tasks
- **Phase 6 (US4)**: 10 tasks
- **Phase 7 (US5)**: 9 tasks
- **Phase 8 (US6)**: 70 tasks (migration + implementation + cleanup)
  - Handler migrations: 13 tasks (namespace/storage/table from ddl_legacy.rs)
  - Handler implementations: 16 tasks (flush/jobs/subscription/user/transaction)
  - Registration & exports: 2 tasks
  - Unit tests: 27 tasks
  - Integration tests: 8 tasks
  - Legacy cleanup: 2 tasks (delete ddl_legacy.rs)
  - DML handlers (T052-T061): Already created/complete (10 tasks)
- **Phase 9 (Polish)**: 11 tasks

**Total**: 151 tasks (was 145, +6 for explicit migration tasks and legacy cleanup)

**Parallel Opportunities**: 
- Phase 1: 8 tasks parallelizable (✅ COMPLETE)
- Phase 2: 12 tasks parallelizable
- Phase 4: 2 tasks parallelizable
- Phase 5: 2 tasks parallelizable
- Phase 6: 7 tasks parallelizable (✅ COMPLETE - DML handlers exist)
- Phase 7: 6 tasks parallelizable
- Phase 8: 45 tasks parallelizable (handler migrations + implementations + unit tests)
- Phase 9: 6 tasks parallelizable

**Total Parallelizable**: 88 tasks (58% of all tasks)

**Critical Path for Phase 8 (US6)**:
1. Migrate all handlers from ddl_legacy.rs (T072-T084) - 13 tasks - MUST complete first
2. Implement new handlers (flush/jobs/subscription/user/transaction) (T100-T129) - 30 tasks - can run in parallel with migrations
3. Registration & exports (T085-T086) - 2 tasks - depends on handler completion
4. Unit tests (T087-T129) - 43 tasks total - can run in parallel after handlers done
5. Integration tests (T132-T139) - 8 tasks - can run in parallel after handlers done
6. Delete ddl_legacy.rs (T130-T131) - 2 tasks - MUST be last (after all migrations verified)

**Migration Priority** (Phase 8):
- **High Priority** (blocking other work):
  - T079: CreateTableHandler (445+ lines, complex, needed for integration tests)
  - T081: DropTableHandler (400+ lines, JobsManager integration pattern)
  - T073: DropNamespaceHandler (needed for namespace tests)
- **Medium Priority** (simpler migrations):
  - T072, T074, T075-T078: Namespace/Storage handlers (~50-100 lines each)
  - T080, T082-T084: Alter/Show handlers (straightforward CRUD)
- **Low Priority** (implement from scratch, not migrations):
  - T100-T129: Flush/Jobs/Subscription/User/Transaction handlers (no legacy code to migrate)

---

## Notes

- [P] tasks = different files, no dependencies - can run in parallel
- [Story] label maps task to specific user story (US1-US6) for traceability
- Each user story should be independently testable at its checkpoint
- **Migration Strategy**: Copy logic from ddl_legacy.rs → individual handlers → delete ddl_legacy.rs (T130-T131 LAST)
- **Reference Implementation**: CreateNamespaceHandler in handlers/namespace/create.rs (✅ COMPLETE - follow this pattern)
- **Required Pattern**: All handlers MUST use `impl TypedStatementHandler` trait
- **Data Access**: All handlers use Arc<AppContext> pattern (Phase 10 architecture)
- **Schema Access**: All handlers use SchemaRegistry for 50-100× faster schema lookups (no SQL queries)
- **DML Delegation**: DML handlers (INSERT/UPDATE/DELETE) delegate to execute_via_datafusion with ScalarValue parameters
- **Transaction Placeholders**: Transaction handlers return KalamDbError::NotImplemented until Phase 11 transaction manager
- **Job Integration**: Flush/Job handlers use Phase 9 UnifiedJobManager with typed JobIds and idempotency
- **Commit Strategy**: Commit after each logical group of tasks (per handler or per category)
- **Checkpoint Validation**: Stop at any checkpoint to validate story independently
- **Phase 8 Size**: Largest phase (70 tasks) - focus on migrations first (T072-T084), then new handlers
- **⚠️ CRITICAL**: Do NOT delete ddl_legacy.rs until ALL handler migrations (T072-T084) are complete and tested
