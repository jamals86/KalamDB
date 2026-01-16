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

### Phase 3 Audit Results (2025-11-04)

#### T020-T024: Module Consolidation (Complete)
**Re-export Strategy for Backward Compatibility**:
- **Pattern**: Move modules to logical locations, add re-exports in old locations
- **Benefit**: Zero breaking changes for existing code, gradual migration path
- **Implementation**:
  1. **System Table Store** (`stores/system_table.rs` → `tables/system/system_table_store.rs`):
     - Re-export added in `stores/system_table.rs`: `pub use crate::tables::system::system_table_store::*;`
     - Both import paths work: `stores::system_table::*` and `tables::system::system_table_store::*`
  
  2. **Table Row Models** (`models/tables.rs` → `tables/*/mod.rs`):
     - Models already existed in `tables/user_tables/user_table_store.rs`, `tables/shared_tables/shared_table_store.rs`, `tables/stream_tables/stream_table_store.rs`
     - Replaced duplicate definitions in `models/tables.rs` with re-exports
     - Both import paths work: `models::tables::*` and `tables::<type>_tables::*`
  
  3. **SchemaRegistry** (exposed from catalog module):
     - Added re-export in `catalog/mod.rs`: `pub use crate::schema::{SchemaRegistry, TableMetadata};`
     - Both import paths work: `schema::SchemaRegistry` and `catalog::SchemaRegistry`

**Benefits**:
- ✅ No code changes required in existing files
- ✅ New code can use cleaner import paths
- ✅ Gradual migration: old imports can be updated incrementally
- ✅ Zero breaking changes

#### T025: Storage Module Audit
- **All modules are actively used** - No dead code found
- **Module Status**:
  - `column_family_manager.rs`: ✅ Used by user_table_service.rs (legacy service to be removed in Phase 8)
  - `filesystem_backend.rs`: ✅ Used internally (self-contained, no external usage)
  - `parquet_writer.rs`: ✅ Used by flush jobs (user_table_flush.rs, shared_table_flush.rs)
  - `path_template.rs`: ✅ Used for storage path template substitution (self-contained utility)
  - `storage_registry.rs`: ✅ Used by AppContext and DDL handlers for storage validation
- **Recommendation**: Keep all modules, remove only when legacy services are deleted in Phase 8

#### T026: Test Results
- **Build Status**: kalamdb-core has 124 pre-existing compilation errors (unrelated to Phase 3)
- **Phase 3 Impact**: 0 new errors introduced by module consolidation
- **Verification**: No imports of `models/tables`, `stores/system_table`, or `catalog::SchemaRegistry` exist yet
- **Conclusion**: Module consolidation successful, backward compatibility preserved

### Phase 4 Audit Results (2025-11-05)

#### T029-T034: Information Schema Provider Investigation
**Critical Finding**: Phase 4 (US0a) is based on **FALSE ASSUMPTION** - there is NO "legacy" implementation to delete.

**Current Reality**:
- Only ONE implementation exists: `information_schema_tables.rs` and `information_schema_columns.rs`
- These providers ARE registered in SystemTablesRegistry (registry.rs lines 79-80)
- They are ACTIVELY used by SqlExecutor (sql/executor/mod.rs lines 1091, 1103) and old executor.rs (lines 991, 1003)
- Deleting them would BREAK the system completely

**The Real Problem**:
- These providers depend on `KalamSql` (registered in registry.rs with `kalam_sql.clone()`)
- Phase 6 aims to remove KalamSql dependency
- Therefore, information_schema providers need REFACTORING, not DELETION

**Corrected Plan**:
- **Skip Phase 4** (based on false premise)
- **Phase 6 extension**: Refactor information_schema providers to use SchemaRegistry instead of KalamSql
- Keep providers in place, just update their data source

**Recommendation**: Mark Phase 4 as "SKIPPED - Invalid assumption, addressed in Phase 6"

### Phase 1 Audit Results (2025-11-04)

#### T001: StorageAdapter Audit
- **Total References**: 11 occurrences in backend/
- **Primary Location**: backend/crates/kalamdb-sql/src/adapter.rs (struct definition)
- **Usage Pattern**: 
  - Type alias RocksDbAdapter = StorageAdapter (backwards compatibility)
  - Used in KalamSql initialization
  - Referenced in schema/registry.rs comments (Phase 10.1 migration notes)
- **Removal Plan**: 
  1. Delete kalamdb-sql/src/adapter.rs
  2. Remove StorageAdapter from kalamdb-sql/src/lib.rs exports
  3. Update lifecycle.rs to remove RocksDbAdapter initialization
  4. Clean up migration comments in schema/registry.rs

#### T002: KalamSql Usage Audit
- **Total References**: 100+ `use kalamdb_sql` imports across backend/
- **Core Locations (kalamdb-core)**:
  - tables/system/registry.rs (SystemTablesRegistry constructor)
  - tables/system/information_schema_*.rs (2 providers)
  - live_query/manager.rs, live_query/change_detector.rs
  - storage/storage_registry.rs
  - app_context.rs
  - executor.rs (old, may be obsolete)
  - jobs/*.rs (executor, stream_eviction, user_cleanup)
  - services/*.rs (ALL 5 legacy services + backup/restore/schema_evolution)
  - sql/executor/handlers/ddl.rs
- **Test Files**: backend/tests/* (17+ integration test files)
- **Removal Plan**:
  1. Phase 6: Remove from lifecycle.rs first
  2. Phase 6: Update DDL handlers to use SchemaRegistry + SystemTablesRegistry
  3. Phase 6: Remove from test helpers
  4. Phase 8: Delete legacy services (removes 5 service imports)
  5. Keep: backup/restore/schema_evolution services (still valid)
  6. Final: Remove information_schema providers or migrate to pure providers

#### T003: Legacy Services Audit
- **Services to Remove** (5 total):
  1. NamespaceService (backend/crates/kalamdb-core/src/services/namespace_service.rs)
  2. UserTableService (backend/crates/kalamdb-core/src/services/user_table_service.rs)
  3. SharedTableService (backend/crates/kalamdb-core/src/services/shared_table_service.rs)
  4. StreamTableService (backend/crates/kalamdb-core/src/services/stream_table_service.rs)
  5. TableDeletionService (backend/crates/kalamdb-core/src/services/table_deletion_service.rs)
- **Usage Locations**:
  - backend/src/lifecycle.rs (initialization)
  - backend/crates/kalamdb-core/src/sql/executor/mod.rs
  - backend/crates/kalamdb-core/src/sql/executor/handlers/ddl.rs
  - backend/tests/* (17 integration test files)
- **Services to Keep** (3 valid services):
  - backup_service.rs
  - restore_service.rs
  - schema_evolution_service.rs
- **Removal Plan**: Phase 8 (after handlers complete) - delete files + update imports

#### T004: SqlExecutor Handler Structure
- **Current Structure**:
  ```
  backend/crates/kalamdb-core/src/sql/executor/
  ├── mod.rs (orchestrator)
  ├── handlers/
  │   ├── mod.rs
  │   ├── authorization.rs ✓
  │   ├── ddl.rs ✓
  │   ├── transaction.rs ✓
  │   ├── types.rs ✓
  │   └── tests/
  └── tests/
  ```
- **Missing Handlers** (from plan.md):
  - dml.rs (INSERT, UPDATE, DELETE)
  - query.rs (SELECT, DESCRIBE, SHOW)
  - flush.rs (STORAGE FLUSH TABLE)
  - subscription.rs (LIVE SELECT)
  - user_management.rs (CREATE/ALTER/DROP USER)
  - table_registry.rs (REGISTER/UNREGISTER TABLE)
  - system_commands.rs (VACUUM, OPTIMIZE, ANALYZE)
  - helpers.rs (common utilities)
  - audit.rs (audit logging utilities)
- **Creation Plan**: Phase 7 (User Story 3) - create 9 new handler files

#### T005: SystemTablesRegistry Coverage
- **Verified Complete**: registry.rs contains all 10 system table providers
- **Providers Registered**:
  1. system.users ✓ (UsersTableProvider)
  2. system.jobs ✓ (JobsTableProvider)
  3. system.namespaces ✓ (NamespacesTableProvider)
  4. system.storages ✓ (StoragesTableProvider)
  5. system.live_queries ✓ (LiveQueriesTableProvider)
  6. system.tables ✓ (TablesTableProvider)
  7. system.audit_logs ✓ (AuditLogsTableProvider)
  8. system.stats ✓ (StatsTableProvider - virtual)
  9. information_schema.tables ✓ (InformationSchemaTablesProvider)
  10. information_schema.columns ✓ (InformationSchemaColumnsProvider)
- **Current Dependencies**: 
  - KalamSql used for information_schema providers (lines 13, 75-76)
  - StorageBackend used for EntityStore providers
- **Migration Note**: Phase 6 will need to handle information_schema providers differently

#### T006: Job Types Review
- **Current JobType Enum** (backend/crates/kalamdb-commons/src/models/job_type.rs):
  - Flush ✓
  - Compact ✓
  - Cleanup ✓
  - Backup ✓
  - Restore ✓
- **Missing from Enum** (per data-model.md spec):
  - Retention (NOT in current enum)
  - StreamEviction (NOT in current enum)
  - UserCleanup (NOT in current enum)
  - Unknown (NOT in current enum - fallback variant)
- **Implementation Status**:
  - Executors exist for stream_eviction and user_cleanup in jobs/ directory
  - No short_prefix() method on JobType enum
  - JobStatus enum needs New, Queued, Retrying variants
- **Addition Plan**: Phase 2 (T008-T011) - extend enums with missing variants

### Action Items Summary

**Phase 2 (Foundation)**: 
- Add 4 missing job types (Retention, StreamEviction, UserCleanup, Unknown)
- Add short_prefix() method to JobType
- Extend JobStatus with New, Queued, Retrying

**Phase 5 (US1 - Remove StorageAdapter)**:
- Delete kalamdb-sql/src/adapter.rs
- Remove StorageAdapter from lifecycle.rs

**Phase 6 (US2 - Remove KalamSql)**:
- Remove KalamSql from lifecycle.rs
- Update DDL handlers
- Update SystemTablesRegistry (handle information_schema providers)
- Update test helpers

**Phase 7 (US3 - Handlers)**:
- Create 9 missing handler files

**Phase 8 (US4 - Legacy Services)**:
- Delete 5 legacy service files
- Update imports in lifecycle.rs, executor, tests

### Original Findings Log

- Detected kalamdb_sql and legacy services in server lifecycle and tests:
	- backend/src/lifecycle.rs imports kalamdb_sql::{KalamSql, RocksDbAdapter} and initializes NamespaceService, UserTableService, SharedTableService, StreamTableService, TableDeletionService
	- Multiple backend/tests/* files import KalamSql and legacy services (flush persistence, audit logging, edge cases, user sql commands, oauth, datatypes)
- DDL handler still references services and kalamdb_sql:
	- backend/crates/kalamdb-core/src/sql/executor/handlers/ddl.rs imports services and kalamdb_sql::ddl; constructs kalamdb_sql::Table/Storage
- kalamdb-core services modules still exist (namespace/shared/stream/table_deletion), with comments referencing kalamdb-sql
- kalamdb-auth references RocksDbAdapter in extractor/service

Action: Prioritize removing kalamdb_sql from lifecycle.rs and ddl.rs, replacing with AppContext + SchemaRegistry + SystemTablesRegistry, and delete or no-op legacy services after handlerization.
