# Tasks: Core Architecture Refactor

**Input**: Design documents from `/specs/009-core-architecture/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: Tests are included for critical functionality validation

**Organization**: Tasks are grouped by user story (priority P1 first, then P2) to enable independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US0, US1, US2...)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Audit codebase, identify all legacy dependencies, plan migration paths

- [ ] T001 Audit all StorageAdapter references using grep in backend/ and document removal plan
- [ ] T002 Audit all KalamSql usages in backend/crates/kalamdb-core and backend/src/lifecycle.rs
- [ ] T003 [P] Audit legacy services (NamespaceService, UserTableService, SharedTableService, StreamTableService, TableDeletionService) for functionality migration
- [ ] T004 [P] Document current SqlExecutor handler structure in backend/crates/kalamdb-core/src/sql/executor/
- [ ] T005 Verify SystemTablesRegistry covers all system table operations in backend/crates/kalamdb-core/src/tables/system/registry.rs
- [ ] T006 [P] Review existing job types and confirm exhaustive list (Flush, Cleanup, Retention, StreamEviction, UserCleanup, Compact, Backup, Restore)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure changes that MUST be complete before user stories can proceed

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

### Job System Foundation

- [X] T007 Create JobId newtype with base62 generator in backend/crates/kalamdb-commons/src/ids.rs we already have JobId type defined
- [ ] T008 [P] Add short_prefix() method to JobType enum in backend/crates/kalamdb-commons/src/models/system.rs
- [ ] T009 [P] Extend JobStatus enum with New, Queued, Retrying variants in backend/crates/kalamdb-commons/src/models/system.rs
- [ ] T010 Update Job model in backend/crates/kalamdb-commons/src/models/system.rs with unified message field, exception_trace, parameters as JSON object, idempotency_key, retry_count, max_retries, queue, priority
- [ ] T011 Add JobOptions and JobFilter types in backend/crates/kalamdb-commons/src/models/system.rs
- [ ] T012 Add IdempotentConflict error variant to KalamDbError in backend/crates/kalamdb-core/src/error.rs

### Job Executor Trait Architecture

- [ ] T012a Create backend/crates/kalamdb-core/src/jobs/executors/ directory for unified job executor trait pattern
- [ ] T012b [P] Create jobs/executors/executor_trait.rs with JobExecutor trait (job_type, name, validate_params, execute, cancel methods)
- [ ] T012c [P] Add JobDecision enum in executor_trait.rs (Completed, Retry with backoff_ms, Failed with exception_trace)
- [ ] T012d [P] Add JobContext struct in executor_trait.rs (app_ctx, logger, cancellation_token, timestamp helpers)
- [ ] T012e Create jobs/executors/registry.rs with JobRegistry (DashMap<JobType, Arc<dyn JobExecutor>>)
- [ ] T012f Create jobs/executors/mod.rs to re-export trait, context, decision, and registry
- [ ] T012g Update jobs/mod.rs to add pub mod executors and re-export JobExecutor, JobDecision, JobContext, JobRegistry

### ExecutionContext Consolidation

- [ ] T013 Create unified ExecutionContext struct in backend/crates/kalamdb-core/src/sql/executor/handlers/types.rs with user_id, user_role, namespace_id, request_id, ip_address, timestamp
- [ ] T014 Remove or deprecate KalamSessionState (verify no remaining usages after ExecutionContext migration)
- [ ] T015 Update SqlExecutor::execute_with_metadata signature to accept ExecutionContext in backend/crates/kalamdb-core/src/sql/executor/mod.rs

### Handler Infrastructure

- [ ] T016 Create StatementHandler trait in backend/crates/kalamdb-core/src/sql/executor/handlers/mod.rs
- [ ] T017 [P] Create helpers.rs with common utilities (namespace resolution, table lookup, error formatting) in backend/crates/kalamdb-core/src/sql/executor/handlers/
- [ ] T018 [P] Create audit.rs with audit logging utilities in backend/crates/kalamdb-core/src/sql/executor/handlers/
- [ ] T019 Update authorization.rs with check_authorization gateway function in backend/crates/kalamdb-core/src/sql/executor/handlers/

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 0 - Codebase Cleanup and Module Consolidation (Priority: P1)

**Goal**: Colocate related modules, remove redundant layers, improve codebase navigability

**Independent Test**: Grep for old import paths returns 0 matches; all tests pass; re-exports maintain backward compatibility

### Module Consolidation

- [ ] T020 [P] [US0] Move backend/crates/kalamdb-core/src/stores/system_table.rs to backend/crates/kalamdb-core/src/tables/system_table_store.rs
- [ ] T021 [US0] Add re-export in backend/crates/kalamdb-core/src/stores/mod.rs for backward compatibility
- [ ] T022 [P] [US0] Expose SchemaRegistry from catalog in backend/crates/kalamdb-core/src/catalog/mod.rs (merge schema and catalog public surface)
- [ ] T023 [P] [US0] Move table models from backend/crates/kalamdb-core/src/models/tables.rs into respective backend/crates/kalamdb-core/src/tables/* modules (user_tables/, shared_tables/, stream_tables/)
- [ ] T024 [US0] Add transitional re-exports in backend/crates/kalamdb-core/src/models/tables.rs for backward compatibility
- [ ] T025 [US0] Audit backend/crates/kalamdb-core/src/storage/ module for dead code and document any needed relocations
- [ ] T026 [US0] Run full workspace tests to verify no regressions from module moves (cargo test)

### Documentation Updates

- [ ] T027 [P] [US0] Update import paths in AGENTS.md to reflect new module locations
- [ ] T028 [P] [US0] Document re-export strategy for backward compatibility in specs/009-core-architecture/research.md

**Checkpoint**: Module structure cleaned up, all imports work via re-exports, tests pass

---

## Phase 4: User Story 0a - Remove Legacy Information Schema Providers (Priority: P1)

**Goal**: Delete obsolete information_schema provider files, ensure consolidated registry works

**Independent Test**: Grep for information_schema_*.rs returns 0 source files; queries against information_schema.tables and information_schema.columns succeed

- [ ] T029 [P] [US0a] Delete backend/crates/kalamdb-core/src/tables/system/information_schema_columns.rs
- [ ] T030 [P] [US0a] Delete backend/crates/kalamdb-core/src/tables/system/information_schema_tables.rs
- [ ] T031 [US0a] Remove references to deleted files from backend/crates/kalamdb-core/src/tables/system/mod.rs
- [ ] T032 [US0a] Verify information_schema tables are registered via SystemTablesRegistry in backend/crates/kalamdb-core/src/system_table_registration.rs
- [ ] T033 [US0a] Run information_schema queries in tests to validate consolidated providers work (SELECT * FROM information_schema.tables, SELECT * FROM information_schema.columns)
- [ ] T034 [US0a] Verify no imports reference deleted files (grep for information_schema_columns|information_schema_tables in backend/)

**Checkpoint**: Legacy information_schema providers removed, consolidated providers working

---

## Phase 5: User Story 1 - Providers-Only Data Access (Priority: P1) 🎯 MVP Component 1

**Goal**: Eliminate StorageAdapter, route all CRUD through table-specific providers

**Independent Test**: Grep for StorageAdapter returns 0 matches; all existing provider tests pass

### Remove StorageAdapter

- [ ] T035 [P] [US1] Document all StorageAdapter usages and their provider replacements in specs/009-core-architecture/research.md
- [ ] T036 [US1] Replace StorageAdapter references in backend/src/lifecycle.rs with direct provider usage via AppContext
- [ ] T037 [P] [US1] Update backend/crates/kalamdb-auth to use providers instead of RocksDbAdapter (if any StorageAdapter usage exists)
- [ ] T038 [US1] Delete StorageAdapter type/trait definitions from codebase
- [ ] T039 [US1] Run grep to verify zero StorageAdapter references remain (grep -r "StorageAdapter" backend/)
- [ ] T040 [US1] Run full workspace build to confirm compilation success (cargo build)

### Provider Validation Tests

- [ ] T041 [P] [US1] Run all system table provider tests in backend/crates/kalamdb-core/src/tables/system/*/tests/
- [ ] T042 [P] [US1] Run all user/shared/stream table tests in backend/tests/
- [ ] T043 [US1] Verify no behavior regressions in CRUD operations (run integration tests)

**Checkpoint**: StorageAdapter fully removed, all data access via providers, tests green

---

## Phase 6: User Story 2 - Remove KalamSql Dependency (Priority: P1) 🎯 MVP Component 2

**Goal**: Eliminate KalamSql from execution path, route operations through providers/registries

**Independent Test**: Grep for kalamdb_sql imports returns 0 matches in core execution paths; all tests pass

### Remove KalamSql from Lifecycle

- [ ] T044 [US2] Remove kalamdb_sql::KalamSql import from backend/src/lifecycle.rs
- [ ] T045 [US2] Remove KalamSql initialization in backend/src/lifecycle.rs (lines 115-118 approx)
- [ ] T046 [US2] Replace KalamSql usages with AppContext + SchemaRegistry + SystemTablesRegistry in lifecycle
- [ ] T047 [US2] Update lifecycle wiring to use providers exclusively for system table setup

### Remove KalamSql from DDL Handlers

- [ ] T048 [US2] Remove kalamdb_sql imports from backend/crates/kalamdb-core/src/sql/executor/handlers/ddl.rs
- [ ] T049 [US2] Replace KalamSql table lookups with SchemaRegistry.get_table_metadata() in ddl.rs
- [ ] T050 [US2] Replace KalamSql namespace operations with NamespaceProvider (via SystemTablesRegistry) in ddl.rs
- [ ] T051 [US2] Replace KalamSql storage operations with StorageProvider (via SystemTablesRegistry) in ddl.rs
- [ ] T052 [US2] Update execute_create_namespace to use SystemTablesRegistry.namespaces() provider
- [ ] T053 [US2] Update execute_drop_namespace to use SystemTablesRegistry.namespaces() provider
- [ ] T054 [US2] Update execute_create_storage to use SystemTablesRegistry.storages() provider
- [ ] T055 [US2] Update execute_create_table to use SchemaRegistry + appropriate table store (user/shared/stream)
- [ ] T056 [US2] Update execute_alter_table to use SchemaRegistry for lookups and invalidation
- [ ] T057 [US2] Update execute_drop_table to use SchemaRegistry for lookups and invalidation

### Remove KalamSql from Tests

- [ ] T058 [US2] Update backend/crates/kalamdb-core/src/sql/executor/handlers/tests/ddl_tests.rs to use providers instead of KalamSql
- [ ] T059 [P] [US2] Update backend/tests/ integration tests to remove KalamSql imports and use AppContext
- [ ] T060 [US2] Remove kalamdb_sql from test helper creation in backend/crates/kalamdb-core/src/test_helpers.rs

### Verify Removal

- [ ] T061 [US2] Run grep to verify zero kalamdb_sql imports in backend/crates/kalamdb-core (grep -r "use kalamdb_sql" backend/crates/kalamdb-core/)
- [ ] T062 [US2] Run grep to verify zero kalamdb_sql imports in backend/src/ (grep -r "use kalamdb_sql" backend/src/)
- [ ] T063 [US2] Run full workspace build to confirm compilation success (cargo build)
- [ ] T064 [US2] Run all kalamdb-core tests (cargo test -p kalamdb-core)
- [ ] T065 [US2] Run all backend integration tests (cargo test --test '*' in backend/)

**Checkpoint**: KalamSql completely removed from execution path, all operations via providers, tests green

---

## Phase 7: User Story 3 - Handler-based SqlExecutor using AppContext (Priority: P1) 🎯 MVP Component 3

**Goal**: Compose SqlExecutor from focused handlers using SchemaRegistry and AppContext

**Independent Test**: All DDL/DML operations route through handlers; existing tests pass; smoke tests confirm behavior

### Create Missing Handlers

- [ ] T066 [P] [US3] Create dml.rs handler in backend/crates/kalamdb-core/src/sql/executor/handlers/ with execute_insert, execute_update, execute_delete
- [ ] T067 [P] [US3] Create query.rs handler in backend/crates/kalamdb-core/src/sql/executor/handlers/ with execute_select, execute_describe, execute_show
- [ ] T068 [P] [US3] Create flush.rs handler in backend/crates/kalamdb-core/src/sql/executor/handlers/ with execute_flush
- [ ] T069 [P] [US3] Create subscription.rs handler in backend/crates/kalamdb-core/src/sql/executor/handlers/ with execute_live_select
- [ ] T070 [P] [US3] Create user_management.rs handler in backend/crates/kalamdb-core/src/sql/executor/handlers/ with execute_create_user, execute_alter_user, execute_drop_user
- [ ] T071 [P] [US3] Create table_registry.rs handler in backend/crates/kalamdb-core/src/sql/executor/handlers/ with execute_register_table, execute_unregister_table
- [ ] T072 [P] [US3] Create system_commands.rs handler in backend/crates/kalamdb-core/src/sql/executor/handlers/ with execute_vacuum, execute_optimize, execute_analyze

### Refactor SqlExecutor Routing

- [ ] T073 [US3] Update SqlExecutor::execute_with_metadata to use single-pass parsing via kalamdb_sql::parse() in backend/crates/kalamdb-core/src/sql/executor/mod.rs
- [ ] T074 [US3] Add authorization gateway (check_authorization) before routing in execute_with_metadata
- [ ] T075 [US3] Implement handler routing based on SqlStatement variant in execute_with_metadata
- [ ] T076 [US3] Route CREATE TABLE to ddl::execute_create_table
- [ ] T077 [US3] Route ALTER TABLE to ddl::execute_alter_table
- [ ] T078 [US3] Route DROP TABLE to ddl::execute_drop_table
- [ ] T079 [US3] Route CREATE NAMESPACE to ddl::execute_create_namespace
- [ ] T080 [US3] Route DROP NAMESPACE to ddl::execute_drop_namespace
- [ ] T081 [US3] Route CREATE STORAGE to ddl::execute_create_storage
- [ ] T082 [US3] Route SELECT to query::execute_select
- [ ] T083 [US3] Route INSERT to dml::execute_insert with parameter binding support
- [ ] T084 [US3] Route UPDATE to dml::execute_update with parameter binding support
- [ ] T085 [US3] Route DELETE to dml::execute_delete with parameter binding support
- [ ] T086 [US3] Route FLUSH TABLE to flush::execute_flush
- [ ] T087 [US3] Route LIVE SELECT to subscription::execute_live_select
- [ ] T088 [US3] Route CREATE/ALTER/DROP USER to user_management handlers
- [ ] T089 [US3] Route REGISTER/UNREGISTER TABLE to table_registry handlers
- [ ] T090 [US3] Route VACUUM/OPTIMIZE/ANALYZE to system_commands handlers
- [ ] T091 [US3] Add audit logging after handler execution in execute_with_metadata

### Handler Implementation Pattern

- [ ] T092 [US3] Ensure all DML handlers use UserTableStore/SharedTableStore/StreamTableStore via AppContext
- [ ] T093 [US3] Ensure all DDL handlers use SchemaRegistry for schema reads and cache invalidation
- [ ] T094 [US3] Ensure all system table handlers use SystemTablesRegistry providers
- [ ] T095 [US3] Ensure all handlers apply authorization checks via ExecutionContext
- [ ] T096 [US3] Ensure all handlers perform namespace extraction with fallback logic

### Extract Common Code

- [ ] T097 [US3] Extract repeated namespace resolution logic to helpers.rs
- [ ] T098 [US3] Extract repeated table metadata lookups to helpers.rs (using SchemaRegistry)
- [ ] T099 [US3] Extract common authorization patterns to authorization.rs
- [ ] T100 [US3] Extract audit log entry creation to audit.rs
- [ ] T101 [US3] Extract parameter value conversion utilities to helpers.rs

### Testing

- [ ] T102 [P] [US3] Create smoke test for CREATE/ALTER/DROP TABLE via handlers in backend/tests/test_ddl_handlers.rs
- [ ] T103 [P] [US3] Create smoke test for SELECT/INSERT/UPDATE/DELETE via handlers in backend/tests/test_dml_handlers.rs
- [ ] T104 [P] [US3] Create smoke test for system table operations via handlers in backend/tests/test_system_handlers.rs
- [ ] T105 [US3] Run all existing DDL/DML tests to verify behavior parity
- [ ] T106 [US3] Run full workspace tests (cargo test)

**Checkpoint**: SqlExecutor fully handler-based, all operations route through AppContext, tests green

---

## Phase 8: User Story 4 - Deprecate Legacy Services (Priority: P2)

**Goal**: Remove obsolete service layers (NamespaceService, UserTableService, SharedTableService, StreamTableService, TableDeletionService)

**Independent Test**: Grep for legacy service names returns 0 matches; all tests pass

### Remove Legacy Services

- [ ] T107 [P] [US4] Delete backend/crates/kalamdb-core/src/services/namespace_service.rs
- [ ] T108 [P] [US4] Delete backend/crates/kalamdb-core/src/services/user_table_service.rs
- [ ] T109 [P] [US4] Delete backend/crates/kalamdb-core/src/services/shared_table_service.rs
- [ ] T110 [P] [US4] Delete backend/crates/kalamdb-core/src/services/stream_table_service.rs
- [ ] T111 [P] [US4] Delete backend/crates/kalamdb-core/src/services/table_deletion_service.rs
- [ ] T112 [US4] Remove service imports from backend/crates/kalamdb-core/src/services/mod.rs
- [ ] T113 [US4] Remove service initialization from backend/src/lifecycle.rs (lines 118-122 approx)
- [ ] T114 [US4] Remove service references from backend/crates/kalamdb-core/src/sql/executor/handlers/tests/ddl_tests.rs

### Keep Valid Services

- [ ] T115 [US4] Verify backup_service.rs, restore_service.rs, schema_evolution_service.rs remain (these are still needed)
- [ ] T116 [US4] Update backend/crates/kalamdb-core/src/services/mod.rs to export only retained services

### Validation

- [ ] T117 [US4] Run grep to verify zero legacy service references (grep -r "NamespaceService|UserTableService|SharedTableService|StreamTableService|TableDeletionService" backend/)
- [ ] T118 [US4] Run full workspace build (cargo build)
- [ ] T119 [US4] Run all tests (cargo test)

**Checkpoint**: Legacy services removed, build green, tests passing

---

## Phase 9: User Story 6 - Unified Job Management System (Priority: P1) 🎯 MVP Component 4

**Goal**: Single JobManager with typed JobIds, richer statuses, idempotency, retry/backoff, dedicated logging

**Independent Test**: Create jobs of all types; verify status transitions; test idempotency and retry logic; check jobs.log

### JobManager Implementation

- [ ] T120 [US6] Create unified JobManager struct in backend/crates/kalamdb-core/src/jobs/unified_manager.rs
- [ ] T121 [US6] Implement create_job(job_type, params, idempotency_key, options) -> Result<JobId>
- [ ] T122 [US6] Implement cancel_job(job_id) -> Result<()>
- [ ] T123 [US6] Implement get_job(job_id) -> Result<Job>
- [ ] T124 [US6] Implement list_jobs(filter) -> Result<Vec<Job>>
- [ ] T125 [US6] Implement run_loop(max_concurrent) -> Result<()> with backoff and idempotency enforcement
- [ ] T126 [US6] Implement poll_next() -> Result<Option<Job>> with idempotency check
- [ ] T127 [US6] Add JobId generation with prefix mapping (FL, CL, RT, SE, UC, CO, BK, RS)
- [ ] T128 [US6] Implement idempotency key enforcement (check for active jobs with same key)

### Job State Transitions

- [ ] T129 [US6] Implement job.new() -> status=New in JobManager
- [ ] T130 [US6] Implement job.queue() -> status=Queued transition
- [ ] T131 [US6] Implement job.start() -> status=Running transition with started_at timestamp
- [ ] T132 [US6] Implement job.complete(message) -> status=Completed transition with finished_at, clear exception_trace
- [ ] T133 [US6] Implement job.fail(message, exception_trace) -> status=Failed or Retrying based on can_retry()
- [ ] T134 [US6] Implement job.retry(message, exception_trace) -> increment retry_count, status=Retrying
- [ ] T135 [US6] Implement job.cancel() -> status=Cancelled transition with cleanup
- [ ] T136 [US6] Implement job.can_retry() -> bool (retry_count < max_retries)

### Jobs Logging

- [ ] T137 [US6] Create jobs.log file initialization in backend/src/lifecycle.rs
- [ ] T138 [US6] Add log_job_event(job_id, level, message) utility in JobManager
- [ ] T139 [US6] Prefix all job log lines with [JobId] format
- [ ] T140 [US6] Log job queued event
- [ ] T141 [US6] Log job started event
- [ ] T142 [US6] Log job retry event with attempt count and delay
- [ ] T143 [US6] Log job completed event with result summary
- [ ] T144 [US6] Log job failed event with error
- [ ] T145 [US6] Log job cancelled event

### Job Executors Integration (Trait-based Architecture)

- [ ] T146 [P] [US6] Create jobs/executors/flush.rs implementing JobExecutor trait for flush operations
- [ ] T147 [P] [US6] Create jobs/executors/cleanup.rs implementing JobExecutor trait for cleanup operations
- [ ] T148 [P] [US6] Create jobs/executors/retention.rs implementing JobExecutor trait for retention operations
- [ ] T149 [P] [US6] Create jobs/executors/stream_eviction.rs implementing JobExecutor trait for stream eviction
- [ ] T150 [P] [US6] Create jobs/executors/user_cleanup.rs implementing JobExecutor trait for user cleanup
- [ ] T151 [P] [US6] Create jobs/executors/compact.rs implementing JobExecutor trait for compaction (placeholder)
- [ ] T152 [P] [US6] Create jobs/executors/backup.rs implementing JobExecutor trait for backup (placeholder)
- [ ] T153 [P] [US6] Create jobs/executors/restore.rs implementing JobExecutor trait for restore (placeholder)
- [ ] T154 [US6] Register all 8 executors in JobRegistry during lifecycle initialization
- [ ] T155 [US6] Update JobManager to dispatch via registry.get(job.job_type).execute(ctx, &job)
- [ ] T156 [US6] Ensure all executors use JobContext for logging (auto-prefixed with [JobId])
- [ ] T157 [US6] Ensure all executors return JobDecision (Completed/Retry/Failed) with appropriate messages

### Crash Recovery

- [ ] T158 [US6] Implement startup recovery in JobManager to mark incomplete jobs (Running status) as Failed with "Server restarted" error
- [ ] T159 [US6] Add recovery logging for restarted jobs
- [ ] T160 [US6] Test crash recovery scenario (stop server mid-job, restart, verify job marked Failed)

### Replace Legacy Job Management

- [ ] T161 [US6] Replace job_manager.rs with unified_manager.rs references in backend/crates/kalamdb-core/src/jobs/mod.rs
- [ ] T162 [US6] Replace tokio_job_manager.rs with unified JobManager integration
- [ ] T163 [US6] Update lifecycle.rs to initialize unified JobManager with JobRegistry instead of old managers
- [ ] T164 [US6] Remove or deprecate old job_manager.rs and tokio_job_manager.rs
- [ ] T165 [US6] Migrate existing executor.rs, retention.rs, stream_eviction.rs, user_cleanup.rs logic into new trait-based executors

### Testing - Status Transitions (Acceptance Scenarios 1-10)

- [ ] T166 [P] [US6] Test job creation with status=New and FL- prefix in backend/tests/test_job_manager.rs (AS1)
- [ ] T167 [P] [US6] Test job transitions New → Queued → Running with logging (AS2)
- [ ] T168 [P] [US6] Test job completion with status=Completed and result storage (AS3)
- [ ] T169 [P] [US6] Test job retry on transient failure with status=Retrying (AS4)
- [ ] T170 [P] [US6] Test job permanent failure after max retries with status=Failed (AS5)
- [ ] T171 [P] [US6] Test crash recovery marks Running jobs as Failed (AS6)
- [ ] T172 [P] [US6] Test all job types have correct prefixes (FL, CL, RT, SE, UC, CO, BK, RS) (AS7)
- [ ] T173 [P] [US6] Test jobs.log filtering by JobId prefix (AS8)
- [ ] T174 [P] [US6] Test concurrent job execution with max_concurrent_jobs limit (AS9)
- [ ] T175 [P] [US6] Test job cancellation with Cancelled status and cleanup (AS10)

### Testing - Idempotency (Acceptance Scenarios 11-16)

- [ ] T176 [P] [US6] Test job creation with idempotency key succeeds (AS11)
- [ ] T177 [P] [US6] Test duplicate job creation with Running status fails with IdempotentConflict (AS12)
- [ ] T178 [P] [US6] Test duplicate job creation after Completed status succeeds (AS13)
- [ ] T179 [P] [US6] Test duplicate job creation with Retrying status fails (AS14)
- [ ] T180 [P] [US6] Test hourly cleanup idempotency prevents duplicates (AS15)
- [ ] T181 [P] [US6] Test jobs without idempotency key allow duplicates (AS16)

### Testing - Message and Exception Trace (Acceptance Scenarios 17-25)

- [ ] T182 [P] [US6] Test successful job stores message and clears exception_trace (AS17, AS22)
- [ ] T183 [P] [US6] Test failed job stores error in message field (AS18)
- [ ] T184 [P] [US6] Test message field replaces old result field (AS19, AS20)
- [ ] T185 [P] [US6] Test exception_trace stores full stack trace on panic (AS21)
- [ ] T186 [P] [US6] Test long stack traces stored in exception_trace (AS23)
- [ ] T187 [P] [US6] Test retry updates both message and exception_trace (AS24)
- [ ] T188 [P] [US6] Test querying failed jobs returns message and exception_trace (AS25)

### Testing - Retry Logic (Acceptance Scenarios 26-30)

- [ ] T189 [P] [US6] Test job creation sets retry_count=0 and max_retries=3 (AS26)
- [ ] T190 [P] [US6] Test first failure increments retry_count to 1 (AS27)
- [ ] T191 [P] [US6] Test third retry attempt allowed (retry_count=2 < max_retries=3) (AS28)
- [ ] T192 [P] [US6] Test permanent failure when retry_count >= max_retries (AS29)
- [ ] T193 [P] [US6] Test custom max_retries configuration (AS30)

### Testing - Parameters Format (Acceptance Scenarios 31-33)

- [ ] T194 [P] [US6] Test job parameters stored as JSON object (AS31)
- [ ] T195 [P] [US6] Test executor parses parameters by key (AS32)
- [ ] T196 [P] [US6] Test legacy array parameters migrated to object format (AS33)

**Checkpoint**: Unified JobManager operational, all acceptance scenarios passing, jobs.log working

---

## Phase 10: User Story 5 - Build Green and Tests Passing (Priority: P1) 🎯 Quality Gate

**Goal**: Ensure entire workspace compiles and all tests pass after refactor

**Independent Test**: cargo build succeeds; cargo test shows 100% pass rate

### Workspace Compilation

- [ ] T197 [US5] Run cargo build on entire workspace
- [ ] T198 [US5] Fix any compilation errors in backend crates
- [ ] T199 [US5] Fix any compilation errors in CLI crate
- [ ] T200 [US5] Fix any compilation errors in link SDK
- [ ] T201 [US5] Verify zero warnings in workspace build

### Backend Tests

- [ ] T202 [US5] Run cargo test -p kalamdb-core and verify 100% pass
- [ ] T203 [US5] Run cargo test -p kalamdb-commons and verify 100% pass
- [ ] T204 [US5] Run cargo test -p kalamdb-store and verify 100% pass
- [ ] T205 [US5] Run cargo test -p kalamdb-sql and verify 100% pass (if kept)
- [ ] T206 [US5] Run cargo test -p kalamdb-auth and verify 100% pass
- [ ] T207 [US5] Run cargo test -p kalamdb-api and verify 100% pass
- [ ] T208 [US5] Run cargo test -p kalamdb-live and verify 100% pass

### Backend Integration Tests

- [ ] T209 [US5] Run all integration tests in backend/tests/ (cargo test --test '*')
- [ ] T210 [US5] Fix any failing integration tests
- [ ] T211 [US5] Verify DDL operations work correctly (CREATE/ALTER/DROP TABLE, CREATE/DROP NAMESPACE, CREATE STORAGE)
- [ ] T212 [US5] Verify DML operations work correctly (INSERT, UPDATE, DELETE, SELECT)
- [ ] T213 [US5] Verify system table operations work (users, jobs, namespaces, storages, live_queries, tables, information_schema)
- [ ] T214 [US5] Verify flush operations work correctly
- [ ] T215 [US5] Verify schema cache invalidation works on ALTER/DROP

### CLI Tests

- [ ] T216 [US5] Run cargo test in cli/ crate
- [ ] T217 [US5] Fix any failing CLI tests
- [ ] T218 [US5] Verify CLI commands work with refactored backend

### Link SDK Tests

- [ ] T219 [US5] Run cargo test in link/ crate
- [ ] T220 [US5] Fix any failing link tests
- [ ] T221 [US5] Run TypeScript SDK tests in link/sdks/typescript/
- [ ] T222 [US5] Verify SDK works with refactored backend API

### Performance Validation

- [ ] T223 [US5] Run performance benchmarks to verify no >10% regressions
- [ ] T224 [US5] Measure schema lookup latency (target: <2μs via SchemaRegistry)
- [ ] T225 [US5] Measure DDL operation latency baseline
- [ ] T226 [US5] Measure DML operation latency baseline

**Checkpoint**: All tests passing, build green, performance within targets

---

## Phase 11: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, final cleanup, validation

### Documentation

- [ ] T227 [P] Update AGENTS.md with refactor completion details
- [ ] T228 [P] Document handler architecture in docs/architecture/SQL_EXECUTOR_HANDLERS.md
- [ ] T229 [P] Document unified job system with trait architecture in docs/architecture/JOB_MANAGEMENT.md
- [ ] T230 [P] Update API documentation for job endpoints
- [ ] T231 [P] Update CLI documentation for job commands
- [ ] T232 [P] Create migration guide for any breaking internal API changes

### Code Cleanup

- [ ] T233 Perform final grep audits for forbidden symbols (StorageAdapter, kalamdb_sql, legacy services)
- [ ] T234 Remove any remaining dead code or unused imports
- [ ] T235 Run cargo clippy on workspace and fix any warnings
- [ ] T236 Run cargo fmt on workspace to enforce consistent formatting
- [ ] T237 Review and clean up any TODO/FIXME comments added during refactor

### Validation

- [ ] T238 Run quickstart.md scenarios to validate end-to-end functionality
- [ ] T239 Perform smoke tests on all major features (DDL, DML, jobs, subscriptions)
- [ ] T240 Verify backward compatibility at API level (no breaking changes for users)
- [ ] T241 Update specs/009-core-architecture/research.md with final findings and decisions
- [ ] T242 Mark feature as complete in AGENTS.md recent changes section

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phase 3-9)**: All depend on Foundational phase completion
  - US0, US0a, US1, US2, US3 are P1 priority (MVP critical path)
  - US4 is P2 (can be done after MVP if needed)
  - US6 is P1 but can run parallel to US1-US3 (different subsystem)
  - US5 is continuous validation (run after each phase)
- **Polish (Phase 11)**: Depends on all user stories being complete

### Critical Path (MVP)

1. **Phase 1**: Setup (audit and planning)
2. **Phase 2**: Foundational (ExecutionContext, JobId, handlers infrastructure)
3. **Phase 3**: US0 (module consolidation)
4. **Phase 4**: US0a (remove legacy info schema providers)
5. **Phase 5**: US1 (remove StorageAdapter)
6. **Phase 6**: US2 (remove KalamSql) ← **High Risk, Test Heavily**
7. **Phase 7**: US3 (handler-based SqlExecutor) ← **Core Architecture Change**
8. **Phase 9**: US6 (unified job management) ← **Can run parallel to US1-US3**
9. **Phase 10**: US5 (build green validation) ← **Quality Gate**
10. **Phase 8**: US4 (remove legacy services) ← **Lower risk, can defer**
11. **Phase 11**: Polish (documentation and cleanup)

### Parallel Opportunities

**Setup Phase (Phase 1)**:
- T001-T006 can all run in parallel (independent audits)

**Foundational Phase (Phase 2)**:
- T007-T012 (Job System Foundation) can run parallel
- T016-T019 (Handler Infrastructure) can run parallel
- Job system and handler work are independent

**US0 (Phase 3)**:
- T020, T022, T023, T025 (module moves) can run parallel
- T027-T028 (docs) can run parallel

**US0a (Phase 4)**:
- T029-T030 (file deletions) can run parallel

**US1 (Phase 5)**:
- T035, T037 (provider migration docs) can run parallel
- T041-T042 (test runs) can run parallel

**US3 (Phase 7)**:
- T066-T072 (create handlers) can all run parallel (different files)
- T097-T101 (extract common code) can run parallel

**US4 (Phase 8)**:
- T107-T111 (delete services) can run parallel (different files)

**US6 (Phase 9)**:
- Most test tasks (T159-T189) can run in parallel
- Job executor updates (T146-T151) can run parallel

**US5 (Phase 10)**:
- T196-T201 (individual crate tests) can run parallel
- T220-T225 (documentation) can run parallel in Phase 11

### Within Each User Story

- Complete infrastructure/foundation tasks before implementation
- Run tests after each major change to catch regressions early
- US5 (build green) should be run after EVERY other phase completes
- Commit frequently to avoid losing work

---

## Parallel Example: Foundational Phase

```bash
# Launch Job System Foundation in parallel:
Task T007: "Create JobId newtype with base62 generator"
Task T008: "Add short_prefix() method to JobType enum"
Task T009: "Extend JobStatus enum with New, Queued, Retrying"

# Launch Handler Infrastructure in parallel:
Task T016: "Create StatementHandler trait"
Task T017: "Create helpers.rs with common utilities"
Task T018: "Create audit.rs with audit logging"
```

## Parallel Example: User Story 3

```bash
# Launch all handler file creations in parallel:
Task T066: "Create dml.rs handler"
Task T067: "Create query.rs handler"
Task T068: "Create flush.rs handler"
Task T069: "Create subscription.rs handler"
Task T070: "Create user_management.rs handler"
Task T071: "Create table_registry.rs handler"
Task T072: "Create system_commands.rs handler"
```

---

## Implementation Strategy

### MVP First (Critical Path)

1. Complete Phase 1: Setup (audit)
2. Complete Phase 2: Foundational (CRITICAL - blocks everything)
3. Complete Phase 3-4: US0, US0a (module cleanup)
4. Complete Phase 5-7: US1, US2, US3 (core refactor - most risky)
5. Complete Phase 9: US6 (job system - independent subsystem)
6. Complete Phase 10: US5 (validate everything works)
7. **STOP and VALIDATE**: Run full test suite, performance tests
8. Deploy/demo if ready

### Incremental Delivery

1. **Milestone 1**: Foundation ready (Phase 1-2)
2. **Milestone 2**: Modules consolidated (Phase 3-4) → Test independently
3. **Milestone 3**: StorageAdapter removed (Phase 5) → Test independently
4. **Milestone 4**: KalamSql removed (Phase 6) → **CRITICAL TEST GATE**
5. **Milestone 5**: Handlers implemented (Phase 7) → **CRITICAL TEST GATE**
6. **Milestone 6**: Job system unified (Phase 9) → Test independently
7. **Milestone 7**: Build green (Phase 10) → **FINAL QUALITY GATE**
8. **Milestone 8**: Legacy services removed (Phase 8) → Test independently
9. **Milestone 9**: Documentation complete (Phase 11) → Ready for merge

### Risk Mitigation Strategy

**High Risk Areas**:
- Phase 6 (US2): Removing KalamSql - **TEST HEAVILY AFTER EACH CHANGE**
- Phase 7 (US3): Handler-based routing - **VALIDATE EVERY STATEMENT TYPE**
- Phase 10 (US5): Build green gate - **BLOCK MERGE IF ANY FAILURES**

**Mitigation**:
- Run tests after EVERY task in Phase 6 and 7
- Create backup branches before high-risk changes
- Keep changes small and focused
- Commit after each successful task
- Use git bisect if regressions appear

**Rollback Plan**:
- Each phase is independently committable
- Can roll back to any milestone if issues arise
- US4 (legacy services) can be deferred if schedule pressure

---

## Notes

- [P] tasks = different files, no dependencies, can run in parallel
- [Story] label maps task to specific user story for traceability
- Each user story should be independently completable and testable
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- US5 (build green) is not a traditional "story" but a continuous validation gate
- Run US5 validation after EVERY other phase completes
- Do NOT proceed past Phase 7 (US3) if build is not green
- Phase 6 and 7 are highest risk - test extensively

---

## Total Task Count

- **Setup**: 6 tasks
- **Foundational**: 20 tasks (CRITICAL - blocks all stories) ← **+7 tasks for JobExecutor trait architecture**
- **US0 (Module Consolidation)**: 9 tasks (P1)
- **US0a (Remove Legacy Info Schema)**: 6 tasks (P1)
- **US1 (Providers-Only)**: 9 tasks (P1)
- **US2 (Remove KalamSql)**: 22 tasks (P1)
- **US3 (Handler-based SqlExecutor)**: 41 tasks (P1)
- **US4 (Deprecate Legacy Services)**: 13 tasks (P2)
- **US6 (Unified Job Management)**: 77 tasks (P1) ← **Highest Task Count** (+7 for trait-based executors)
- **US5 (Build Green)**: 30 tasks (P1 - Quality Gate)
- **Polish**: 16 tasks

**Total: 249 tasks** (+14 tasks from original 235)

**Critical Path (MVP)**: ~174 tasks (Phases 1-7, 9-10)
**Optional/Deferrable**: ~59 tasks (Phase 8 legacy services, some polish)

**Parallel Opportunities**: ~100 tasks marked [P] across all phases (+10 from trait-based architecture)

**Estimated Effort**:
- High complexity: Phase 6 (US2), Phase 7 (US3), Phase 9 (US6)
- Medium complexity: Phase 3-5, Phase 10
- Low complexity: Phase 1, Phase 8, Phase 11

**Suggested MVP Scope**: Phases 1-7, 9-10 (US0, US0a, US1, US2, US3, US6, US5)
