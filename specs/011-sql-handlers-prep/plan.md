# Implementation Plan: SQL Handlers Prep

**Branch**: `011-sql-handlers-prep` | **Date**: 2025-11-07 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/011-sql-handlers-prep/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

Prepare the repository for comprehensive SQL handler implementation by:
1. Refactoring execution models into modular structure (ExecutionContext, ExecutionResult, ExecutionMetadata)
2. Moving handler utilities to standardized locations (sql/executor/helpers)
3. Implementing DataFusion parameter binding with PostgreSQL-style placeholders ($1, $2)
4. Creating typed handler architecture with HandlerRegistry and zero-boilerplate registration
5. Implementing all 28 SQL statement handlers (excluding SELECT which uses DataFusion directly)
6. Establishing structured error format with machine-readable codes
7. Configurable execution timeouts and parameter size limits

## Technical Context

**Language/Version**: Rust 1.90+ (edition 2021)
**Primary Dependencies**: DataFusion 40.0, Apache Arrow 52.0, Apache Parquet 52.0, Actix-Web 4.4, sqlparser-rs (via DataFusion), tokio 1.48, serde 1.0
**Storage**: RocksDB 0.24 (write path), Parquet files (flushed segments)
**Testing**: cargo test (unit + integration), criterion (benchmarks)
**Target Platform**: Linux/macOS server, WASM (future SDK)
**Project Type**: Backend library + REST API (workspace structure)
**Performance Goals**: 
  - SELECT execution: <1 second for simple queries (SC-002)
  - Handler dispatch: <2μs overhead vs direct match
  - Parameter validation: <10μs for 50 parameters
  - Schema lookup via SchemaRegistry: 1-2μs (50-100× faster than SQL queries)
**Constraints**: 
  - Parameter limits: max 50 parameters, 512KB per parameter
  - Handler timeout: 30 seconds default (configurable via config.toml)
  - Zero-boilerplate handler registration pattern
  - Last-write-wins for concurrent DML (no locking)
  - **Row count semantics**: All operations return `row_count` (SELECT/SHOW) or `rows_affected` (DML/DDL) following MySQL conventions
**Scale/Scope**: 
  - 28 SQL statement handlers (excluding SELECT)
  - 7 handler categories: DDL (14), DML (3), Flush (2), Jobs (2), Subscription (1), User (3), Transaction (3)
  - Structured error responses with machine-readable codes

---

## Row Count Behavior

All handler responses MUST include a row count field following MySQL semantics:

### DML Operations
- **INSERT**: `rows_affected` = Number of rows inserted (e.g., multi-value INSERT returns count)
- **UPDATE**: `rows_affected` = Number of rows with **actual changes** (not rows matched by WHERE)
- **DELETE**: `rows_affected` = Number of rows removed
- **SELECT**: `row_count` = Number of rows returned in result set

### DDL Operations (always return 1)
- **CREATE/DROP NAMESPACE**: `rows_affected = 1` (single namespace created/dropped)
- **CREATE/ALTER/DROP TABLE**: `rows_affected = 1` (single table created/altered/dropped)
- **CREATE/ALTER/DROP STORAGE**: `rows_affected = 1` (single storage created/altered/dropped)

### User Management (always return 1)
- **CREATE/ALTER/DROP USER**: `rows_affected = 1` (single user created/altered/dropped)

### Job Operations (always return 1)
- **KILL JOB**: `rows_affected = 1` (single job killed)
- **KILL LIVE QUERY**: `rows_affected = 1` (single subscription cancelled)

### Flush Operations
- **FLUSH TABLE**: `rows_affected = 1` (single table flushed)
- **FLUSH ALL TABLES**: `rows_affected = N` (N = number of tables flushed)

### Query Operations (return count of result rows)
- **SHOW NAMESPACES**: `row_count = N` (N = number of namespaces)
- **SHOW TABLES**: `row_count = N` (N = number of tables)
- **SHOW STORAGES**: `row_count = N` (N = number of storages)
- **DESCRIBE TABLE**: `row_count = N` (N = number of columns)

### Special Cases
- **CREATE IF NOT EXISTS**: Return `rows_affected = 0` if entity already exists (with warning message), `rows_affected = 1` if created
- **UPDATE with no changes**: Return `rows_affected = 0` if WHERE matches rows but values are identical

**Implementation Pattern**:
```rust
// DML via DataFusion
let batches = execute_via_datafusion(ctx, &sql, params).await?;
let rows_affected: usize = batches.iter().map(|b| b.num_rows()).sum();

// DDL operations
let rows_affected = 1; // Single entity created/modified/dropped

// SHOW/DESCRIBE operations
let row_count = result_batches.iter().map(|b| b.num_rows()).sum();
```

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

**Status**: Constitution template is empty/not ratified for this project. Proceeding with standard best practices:
- ✅ Modular architecture (handlers in separate files)
- ✅ Test coverage required (unit tests per handler, integration tests per category)
- ✅ Zero-boilerplate patterns (generic TypedHandlerAdapter)
- ✅ Clear error handling (structured ErrorResponse with codes)
- ✅ Performance constraints documented (timeouts, parameter limits)

**No violations** - Feature aligns with Rust best practices and existing KalamDB architecture patterns.

## Project Structure

### Documentation (this feature)

```text
specs/[###-feature]/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (/speckit.plan command)
├── data-model.md        # Phase 1 output (/speckit.plan command)
├── quickstart.md        # Phase 1 output (/speckit.plan command)
├── contracts/           # Phase 1 output (/speckit.plan command)
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
backend/crates/kalamdb-core/src/
├── sql/
│   └── executor/
│       ├── models/              # NEW: Execution models (one per file)
│       │   ├── context.rs       # ExecutionContext
│       │   ├── result.rs        # ExecutionResult
│       │   └── metadata.rs      # ExecutionMetadata
│       ├── helpers/             # MOVED: From handlers/
│       │   ├── audit.rs         # Audit logging helpers
│       │   └── helpers.rs       # Shared utilities
│       ├── handlers/            # Handler implementations
│       │   ├── ddl/             # DDL handlers (14 files)
│       │   │   ├── mod.rs
│       │   │   ├── create_namespace.rs  # ✅ COMPLETE
│       │   │   ├── alter_namespace.rs
│       │   │   ├── drop_namespace.rs
│       │   │   ├── show_namespaces.rs
│       │   │   ├── create_storage.rs
│       │   │   ├── alter_storage.rs
│       │   │   ├── drop_storage.rs
│       │   │   ├── show_storages.rs
│       │   │   ├── create_table.rs
│       │   │   ├── alter_table.rs
│       │   │   ├── drop_table.rs
│       │   │   ├── show_tables.rs
│       │   │   ├── describe_table.rs
│       │   │   └── show_stats.rs
│       │   ├── dml/             # DML handlers (3 files)
│       │   │   ├── mod.rs
│       │   │   ├── insert.rs
│       │   │   ├── update.rs
│       │   │   └── delete.rs
│       │   ├── flush/           # Flush handlers (2 files)
│       │   │   ├── mod.rs
│       │   │   ├── flush_table.rs
│       │   │   └── flush_all_tables.rs
│       │   ├── jobs/            # Job handlers (2 files)
│       │   │   ├── mod.rs
│       │   │   ├── kill_job.rs
│       │   │   └── kill_live_query.rs
│       │   ├── subscription/    # Subscription handler (1 file)
│       │   │   ├── mod.rs
│       │   │   └── subscribe.rs
│       │   ├── user/            # User handlers (3 files)
│       │   │   ├── mod.rs
│       │   │   ├── create_user.rs
│       │   │   ├── alter_user.rs
│       │   │   └── drop_user.rs
│       │   ├── transaction/     # Transaction handlers (3 files)
│       │   │   ├── mod.rs
│       │   │   ├── begin.rs
│       │   │   ├── commit.rs
│       │   │   └── rollback.rs
│       │   ├── typed.rs         # TypedStatementHandler trait
│       │   ├── handler_adapter.rs   # Generic adapter
│       │   ├── handler_registry.rs  # Handler registry
│       │   └── mod.rs
│       └── mod.rs               # SqlExecutor (refactored)
├── app_context.rs              # AppContext singleton
├── schema_registry/            # Schema cache + Arrow memoization
└── jobs/                       # UnifiedJobManager

backend/crates/kalamdb-api/src/
└── routes/                     # REST API routes (ExecutionContext construction)

tests/
├── integration/
│   ├── test_ddl_handlers.rs    # DDL category tests
│   ├── test_dml_handlers.rs    # DML category tests
│   ├── test_handler_params.rs  # Parameter binding tests
│   └── test_handler_errors.rs  # Error format tests
└── unit/
    └── handlers/               # Per-handler unit tests
```

**Structure Decision**: Backend library structure with handlers organized by category (DDL, DML, Flush, Jobs, Subscription, User, Transaction). Each handler in its own file following single-responsibility principle. Generic adapter eliminates boilerplate.

## Complexity Tracking

**No violations** - This feature simplifies the codebase:
- Reduces handler boilerplate by 89% (900 lines → 150 lines via generic adapter)
- Consolidates execution models into single location (sql/executor/models/)
- Eliminates duplicate handler registration code
- Standardizes error handling across all statement types

This feature is a refactoring/architecture improvement with no added complexity.
