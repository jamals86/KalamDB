# Implementation Plan: Core Architecture Refactor

**Branch**: `009-core-architecture` | **Date**: 2025-11-04 | **Spec**: specs/009-core-architecture/spec.md
**Input**: Feature specification from `/specs/009-core-architecture/spec.md`

Note: This plan is derived directly from the spec and AGENTS.md. It will guide Phase 0–2 outputs and code work.

## Summary

Refactor the core to a providers-only architecture by removing StorageAdapter and KalamSql, routing all SQL operations through focused SqlExecutor handlers that depend only on AppContext and SchemaRegistry. Consolidate modules (schema→catalog public surface, stores/system_table.rs under tables/, models/tables.rs into tables/*), and implement a Unified Job Management System with short typed JobIds, richer statuses, idempotency, retry/backoff, and dedicated jobs.log—keeping the build green with all tests passing.

## Technical Context

**Language/Version**: Rust 1.90 (edition 2021)  
**Primary Dependencies**: DataFusion 40, Apache Arrow 52, Apache Parquet 52, RocksDB 0.24, Actix-Web 4, serde 1.0, bincode, DashMap  
**Storage**: RocksDB for buffered writes via EntityStore; Parquet for flushed segments via StorageBackend abstraction  
**Testing**: cargo test across workspace; backend/cli/link crates have existing suites (477+ tests passing baseline per AGENTS.md)  
**Target Platform**: Linux/macOS servers; WASM client for link SDK  
**Project Type**: Multi-crate Rust workspace (backend, cli, link SDK)  
**Performance Goals**: Maintain or improve current hot-path latency; keep schema lookups ~1–2µs via SchemaRegistry; avoid >10% regressions  
**Constraints**: Enforce workspace dependency pattern; no duplicate system models; prefer enums over strings; AppContext as SoT for wiring  
**Scale/Scope**: System tables + user/shared/stream tables; full SQL executor routing; job system unification

## Constitution Check

GATE: Must pass before Phase 0 research; re-check after Phase 1 design and before code merge.

- Single source of truth for system models (kalamdb-commons/src/models/system.rs); no duplicates introduced — PASS (policy acknowledged; changes will import from commons)
- Workspace dependency management (root Cargo.toml `workspace.dependencies`; crates use `{ workspace = true }`) — PASS (verified policy; future deps will follow)
- Prefer enums over strings for types/roles/table kinds — PASS (design follows existing enums; any new fields will add enums in commons)
- AppContext provides SchemaRegistry and SystemTablesRegistry as integration points — PASS (current architecture; refactor will continue to use it)
- Remove StorageAdapter and KalamSql entirely — PENDING (implementation task; validated by grep and build)
- SqlExecutor handlerization using SchemaRegistry for lookups — PENDING (implementation task; validated by tests)
- Legacy services removed (NamespaceService, User/Shared/Stream services, TableDeletionService) — PENDING (implementation task)
- Build green with tests passing across backend/cli/link — PENDING (quality gate before merge)

Conclusion: Pre-check acceptable. Proceed to Phase 0 research with PENDING items tracked as primary objectives.

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
<!--
  ACTION REQUIRED: Replace the placeholder tree below with the concrete layout
  for this feature. Delete unused options and expand the chosen structure with
  real paths (e.g., apps/admin, packages/something). The delivered plan must
  not include Option labels.
-->

```text
backend/
├── crates/
│   ├── kalamdb-core/
│   │   ├── src/
│   │   │   ├── app_context.rs            # AppContext singleton (SoT for wiring)
│   │   │   ├── schema/                    # To be surfaced via catalog/ (SchemaRegistry lives here)
│   │   │   ├── catalog/                   # Public surface for schema + cache
│   │   │   ├── tables/
│   │   │   │   ├── system/                # SystemTablesRegistry + providers
│   │   │   │   ├── user_tables/
│   │   │   │   ├── shared_tables/
│   │   │   │   └── stream_tables/
│   │   │   ├── stores/                    # system_table store to migrate under tables/
│   │   │   ├── sql/executor/handlers/     # DDL/DML handlers (target of refactor)
│   │   │   └── jobs/                      # Legacy managers/executors (to be unified)
│   │   └── tests/
│   ├── kalamdb-commons/                   # System models, constants (single SoT)
│   ├── kalamdb-auth/
│   ├── kalamdb-store/
│   ├── kalamdb-sql/                       # To be removed from core execution path
│   └── ...
├── src/                                   # Server binary (lifecycle, routes)
└── tests/                                 # Integration tests for backend

cli/                                       # CLI crate (no changes needed in plan phase)
link/                                      # WASM client + TypeScript SDK
specs/009-core-architecture/               # This feature’s docs (plan/research/data-model/contracts)
```

Structure Decision: Multi-crate Rust workspace with focus on kalamdb-core. Refactor touches kalamdb-core (schema/catalog surface, tables/, sql/executor/handlers/, jobs/), backend/src (lifecycle wiring), and kalamdb-commons (job models update). No new top-level packages are created.

## Handler Architecture

### Target Structure

```text
backend/crates/kalamdb-core/src/sql/executor/
├── mod.rs                       # Orchestrator (~300 lines max)
│                                # - Parse SQL once via kalamdb_sql
│                                # - Authorization gateway (fail-fast)
│                                # - Route to handler based on SqlStatement variant
│                                # - No business logic in routing layer
└── handlers/
    ├── mod.rs                   # Handler exports and common traits
    ├── types.rs                 # Shared types (ExecutionResult, ExecutionContext, ParamValue)
    ├── helpers.rs               # Common utilities (extract repeated code)
    ├── authorization.rs         # check_authorization() gateway
    ├── audit.rs                 # Audit logging utilities
    ├── ddl.rs                   # CREATE, ALTER, DROP (tables/namespaces/storages)
    ├── dml.rs                   # INSERT, UPDATE, DELETE
    ├── query.rs                 # SELECT, DESCRIBE, SHOW
    ├── flush.rs                 # STORAGE FLUSH TABLE
    ├── subscription.rs          # LIVE SELECT
    ├── user_management.rs       # CREATE/ALTER/DROP USER
    ├── table_registry.rs        # REGISTER/UNREGISTER TABLE
    └── system_commands.rs       # VACUUM, OPTIMIZE, ANALYZE
```

### Key Principles

1. **Single-Pass Parsing**: Parse SQL once via `kalamdb_sql`, pass `SqlStatement` to handlers
2. **Authorization Gateway**: Fail-fast security check before routing
3. **Type-Safe Routing**: Use `kalamdb_sql::SqlStatement` enum (30+ variants already exist!)
4. **Parameter Binding**: Support `?` placeholders via `params: Vec<ParamValue>`
5. **Namespace Extraction**: Extract `NamespaceId` from statement, fallback if not specified
6. **Type-Safe Identifiers**: Use `NamespaceId`, `UserId`, `TableName`, `TableId` (NOT strings!)
7. **DataFusion Caching**: Investigate built-in query caching (DataFusion v40.0+) before custom implementation
8. **No Code Duplication**: Extract repeated patterns to `helpers.rs` or parent traits
9. **Minimal Handlers**: Each handler file is focused, delegates to providers/stores/registries

### Unified Execution Context

**CRITICAL CONSOLIDATION**: Replace `KalamSessionState` with enhanced `ExecutionContext` to eliminate duplication.

```rust
/// Unified execution context for SQL queries and DataFusion sessions
/// 
/// This struct consolidates the previous `ExecutionContext` and 
/// `KalamSessionState` to eliminate duplication. It contains ALL context needed for:
/// - SQL executor authorization (user_id, user_role)
/// - DataFusion session creation (namespace_id)
/// - Audit logging (request_id, ip_address, timestamp)
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// User ID executing the query (type-safe wrapper)
    pub user_id: UserId,
    
    /// User role for authorization checks (User, Service, Dba, System)
    pub user_role: Role,
    
    /// Namespace for the query (for DataFusion catalog context)
    pub namespace_id: NamespaceId,
    
    /// Optional request ID for distributed tracing and audit logging
    pub request_id: Option<String>,
    
    /// Optional client IP address for audit logging
    pub ip_address: Option<String>,
    
    /// Timestamp when the context was created
    pub timestamp: SystemTime,
}
```

### SqlExecutor Signature (Refactored)

```rust
impl SqlExecutor {
    /// Execute SQL with parameter binding and metadata
    pub async fn execute_with_metadata(
        &self,
        session: &SessionContext,                  // DataFusion session (ONE source)
        sql: &str,                                  // Raw SQL query
        params: Vec<ParamValue>,                    // NEW: Parameter binding for `?` placeholders
        fallback_namespace: Option<NamespaceId>,   // CHANGED: Type-safe + optional
        execution_context: &ExecutionContext,       // CONSOLIDATED: All auth + audit context
    ) -> Result<(ExecutionResult, ExecutionMetadata), KalamDbError> {
        // 1. Parse SQL once (single-pass)
        let statement = kalamdb_sql::parse(sql)?;
        
        // 2. Extract namespace from statement or use fallback
        let namespace = statement.namespace().or(fallback_namespace)
            .ok_or(KalamDbError::MissingNamespace)?;
        
        // 3. Authorization gateway (fail-fast)
        authorization::check_authorization(&statement, execution_context)?;
        
        // 4. Route to handler based on SqlStatement variant
        let result = match statement {
            SqlStatement::CreateTable { .. } => ddl::execute_create_table(session, statement, execution_context).await?,
            SqlStatement::AlterTable { .. } => ddl::execute_alter_table(session, statement, execution_context).await?,
            SqlStatement::DropTable { .. } => ddl::execute_drop_table(session, statement, execution_context).await?,
            SqlStatement::Insert { .. } => dml::execute_insert(session, statement, params, execution_context).await?,
            SqlStatement::Select { .. } => query::execute_select(session, statement, params, execution_context).await?,
            // ... 25+ more variants
            _ => return Err(KalamDbError::UnsupportedStatement),
        };
        
        // 5. Audit logging
        audit::log_execution(&statement, &result, execution_context).await?;
        
        Ok((result, metadata))
    }
}
```

### Handler Contract (Common Trait)

```rust
/// Common trait for all SQL statement handlers
pub trait StatementHandler {
    /// Execute a SQL statement with full context
    async fn execute(
        &self,
        session: &SessionContext,
        statement: SqlStatement,
        params: Vec<ParamValue>,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError>;
    
    /// Validate authorization before execution (called by gateway)
    fn check_authorization(
        &self,
        statement: &SqlStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError>;
}
```

### Code Duplication Strategy

**Extract to `helpers.rs`**:
- Namespace resolution logic
- Table metadata lookups via SchemaRegistry
- Common authorization patterns
- Error message formatting
- Audit log entry creation
- Parameter value conversion

**Extract to parent traits**:
- `StatementHandler` trait (common interface)
- `AuthorizationCheck` trait (per-statement validation)
- `AuditLogger` trait (structured logging)

### Dependencies

- **User Story 8**: SchemaRegistry integration for cache invalidation
- **AppContext**: Dependency injection for all handlers (registry access)
- **kalamdb-sql crate**: `SqlStatement` enum, parser, `QueryCache` (existing infrastructure)
- **Unified ExecutionContext**: Single source of session + auth state (eliminates `KalamSessionState`)

### Parameter Binding & Caching

**Parameter Binding**:
- Support `?` placeholders in SQL
- `params: Vec<ParamValue>` passed to handlers
- Handlers bind parameters to DataFusion logical plan

**Query Caching Preparation** (future optimization):
- Normalize SQL with placeholders for cache keys
- Investigate DataFusion v40.0+ built-in query plan caching
- Store prepared `LogicalPlan` in SchemaRegistry if needed
- Deferred to post-MVP (focus on correctness first)

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| N/A | — | — |
