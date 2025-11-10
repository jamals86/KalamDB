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
- Concurrent DML operations on the same row use last-write-wins semantics; no conflict detection or locking (most recent write based on timestamp prevails).
- Parameter array exceeding 50 elements returns validation error before execution.
- Individual parameter value exceeding 512KB returns validation error before execution.
- Handler execution exceeding configured timeout (default 30s) returns timeout error with elapsed time; partial results are discarded.

## Requirements (mandatory)

### Functional Requirements

- FR-001: Core execution models MUST be refactored into `sql/executor/models` with one type per file (ExecutionContext, ExecutionResult, ExecutionMetadata).
- FR-002: Handler utilities MUST live under `sql/executor/helpers` (audit.rs, helpers.rs) and be importable from there.
- FR-003: The legacy `handlers/table_registry.rs` MUST be removed from the handler registry and not compiled.
- FR-004: DataFusion session factory MUST stop using KalamSessionState; CURRENT_USER registration MUST work via user id only.
- FR-005: SqlExecutor public API MUST accept `Vec<ScalarValue>` (from datafusion::scalar::ScalarValue) and pass it to DataFusion's `with_params()` method.
- FR-005a: Parameter validation MUST enforce maximum 50 parameters per statement and maximum 512KB per individual parameter value to prevent memory exhaustion.
- FR-005b: Handler execution MUST enforce a configurable timeout (default 30 seconds, configured via `[execution].handler_timeout_seconds` in config.toml). On timeout, handler MUST return `KalamDbError::Timeout` with elapsed time.
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
- ErrorResponse: Structured error format with `code` (machine-readable string), `message` (human-readable description), and `details` (contextual JSON object with fields like `expected`, `actual`, `elapsed_ms`).

### Row Count Behavior

All SQL operations MUST return a `row_count` or `rows_affected` field in the response, following MySQL semantics:

| Statement | Field Name | Meaning | Example |
|-----------|------------|---------|---------|
| **SELECT** | `row_count` | Number of rows returned in result set | `SELECT * FROM users WHERE country = 'US'` → `10 rows in set` |
| **INSERT** | `rows_affected` | Number of rows inserted | `INSERT INTO users (name) VALUES ('Alice'), ('Bob')` → `2 rows affected` |
| **UPDATE** | `rows_affected` | Number of rows with actual changes (not rows matched) | `UPDATE users SET active = 1 WHERE country = 'US'` → `5 rows affected` (only if value changed) |
| **DELETE** | `rows_affected` | Number of rows removed | `DELETE FROM users WHERE active = 0` → `3 rows affected` |
| **CREATE NAMESPACE** | `rows_affected` | Always 1 (namespace created) | `CREATE NAMESPACE prod` → `1 row affected` |
| **DROP NAMESPACE** | `rows_affected` | Always 1 (namespace dropped) | `DROP NAMESPACE test` → `1 row affected` |
| **CREATE TABLE** | `rows_affected` | Always 1 (table created) | `CREATE TABLE users (...)` → `1 row affected` |
| **ALTER TABLE** | `rows_affected` | Always 1 (table altered) | `ALTER TABLE users ADD COLUMN age INT` → `1 row affected` |
| **DROP TABLE** | `rows_affected` | Always 1 (table dropped) | `DROP TABLE users` → `1 row affected` |
| **CREATE STORAGE** | `rows_affected` | Always 1 (storage created) | `CREATE STORAGE s3_prod` → `1 row affected` |
| **DROP STORAGE** | `rows_affected` | Always 1 (storage dropped) | `DROP STORAGE s3_test` → `1 row affected` |
| **CREATE USER** | `rows_affected` | Always 1 (user created) | `CREATE USER alice` → `1 row affected` |
| **ALTER USER** | `rows_affected` | Always 1 (user altered) | `ALTER USER alice SET PASSWORD '...'` → `1 row affected` |
| **DROP USER** | `rows_affected` | Always 1 (user dropped) | `DROP USER bob` → `1 row affected` |
| **FLUSH TABLE** | `rows_affected` | Number of tables flushed (always 1) | `FLUSH TABLE users` → `1 row affected` |
| **FLUSH ALL TABLES** | `rows_affected` | Number of tables flushed | `FLUSH ALL TABLES` → `15 rows affected` (15 tables) |
| **KILL JOB** | `rows_affected` | Always 1 (job killed) | `KILL JOB 'FL-abc123'` → `1 row affected` |
| **KILL LIVE QUERY** | `rows_affected` | Always 1 (subscription cancelled) | `KILL LIVE QUERY 'sub_xyz789'` → `1 row affected` |
| **SHOW NAMESPACES** | `row_count` | Number of namespaces returned | `SHOW NAMESPACES` → `5 rows in set` |
| **SHOW TABLES** | `row_count` | Number of tables returned | `SHOW TABLES` → `12 rows in set` |
| **SHOW STORAGES** | `row_count` | Number of storages returned | `SHOW STORAGES` → `3 rows in set` |
| **DESCRIBE TABLE** | `row_count` | Number of columns returned | `DESCRIBE users` → `8 rows in set` |

**Implementation Notes**:
- DML operations (INSERT/UPDATE/DELETE): Use `RecordBatch.num_rows()` sum from DataFusion execution
- UPDATE: Only count rows where values actually changed (not rows matched by WHERE clause)
- DDL operations: Always return 1 (single entity created/modified/dropped)
- SHOW/DESCRIBE: Return count of result rows
- FLUSH ALL TABLES: Return count of tables flushed
- CREATE IF NOT EXISTS: Return 1 if created, 0 if already exists (with warning message)

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

## Clarifications

### Session 2025-11-07

- Q: What should BEGIN/COMMIT/ROLLBACK handlers do if transaction manager isn't implemented yet (FR-029 placeholder implementations)? → A: Return "NotImplemented" error with message "Transaction support planned for Phase 11" - clear failure indicating feature unavailability
- Q: How should concurrent DML operations on the same row be handled? → A: Last-write-wins - no conflict detection, most recent write overwrites previous (timestamp-based)
- Q: What are the maximum allowed parameter array sizes and individual parameter sizes? → A: Max 50 parameters, 512KB per parameter
- Q: Should handlers have execution timeouts? What happens on timeout? → A: 30 seconds default timeout, configurable via config.toml
- Q: Should parameter validation errors return structured error codes or just messages? → A: Structured error codes + messages - e.g., `{"code": "PARAM_COUNT_MISMATCH", "message": "...", "expected": 2, "actual": 3}` (machine-readable, enables client-side handling)

## Decisions

- Parameter Semantics: Use DataFusion's native `ScalarValue` and `with_params()` API for parameter binding; DataFusion handles count/type validation during query planning (no custom validator needed).
- Role Matrix Granularity: Allow self ALTER USER for password changes (non-admins may modify their own account only). All other user modifications require DBA/System roles.
- Parameterized Scope: Limit parameterized execution to SELECT/INSERT/UPDATE/DELETE only; other statement types return a "parameters not supported" error at the executor boundary.
- Schema Awareness: DataFusion's query planner validates parameter types against table schemas automatically; no SchemaRegistry pre-validation needed.
- Transaction Handler Placeholders: BEGIN/COMMIT/ROLLBACK handlers return `KalamDbError::NotImplemented` with message "Transaction support planned for Phase 11" until full transaction manager is implemented.
- Concurrent Write Semantics: DML operations use last-write-wins with no conflict detection; most recent write (by timestamp) overwrites previous values without locking or version checks.
- Parameter Size Limits: Maximum 50 parameters per statement and 512KB per individual parameter value enforced at API boundary to prevent memory exhaustion and DoS attacks.
- Handler Execution Timeout: Default 30-second timeout for all handler executions, configurable via `[execution].handler_timeout_seconds` in config.toml. Timeout returns error with elapsed time; partial results discarded.
- Error Response Format: All errors return structured JSON with machine-readable `code` field (e.g., `PARAM_COUNT_MISMATCH`), human-readable `message`, and contextual `details` object for client-side parsing and handling.

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

---

### User Story 6 - Complete Handler Implementation for All SQL Statements (Priority: P0)

As a developer, I want every SQL statement type (except SELECT, which is handled directly by DataFusion) to have a fully implemented typed handler in its own file so that the codebase is modular, maintainable, and follows the established handler registry pattern.

Why this priority: This completes the handler architecture migration started with CreateNamespaceHandler, ensuring all 28 statement types (excluding SELECT) have consistent implementation patterns with zero boilerplate.

**Current Status**: 
- ✅ 1/28 handlers implemented (CreateNamespace in handlers/namespace/create.rs)
- ⚠️ 13/28 handlers need migration from ddl_legacy.rs (namespace/storage/table operations)
- ⏳ 14/28 handlers need new implementation (flush/jobs/subscription/user/transaction)
- SELECT is handled directly in `execute_via_datafusion()` and does not need a separate handler.

**Migration Source**: All DDL logic exists in `backend/crates/kalamdb-core/src/sql/executor/handlers/ddl_legacy.rs` (1500+ lines) and must be migrated to individual handler files before deletion.

**Target Architecture**:
```
backend/crates/kalamdb-core/src/sql/executor/handlers/
├── namespace/                   # ✅ Directory exists
│   ├── mod.rs
│   ├── create.rs               # ✅ CreateNamespaceHandler (reference implementation)
│   ├── alter.rs                # ⚠️ Stub - needs migration from ddl_legacy.rs
│   ├── drop.rs                 # ⚠️ Stub - needs execute_drop_namespace logic
│   └── show.rs                 # ⚠️ Stub - needs implementation
├── storage/                     # ✅ Directory exists
│   ├── mod.rs
│   ├── create.rs               # ⚠️ Stub - needs execute_create_storage logic
│   ├── alter.rs                # ⚠️ Stub - needs migration
│   ├── drop.rs                 # ⚠️ Stub - needs migration
│   └── show.rs                 # ⚠️ Stub - needs implementation
├── table/                       # ✅ Directory exists
│   ├── mod.rs
│   ├── create.rs               # ⚠️ Stub - needs execute_create_table + helpers (445+ lines)
│   ├── alter.rs                # ⚠️ Stub - needs execute_alter_table (Phase 10.2 patterns)
│   ├── drop.rs                 # ⚠️ Stub - needs execute_drop_table + 6 helpers (400+ lines)
│   ├── show.rs                 # ⚠️ Stub - needs SchemaRegistry.scan_namespace logic
│   ├── describe.rs             # ⚠️ Stub - needs SchemaRegistry.get_table_definition logic
│   └── show_stats.rs           # ⚠️ Stub - needs implementation
├── dml/                         # ✅ Directory exists + handlers created
│   ├── mod.rs
│   ├── insert.rs               # ✅ InsertHandler created (delegates to DataFusion)
│   ├── update.rs               # ✅ UpdateHandler created (delegates to DataFusion)
│   └── delete.rs               # ✅ DeleteHandler created (delegates to DataFusion)
├── flush/                       # ✅ Directory exists
│   ├── mod.rs
│   ├── flush_table.rs          # ⏳ New implementation needed (JobsManager pattern)
│   └── flush_all_tables.rs     # ⏳ New implementation needed (JobsManager + SchemaRegistry)
├── jobs/                        # ✅ Directory exists
│   ├── mod.rs
│   ├── kill_job.rs             # ⏳ New implementation needed (JobsManager.cancel_job)
│   └── kill_live_query.rs      # ⏳ New implementation needed (LiveQueryManager)
├── subscription/                # ✅ Directory exists
│   ├── mod.rs
│   └── subscribe.rs            # ⏳ New implementation needed (LiveQueryManager)
├── user/                        # ✅ Directory exists
│   ├── mod.rs
│   ├── create.rs               # ⏳ New implementation needed (bcrypt password hashing)
│   ├── alter.rs                # ⏳ New implementation needed (self-service + admin checks)
│   └── drop.rs                 # ⏳ New implementation needed (soft delete)
└── transaction/                 # ✅ Directory exists
    ├── mod.rs
    ├── begin.rs                # ⏳ Placeholder (returns NotImplemented)
    ├── commit.rs               # ⏳ Placeholder (returns NotImplemented)
    └── rollback.rs             # ⏳ Placeholder (returns NotImplemented)
```

**Implementation Checklist** (28 handlers total, SELECT handled separately):

**Namespace Handlers (4):**
- [x] CreateNamespace - ✅ COMPLETE (handlers/namespace/create.rs)
- [ ] AlterNamespace - Migrate from ddl_legacy.rs
- [ ] DropNamespace - Migrate execute_drop_namespace from ddl_legacy.rs
- [ ] ShowNamespaces - Implement using AppContext.system_tables().namespaces()

**Storage Handlers (4):**
- [ ] CreateStorage - Migrate execute_create_storage from ddl_legacy.rs
- [ ] AlterStorage - Migrate from ddl_legacy.rs
- [ ] DropStorage - Migrate from ddl_legacy.rs
- [ ] ShowStorages - Implement using AppContext.system_tables().storages()

**Table Handlers (7):**
- [ ] CreateTable - Migrate execute_create_table + 3 helpers from ddl_legacy.rs (~500 lines)
- [ ] AlterTable - Migrate execute_alter_table from ddl_legacy.rs (Phase 10.2 SchemaRegistry pattern)
- [ ] DropTable - Migrate execute_drop_table + 6 helpers from ddl_legacy.rs (~400 lines)
- [ ] ShowTables - Implement using SchemaRegistry.scan_namespace()
- [ ] DescribeTable - Implement using SchemaRegistry.get_table_definition()
- [ ] ShowStats - Implement new (system statistics)

**DML Handlers (3):**
- [x] Insert - ✅ Created in handlers/dml/insert.rs (delegates to DataFusion)
- [x] Update - ✅ Created in handlers/dml/update.rs (delegates to DataFusion)
- [x] Delete - ✅ Created in handlers/dml/delete.rs (delegates to DataFusion)

**Note**: SELECT is handled directly in `execute_via_datafusion()` and does NOT need a separate handler. The 3 DML handlers above are thin wrappers that delegate to the same DataFusion execution path.

**Flush Handlers (2):**
- [ ] FlushTable - Implement using JobsManager (Phase 9 pattern)
- [ ] FlushAllTables - Implement using JobsManager + SchemaRegistry

**Job Management Handlers (2):**
- [ ] KillJob - Implement using JobsManager.cancel_job()
- [ ] KillLiveQuery - Implement using LiveQueryManager

**Subscription Handler (1):**
- [ ] Subscribe - Implement using LiveQueryManager

**User Management Handlers (3):**
- [ ] CreateUser - Implement using AppContext.system_tables().users() + bcrypt
- [ ] AlterUser - Implement with self-service password + admin-only role change
- [ ] DropUser - Implement with soft delete (deleted_at timestamp)

**Transaction Handlers (3):**
- [ ] BeginTransaction - Placeholder (returns NotImplemented)
- [ ] CommitTransaction - Placeholder (returns NotImplemented)
- [ ] RollbackTransaction - Placeholder (returns NotImplemented)

**Per-Handler Requirements**:
1. Each handler MUST be in its own file (one handler per file)
2. Each handler MUST implement `TypedStatementHandler<T>` trait
3. Each handler MUST have `execute()` and `check_authorization()` methods
4. Each handler MUST be registered in `HandlerRegistry::new()` using the generic adapter
5. Each handler MUST have unit tests (at least 2: success case + authorization check)
6. Each handler MUST use `Arc<AppContext>` for data access
7. Each handler MUST return descriptive error messages using `KalamDbError` variants
8. **Migration handlers** MUST copy logic from ddl_legacy.rs and adapt to TypedStatementHandler pattern
9. **New handlers** MUST follow CreateNamespaceHandler reference implementation pattern

**Migration Strategy**:
1. **Phase 1**: Migrate namespace/storage/table handlers from ddl_legacy.rs (T072-T084)
   - Copy execute_* methods from ddl_legacy.rs
   - Adapt to TypedStatementHandler pattern (receive parsed statement, not SQL string)
   - Update to use AppContext instead of individual parameters
   - Preserve all business logic (validation, RBAC checks, error handling)
2. **Phase 2**: Implement new handlers (flush/jobs/subscription/user/transaction) (T100-T129)
   - Follow CreateNamespaceHandler pattern
   - Use Phase 9 JobsManager for flush/job operations
   - Use LiveQueryManager for subscription operations
   - Use bcrypt for user password hashing
3. **Phase 3**: Delete ddl_legacy.rs after all migrations complete and verified (T130-T131)

**Registration Pattern** (zero boilerplate):
```rust
// In handler_registry.rs
registry.register_typed(
    SqlStatement::MyStatement(MyStatement { /* placeholder */ }),
    MyStatementHandler::new(app_context.clone()),
    |stmt| match stmt {
        SqlStatement::MyStatement(s) => Some(s),
        _ => None,
    },
);
```

**Reference Implementation**: See CreateNamespaceHandler in handlers/namespace/create.rs for complete example

Acceptance Scenarios:
1. Given all 28 handlers migrated/implemented, when building kalamdb-core, then compilation succeeds with zero errors.
2. Given all handlers registered, when HandlerRegistry is created, then `has_handler()` returns true for all 28 statement types (SELECT routes directly to DataFusion).
3. Given any SQL statement type, when executed via SqlExecutor, then it routes to the correct handler (or DataFusion for SELECT) and returns appropriate ExecutionResult.
4. Given each handler's unit tests, when running `cargo test`, then all tests pass with 100% handler coverage.
5. Given the handler registry guide, when a developer adds a new statement, then they can follow the 3-step process in under 30 minutes.
6. **Given ddl_legacy.rs deleted (T130-T131), when searching for ddl_legacy imports, then zero references found.**

Independent Test: 
- Execute one statement from each category (DDL, DML, Flush, Jobs, Subscription, User, Transaction) via REST API
- Verify all 7 categories route correctly to their handlers
- Verify authorization is enforced for admin-only operations
- Verify non-admin operations succeed for regular users
- **Verify ddl_legacy.rs file does not exist in codebase**

## Parameter Binding Implementation Notes

**Unified DataFusion Approach**: All DML operations (SELECT, INSERT, UPDATE, DELETE) use DataFusion's native parameter binding with `ScalarValue`:

### Architecture Overview

```rust
use datafusion::scalar::ScalarValue;
use datafusion::logical_expr::Expr;

// API layer: deserialize JSON params to Vec<ScalarValue>
let params: Vec<ScalarValue> = vec![
    ScalarValue::Int64(Some(123)),
    ScalarValue::Utf8(Some("Alice".to_string())),
];

// Executor: pass to DataFusion via execute_via_datafusion()
async fn execute_via_datafusion(
    ctx: &ExecutionContext,
    sql: &str,
    params: Vec<ScalarValue>,
) -> Result<Vec<RecordBatch>, KalamDbError> {
    let df = ctx.session.sql(sql).await?;
    
    // Parameter binding via LogicalPlan manipulation:
    // 1. Parse SQL with DataFusion (uses sqlparser-rs internally)
    // 2. DataFusion recognizes $1, $2, etc. as Expr::Placeholder
    // 3. Replace placeholders in LogicalPlan with ScalarValue literals
    // 4. Execute modified plan
    
    // TODO: Implement LogicalPlan traversal and placeholder replacement
    // Reference: DataFusion's PREPARE/EXECUTE implementation
    let batches = df.collect().await?;
    Ok(batches)
}
```

### Parsing Strategy

**Use DataFusion's Native Parser** (wraps sqlparser-rs):
- DataFusion already parses INSERT/UPDATE/DELETE statements
- Recognizes `$1`, `$2` positional placeholders automatically
- Converts to `Expr::Placeholder` in LogicalPlan
- No need for separate sqlparser-rs parsing in handlers

**Placeholder Replacement Logic**:
```rust
// Pseudo-code for parameter binding implementation
fn replace_placeholders_in_plan(
    plan: LogicalPlan,
    params: &[ScalarValue],
) -> Result<LogicalPlan> {
    // Traverse LogicalPlan recursively
    // Find all Expr::Placeholder nodes
    // Replace with Expr::Literal(params[placeholder_id - 1])
    // Validate placeholder count matches params.len()
    
    // DataFusion provides utilities for plan transformation:
    // - TreeNode trait for traversal
    // - ExprRewriter for expression replacement
    unimplemented!("See DataFusion's PreparedStatement implementation")
}
```

### DML Handler Strategy

- ✅ **SELECT**: Handled directly in `execute_via_datafusion()` - NO separate handler needed
- ✅ **INSERT/UPDATE/DELETE**: Thin wrapper handlers that delegate to `execute_via_datafusion()` with same parameter logic
- ⏳ **Parameter Binding**: Deferred until query handler implementation (requires LogicalPlan rewrite for `$1`, `$2` placeholder replacement)

### Implementation Pattern for INSERT/UPDATE/DELETE Handlers

```rust
impl TypedStatementHandler<InsertStatement> for InsertHandler {
    async fn execute(
        &self,
        stmt: &InsertStatement,
        ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Convert statement to SQL string with placeholders
        // DataFusion's sql() method already parses this correctly
        let sql = format!("INSERT INTO {} VALUES ($1, $2)", stmt.table_name);
        
        // Delegate to DataFusion with params
        // DataFusion recognizes $1, $2 as placeholders
        let batches = execute_via_datafusion(ctx, &sql, ctx.params.clone()).await?;
        
        // Return execution result
        Ok(ExecutionResult::Inserted {
            rows_affected: batches.iter().map(|b| b.num_rows()).sum(),
        })
    }
}
```

### Supported Placeholder Syntax

DataFusion supports PostgreSQL-style positional parameters:
- `$1`, `$2`, `$3`, ... (1-indexed)
- Example: `INSERT INTO users (id, name) VALUES ($1, $2)`
- Example: `UPDATE users SET name = $1 WHERE id = $2`
- Example: `DELETE FROM users WHERE id = $1`
- Example: `SELECT * FROM users WHERE id = $1 AND name = $2`

### Parameter Type Validation

DataFusion automatically validates:
- Parameter count (placeholder IDs must be ≤ params.len())
- Parameter types (via schema compatibility checks during planning)
- No custom validation needed in handlers

### Benefits

- **Zero conversion overhead**: No ParamValue → ScalarValue mapping
- **Unified parsing**: DataFusion handles all SQL parsing (wraps sqlparser-rs internally)
- **Automatic validation**: DataFusion validates parameter count, types, and schema compatibility
- **Consistent behavior**: All DML operations (SELECT, INSERT, UPDATE, DELETE) use identical parameter logic
- **No AST walking needed**: DataFusion's LogicalPlan provides structured access to placeholders
- **Battle-tested**: Leverages DataFusion's PREPARE/EXECUTE statement implementation

### Implementation References

- **DataFusion Prepared Statements**: `docs/source/user-guide/sql/prepared_statements.md`
- **Placeholder Expression**: `Expr::Placeholder` in DataFusion's logical expression tree
- **Example**: `PREPARE stmt(INT, VARCHAR) AS SELECT * FROM t WHERE id = $1 AND name = $2`

Functional Requirements:
- FR-013: Create `handlers/ddl/` directory and split responsibilities into `namespace.rs`, `create_table.rs`, `alter_table.rs`, and `helpers.rs`.
- FR-014: Move shared DDL utilities into `helpers.rs` with functions: `inject_auto_increment_field`, `inject_system_columns`, and `save_table_definition`.
- FR-015: Preserve the public API by re-exporting a single `ddl` module from `handlers::ddl` so external call sites remain stable.
- FR-016: Ensure all DDL-related tests compile and pass after the split, with no behavioral regressions.

### Handler Structure Requirements (User Story 6)

- FR-017: Create `handlers/ddl/` directory structure with separate files for namespace operations (`namespace.rs`), storage operations (`storage.rs`), and table operations (`table.rs`).
- FR-018: Create `handlers/dml/` directory with files for `insert.rs`, `update.rs`, and `delete.rs` (all delegate to `execute_via_datafusion` with DataFusion's ScalarValue parameter binding). SELECT is handled directly in `execute_via_datafusion()` and does NOT need a separate handler.
- FR-019: Create `handlers/flush/` directory with `table.rs` (FLUSH TABLE) and `all_tables.rs` (FLUSH ALL TABLES).
- FR-020: Create `handlers/jobs/` directory with `kill_job.rs` and `kill_live_query.rs` handlers.
- FR-021: Create `handlers/subscription/` directory with `subscribe.rs` handler for live query subscriptions.
- FR-022: Create `handlers/user/` directory with `create_user.rs`, `alter_user.rs`, and `drop_user.rs` handlers.
- FR-023: Create `handlers/transaction/` directory with `begin.rs`, `commit.rs`, and `rollback.rs` handlers.
- FR-024: Each handler file must contain: (1) struct implementing `TypedStatementHandler<T>`, (2) unit tests, (3) integration test hooks.
- FR-025: All handlers must accept `Arc<AppContext>` via constructor following Phase 10 patterns (no individual field passing).
- FR-026: DDL handlers must use `SchemaRegistry` for 50-100× faster lookups (no SQL queries via KalamSql).
- FR-027: DML handlers must delegate to DataFusion using `ScalarValue` for parameter binding. DataFusion's native parser (wrapping sqlparser-rs) recognizes `$1`, `$2` placeholders as `Expr::Placeholder` in LogicalPlan. Implement placeholder replacement by traversing the LogicalPlan and substituting placeholders with `Expr::Literal` values from `params: Vec<ScalarValue>`.
- FR-028: Job-related handlers must use `UnifiedJobManager` with typed JobIds and idempotency enforcement (Phase 9 patterns).

### Job Executors Requirements (Phase 8.5)

**Critical for System Tests**: Job executor implementations are REQUIRED for smoke tests and system tests to pass. Current status has most executors with TODO placeholders.

**Reference Architecture**: See `specs/009-core-architecture/PHASE9_EXECUTOR_IMPLEMENTATIONS.md` for detailed implementation patterns, retry logic, and status transitions.

**Executor Infrastructure** (Already Complete from Phase 9):
- UnifiedJobManager with retry logic (3× default, configurable via config.toml)
- JobRegistry with all 8 executors registered in AppContext
- Typed JobIds: FL-* (Flush), CL-* (Cleanup), RT-* (Retention), SE-* (StreamEviction), UC-* (UserCleanup), CO-* (Compact), BK-* (Backup), RS-* (Restore)
- Job status state machine: New → Queued → Running → Completed/Failed/Retrying/Cancelled
- Idempotency enforcement (prevents duplicate jobs with same key)
- Crash recovery (Running jobs marked as Failed on server restart)
- Exponential backoff for retries (1s → 2s → 4s → 8s configurable)

**Implementation Requirements**:

- FR-029: **FlushExecutor** - ✅ COMPLETE (100% functional, calls existing UserTableFlushJob/SharedTableFlushJob/StreamTableFlushJob)
  - Returns metrics: rows_flushed, parquet_files count
  - Handles User/Shared/Stream table types
  - Cannot be cancelled (flush must complete to maintain consistency)

- FR-030: **CleanupExecutor** - ⚠️ SIGNATURE ONLY (requires DDL refactoring)
  - Must call refactored DDL cleanup methods: `cleanup_table_data_internal()`, `cleanup_parquet_files_internal()`, `cleanup_metadata_internal()`
  - Validates parameters: table_id, table_type, operation ("drop_table")
  - Cannot be cancelled (ensures complete cleanup)
  - Returns metrics: tables_deleted, rows_deleted, bytes_freed

- FR-031: **RetentionExecutor** - ⚠️ SIGNATURE ONLY (requires store.scan_iter implementation)
  - Enforces `deleted_retention_hours` policy for soft-deleted records
  - Validates parameters: namespace_id, table_name, table_type, retention_hours
  - Implements batched deletion (1000 records per batch)
  - Can be cancelled (partial retention enforcement acceptable)
  - Returns JobDecision::Retry if more records remain (with backoff_ms)
  - Returns metrics: records_deleted, batches_processed

- FR-032: **StreamEvictionExecutor** - ⚠️ SIGNATURE ONLY (requires stream_table_store.scan_iter)
  - Enforces TTL policy based on `created_at` timestamp
  - Validates parameters: namespace_id, table_name, table_type ("Stream"), ttl_seconds, batch_size (default 10000)
  - Implements batched deletion with continuation support
  - Can be cancelled (partial eviction acceptable)
  - Returns JobDecision::Retry if batch_size records deleted (more may remain)
  - Returns metrics: records_evicted, batches_processed

- FR-033: **UserCleanupExecutor** - ⚠️ SIGNATURE ONLY (requires system table provider usage)
  - Permanently deletes soft-deleted user accounts
  - Validates parameters: user_id, username, cascade (boolean)
  - Implements cascade logic: delete user's tables (via cleanup jobs), remove from ACLs, delete live queries
  - Cannot be cancelled (ensures complete cleanup)
  - Returns metrics: user_deleted, tables_deleted, acl_entries_removed, live_queries_deleted

- FR-034: **CompactExecutor** - PLACEHOLDER (returns NotImplemented)
  - Future Phase: Merge multiple small Parquet files → single large file
  - Validates parameters: namespace_id, table_name, table_type, min_files (trigger threshold)
  - Returns metrics: files_before, files_after, bytes_saved

- FR-035: **BackupExecutor** - PLACEHOLDER (returns NotImplemented)
  - Future Phase: Create Parquet snapshots + metadata JSON
  - Validates parameters: namespace_id, table_name, backup_path, backend ("s3" | "filesystem")
  - Returns metrics: snapshot_size_bytes, backup_path, duration_ms

- FR-036: **RestoreExecutor** - PLACEHOLDER (returns NotImplemented)
  - Future Phase: Recreate table from Parquet snapshots
  - Validates parameters: namespace_id, table_name, backup_path, backend, overwrite (boolean)
  - Implements schema compatibility checks
  - Returns metrics: rows_restored, restore_path, duration_ms

**Retry Logic Requirements** (from specs/009-core-architecture/spec.md):

- FR-037: All executors must return `Result<JobDecision, KalamDbError>` from execute() method
- FR-038: JobDecision variants:
  - `Completed { message: Option<String> }` - Job succeeded, status → Completed
  - `Retry { message: String, exception_trace: Option<String>, backoff_ms: u64 }` - Temporary failure, retry with backoff
  - `Cancelled` - Job was cancelled, status → Cancelled
- FR-039: JobDecision::Retry triggers automatic retry up to max_retries (default 3, configurable)
- FR-040: Retry backoff must use exponential strategy: 1s × 2^(retry_count-1) (1s → 2s → 4s → 8s)
- FR-041: After max_retries exhausted, job status → Failed permanently (no more retries)
- FR-042: Job cancellation (via KILL JOB command) must call executor.cancel() method before transition to Cancelled status

**Status Transitions** (from specs/009-core-architecture/spec.md):

- FR-043: New → Queued: Job created with idempotency key, persisted to system.jobs
- FR-044: Queued → Running: JobManager.run_loop() picks job from queue, updates status, logs "[JobId] INFO - Job started"
- FR-045: Running → Completed: JobDecision::Completed returned, result stored, logs "[JobId] INFO - Job completed: {message}"
- FR-046: Running → Retrying: JobDecision::Retry returned, retry_count increments, logs "[JobId] WARN - Job failed, retrying in {backoff_ms}ms (attempt {retry_count}/{max_retries})"
- FR-047: Running → Failed: JobDecision::Retry returned but retry_count ≥ max_retries, logs "[JobId] ERROR - Job failed permanently: {message}"
- FR-048: Running → Cancelled: KILL JOB command received, executor.cancel() called, logs "[JobId] INFO - Job cancelled by user"
- FR-049: Retrying → Running: After backoff_ms delay, job re-queued for execution
- FR-050: Server restart: All Running jobs marked as Failed with message "Server restarted during execution"

**Idempotency Requirements** (from specs/009-core-architecture/spec.md):

- FR-051: Idempotency key format: `{job_type}:{namespace}:{table_name}:{date}` (e.g., "flush:default:events:20251104")
- FR-052: has_active_job_with_key() checks for Running, Queued, Retrying statuses (Completed and Failed jobs do NOT block)
- FR-053: Duplicate job creation returns error: "Job already running: {job_id} (status: {status}, created: {timestamp})"

**Testing Requirements**:

- FR-054: Unit tests for each executor: success scenario, error handling, parameter validation
- FR-055: Integration tests for retry logic: force failure → verify 3× retry → verify backoff timing
- FR-056: Integration tests for cancellation: start long-running job → KILL JOB → verify cleanup
- FR-057: Smoke tests must verify FlushExecutor works end-to-end: FLUSH TABLE → job created → execution → metrics returned
- FR-058: System tests for RetentionExecutor: create soft-deleted records → wait → verify cleanup
- FR-059: System tests for StreamEvictionExecutor: create TTL records → wait → verify eviction
- FR-029: Transaction handlers must integrate with future transaction manager (placeholder implementations for Phase 11). Until transaction manager is implemented, BEGIN/COMMIT/ROLLBACK handlers must return `KalamDbError::NotImplemented` with explicit message: "Transaction support planned for Phase 11".
- FR-030: All handlers must return `Result<ExecutionResult, KalamDbError>` with appropriate success messages and error context.
- FR-030a: Error responses MUST use structured format with machine-readable error codes, human-readable messages, and contextual details. Format: `{"code": "ERROR_CODE", "message": "description", "details": {...}}`. Common codes: `PARAM_COUNT_MISMATCH`, `PARAM_SIZE_EXCEEDED`, `PARAM_TYPE_MISMATCH`, `TIMEOUT`, `AUTHORIZATION_FAILED`, `NOT_IMPLEMENTED`.
- FR-031: Handler registration in `HandlerRegistry::new()` must use zero-boilerplate pattern: `registry.register_typed(handler, |stmt| match stmt { Variant(s) => Some(s), _ => None })`.
