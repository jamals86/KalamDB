# Feature Specification: Core Architecture Refactor

**Feature Branch**: `009-core-architecture`  
**Created**: November 4, 2025  
**Status**: Draft  
**Input**: User description: "Core architecture refactor to providers-only, remove KalamSql, AppContext-driven SqlExecutor, and service deprecations"

## Overview

Streamline the core by removing legacy adapters and centralizing all data access through table-specific providers and the unified SchemaRegistry. The SQL execution layer will depend only on AppContext and dispatch operations to the correct providers/registries. This reduces duplication, improves performance, and simplifies maintenance.

## Goals

- Single, consistent source for data access: providers and registries
- Eliminate legacy indirection layers (StorageAdapter, KalamSql)
- Make SqlExecutor handler-based, using only AppContext dependencies
- Maintain full backward-compatible behavior at the API level
- Keep build green with all existing tests passing

## Non-Goals

- Changing SQL syntax or user-facing API semantics
- Altering on-disk formats beyond necessary refactors
- Introducing new table types or auth models

## Actors

- Developer: implements, tests, and debugs core changes
- Operator: runs smoke tests to validate server health

## User Scenarios & Testing (mandatory)

### User Story 0 - Codebase Cleanup and Module Consolidation (Priority: P1)

As a maintainer, I want related modules colocated and redundant layers removed so the codebase is easier to navigate and reason about.

Independent Test:
- Move `backend/crates/kalamdb-core/src/stores/system_table.rs` under `tables/` and keep compatibility via re-exports.
- Expose schema utilities and registry from `catalog` (merge surface area) so both `schema` and `catalog` provide the same public API.
- Relocate models from `models/tables.rs` into `tables/*` modules (user/shared/stream) with transitional re-exports.
- Audit `storage/` for dead code and plan relocations; avoid functional regressions.

Acceptance Scenarios:
1. Given the repository, when searching for `stores/system_table.rs`, then the implementation lives under `tables/` and `stores::system_table::*` still resolves via re-export.
2. Given code importing from `catalog`, when using schema helpers and `SchemaRegistry`, then items are available via `catalog::*` without import churn.
3. Given `models/tables.rs`, when building, then types are accessible from the `tables` namespace and legacy imports still compile via re-exports.
4. Given the `storage/` module, when running tests, then no behavior regressions occur and any removals/moves are low-risk and documented.

### User Story 0a - Remove legacy information_schema providers (Priority: P1)

As a maintainer, I want to delete obsolete `information_schema_*` provider files under system tables so we only keep the consolidated registry-backed implementation.

Scope:
- Remove files matching `backend/crates/kalamdb-core/src/tables/system/information_schema_*`
- Ensure information_schema tables are still available via the consolidated SystemTablesRegistry

Independent Test:
- Grep the repository for `tables/system/information_schema_` and confirm no source files remain
- Run existing information_schema-related tests/queries; they must pass using the consolidated providers
- Verify no imports reference the deleted files

Acceptance Scenarios:
1. Given the repository, when searching for `backend/crates/kalamdb-core/src/tables/system/information_schema_*`, then no such files exist (deleted from source control)
2. Given DataFusion session registration, when listing schemas, then `information_schema` is present and tables resolve via the consolidated registry
3. Given queries against `information_schema.tables` and `information_schema.columns`, when executed, then results are returned successfully without referencing legacy provider files

### User Story 1 - Providers-Only Data Access (Priority: P1)

Developers need a single, predictable place to read/write data for each table type. All core read/write paths use the appropriate provider; StorageAdapter is gone.

**Why this priority**: Reduces complexity and eliminates duplicated logic across layers.

**Independent Test**: Replace all StorageAdapter usages with providers, run existing tests; verify no behavior change.

**Acceptance Scenarios**:

1. Given a system/user/shared/stream table, When code performs CRUD, Then the operation goes through that table’s provider exclusively.
2. Given removal of StorageAdapter, When building the workspace, Then no references to StorageAdapter remain and compilation succeeds.

---

### User Story 2 - Remove KalamSql dependency (Priority: P1)

All prior functionality previously surfaced via KalamSql is accessible via providers/registries. SqlExecutor and services no longer depend on KalamSql.

**Why this priority**: Eliminates indirection and tight coupling; simplifies dependency graph.

**Independent Test**: Remove KalamSql imports from core crates, build and run tests; all passing.

**Acceptance Scenarios**:

1. Given any DDL/DML path, When executed, Then zero KalamSql code paths are invoked.
2. Given unit/integration tests, When running, Then all compile and pass without KalamSql present.

---

### User Story 3 - Handler-based SqlExecutor using AppContext (Priority: P1)

SqlExecutor is composed of focused handlers under `sql/executor/handlers/`. Each handler consults SchemaRegistry for schema and dispatches to appropriate provider or store via AppContext.

**Why this priority**: Improves maintainability and performance through clear routing and cache-first lookups.

**Independent Test**: Route CREATE/ALTER/DROP/SELECT/INSERT/UPDATE/DELETE via handlers, confirm behavior using existing tests and new smoke checks.

**Acceptance Scenarios**:

1. Given CREATE/ALTER/DROP TABLE, When executed, Then handlers read schema via SchemaRegistry and use the correct provider/store.
2. Given SELECT/INSERT/UPDATE/DELETE on user/shared/stream tables, When executed, Then handlers use UserTableStore/SharedTableStore/StreamTableStore accordingly.
3. Given system-entity actions (users, jobs, namespaces, tables), When executed, Then handlers use SystemTablesRegistry providers.

---

### User Story 4 - Deprecate legacy services (Priority: P2)

Remove obsolete service layers (NamespaceService, UserTableService, SharedTableService, StreamTableService, TableDeletionService) in favor of providers/registries.

**Why this priority**: Shrinks code surface and removes redundant abstractions.

**Independent Test**: Delete service structs and references, verify compilation and test pass.

**Acceptance Scenarios**:

1. Given the codebase, When searching for legacy services, Then no usages remain.
2. Given the build/test pipeline, When executed, Then no regressions occur.

---

### User Story 5 - Build Green and Tests Passing (Priority: P1)

Ensure entire workspace compiles and all tests pass after refactor.

**Why this priority**: Guarantees release readiness and prevents regressions.

**Independent Test**: Run backend, cli, and link tests; confirm 100% pass.

**Acceptance Scenarios**:

1. Given the backend crate, When running tests, Then all tests pass with 0 failures.
2. Given the CLI crate, When running tests, Then all tests pass with 0 failures.
3. Given the link SDK, When running tests, Then all tests pass with 0 failures.

---

### User Story 6 - Unified Job Management System (Priority: P1)

As an operator and developer, I want a single JobManager that handles all background jobs with short, typed IDs, richer statuses, and dedicated logging so I can monitor and recover jobs reliably.

Independent Test:
- Create jobs of each type (flush, cleanup, retention, stream eviction, user cleanup, compact, backup, restore).
- Verify all jobs appear in `system.jobs` with correct status transitions and type-specific ID prefixes.
- Check jobs.log contains job-specific entries with job_id prefixes.
- Validate job ID format (e.g., "FL-abc123" for flush, "CL-def456" for cleanup).
- Test crash recovery by restarting server mid-job and verifying resume/fail behavior.

Acceptance Scenarios:
1. Given JobManager is initialized at startup, When creating a new flush job, Then job is created with status=New, assigned JobId with FL- prefix (e.g., "FL-abc123"), and persisted to system.jobs

2. Given a job is created with status=New, When JobManager processes queue, Then job transitions to Queued → Running with timestamp updates and logs to jobs.log: "`[FL-abc123]` INFO - Job queued" then "`[FL-abc123]` INFO - Job started"

3. Given a job is running, When job completes successfully, Then status transitions to Completed, result is stored, and logs: "`[FL-abc123]` INFO - Job completed: Flushed 1000 rows"

4. Given a job fails with transient error, When Job.should_retry() returns true, Then status transitions to Retrying, retry_count increments, and logs: "`[FL-abc123]` WARN - Job failed, retrying in 2s (attempt 1/3)"

5. Given a job fails after max retries (3 attempts), When final retry fails, Then status transitions to Failed, error_message stored, and logs: "`[FL-abc123]` ERROR - Job failed permanently: {error}"

6. Given server restarts with jobs in Running status, When JobManager starts, Then incomplete jobs are marked as Failed with error "Server restarted during execution" and logged

7. Given all job types (Flush, Cleanup, Retention, StreamEviction, UserCleanup, Compact, Backup, Restore), When jobs are created, Then each has correct prefix (FL-, CL-, RT-, SE-, UC-, CO-, BK-, RS-) and appears in system.jobs

8. Given jobs.log file exists, When filtering logs by job ID "FL-abc123", Then all entries for that specific job are returned (grep "`[FL-abc123]`" jobs.log)

9. Given JobManager with max_concurrent_jobs=5, When 10 jobs are submitted, Then 5 jobs execute immediately (Running), 5 jobs wait (Queued), and queue drains as jobs complete

10. Given a job is cancelled via CANCEL JOB command, When job is running, Then job receives cancellation signal, performs cleanup, transitions to Cancelled status, and logs: "`[FL-abc123]` INFO - Job cancelled by user"

Idempotency Acceptance Scenarios:

11. Given a daily flush job for table "events" on 2025-11-04, When JobManager creates job with idempotency key "flush:default:events:20251104", Then job is created successfully with New status

12. Given job "FL-abc123" is Running with idempotency key "flush:default:events:20251104", When another flush job is created with same idempotency key, Then creation fails with error: "Job already running: FL-abc123 (status: Running, created: 1730000000000)"

13. Given job "FL-abc123" completed on 2025-11-04 with status=Completed, When another flush job is created on 2025-11-04 with same idempotency key "flush:default:events:20251104", Then job is created successfully (previous job is completed, not active)

14. Given job "FL-abc123" failed and transitioned to Retrying with idempotency key "flush:default:events:20251104", When another flush job is created with same key, Then creation fails with error: "Job already running: FL-abc123 (status: Retrying, ...)"

15. Given hourly cleanup job with idempotency key "cleanup:retention:20251104T14", When same cleanup runs twice in same hour, Then second attempt fails with "Job already running" error (idempotency prevents duplicate cleanup)

16. Given job is created without idempotency key (idempotency_key = None), When creating multiple jobs of same type, Then all jobs are created successfully (no idempotency check when key is absent)

Unified Message Field Acceptance Scenarios:

17. Given flush job completes successfully, When job.complete() is called with message "Flushed 1,234 rows (5.2 MB) to Parquet in 342ms", Then job.message contains success message and job.exception_trace is None

18. Given cleanup job fails with RocksDB error, When job.fail() is called with error_message "RocksDB read failed: IO error at offset 12345", Then job.message contains error summary (not "result" or "error_message" field)

19. Given retention job completes with message "Deleted 500 old rows before 2025-10-01", When querying system.jobs, Then message field is populated (old "result" field no longer exists)

20. Given compaction job fails with message "Merge failed: insufficient disk space", When querying system.jobs, Then message field contains error (old "error_message" field no longer exists)

Exception Trace Acceptance Scenarios:

21. Given flush job fails with panic/exception, When job.fail() is called with exception_trace containing multi-line stack trace, Then system.jobs stores full trace in exception_trace field

22. Given job completes successfully, When job.complete() is called, Then exception_trace is explicitly set to None (cleared from previous failures)

23. Given job fails with short error "IO error", When exception_trace is provided with full stack trace (100+ lines), Then message contains concise error, exception_trace contains detailed debugging info

24. Given job retry after failure, When job.retry() is called with error_message and exception_trace, Then both message and exception_trace are updated (not cleared until success)

25. Given developer queries system.jobs for failed jobs, When filtering WHERE status = 'Failed', Then results include both message (error summary) and exception_trace (full stack) for debugging

Retry Logic Acceptance Scenarios:

26. Given job is created, When job.new() is called, Then retry_count = 0 and max_retries = 3 (default)

27. Given job fails with transient error, When job.can_retry() returns true and job.retry() is called, Then retry_count increments to 1, status = Retrying, message and exception_trace updated

28. Given job has retry_count = 2 (2 previous failures), When job fails again and max_retries = 3, Then job.can_retry() returns true (2 < 3), job transitions to Retrying for 3rd attempt

29. Given job has retry_count = 3 and max_retries = 3, When job fails, Then job.can_retry() returns false (3 >= 3), job transitions to Failed permanently (no more retries)

30. Given job is created with custom max_retries, When job.with_max_retries(5) is called, Then job can retry up to 5 times before permanent failure

Parameters Change Acceptance Scenarios:

31. Given flush job is created, When parameters are set with JSON object {"threshold_bytes": 1048576, "force": false}, Then system.jobs stores parameters as JSON object (not array)

32. Given cleanup job with parameters {"retention_days": 30, "batch_size": 1000}, When job executor parses parameters, Then values are extracted by key (retention_days, batch_size) not array index

33. Given legacy job created with old parameters format (JSON array), When migrating to new format, Then migration converts array `["param1", "param2"]` to object {"arg0": "param1", "arg1": "param2"} for backward compatibility

---

### Edge Cases

- What happens if a provider for a system table is unavailable? The operation must fail with a clear, user-friendly error and no partial state changes.
- How are concurrent schema updates handled? SchemaRegistry invalidation must be atomic and visible to subsequent operations.
- What if legacy code paths still reference removed layers? The build must fail with actionable errors (no silent fallbacks).

## Requirements (mandatory)

### Functional Requirements

- FR-001: The system MUST route all table operations through table-specific providers (system and user/shared/stream) exclusively.
- FR-002: The system MUST eliminate StorageAdapter and remove all references/usages from the codebase.
- FR-003: The system MUST remove KalamSql from the execution path and project dependencies.
- FR-004: SqlExecutor MUST be divided into handlers that use SchemaRegistry for schema reads and AppContext to locate providers/stores.
- FR-005: System table operations (users, jobs, namespaces, tables, info schema) MUST be executed via SystemTablesRegistry providers.
- FR-006: User/shared/stream table operations MUST use UserTableStore, SharedTableStore, and StreamTableStore respectively via AppContext.
- FR-007: Legacy services (NamespaceService, UserTableService, SharedTableService, StreamTableService, TableDeletionService) MUST be removed with no remaining references.
- FR-008: All public behavior MUST remain backward-compatible at the API level (no user-facing breaking changes).
- FR-009: The entire workspace MUST compile, and all tests MUST pass post-refactor.
- FR-010: SchemaRegistry MUST be the single source for table schema/definition reads in handlers.

Unclear/Decision Items:

- FR-011: Migration of any KalamSql-specific helpers MUST be handled by \[NEEDS CLARIFICATION: Should we port remaining utility methods into providers vs. add minimal facades in SystemTablesRegistry?]
- FR-012: Backward compatibility strategy for external integrations MUST be defined \[NEEDS CLARIFICATION: Are any external tools directly expecting KalamSql semantics?]
- FR-013: Versioning of internal APIs MUST be documented \[NEEDS CLARIFICATION: Do we tag this refactor under a minor or patch release for Alpha?]

### Key Entities

- AppContext: Central access point to SchemaRegistry, SystemTablesRegistry, and table stores.
- SchemaRegistry: Authoritative cache+persistence facade for table definitions and schema metadata.
- SystemTablesRegistry: Providers for system tables (users, jobs, namespaces, storages, live queries, tables, information_schema tables).
- User/Shared/Stream Table Stores: Storage abstractions for non-system tables.
- SqlExecutor Handlers: Focused units that map SQL statements to provider/store operations using AppContext and SchemaRegistry.

## Success Criteria (mandatory)

### Measurable Outcomes

- SC-001: 100% of legacy references to StorageAdapter and KalamSql removed, with workspace compiling successfully.
- SC-002: 100% of DDL/DML operations resolve providers/stores through AppContext in handlers (verified by code audit and tests).
- SC-003: All existing backend, CLI, and link tests pass (0 failures) after refactor.
- SC-004: Schema lookups in handlers occur via SchemaRegistry in >99% of code paths (no direct ad-hoc reads).
- SC-005: Build time and memory footprint remain within ±10% of pre-refactor baselines, or improve.

## Assumptions

- Existing providers cover all capabilities previously exposed via adapters/KalamSql.
- No user-facing SQL syntax changes are required to achieve parity.
- SystemTablesRegistry exposes all required read/write operations for system tables.

## Dependencies

- Current unified SchemaRegistry is stable and used as the schema SoT.
- AppContext initialization remains the single place wiring providers, registries, and stores.

## Risks & Mitigations

- Risk: Hidden dependency on KalamSql in corner-case tests. Mitigation: Grep for imports/usages and add targeted unit tests.
- Risk: Behavior drift in system table operations. Mitigation: Add golden tests around users/jobs/namespaces flows.
- Risk: Cache invalidation bugs after handlerization. Mitigation: Reuse existing invalidation patterns in DDL handlers.

## Out of Scope

- New datatypes, new table kinds, or auth/RBAC changes.
- Query planner/optimizer changes beyond routing.

## Milestones

1. Remove StorageAdapter (compile green)
2. Remove KalamSql (compile green)
3. Handlerize SqlExecutor for DDL and DML (routing via AppContext)
4. Remove legacy services and usages
5. Full test pass across backend/cli/link

