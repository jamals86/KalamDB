# Feature Specification: SQL Handlers Prep

**Feature Branch**: `[011-sql-handlers-prep]`  
**Created**: 2025-11-06  
**Status**: Draft  
**Input**: Prepare the repo for starting SQL handlers. Changes: (1) move handler types into `sql/executor/models` one struct per file; (2) move `handlers/audit.rs` and `handlers/helpers.rs` to `sql/executor/helpers`; (3) remove `handlers/table_registry.rs`; (4) migrate `sql/datafusion_session.rs` off `KalamSessionState` to a single `ExecutionContext`; update `sql/functions/current_user.rs`; (5) use DataFusion's native `ScalarValue` for parameters (via SessionContext::sql().with_params()); (6) verify end-to-end flow from kalamdb-api to handlers and prepare role checks per statement.

## User Scenarios & Testing (mandatory)

### User Story 1 - Execute SQL with request context (Priority: P1)

As an authenticated caller, I can execute a SQL statement via the REST API and have my user/role context applied end-to-end so that authorization is enforced correctly and results return in a consistent format.

Why this priority: This is the core path for running SQL and validating handler wiring; everything else depends on this working reliably.

Independent Test: Call POST /v1/api/sql with a simple SELECT; verify ExecutionContext is created from auth and AuthorizationHandler runs; response is a success with rows or a clear error.

Acceptance Scenarios:
1. Given a valid JWT or Basic auth, When I POST a SELECT to /v1/api/sql, Then the request succeeds and returns rows in JSON with columns and row_count.
2. Given a non-admin user, When I POST a CREATE NAMESPACE, Then the request is rejected with an authorization error message.

---

### User Story 2 - Parameterized execution support (Priority: P2)

As a client, I can pass SQL parameters via the REST API so that DataFusion's native parameter binding validates and substitutes them securely, enabling prepared-like execution.

Why this priority: Needed to support secure parameter binding and future prepared statements; leverages DataFusion's built-in validation instead of custom code.

Scope: Parameterized execution applies only to SELECT, INSERT, UPDATE, and DELETE. Other statement types (DDL, system commands, transactions, etc.) do not accept parameters and must return a clear error.

Independent Test: Call POST /v1/api/sql with `{"sql": "SELECT * FROM t WHERE id = $1", "params": [123]}` and verify DataFusion validates parameter types/count; unit test asserts error for params on unsupported statements.

Acceptance Scenarios:
1. Given a SQL statement with `$1`, `$2` placeholders and matching ScalarValue parameters, When I execute via DataFusion, Then the query succeeds with parameters properly substituted.
2. Given mismatched parameter counts or types, When DataFusion validates, Then an informative error is returned during query planning (before execution).
3. Given a non-DQL/DML statement (e.g., CREATE TABLE) with parameters, When execution begins, Then an error is returned stating parameters are not supported for this statement type.

---

### User Story 3 - Auditable operations (Priority: P3)

As an operator, I want audit helpers available from a stable location to record who did what, where, and when so that we can persist and review actions later.

Why this priority: Ensures consistent audit logging API for upcoming handlers without blocking initial wiring.

Independent Test: Unit tests calling helper functions produce AuditLogEntry with expected fields and actor information from ExecutionContext.

Acceptance Scenarios:
1. Given an ExecutionContext with request_id and ip_address, When I call log_ddl_operation, Then the returned AuditLogEntry includes these fields.
2. Given a DML action, When I call log_dml_operation with rows_affected, Then details include the count.

---

### User Story 4 - Modular DML handlers (Priority: P2)

As a developer, I want DML handling split into focused modules so INSERT, DELETE, and UPDATE logic are isolated, testable, and easy to evolve independently.

Why this priority: Prepares the codebase for native write paths and reduces risk by isolating complex logic per operation.

Target structure:
- `backend/crates/kalamdb-core/src/sql/executor/handlers/dml/`
	- `insert.rs`
	- `delete.rs`
	- `update.rs`
	- `mod.rs` (optional coordinator)

Acceptance Scenarios:
1. Given the new dml/ module layout, when building the workspace, then compilation succeeds and handlers are re-exported from `handlers::dml`.
2. Given existing executor routing, when a DML statement is dispatched, then it can be routed to the corresponding handler without signature changes to public APIs.
3. Given unit tests for each handler stub, when executed, then they return NotYetImplemented with correct operation name.

---

### User Story 5 - Full DML implementation with DataFusion parameters (Priority: P1)

As a developer, I want INSERT, DELETE, and UPDATE to execute natively using our storage providers while accepting parameters provided as DataFusion `ScalarValue`s, so writes are safe, typed, and performant.

Why this priority: Enables production-ready write paths without relying on DataFusion DML (which we don’t use for writes).

Scope:
- SELECT continues to use DataFusion.
- INSERT/DELETE/UPDATE use native write paths (already present), wired to accept `Vec<ScalarValue>` for parameter binding.
- Parameters are positional and validated against target table schema.

Acceptance Scenarios:
1. Given an INSERT with `$1, $2` placeholders and matching `Vec<ScalarValue>`, when executed, then rows are written to the correct table with values bound in-order.
2. Given a DELETE with a parameterized predicate, when executed, then only matching rows are removed and `rows_affected` is returned.
3. Given an UPDATE with parameterized SET/WHERE, when executed, then matching rows are updated and `rows_affected` is returned.
4. Given a parameter count/type mismatch, when execution begins, then a clear validation error is returned before any writes occur.

### Edge Cases

- Missing/empty SQL input returns a clear validation error.
- Authenticated user with insufficient role receives an explicit authorization message.
- Multiple statements in a single payload succeed sequentially until a failure; the failing statement returns an error annotated with statement index.
- Parameters are provided for a statement with no placeholders; this results in a parameter count error before execution.
- Parameters are provided for unsupported statement types (DDL, system commands, transactions); an error is returned indicating parameters are not supported for that statement.

## Requirements (mandatory)

### Functional Requirements

- FR-001: Core execution models MUST be refactored into `sql/executor/models` with one type per file (ExecutionContext, ExecutionResult, ExecutionMetadata).
- FR-002: Handler utilities MUST live under `sql/executor/helpers` (audit.rs, helpers.rs) and be importable from there.
- FR-003: The legacy `handlers/table_registry.rs` MUST be removed from the handler registry and not compiled.
- FR-004: DataFusion session factory MUST stop using KalamSessionState; CURRENT_USER registration MUST work via user id only.
- FR-005: SqlExecutor public API MUST accept `Vec<ScalarValue>` (from datafusion::scalar::ScalarValue) and pass it to DataFusion's `with_params()` method.
- FR-006: REST API path MUST construct ExecutionContext from authenticated user and pass it into the executor.
- FR-007: Authorization MUST be applied per classified statement prior to handler execution; non-admin admin-only ops MUST be rejected.
- FR-008: All imports, modules, and re-exports MUST compile across workspace and tests after refactor.
- FR-009: Helpers MUST preserve existing behavior (formatting/validation) after relocation.
- FR-010: Unit/integration tests referencing ExecutionContext MUST compile using the new module path.

- FR-011: Parameterized execution support is LIMITED to SELECT, INSERT, UPDATE, and DELETE; other statement types MUST reject parameters with a clear error at the executor boundary (before DataFusion).
- FR-012: Self-service ALTER USER for password changes MUST be allowed for non-admins only on their own account; modifying other users remains restricted to DBA/System roles.
- FR-013: Create `handlers/ddl/` directory and split responsibilities into `namespace.rs`, `create_table.rs`, `alter_table.rs`, and `helpers.rs`.

- FR-013: Create `handlers/ddl/` directory and split responsibilities into `namespace.rs`, `create_table.rs`, `alter_table.rs`, and `helpers.rs`.
- FR-014: Move shared DDL utilities into `helpers.rs` with functions: `inject_auto_increment_field`, `inject_system_columns`, and `save_table_definition`.
- FR-015: Preserve the public API by re-exporting a single `ddl` module from `handlers::ddl` so external call sites remain stable.
- FR-016: Ensure all DDL-related tests compile and pass after the split, with no behavioral regressions.

- FR-019: Create `handlers/dml/` directory with `insert.rs`, `delete.rs`, `update.rs` (and optional `mod.rs`) and re-export from `handlers::dml`.
- FR-020: SELECT continues to use DataFusion; INSERT/DELETE/UPDATE use native handlers accepting `Vec<ScalarValue>` for parameters.
- FR-021: Add lightweight parameter helpers for DML: `validate_param_count()` and a placeholder `coerce_params()` (future DataFusion coercion integration).

### Key Entities

- ExecutionContext: User identity, role, optional namespace, request id, IP, timestamp.
- ScalarValue: DataFusion's native parameter type (Int64, Utf8, Float64, Boolean, Null, etc.).
- ExecutionResult: Success message, single/multiple batches, or subscription metadata.
- AuditLogEntry: Existing system type consumed by helpers (not modified here).

## Success Criteria (mandatory)

### Measurable Outcomes

- SC-001: Workspace compiles with 0 errors after refactor; no unresolved imports for moved modules.
- SC-002: API executes a simple SELECT end-to-end with correct auth context applied in under 1 second on a developer machine.
- SC-003: A non-admin CREATE NAMESPACE attempt is rejected with a clear authorization error 100% of the time.
- SC-004: Parameters provided to executor are passed to DataFusion's `with_params()` and validated by DataFusion's query planner (unit test asserts proper error for count/type mismatch).

## Assumptions

- A single ExecutionContext suffices for handlers; DataFusion sessions remain per-request as before.
- Parameter binding remains positional (Vec order) unless/until named params are introduced.
- System namespace operations are restricted to System/DBA roles per existing RBAC rules.

## Decisions

- Parameter Semantics: Use DataFusion's native `ScalarValue` and `with_params()` API for parameter binding; DataFusion handles count/type validation during query planning (no custom validator needed).
- Role Matrix Granularity: Allow self ALTER USER for password changes (non-admins may modify their own account only). All other user modifications require DBA/System roles.
- Parameterized Scope: Limit parameterized execution to SELECT/INSERT/UPDATE/DELETE only; other statement types return a "parameters not supported" error at the executor boundary.
- Schema Awareness: DataFusion's query planner validates parameter types against table schemas automatically; no SchemaRegistry pre-validation needed.

## DDL handler modularization

Problem: `backend/crates/kalamdb-core/src/sql/executor/handlers/ddl.rs` is too large and hard to maintain.

Goal: Split DDL handling into focused submodules under a new `ddl/` folder, improving readability, testability, and ownership boundaries.

Target structure:
- `backend/crates/kalamdb-core/src/sql/executor/handlers/ddl/`
	- `namespace.rs` — CREATE/DROP NAMESPACE responsibilities
	- `create_table.rs` — CREATE TABLE responsibilities
	- `alter_table.rs` — ALTER TABLE responsibilities
	- `helpers.rs` — Shared utilities: `inject_auto_increment_field`, `inject_system_columns`, `save_table_definition`

Acceptance Scenarios:
1. Given the new `ddl/` module layout, when building the workspace, then compilation succeeds with unchanged public `handlers::ddl` API surface (re-exports maintained).
2. Given existing DDL call sites in the executor, when refactor is complete, then no call sites require signature changes (only import paths change inside handlers).
3. Given unit tests for CREATE/DROP NAMESPACE, CREATE TABLE, and ALTER TABLE, when run after the split, then they pass unchanged (behavior preserved).

## Parameter Binding Implementation Notes

**DataFusion Native Approach**: Instead of custom `ParamValue` wrapper, use DataFusion's `ScalarValue` directly:

```rust
use datafusion::scalar::ScalarValue;

// API layer: deserialize JSON params to Vec<ScalarValue>
let params: Vec<ScalarValue> = vec![
    ScalarValue::Int64(Some(123)),
    ScalarValue::Utf8(Some("Alice".to_string())),
];

// Executor: pass to DataFusion
let df = session.sql("SELECT * FROM users WHERE id = $1 AND name = $2").await?;
// TODO: Parameter binding will be implemented via LogicalPlan manipulation
// DataFrame API doesn't expose with_params() directly in DataFusion 50.x
let batches = df.collect().await?;
```

**Current Status**:
- ✅ SELECT/INSERT/DELETE route to DataFusion (without params for now)
- ✅ UPDATE reserved for custom handling
- ⏳ Parameter binding deferred until query handler implementation (requires LogicalPlan rewrite)

**Benefits**:
- Zero conversion overhead (no ParamValue → ScalarValue mapping)
- DataFusion validates parameter count, types, and schema compatibility automatically (when implemented)
- Handlers receive already-validated RecordBatch results (no placeholder visibility)
- API only needs JSON → ScalarValue deserialization logic

Functional Requirements:
- FR-013: Create `handlers/ddl/` directory and split responsibilities into `namespace.rs`, `create_table.rs`, `alter_table.rs`, and `helpers.rs`.
- FR-014: Move shared DDL utilities into `helpers.rs` with functions: `inject_auto_increment_field`, `inject_system_columns`, and `save_table_definition`.
- FR-015: Preserve the public API by re-exporting a single `ddl` module from `handlers::ddl` so external call sites remain stable.
- FR-016: Ensure all DDL-related tests compile and pass after the split, with no behavioral regressions.
