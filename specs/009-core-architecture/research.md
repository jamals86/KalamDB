# Phase 0 Research: Core Architecture Refactor (009)

Scope: Validate assumptions, surface unknowns, and create an actionable checklist to remove StorageAdapter and KalamSql, consolidate modules, and unify the Job system without breaking tests.

## Open Questions (NEEDS CLARIFICATION)

- External dependencies on KalamSql semantics? Any tooling or scripts directly importing kalamdb-sql beyond tests?
- Legacy information_schema providers: confirm SystemTablesRegistry fully covers tables/columns without private helpers.
- Job system: confirm list of all job types currently in use and whether any dynamic types are created at runtime.
- Backward compatibility: do any consumers parse system.jobs “result” or “error_message” fields explicitly (e.g., CLI, SDK, dashboards)?

## Codebase Archaeology Checklist

- StorageAdapter: grep for `StorageAdapter` type, trait, and constructors; inventory all usages and plan removals.
- KalamSql: grep `kalamdb-sql` imports and any `use kalamdb_sql::` paths inside kalamdb-core and server binary.
- Legacy services: grep `NamespaceService`, `UserTableService`, `SharedTableService`, `StreamTableService`, `TableDeletionService` in kalamdb-core and backend/src.
- SqlExecutor routing: map all SqlStatement variants to current handlers; identify any inline logic still outside handlers.
- System tables: list providers in `tables/system/` and confirm registry registration paths in AppContext.
- Jobs: list files under `kalamdb-core/src/jobs/` (executor, managers, schedulers); identify code to be replaced by Unified JobManager.
- Models move: inspect `models/tables.rs` and plan target modules under `tables/*`; ensure temporary re-exports for compatibility.
- Schema vs catalog: list public exports; define re-export shim so `catalog::*` exposes SchemaRegistry and helpers.

## Risks and Mitigations

- Hidden KalamSql helper usage in tests — Mitigation: add thin provider helpers or migrate logic into providers; run focused tests.
- Cache invalidation errors after handlerization — Mitigation: follow Phase 10.2 invalidation patterns already in ddl.rs.
- Jobs status migration breaks dashboards — Mitigation: provide field-level BC mapping in query layer (message replaces result/error_message) and document changes.
- Wide changes increase merge conflicts — Mitigation: land module moves first (low risk), then handlerization, then jobs unification.

## Verification Strategy

- Pre-merge gates: grep checks for forbidden symbols (StorageAdapter, kalamdb_sql), and ensure zero matches.
- Build + tests: run workspace tests; target PASS without flaky skips.
- SchemaRegistry usage: audit handlers; enforce lookups only via registry.
- Jobs acceptance: implement smoke tests for status transitions and idempotency.

## Work Breakdown (Phase 0 Outputs)

1) Inventory and confirm all references (checklist above) — deliver findings inline here.
2) Decide BC approach for jobs fields and parameters migration.
3) Finalize re-export plan for schema→catalog and models moves.
4) Approve JobId prefixes and mapping table.

## Findings Log

- Detected kalamdb_sql and legacy services in server lifecycle and tests:
	- backend/src/lifecycle.rs imports kalamdb_sql::{KalamSql, RocksDbAdapter} and initializes NamespaceService, UserTableService, SharedTableService, StreamTableService, TableDeletionService
	- Multiple backend/tests/* files import KalamSql and legacy services (flush persistence, audit logging, edge cases, user sql commands, oauth, datatypes)
- DDL handler still references services and kalamdb_sql:
	- backend/crates/kalamdb-core/src/sql/executor/handlers/ddl.rs imports services and kalamdb_sql::ddl; constructs kalamdb_sql::Table/Storage
- kalamdb-core services modules still exist (namespace/shared/stream/table_deletion), with comments referencing kalamdb-sql
- kalamdb-auth references RocksDbAdapter in extractor/service

Action: Prioritize removing kalamdb_sql from lifecycle.rs and ddl.rs, replacing with AppContext + SchemaRegistry + SystemTablesRegistry, and delete or no-op legacy services after handlerization.
