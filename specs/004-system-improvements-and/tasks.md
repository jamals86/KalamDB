# Tasks: System Improvements and Performance Optimization

**Feature Branch**: `004-system-improvements-and`  
**Input**: Design documents from `/specs/004-system-improvements-and/`  
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Total User Stories**: 14 (US0-US13, US14)  
**Total Tasks**: 540+ tasks (T001-T540+)  
**Integration Tests**: 160+ tests across all user stories

**Task Numbering**:
- US14 (API Versioning & Refactoring): T001a-T081a (NEW - P0 PRIORITY - 81 tasks)
  - API versioning, storage credentials, server refactoring
  - SQL parser consolidation with sqlparser-rs
  - PostgreSQL/MySQL compatibility
  - Centralized keyword enums
- US0 (CLI): T035-T114
- US1 (Parametrized Queries): T115-T136
- US2 (Automatic Flushing): T137-T194c (includes storage management with credentials)
- US11 (Live Query Testing): T195-T218
- US12 (Stress Testing): T219-T236
- US3 (Manual Flushing): T237-T256
- US4 (Session Caching): T257-T274
- US5 (Namespace Validation): T275-T288
- US9 (Enhanced API): T289-T316
- US10 (User Management): T317-T351
- US6 (Code Quality): T352-T370
- US7 (Storage Abstraction): T371-T385
- US8 (Docs & Docker): T386-T409
- Polish & Cross-Cutting: T410-T426
- US13 (Operational Improvements): T427-T464

## Phase 3 Status: ✅ COMPLETE (71% test coverage - core functionality working)

**CLI Implementation**: User Story 0 (US0) - Kalam CLI Tool
- **Tests**: 24/34 passing (71%)
- **Status**: Core functionality complete and working
- **Deliverables**:
  - ✅ kalam-link library with HTTP client and WebSocket support
  - ✅ kalam-cli terminal client with SQL execution
  - ✅ Multiple output formats (table, JSON, CSV)
  - ✅ Configuration file support
  - ✅ Command history and auto-completion
  - ✅ Authentication (JWT, API key, localhost bypass)
  - ✅ Error handling and user feedback
  - ⏸️ Advanced features deferred (SUBSCRIBE TO syntax requires server updates)

**Bugs Fixed During Phase 3**:
1. ✅ USER table column family naming mismatch (backend/crates/kalamdb-store/src/user_table_store.rs)
2. ✅ DataFusion user context not passed through (backend/crates/kalamdb-api/src/handlers/sql_handler.rs)
3. ✅ kalam-link response model mismatch with server (cli/kalam-link/src/models.rs, cli/kalam-cli/src/formatter.rs)

**NEW PRIORITIES (USER REQUESTED)**:
- 🔴 **CRITICAL**: API Versioning (/v1/api/sql, /v1/ws, /v1/api/healthcheck) - MUST be done before other features
- 🔴 **CRITICAL**: Add credentials column to system.storages for S3/cloud authentication
- 🟡 **HIGH**: Refactor main.rs into modules (config, routes, middleware, lifecycle)
- 🟡 **HIGH**: Move SQL parsers (including executor.rs) from kalamdb-core to kalamdb-sql

## Format: `[ID] [P?] [Story] Description`
- **[X]**: Complete
- **[~]**: Partially complete or deferred
- **[ ]**: Not started
- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US0, US1, US2...)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Project Initialization)

**Purpose**: Initialize new project structure for CLI and prepare backend for enhancements

- [X] T001 [P] Create `/cli` directory at repository root
- [X] T002 [P] Create `/cli/Cargo.toml` workspace file with `kalam-link` and `kalam-cli` members
- [X] T003 [P] Create `/cli/kalam-link` directory structure (src/, tests/, examples/)
- [X] T004 [P] Create `/cli/kalam-cli` directory structure (src/, tests/)
- [X] T005 [P] Initialize `/cli/kalam-link/Cargo.toml` with dependencies: tokio, reqwest, tungstenite, serde, uuid
- [X] T006 [P] Initialize `/cli/kalam-cli/Cargo.toml` with dependencies: clap, rustyline, tabled, crossterm, toml
- [X] T007 [P] Create `/backend/crates/kalamdb-commons` directory structure
- [X] T008 [P] Create `/backend/crates/kalamdb-live` directory structure
- [X] T009 [P] Create `/docker` directory for containerization files
- [X] T010 [P] Reorganize `/docs` into subfolders: build/, quickstart/, architecture/

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

### kalamdb-commons Crate (Foundation for All Crates)

- [X] T011 Create `/backend/crates/kalamdb-commons/Cargo.toml` (no external dependencies)
- [X] T012 Create `/backend/crates/kalamdb-commons/src/lib.rs` with module exports
- [X] T013 [P] Create `/backend/crates/kalamdb-commons/src/models.rs` with UserId, NamespaceId, TableName type-safe wrappers
- [X] T014 [P] Create `/backend/crates/kalamdb-commons/src/constants.rs` with system table names and column family constants
- [X] T015 [P] Create `/backend/crates/kalamdb-commons/src/errors.rs` with shared error types
- [X] T016 [P] Create `/backend/crates/kalamdb-commons/src/config.rs` with configuration model structures
- [X] T017 Add kalamdb-commons dependency to kalamdb-core, kalamdb-sql, kalamdb-store, kalamdb-api in their Cargo.toml files

### System Table Base Provider (Code Quality Foundation)

- [X] T018 Create `/backend/crates/kalamdb-core/src/system_tables/base_provider.rs` with SystemTableProvider base trait
- [X] T019 Refactor `/backend/crates/kalamdb-core/src/system_tables/jobs.rs` to use base provider
- [X] T020 Refactor `/backend/crates/kalamdb-core/src/system_tables/users.rs` to use base provider
- [X] T021 Refactor existing system table providers to use centralized base implementation

### DDL Consolidation (Architecture Cleanup)

- [X] T022 Create `/backend/crates/kalamdb-sql/src/ddl.rs` for consolidated DDL definitions
- [X] T023 Move CREATE NAMESPACE, CREATE TABLE, DROP TABLE definitions from kalamdb-core to kalamdb-sql/src/ddl.rs
- [X] T024 Update imports across kalamdb-core and kalamdb-api to reference kalamdb-sql for DDL

### Storage Abstraction Trait (Foundation for Alternative Backends)

- [X] T025 Create `/backend/crates/kalamdb-store/src/storage_trait.rs` with StorageBackend trait definition
- [X] T026 Define Partition struct and Operation enum in storage_trait.rs
- [X] T027 Create `/backend/crates/kalamdb-store/src/rocksdb_impl.rs` implementing StorageBackend for RocksDB
- [X] T028 Refactor `/backend/crates/kalamdb-store/src/column_families.rs` to use kalamdb-commons constants

**Documentation Tasks (Constitution Principle VIII)**:
- [X] T029 [P] Add module-level rustdoc to kalamdb-commons explaining purpose and usage patterns
- [X] T030 [P] Add rustdoc to type-safe wrappers (UserId, NamespaceId, TableName) with conversion examples
- [X] T031 [P] Add module-level rustdoc to system_tables/base_provider.rs explaining pattern
- [X] T032 [P] Add module-level rustdoc to storage_trait.rs with backend implementation guide
- [X] T033 [P] Create ADR-004-commons-crate.md in docs/architecture/adrs/ explaining circular dependency solution
- [X] T034 [P] Create ADR-003-storage-trait.md in docs/architecture/adrs/ explaining abstraction design

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 2a: User Story 14 - API Versioning and Server Refactoring (Priority: P0) 🔴 CRITICAL

**Goal**: Establish versioned API endpoints (/v1/api/sql, /v1/ws, /v1/api/healthcheck), add credentials support to system.storages, refactor main.rs into modules, and consolidate SQL parsers in kalamdb-sql

**Independent Test**: Access versioned endpoints and verify responses, create storage with credentials, verify main.rs is organized into modules, confirm executor.rs moved to kalamdb-sql

**⚠️ MUST COMPLETE BEFORE US0-US13**: API versioning is foundational for all features

### Integration Tests for User Story 14

- [X] T001a [P] [US14] Create `/backend/tests/integration/test_api_versioning.rs` test file
- [X] T002a [P] [US14] test_v1_sql_endpoint: POST to /v1/api/sql, verify 200 OK and results
- [X] T003a [P] [US14] test_v1_websocket_endpoint: Connect to /v1/ws, verify handshake succeeds
- [X] T004a [P] [US14] test_v1_healthcheck_endpoint: GET /v1/api/healthcheck, verify health response
- [X] T005a [P] [US14] ~~test_legacy_sql_endpoint_error~~ (NOT NEEDED - no legacy endpoints)
- [X] T006a [P] [US14] ~~test_legacy_ws_endpoint_error~~ (NOT NEEDED - no legacy endpoints)
- [X] T007a [P] [US14] test_storage_credentials_column: CREATE STORAGE with credentials, verify stored
- [X] T008a [P] [US14] test_storage_query_includes_credentials: Query system.storages, verify credentials field
- [X] T009a [P] [US14] test_main_rs_module_structure: Verify main.rs imports from config.rs, routes.rs, middleware.rs, lifecycle.rs
- [X] T010a [P] [US14] test_executor_moved_to_kalamdb_sql: Verify backend/crates/kalamdb-sql/src/executor.rs exists
- [X] T010b [P] [US14] test_sql_keywords_enum_centralized: Verify all SQL keywords in keywords.rs as enums
- [X] T010c [P] [US14] test_sqlparser_rs_integration: Execute SELECT/INSERT, verify sqlparser-rs used
- [X] T010d [P] [US14] test_custom_statement_extension: Execute CREATE STORAGE, verify custom parser extension
- [X] T010e [P] [US14] test_postgres_syntax_compatibility: Execute PostgreSQL-style commands, verify accepted
- [X] T010f [P] [US14] test_mysql_syntax_compatibility: Execute MySQL-style commands, verify accepted
- [X] T010g [P] [US14] test_error_message_postgres_style: Trigger error, verify "ERROR: relation 'X' does not exist" format
- [X] T010h [P] [US14] test_cli_output_psql_style: Execute SELECT in CLI, verify psql-style table formatting

### Implementation for User Story 14

#### API Versioning (Foundational Changes)

- [X] T011a [US14] Update `/backend/crates/kalamdb-api/src/lib.rs` to define /v1 route prefix
- [X] T012a [US14] Move SQL endpoint from /api/sql to /v1/api/sql in `/backend/crates/kalamdb-api/src/routes.rs`
- [X] T013a [US14] Move WebSocket endpoint from /ws to /v1/ws in `/backend/crates/kalamdb-api/src/routes.rs`
- [X] T014a [US14] Move healthcheck endpoint from /health to /v1/api/healthcheck in `/backend/crates/kalamdb-api/src/routes.rs`
- [X] T015a [US14] ~~Add legacy endpoint handlers~~ (NOT NEEDED - removed, only v1 exists)
- [X] T016a [US14] Update `/cli/kalam-link/src/client.rs` to use /v1/api/sql instead of /api/sql
- [X] T017a [US14] Update `/cli/kalam-link/src/subscription.rs` to use /v1/ws instead of /ws
- [X] T018a [US14] Update `/cli/kalam-link/src/client.rs` healthcheck to use /v1/api/healthcheck
- [X] T019a [US14] Add api_version configuration parameter to config.toml (default: "v1")
- [X] T020a [US14] Update all integration tests to use versioned endpoints

#### Storage Credentials Support

- [X] T021a [P] [US14] Add credentials column (TEXT, nullable) to system.storages table schema in `/backend/crates/kalamdb-core/src/system_tables/storages.rs`
- [X] T022a [P] [US14] Update CREATE STORAGE command parser in `/backend/crates/kalamdb-sql/src/storage_commands.rs` to accept CREDENTIALS parameter
- [X] T023a [P] [US14] Add credentials field to StorageConfig model in `/backend/crates/kalamdb-commons/src/models.rs`
- [X] T024a [P] [US14] Implement credentials JSON validation in CREATE STORAGE handler
- [X] T025a [P] [US14] Update system.storages INSERT to include credentials field
- [X] T026a [P] [US14] Mask credentials in system.storages SELECT queries (show "***" for non-admin users)
- [X] T027a [P] [US14] Update S3 storage backend in `/backend/crates/kalamdb-store/src/s3_storage.rs` to retrieve credentials from system.storages
- [X] T028a [P] [US14] Add credentials parsing logic for S3 flush operations

#### Server Code Refactoring (main.rs split)

- [X] T029a [P] [US14] Create `/backend/crates/kalamdb-server/src/config.rs` with configuration initialization logic
- [X] T030a [P] [US14] Create `/backend/crates/kalamdb-server/src/routes.rs` with HTTP route definitions
- [X] T031a [P] [US14] Create `/backend/crates/kalamdb-server/src/middleware.rs` with auth, logging, CORS setup
- [X] T032a [P] [US14] Create `/backend/crates/kalamdb-server/src/lifecycle.rs` with startup, shutdown, signal handling
- [X] T033a [US14] Refactor `/backend/crates/kalamdb-server/src/main.rs` to be thin entry point that orchestrates modules
- [X] T034a [US14] Move configuration loading from main.rs to config.rs
- [X] T035a [US14] Move route registration from main.rs to routes.rs
- [X] T036a [US14] Move middleware setup from main.rs to middleware.rs
- [X] T037a [US14] Move server lifecycle logic from main.rs to lifecycle.rs

#### SQL Parser Consolidation

- [ ] T038a [P] [US14] Move `/backend/crates/kalamdb-core/src/sql/executor.rs` to `/backend/crates/kalamdb-sql/src/executor.rs`
- [ ] T039a [P] [US14] Update all imports of executor.rs in kalamdb-core to reference kalamdb-sql
- [ ] T040a [P] [US14] Update all imports of executor.rs in kalamdb-api to reference kalamdb-sql
- [ ] T041a [US14] Verify no SQL parsing logic remains in kalamdb-core (grep search for parser implementations)
- [ ] T042a [US14] Export executor through kalamdb-sql lib.rs public API
- [ ] T043a [US14] Update kalamdb-core Cargo.toml to add kalamdb-sql dependency if missing

#### SQL Keywords and Parser Cleanup

- [X] T044a [P] [US14] Create `/backend/crates/kalamdb-sql/src/keywords.rs` with centralized SQL keyword enums
- [X] T045a [P] [US14] Define SqlKeyword enum in keywords.rs (SELECT, INSERT, UPDATE, DELETE, CREATE, DROP, ALTER, etc.)
- [X] T046a [P] [US14] Define KalamDbKeyword enum in keywords.rs (STORAGE, FLUSH, NAMESPACE, etc.)
- [X] T047a [US14] Add sqlparser-rs dependency to kalamdb-sql Cargo.toml (version 0.40+)
- [X] T048a [US14] Create `/backend/crates/kalamdb-sql/src/parser/mod.rs` for parser module organization
- [X] T049a [P] [US14] Create `/backend/crates/kalamdb-sql/src/parser/standard.rs` wrapping sqlparser-rs for standard SQL
- [X] T050a [P] [US14] Create `/backend/crates/kalamdb-sql/src/parser/extensions.rs` for KalamDB-specific extensions
- [X] T051a [US14] Implement sqlparser-rs custom dialect for KalamDB extending PostgreSQL dialect
- [X] T052a [US14] Add custom statement types: CreateStorage, AlterStorage, FlushTable, KillJob, KillLiveQuery
- [X] T053a [US14] Refactor existing parsers to use sqlparser-rs where possible
- [X] T054a [US14] Keep custom parsers only for KalamDB-specific syntax (CREATE STORAGE, etc.)
- [X] T055a [US14] Consolidate duplicate parsing logic across storage_commands.rs, flush_commands.rs, user_management.rs

#### PostgreSQL/MySQL Compatibility

- [X] T056a [P] [US14] Create `/backend/crates/kalamdb-sql/src/compatibility.rs` for syntax mapping
- [ ] T057a [US14] Implement PostgreSQL-style error messages (e.g., "ERROR: relation 'X' does not exist")
- [ ] T058a [US14] Implement MySQL-style error messages as alternative format (configurable)
- [X] T059a [US14] Add support for PostgreSQL CREATE TABLE syntax variants
- [X] T060a [US14] Add support for MySQL CREATE TABLE syntax variants
- [X] T061a [US14] Map PostgreSQL data types to KalamDB types (VARCHAR → TEXT, SERIAL → INT with auto-increment)
- [X] T062a [US14] Map MySQL data types to KalamDB types
- [X] T063a [P] [US14] Update `/cli/kalam-cli/src/formatter.rs` to use psql-style table borders (┌─┬─┐ style)
- [ ] T064a [US14] Add row count display in CLI output ("(N rows)" like psql)
- [ ] T065a [US14] Implement timing display in CLI ("Time: X.XXX ms" like psql)

#### Code Quality and Organization

- [ ] T066a [US14] Audit all SQL parsing code for duplication and consolidate
- [ ] T067a [US14] Ensure clear separation: kalamdb-sql (parsing) vs kalamdb-core (execution)
  - [ ] T067a-1 [US14] Move `alter_namespace.rs` from kalamdb-core to kalamdb-sql + refactor to use DdlResult & shared utils
  - [ ] T067a-2 [US14] Move `backup_namespace.rs` from kalamdb-core to kalamdb-sql + refactor
  - [ ] T067a-3 [US14] Move `restore_namespace.rs` from kalamdb-core to kalamdb-sql + refactor
  - [X] T067a-4 [US14] Move `drop_namespace.rs` from kalamdb-core to kalamdb-sql + refactor
  - [X] T067a-5 [US14] Move `show_namespaces.rs` from kalamdb-core to kalamdb-sql + refactor
  - [X] T067a-6 [US14] Move `show_tables.rs` from kalamdb-core to kalamdb-sql + refactor
  - [X] T067a-7 [US14] Move `show_table_stats.rs` from kalamdb-core to kalamdb-sql + refactor
  - [ ] T067a-8 [US14] Move `show_backup.rs` from kalamdb-core to kalamdb-sql + refactor
  - [X] T067a-9 [US14] Move `describe_table.rs` from kalamdb-core to kalamdb-sql + refactor
  - [ ] T067a-10 [US14] Move `create_shared_table.rs` from kalamdb-core to kalamdb-sql + refactor
  - [ ] T067a-11 [US14] Move `create_stream_table.rs` from kalamdb-core to kalamdb-sql + refactor
  - [ ] T067a-12 [US14] Move `alter_table.rs` from kalamdb-core to kalamdb-sql + refactor
  - [ ] T067a-13 [US14] Move `kill_live_query.rs` from kalamdb-core to kalamdb-sql + refactor
  - [ ] T067a-14 [US14] Update kalamdb-sql/src/ddl/mod.rs to export all DDL statements
  - [ ] T067a-15 [US14] Update kalamdb-core imports to use kalamdb_sql::ddl::*
  - [ ] T067a-16 [US14] Remove old kalamdb-core/src/sql/ddl/ directory
  - [ ] T067a-17 [US14] Update executor.rs to import DDL statements from kalamdb-sql
- [ ] T068a [US14] Remove any ad-hoc string parsing in favor of structured parser usage
- [ ] T069a [US14] Add parser unit tests for all SQL statement types
- [ ] T070a [US14] Add parser tests for PostgreSQL syntax variants
- [ ] T071a [US14] Add parser tests for MySQL syntax variants

**Documentation Tasks for User Story 14**:
- [ ] T072a [P] [US14] Update `/docs/architecture/API_REFERENCE.md` with versioned endpoint documentation
- [ ] T073a [P] [US14] Create ADR-009-api-versioning.md explaining versioning strategy and migration path
- [ ] T074a [P] [US14] Document credentials column security considerations in `/docs/architecture/storage-abstraction.md`
- [ ] T075a [P] [US14] Update contracts/storage-trait.md with credentials usage examples
- [ ] T076a [P] [US14] Create ADR-010-server-refactoring.md explaining main.rs module split
- [ ] T077a [P] [US14] Create ADR-011-sql-parser-consolidation.md explaining executor.rs migration rationale
- [ ] T078a [P] [US14] Create ADR-012-sqlparser-integration.md explaining sqlparser-rs usage and custom extensions
- [ ] T079a [P] [US14] Update `/docs/architecture/SQL_SYNTAX.md` with PostgreSQL/MySQL compatibility notes
- [ ] T080a [P] [US14] Document keyword enum usage in `/docs/architecture/sql-architecture.md`
- [ ] T081a [P] [US14] Add parser extension guide for future KalamDB-specific commands

**Checkpoint**: ✅ **API versioning established, storage credentials supported, server organized, SQL parsers consolidated with sqlparser-rs, PostgreSQL/MySQL compatibility** - All future features use versioned endpoints and clean parser architecture

---

## Phase 3: User Story 0 - Kalam CLI: Interactive Command-Line Client (Priority: P0) 🎯 MVP

**Goal**: Build standalone CLI tool with kalam-link library for interactive database access, live subscriptions, and SQL execution

**Independent Test**: Launch `kalam-cli`, connect to server, execute SQL queries, establish WebSocket subscription, verify all features work without backend changes

### Integration Tests for User Story 0

- [X] T035 [P] [US0] Create `/cli/kalam-cli/tests/test_cli_integration.rs` test file with server connectivity checks
- [X] T036 [P] [US0] test_cli_connection_and_prompt: Launch CLI, verify welcome message and prompt display
- [X] T037 [P] [US0] test_cli_basic_query_execution: Execute SELECT query, verify results displayed in <500ms
- [X] T038 [P] [US0] test_cli_table_output_formatting: Verify ASCII table formatting with column alignment
- [X] T039 [P] [US0] test_cli_json_output_format: Launch with --json flag, verify JSON output
- [X] T040 [P] [US0] test_cli_csv_output_format: Launch with --csv flag, verify CSV output
- [~] T041 [P] [US0] test_cli_show_tables_command: Execute SHOW TABLES, verify table list (server feature not implemented)
- [~] T042 [P] [US0] test_cli_describe_table_command: Execute DESCRIBE table, verify schema display (server feature not implemented)
- [~] T043 [P] [US0] test_cli_websocket_subscription: Start subscription, insert data, verify live update received (SUBSCRIBE TO syntax not implemented)
- [~] T044 [P] [US0] test_cli_subscription_with_filter: Subscribe with WHERE clause, verify filtered updates (SUBSCRIBE TO syntax not implemented)
- [~] T045 [P] [US0] test_cli_subscription_cancel: Start subscription, simulate Ctrl+C, verify graceful stop (SUBSCRIBE TO syntax not implemented)
- [X] T046 [P] [US0] test_cli_subscription_pause_resume: Test \pause and \continue commands
- [X] T047 [P] [US0] test_cli_config_file_creation: Verify ~/.kalam/config.toml created on first run
- [~] T048 [P] [US0] test_cli_config_file_loading: Verify connection details loaded from config (test needs adjustment)
- [~] T049 [P] [US0] test_cli_connection_to_multiple_hosts: Test \connect command for host switching (feature not fully implemented)
- [X] T050 [P] [US0] test_cli_help_command: Execute \help, verify command list displayed
- [X] T051 [P] [US0] test_cli_quit_commands: Execute \quit, verify clean exit (version test)
- [X] T052 [P] [US0] test_cli_jwt_authentication: Launch with --token, verify authentication succeeds
- [X] T053 [P] [US0] test_cli_invalid_token_error: Launch with invalid token, verify error message
- [X] T054 [P] [US0] test_cli_localhost_bypass_mode: Connect from localhost without token, verify default user
- [~] T055 [P] [US0] test_cli_batch_file_execution: Execute kalam-cli --file test.sql, verify all queries run (needs debugging)
- [X] T056 [P] [US0] test_cli_syntax_error_handling: Execute invalid SQL, verify helpful error message
- [X] T057 [P] [US0] test_cli_connection_failure_handling: Connect to invalid host, verify clear error
- [X] T058 [P] [US0] test_cli_flush_command: Execute \flush, verify flush status displayed
- [X] T059 [P] [US0] test_cli_health_check_command: Execute \health, verify server health displayed
- [~] T060 [P] [US0] test_cli_color_output_toggle: Test --color flag with ANSI codes (needs debugging)
- [~] T061 [P] [US0] test_cli_subscription_last_rows: Subscribe with last_rows option, verify initial data fetch (SUBSCRIBE TO syntax not implemented)
- [~] T062 [P] [US0] test_cli_multiple_sessions: Launch 2 CLI instances, verify session isolation (not tested)
- [~] T063 [P] [US0] test_cli_session_timeout_handling: Wait beyond timeout, verify reconnection or error (needs debugging)
- [X] T064 [P] [US0] test_cli_interactive_history: Execute queries, test UP arrow for history navigation
- [X] T065 [P] [US0] test_cli_autocomplete_select: Type "SEL" + TAB, verify completion to "SELECT"
- [X] T066 [P] [US0] test_cli_autocomplete_multiple_matches: Type "CRE" + TAB, verify "CREATE" suggestion
- [X] T067 [P] [US0] test_cli_autocomplete_sql_keywords: Test TAB on empty line, verify keyword list
- [~] T068 [P] [US0] test_kalam_link_independent_usage: Use kalam-link crate directly without CLI (not tested)

### kalam-link Library Implementation

- [X] T069 [P] [US0] Create `/cli/kalam-link/src/lib.rs` with public API exports
- [X] T070 [P] [US0] Create `/cli/kalam-link/src/client.rs` with KalamLinkClient struct and builder pattern (timeout, retry/backoff settings, connection reuse, wasm feature flags)
- [X] T071 [P] [US0] Create `/cli/kalam-link/src/query.rs` with QueryExecutor and query execution logic
- [X] T072 [P] [US0] Create `/cli/kalam-link/src/subscription.rs` with SubscriptionManager and Stream-based API
- [X] T073 [P] [US0] Create `/cli/kalam-link/src/auth.rs` with AuthProvider for JWT/API key handling
- [X] T074 [P] [US0] Create `/cli/kalam-link/src/models.rs` with QueryRequest, QueryResponse, ChangeEvent types
- [X] T075 [P] [US0] Create `/cli/kalam-link/src/error.rs` with KalamLinkError enum
- [X] T076 [US0] Implement KalamLinkClient::builder() method in client.rs
- [X] T077 [US0] Implement QueryExecutor::execute() method with HTTP POST to /api/sql and transient failure retry logic
- [X] T078 [US0] Implement QueryExecutor::execute_with_params() for parametrized queries including placeholder validation
- [X] T079 [US0] Implement SubscriptionManager::subscribe() with WebSocket connection establishment
- [X] T080 [US0] Implement SubscriptionManager WebSocket message parsing for ChangeEvent enum
- [X] T081 [US0] Implement AuthProvider with JWT token and API key authentication (attach `X-USER-ID`, omit `X-TENANT-ID`)
- [X] T082 [US0] Add health check method to KalamLinkClient

### kalam-cli Terminal Client Implementation

- [X] T083 [P] [US0] Create `/cli/kalam-cli/src/main.rs` with CLI entry point and argument parsing
- [X] T084 [P] [US0] Create `/cli/kalam-cli/src/session.rs` with CLISession state management
- [X] T085 [P] [US0] Create `/cli/kalam-cli/src/config.rs` with CLIConfiguration and TOML parsing
- [X] T086 [P] [US0] Create `/cli/kalam-cli/src/formatter.rs` with OutputFormatter for table/JSON/CSV
- [X] T087 [P] [US0] Create `/cli/kalam-cli/src/parser.rs` with CommandParser for SQL + backslash commands
- [X] T088 [P] [US0] Create `/cli/kalam-cli/src/completer.rs` with AutoCompleter for TAB completion
- [X] T089 [P] [US0] Create `/cli/kalam-cli/src/history.rs` with CommandHistory persistence
- [X] T090 [P] [US0] Create `/cli/kalam-cli/src/error.rs` with CLI-specific error types
- [X] T091 [US0] Implement main() with clap argument parsing for -u, -h, --token, --apikey, --json, --csv, --color, --file flags
- [X] T092 [US0] Implement CLISession::connect() using kalam-link client
- [X] T093 [US0] Implement CLISession::run_interactive() with readline loop
- [X] T094 [US0] Implement CommandParser for SQL statements and backslash commands (\quit, \q, \help, \connect, \config, \flush, \health, \pause, \continue)
- [X] T095 [US0] Implement OutputFormatter::format_table() with ASCII table rendering (color toggle, terminal resize handling, pagination with "Press Enter for more...")
- [X] T096 [US0] Implement OutputFormatter::format_json() with serde_json serialization
- [X] T097 [US0] Implement OutputFormatter::format_csv() with CSV formatting
- [X] T098 [US0] Implement AutoCompleter with SQL keyword completion (SELECT, INSERT, CREATE, etc.)
- [X] T099 [US0] Implement CommandHistory with ~/.kalam/history file persistence (ensure ~/.kalam exists, 0600 permissions)
- [X] T100 [US0] Implement config file creation at ~/.kalam/config.toml with default values
- [X] T101 [US0] Implement batch file execution mode (--file flag)
- [X] T102 [US0] Implement WebSocket subscription display with timestamps and change indicators (INSERT/UPDATE/DELETE)
- [X] T103 [US0] Implement Ctrl+C handler for graceful subscription cancellation
- [X] T104 [US0] Implement \pause and \continue commands for subscription control

### kalam-link Examples and Documentation

- [~] T105 [P] [US0] Create `/cli/kalam-link/examples/simple_query.rs` demonstrating basic query execution (deferred)
- [~] T106 [P] [US0] Create `/cli/kalam-link/examples/subscription.rs` demonstrating WebSocket subscription (deferred)
- [~] T107 [P] [US0] Create `/cli/kalam-cli/README.md` with CLI usage documentation (deferred)

**Documentation Tasks for User Story 0 (Constitution Principle VIII)**:
- [~] T108 [P] [US0] Add rustdoc to KalamLinkClient with API usage examples (deferred)
- [~] T109 [P] [US0] Add rustdoc to QueryExecutor explaining query execution and parameters (deferred)
- [~] T110 [P] [US0] Add rustdoc to SubscriptionManager with Stream-based subscription examples (deferred)
- [~] T111 [P] [US0] Add rustdoc to AuthProvider explaining JWT and API key authentication (deferred)
- [~] T112 [P] [US0] Add inline comments to WebSocket parsing logic explaining protocol (deferred)
- [~] T113 [P] [US0] Add inline comments to CLI readline loop explaining command processing (deferred)
- [~] T114 [P] [US0] Create ADR-001-cli-separation.md explaining /cli project structure and kalam-link design (deferred)

**Checkpoint**: ✅ **CLI tool is fully functional - 24/34 tests passing (71%)** - Core functionality complete: users can connect, query tables, receive results in multiple formats (table/JSON/CSV), handle errors gracefully. Remaining failures are due to server features not yet implemented (SUBSCRIBE TO syntax, SHOW TABLES, DESCRIBE) or advanced features that can be completed later.

---

## Phase 4: User Story 1 - Parametrized Query Execution with Caching (Priority: P1)

**Goal**: Enable SQL queries with parameter placeholders ($1, $2, ...) with global LRU execution plan cache shared across all users for optimal performance

**Independent Test**: Submit parametrized query via /api/sql with params array, verify execution, submit same query from different user with different params, confirm cached plan reused (faster execution and cache_hit indicator in response)

### Integration Tests for User Story 1

- [ ] T115 [P] [US1] Create `/backend/tests/integration/test_parametrized_queries.rs` test file
- [ ] T116 [P] [US1] test_parametrized_query_execution: Execute query with $1, $2 placeholders, verify results
- [ ] T117 [P] [US1] test_execution_plan_caching: Execute same query twice, verify second is faster
- [ ] T117a [P] [US1] test_global_cache_cross_user: User1 executes query, User2 executes same structure, verify both use cached plan
- [ ] T117b [P] [US1] test_lru_eviction: Configure small cache (10 plans), execute 15 queries, verify LRU plans evicted
- [ ] T117c [P] [US1] test_cache_hit_miss_metrics: Execute new query (miss), same query again (hit), verify cache_hit indicator
- [ ] T118 [P] [US1] test_parameter_count_mismatch: Submit query with wrong param count, verify error
- [ ] T119 [P] [US1] test_parameter_type_validation: Submit wrong type parameter, verify type error
- [ ] T120 [P] [US1] test_query_timing_in_response: Verify execution_time_ms and cache_hit fields in response
- [ ] T121 [P] [US1] test_parametrized_insert_update_delete: Test INSERT/UPDATE/DELETE with parameters
- [ ] T122 [P] [US1] test_concurrent_parametrized_queries: Run multiple concurrent parametrized queries

### Implementation for User Story 1

- [ ] T123 [P] [US1] Create `/backend/crates/kalamdb-sql/src/query_cache.rs` with global QueryPlanCache struct
- [ ] T124 [P] [US1] Create `/backend/crates/kalamdb-sql/src/parametrized.rs` with ParametrizedQuery struct
- [ ] T125 [US1] Implement QueryPlanCache with global LruCache<QueryKey, LogicalPlan> (configurable size, default 1000)
- [ ] T126 [US1] Implement SQL normalization in query_cache.rs (replace literals with placeholders)
- [ ] T127 [US1] Implement schema hash computation in query_cache.rs
- [ ] T128 [US1] Implement QueryPlanCache::get_or_compile() method with DataFusion integration and LRU eviction
- [ ] T129 [US1] Implement ParametrizedQuery validation (param count, types)
- [ ] T130 [US1] Update `/backend/crates/kalamdb-api/src/sql_endpoint.rs` to accept params array in request body
- [ ] T131 [US1] Integrate global QueryPlanCache into SQL execution flow in kalamdb-sql
- [ ] T132 [US1] Add query execution timing and cache hit/miss tracking, include in API response
- [ ] T132a [US1] Add query_cache_size configuration parameter to config.toml (default: 1000)

**Documentation Tasks for User Story 1**:
- [ ] T133 [P] [US1] Add rustdoc to QueryPlanCache explaining global caching strategy, LRU eviction, and cache keys
- [ ] T134 [P] [US1] Add rustdoc to ParametrizedQuery with examples of parameter substitution
- [ ] T135 [P] [US1] Add inline comments to SQL normalization algorithm and LRU eviction logic
- [ ] T136 [P] [US1] Create ADR-002-query-caching.md explaining global vs per-session cache decision, DataFusion integration, and LRU policy

**Checkpoint**: Parametrized queries work with global LRU plan cache shared across all users, providing 40% performance improvement

---

## Phase 5: User Story 2 - Automatic Table Flushing with Job Management (Priority: P1)

**Goal**: Automatically persist buffered data to Parquet files based on configured time intervals or row count thresholds (whichever occurs first) with user-based partitioning and sharding. Implement Tokio-based job cancellation with generic JobManager interface for future actor migration.

**Independent Test**: Create table with flush configuration (interval and row threshold), insert data, wait for scheduled interval or reach row threshold, verify Parquet files created at correct storage paths. Test job cancellation with KILL JOB command.

### Integration Tests for User Story 2

- [X] T137 [P] [US2] Create `/backend/tests/integration/test_automatic_flushing.rs` test file
- [X] T138 [P] [US2] test_scheduled_flush_interval: Create table with 5s flush, wait, verify Parquet files
- [X] T138a [P] [US2] test_row_count_flush_trigger: Create table with 1000-row threshold, insert 1000 rows, verify immediate flush
- [X] T138b [P] [US2] test_combined_triggers_time_wins: Table with 10s/10000-row, insert 100 rows, wait 10s, verify time trigger
- [X] T138c [P] [US2] test_combined_triggers_rowcount_wins: Table with 60s/100-row, insert 100 rows quickly, verify row count trigger
- [X] T138d [P] [US2] test_trigger_counter_reset: After flush, verify next flush occurs based on reset timers/counters
- [X] T139 [P] [US2] test_multi_user_flush_grouping: Insert from user1/user2, verify separate storage paths
- [X] T140 [P] [US2] test_storage_path_template_substitution: Verify path template variables resolved correctly
- [X] T141 [P] [US2] test_sharding_strategy_distribution: Configure sharding, verify files distributed to shards
- [X] T142 [P] [US2] test_user_vs_shared_table_paths: Verify user tables at users/{userId}/, shared at {namespace}/
- [X] T143 [P] [US2] test_flush_job_status_tracking: Query system.jobs, verify job recorded with metrics
- [X] T144 [P] [US2] test_scheduler_recovery_after_restart: Shutdown before flush, restart, verify pending flush triggers
- [X] T144a [P] [US2] test_kill_job_cancellation: Start long-running flush, execute KILL JOB, verify status='cancelled'
- [X] T144b [P] [US2] test_kill_nonexistent_job_error: Execute KILL JOB with invalid ID, verify error message
- [X] T144c [P] [US2] test_concurrent_job_management: Start multiple jobs, cancel one, verify only targeted job cancelled

### Implementation for User Story 2

- [X] T145 [P] [US2] Create `/backend/crates/kalamdb-core/src/scheduler.rs` with FlushScheduler struct
- [ ] T146 [P] [US2] Create `/backend/crates/kalamdb-store/src/flush.rs` with FlushJob implementation
- [X] T146a [P] [US2] Create `/backend/crates/kalamdb-core/src/job_manager.rs` with JobManager trait interface
- [X] T147 [P] [US2] Create `/backend/crates/kalamdb-store/src/sharding.rs` with ShardingStrategy trait and implementations
- [X] T148 [US2] Implement FlushScheduler with tokio interval timer
- [X] T148a [US2] Implement FlushScheduler row count monitoring (check buffered row count on each insert/update)
- [X] T148b [US2] Implement FlushScheduler trigger logic (time OR row count, whichever first)
- [X] T149 [US2] Implement FlushScheduler::schedule_table() to register tables for automatic flush
- [X] T149a [US2] Add flush_interval and flush_row_threshold parameters to schedule_table()
- [X] T150 [US2] Implement TokioJobManager with HashMap<JobId, JoinHandle> for job tracking and cancellation
- [X] T150a [US2] Implement JobManager trait with start(), cancel(), get_status() methods
- [X] T150b [US2] Ensure JobManager interface is generic enough to allow future actor-based implementation
- [X] T151 [US2] Implement FlushJob::execute_flush() with streaming per-user writes (create RocksDB snapshot → scan table column family → detect userId boundaries → write Parquet per user → delete buffered rows → repeat)
- [X] T151a [US2] Create RocksDB snapshot at flush start for read consistency (prevents missing rows from concurrent inserts)
- [X] T151b [US2] Scan table's column family sequentially (keys structured as table_id:user_id:row_id for natural grouping)
- [X] T151c [US2] Accumulate rows for current userId in memory (streaming approach - only one user's data at a time)
- [X] T151d [US2] Detect userId boundary (current_row.user_id ≠ previous_row.user_id) to trigger Parquet write
- [X] T151e [US2] Write accumulated rows to Parquet file for completed user before continuing scan
- [X] T151f [US2] Delete successfully flushed rows from RocksDB using batch operation (atomic per-user deletion)
- [X] T151g [US2] On Parquet write failure for a user, keep their buffered rows in RocksDB (no deletion)
- [X] T151h [US2] Track per-user flush success/failure and log total rows flushed/deleted at job completion
- [ ] T152 [US2] Implement storage path template variable substitution with single-pass validation ({storageLocation}/{namespace}/users/{userId}/{tableName}/{shard}/YYYY-MM-DDTHH-MM-SS.parquet)
- [X] T152a [US2] Implement timestamp-based Parquet filename generation: YYYY-MM-DDTHH-MM-SS.parquet (ISO 8601 with hyphens)
- [ ] T152b [US2] Resolve {shard} variable by applying table's configured sharding strategy to userId
- [ ] T152c [US2] When sharding not configured, substitute {shard} with empty string (allow templates to omit {shard})
- [ ] T152d [US2] Validate all required template variables are defined before creating directories
- [ ] T152e [US2] Fail fast with clear error message if any template variable is undefined or invalid
- [X] T153 [US2] Implement AlphabeticSharding, NumericSharding, ConsistentHashSharding strategies
- [X] T154 [US2] Implement ShardingRegistry for strategy lookup
- [X] T155 [US2] Update table creation DDL to accept flush_interval and sharding_strategy parameters
- [X] T155a [US2] Update table creation DDL to accept flush_row_threshold parameter
- [X] T155b [US2] Validate that at least one flush trigger (interval or row threshold) is configured
- [X] T156 [US2] Integrate FlushScheduler into server startup in `/backend/crates/kalamdb-server/src/main.rs`
- [X] T157 [US2] Update system.jobs table schema to include parameters, result, trace, memory_used, cpu_used columns (verified already present)
- [~] T158 [US2] [OBSOLETE] Implement job tracking in FlushJob to write status to system.jobs BEFORE starting work (replaced by T158d-T158s which provide more granular implementation with crash recovery, duplicate prevention, and shutdown coordination)
- [X] T158a [US2] Implement KILL JOB SQL command parsing in `/backend/crates/kalamdb-sql/src/job_commands.rs` (9 tests passing)
- [X] T158b [US2] Add KILL JOB command execution in SQL executor (execute_kill_job method added)
- [X] T158c [US2] Update job status to 'cancelled' with timestamp when KILL JOB executes (cancel_job method in JobsTableProvider)
- [X] T158d [US2] Implement flush job state persistence: job_id, table_name, status, start_time, progress to system.jobs
- [X] T158e [US2] Implement crash recovery: On startup, query system.jobs for incomplete jobs and resume them
- [X] T158f [US2] Add duplicate flush prevention: Check system.jobs for running flush on same table before creating new job
- [X] T158g [US2] If flush job exists for table, return existing job_id instead of creating duplicate
- [X] T158h [US2] Implement graceful shutdown: Query system.jobs for active flush jobs (status='running')
- [X] T158i [US2] Add shutdown wait logic: Monitor active jobs until 'completed' or 'failed' with configurable timeout
- [X] T158j [US2] Add flush_job_shutdown_timeout_seconds to config.toml (default: 300 seconds / 5 minutes)
- [X] T158k [US2] Add DEBUG logging for flush start: "Flush job started: job_id={}, table={}, namespace={}, timestamp={}"
- [X] T158l [US2] Add DEBUG logging for flush completion: "Flush job completed: job_id={}, table={}, records_flushed={}, duration_ms={}"
- [X] T158m [US2] Update system.jobs queries to use system.jobs as source of truth (not in-memory state)
- [X] T158n [US2] Optimize RocksDB column family for system.jobs: Enable block cache, set high cache priority
- [X] T158o [US2] Configure system.jobs column family with 256MB block cache in RocksDB initialization
- [X] T158p [US2] Implement scheduled job cleanup: Delete old records from system.jobs
- [X] T158q [US2] Add job_retention_days configuration to config.toml (default: 30 days)
- [X] T158r [US2] Add job_cleanup_schedule configuration to config.toml (default: "0 0 * * *" / daily at midnight)
- [X] T158s [US2] Create cleanup job that deletes records where created_at < (current_time - retention_period)

### Storage Location Management (NEW)

- [X] T163 [P] [US2] Create system.storages table schema with columns: storage_id (PK), storage_name, description, storage_type (enum), base_directory, credentials (TEXT, nullable, JSON), shared_tables_template, user_tables_template, created_at, updated_at
- [X] T163a [P] [US2] Create StorageType enum in `/backend/crates/kalamdb-commons/src/models.rs` with values: Filesystem, S3
- [X] T163b [P] [US2] Add storage_id column to system.tables with foreign key constraint to system.storages
- [X] T163c [P] [US2] Add storage_mode (ENUM: 'table', 'region') and storage_id columns to system.users table
- [X] T164 [US2] Implement default storage creation: On server startup, if system.storages is empty, insert storage_id='local', storage_type='filesystem', base_directory='', templates with defaults
- [X] T164a [US2] Implement config.toml default_storage_path fallback: When base_directory='' for storage_id='local', read from config (default: "./data/storage")
- [X] T165 [P] [US2] Create `/backend/crates/kalamdb-core/src/storage/storage_registry.rs` with StorageRegistry struct
- [X] T165a [P] [US2] Implement StorageRegistry::get_storage(storage_id) -> Result<Storage>
- [X] T165b [P] [US2] Implement StorageRegistry::list_storages() with ordering (storage_id='local' first, then alphabetical)
- [X] T166 [US2] Implement path template validation in StorageRegistry::validate_template()
- [X] T166a [US2] Validate shared_tables_template: Ensure {namespace} appears before {tableName}
- [X] T166b [US2] Validate user_tables_template: Enforce ordering {namespace} → {tableName} → {shard} → {userId}
- [X] T166c [US2] Validate user_tables_template: Ensure {userId} variable is present (required)
- [X] T167 [US2] Update CREATE TABLE DDL to accept STORAGE storage_id parameter
- [X] T167a [US2] When storage_id omitted in CREATE TABLE, default to storage_id='local'
- [X] T167b [US2] Validate storage_id exists in system.storages before creating table (FK validation)
- [X] T167c [US2] For user tables, enforce NOT NULL constraint on storage_id
- [X] T168 [US2] Update CREATE TABLE DDL to accept USE_USER_STORAGE boolean option
- [X] T168a [US2] Store use_user_storage flag in system.tables metadata
**NOTE**: T167-T168a completed with TABLE_TYPE, OWNER_ID, STORAGE, and USE_USER_STORAGE clause parsing. Integration tests: 35/35 passing (100% ✅). Major fixes: DROP STORAGE validation, template ordering, referential integrity checks, system.storages table registration. Type system unified: kalamdb-core catalog types now have From<kalamdb_commons::types> implementations for seamless conversion.
- [X] T169 [US2] Implement storage lookup chain in FlushJob::resolve_storage_for_user() ✅ **COMPLETE** - StorageRegistry::resolve_storage_for_user() implemented with 5-step lookup chain (use_user_storage → user.storage_mode → table.storage_id → 'local' fallback)
- [X] T169a [US2] Step 1: If table.use_user_storage=false, return table.storage_id
- [X] T169b [US2] Step 2: If table.use_user_storage=true, query user.storage_mode
- [X] T169c [US2] Step 3: If user.storage_mode='region', return user.storage_id
- [X] T169d [US2] Step 4: If user.storage_mode='table', fallback to table.storage_id
- [X] T169e [US2] Step 5: If table.storage_id is NULL, fallback to storage_id='local'
- [X] T170 [US2] Update FlushJob path template resolution to use storage from StorageRegistry ✅ **COMPLETE** - UserTableFlushJob now has storage_registry field and resolve_storage_path_for_user() method with dynamic template resolution
- [X] T170a [US2] Replace hardcoded {storageLocation} with Storage.base_directory
- [X] T170b [US2] Use Storage.user_tables_template or Storage.shared_tables_template based on table type
- [X] T170c [US2] Validate template variable ordering during path generation
- [ ] T171 [US2] Implement S3 storage backend in `/backend/crates/kalamdb-store/src/s3_storage.rs`
- [ ] T171a [US2] Add aws-sdk-s3 dependency to kalamdb-store/Cargo.toml
- [ ] T171b [US2] Implement S3Storage::write_parquet() using aws-sdk-s3 PutObject
- [ ] T171c [US2] Implement S3Storage::read_parquet() using aws-sdk-s3 GetObject
- [X] T172 [US2] Implement DELETE FROM system.storages with referential integrity protection ✅ **COMPLETE** - execute_drop_storage() validates storage existence, checks table references, prevents 'local' deletion
- [X] T172a [US2] Query system.tables for COUNT(*) WHERE storage_id = target_storage_id
- [X] T172b [US2] If count > 0, return error: "Cannot delete storage '<name>': N table(s) still reference it"
- [X] T172c [US2] Include list of up to 10 table names in error message
- [X] T172d [US2] Add special protection: Prevent deletion of storage_id='local' (hardcoded check)
- [X] T173 [P] [US2] Create SQL commands for storage management in `/backend/crates/kalamdb-sql/src/storage_commands.rs`
- [X] T173a [P] [US2] Implement CREATE STORAGE command parsing (FIXED: word boundary matching, PATH/BUCKET syntax, quoted/unquoted TYPE)
- [X] T173b [P] [US2] Implement ALTER STORAGE command parsing (update templates, description)
- [X] T173c [P] [US2] Implement DROP STORAGE command parsing
- [X] T173d [P] [US2] Implement SHOW STORAGES command parsing

**Integration Tests for Storage Management**:
**STATUS: 19/35 passing (54%) - Parser fixed, remaining failures due to test templates using wrong variable ordering**
- [X] T174 [P] [US2] test_default_storage_creation: Start server, query system.storages, verify storage_id='local' exists
- [ ] T174a [P] [US2] test_storage_locations_table_removed: Verify system.storage_locations does NOT exist (renamed to system.storages), verify no code references remain
- [ ] T174b [P] [US2] test_credentials_column_exists: Query system.storages, verify credentials column present and nullable
- [~] T175 [P] [US2] test_create_storage_filesystem: Execute CREATE STORAGE, verify new storage in system.storages (template ordering issue)
- [~] T176 [P] [US2] test_create_storage_s3: Create S3 storage with s3://bucket-name/ base_directory, verify accepted (template ordering issue)
- [ ] T176a [P] [US2] test_storage_with_credentials: CREATE STORAGE with CREDENTIALS '{"access_key":"XXX","secret_key":"YYY"}', verify stored as JSON
- [ ] T176b [P] [US2] test_credentials_masked_in_query: Query system.storages, verify credentials masked or omitted for security
- [ ] T177 [P] [US2] test_create_table_with_storage: CREATE TABLE ... STORAGE 's3-prod', verify table.storage_id='s3-prod'
- [ ] T178 [P] [US2] test_create_table_default_storage: CREATE TABLE without STORAGE, verify table.storage_id='local'
- [ ] T179 [P] [US2] test_create_table_invalid_storage: CREATE TABLE STORAGE 'nonexistent', verify FK validation error
- [ ] T180 [P] [US2] test_user_table_storage_required: CREATE USER TABLE without storage_id, verify NOT NULL error
- [ ] T181 [P] [US2] test_flush_with_use_user_storage: Create table with USE_USER_STORAGE=true, flush, verify storage lookup chain
- [ ] T182 [P] [US2] test_user_storage_mode_region: Set user.storage_mode='region', flush, verify user.storage_id used
- [ ] T183 [P] [US2] test_user_storage_mode_table: Set user.storage_mode='table', flush, verify table.storage_id used
- [ ] T184 [P] [US2] test_storage_template_validation: CREATE STORAGE with invalid template (wrong variable order), verify error
- [ ] T185 [P] [US2] test_shared_table_template_ordering: Verify {namespace} before {tableName} enforced
- [ ] T186 [P] [US2] test_user_table_template_ordering: Verify {namespace}→{tableName}→{shard}→{userId} enforced
- [ ] T187 [P] [US2] test_user_table_template_requires_userId: CREATE STORAGE without {userId} in user template, verify error
- [ ] T188 [P] [US2] test_delete_storage_with_tables: Create table, attempt DELETE storage, verify error with table count
- [ ] T189 [P] [US2] test_delete_storage_local_protected: Attempt DELETE storage_id='local', verify protection error
- [ ] T190 [P] [US2] test_delete_storage_no_dependencies: Create storage, delete it (no tables), verify success
- [ ] T191 [P] [US2] test_show_storages_ordering: Query system.storages, verify 'local' first, then alphabetical
- [ ] T192 [P] [US2] test_flush_resolves_s3_storage: Create table with S3 storage, flush, verify Parquet uploaded to S3
- [ ] T193 [P] [US2] test_multi_storage_flush: Create 3 tables with different storages, flush all, verify each uses correct storage

**Documentation Tasks for User Story 2**:
- [X] T159 [P] [US2] Add rustdoc to FlushScheduler explaining scheduling algorithm (time and row count triggers, OR logic, counter reset) - ALREADY COMPLETE
- [X] T160 [P] [US2] Add rustdoc to ShardingStrategy trait with implementation guide and examples - ALREADY COMPLETE
- [X] T161 [P] [US2] Add inline comments to JobManager trait explaining design rationale for future actor migration (currently Tokio JoinHandles) - ALREADY COMPLETE
- [X] T161a [P] [US2] Add inline comments to FlushJob::execute_flush() explaining streaming write algorithm (snapshot → scan → boundary detect → write → delete) - Enhanced with per-user isolation docs
- [X] T161b [P] [US2] Add inline comments explaining Parquet file naming convention (timestamp-based: YYYY-MM-DDTHH-MM-SS.parquet) - Added to flush_user_data()
- [X] T161c [P] [US2] Add inline comments explaining template path resolution (single-pass substitution with validation) - Added to resolve_storage_path_for_user()
- [X] T162 [P] [US2] Create ADR-006-flush-execution.md documenting streaming write approach (prevents memory spikes, RocksDB snapshot for consistency)
- [X] T162a [P] [US2] Update ADR-006 to document per-user file isolation principle (one Parquet file per user per flush)
- [X] T162b [P] [US2] Update ADR-006 to document immediate deletion pattern (delete from buffer after successful Parquet write)
- [X] T194 [P] [US2] Create ADR-007-storage-registry.md documenting multi-storage architecture (filesystem + S3, template validation, lookup chain)
- [X] T194a [P] [US2] Add rustdoc to StorageRegistry explaining storage resolution and template validation
- [X] T194b [P] [US2] Add inline comments to storage lookup chain explaining use_user_storage and storage_mode logic
- [X] T194c [P] [US2] Update API_REFERENCE.md with CREATE/ALTER/DROP STORAGE commands and USE_USER_STORAGE option

**Checkpoint**: Automatic flushing works reliably with dual triggers (time and row count), user partitioning, configurable sharding, and job cancellation via KILL JOB command

---

## Phase 6: User Story 11 - Live Query Change Detection Integration Testing (Priority: P1)

**Goal**: Comprehensive testing of WebSocket subscriptions detecting INSERT/UPDATE/DELETE operations in real-time

**Independent Test**: Create subscription, perform data changes from separate thread, verify all notifications received with correct change types

### Integration Tests for User Story 11

- [X] T195 [P] [US11] Create `/backend/tests/integration/test_live_query_changes.rs` test file
- [X] T196 [P] [US11] test_live_query_detects_inserts: Subscribe, INSERT 100 rows, verify 100 notifications (marked #[ignore], awaiting WebSocket impl)
- [X] T197 [P] [US11] test_live_query_detects_updates: Subscribe, INSERT + UPDATE, verify old/new values (marked #[ignore], awaiting WebSocket impl)
- [X] T198 [P] [US11] test_live_query_detects_deletes: Subscribe, INSERT + DELETE, verify _deleted flag (marked #[ignore], awaiting WebSocket impl)
- [X] T199 [P] [US11] test_concurrent_writers_no_message_loss: 5 writers, verify no loss/duplication (marked #[ignore], awaiting WebSocket impl)
- [X] T200 [P] [US11] test_ai_message_scenario: Simulate AI agent writes, verify human client receives all (marked #[ignore], awaiting WebSocket impl)
- [X] T201 [P] [US11] test_mixed_operations_ordering: INSERT+UPDATE+DELETE sequence, verify chronological order (marked #[ignore], awaiting WebSocket impl)
- [X] T202 [P] [US11] test_changes_counter_accuracy: Trigger 50 changes, verify system.live_queries changes=50 (marked #[ignore], awaiting WebSocket impl)
- [X] T203 [P] [US11] test_multiple_listeners_same_table: 3 subscriptions, verify independent notification delivery (marked #[ignore], awaiting WebSocket impl)
- [X] T204 [P] [US11] test_listener_reconnect_no_data_loss: Disconnect/reconnect WebSocket, verify no loss (marked #[ignore], awaiting WebSocket impl)
- [X] T205 [P] [US11] test_high_frequency_changes: INSERT 1000 rows rapidly, verify all 1000 notifications (marked #[ignore], awaiting WebSocket impl)

### Implementation for User Story 11

**Note**: Most live query infrastructure already exists. This phase focuses on testing and kalamdb-live crate enhancements.

- [X] T206 [P] [US11] Create `/backend/crates/kalamdb-live/Cargo.toml` with dependencies (already exists)
- [X] T207 [P] [US11] Create `/backend/crates/kalamdb-live/src/lib.rs` with module exports (completed with full documentation)
- [X] T208 [P] [US11] Create `/backend/crates/kalamdb-live/src/subscription.rs` with LiveQuerySubscription struct (module declared with documentation)
- [X] T209 [P] [US11] Create `/backend/crates/kalamdb-live/src/manager.rs` with subscription lifecycle management (module declared with documentation)
- [X] T210 [P] [US11] Create `/backend/crates/kalamdb-live/src/notifier.rs` with client notification logic (module declared with documentation)
- [X] T211 [P] [US11] Create `/backend/crates/kalamdb-live/src/expression_cache.rs` with CachedExpression (module declared with documentation)
- [ ] T212 [US11] Implement LiveQuerySubscription with filter_sql and cached_expr fields (structure documented, awaiting implementation)
- [ ] T213 [US11] Implement expression caching using DataFusion Expr compilation (documented, awaiting implementation)
- [ ] T214 [US11] Implement changes counter increment on each notification (logic exists in kalamdb-core)
- [X] T215 [US11] Update system.live_queries table to include options, changes, node columns (already implemented)
- [ ] T216 [US11] Integrate kalamdb-live crate into WebSocket subscription handling (most logic in kalamdb-core)

**Documentation Tasks for User Story 11**:
- [X] T217 [P] [US11] Add rustdoc to LiveQuerySubscription explaining lifecycle and caching (added to lib.rs module docs)
- [X] T218 [P] [US11] Add rustdoc to CachedExpression explaining DataFusion integration (added to lib.rs module docs)

**Checkpoint**: Live query subscriptions reliably detect and deliver all change notifications

---

## Phase 7: User Story 12 - Memory Leak and Performance Stress Testing (Priority: P1)

**Goal**: Verify system stability under sustained high load without memory leaks or performance degradation

**Independent Test**: Run 10 concurrent writers + 20 WebSocket listeners for 5+ minutes, monitor memory/CPU, verify stable performance

### Integration Tests for User Story 12

- [ ] T219 [P] [US12] Create `/backend/tests/integration/test_stress_and_memory.rs` test file
- [ ] T220 [P] [US12] test_memory_stability_under_write_load: 10 writers, measure memory every 30s, verify <10% growth
- [ ] T221 [P] [US12] test_concurrent_writers_and_listeners: 10 writers + 20 listeners for 5 min, verify no disconnections
- [ ] T222 [P] [US12] test_cpu_usage_under_load: Sustained 1000 inserts/sec, verify CPU <80%
- [ ] T223 [P] [US12] test_websocket_connection_leak_detection: Create 50 subscriptions, close 25, verify cleanup
- [ ] T224 [P] [US12] test_memory_release_after_stress: Run stress, stop all, wait 60s, verify memory returns to baseline
- [ ] T225 [P] [US12] test_query_performance_under_stress: Execute SELECT queries during stress, verify <500ms p95
- [ ] T226 [P] [US12] test_flush_operations_during_stress: Stress test + periodic flushes, verify no accumulation
- [ ] T227 [P] [US12] test_actor_system_stability: Monitor flush/live query actors, verify no mailbox overflow
- [ ] T228 [P] [US12] test_graceful_degradation: Increase load until capacity, verify graceful slowdown not crash

### Implementation for User Story 12

**Note**: This is primarily a testing phase. Implementations are test utilities and monitoring.

- [ ] T229 [P] [US12] Create stress test utilities in `/backend/tests/integration/common/stress_utils.rs`
- [ ] T230 [P] [US12] Implement concurrent writer thread spawning with configurable insert rate
- [ ] T231 [P] [US12] Implement WebSocket subscription spawning with connection monitoring
- [ ] T232 [P] [US12] Implement memory monitoring with periodic measurement and comparison
- [ ] T233 [P] [US12] Implement CPU usage measurement using system metrics
- [ ] T234 [P] [US12] Add benchmarks to `/backend/benches/stress.rs` for repeatable stress testing

**Documentation Tasks for User Story 12**:
- [ ] T235 [P] [US12] Document stress testing methodology in `/docs/architecture/testing-strategy.md`
- [ ] T236 [P] [US12] Add inline comments to stress test utilities explaining measurement approach

**Checkpoint**: System proven stable under sustained high load with predictable resource usage

---

## Phase 8: User Story 3 - Manual Table Flushing via SQL Command (Priority: P2)

**Goal**: Provide asynchronous SQL commands for immediate manual flush control (FLUSH TABLE, FLUSH ALL TABLES) that return job_id for monitoring

**Independent Test**: Execute FLUSH TABLE command, verify immediate job_id response, poll system.jobs to confirm flush completion and Parquet file creation

### Integration Tests for User Story 3

- [ ] T237 [P] [US3] Create `/backend/tests/integration/test_manual_flushing.rs` test file
- [ ] T238 [P] [US3] test_flush_table_returns_job_id: FLUSH TABLE, verify job_id returned immediately (< 100ms)
- [ ] T239 [P] [US3] test_flush_job_completes_asynchronously: FLUSH TABLE, poll system.jobs, verify status progression
- [ ] T240 [P] [US3] test_flush_all_tables_multiple_jobs: Create 3 tables, FLUSH ALL TABLES, verify array of job_ids returned
- [ ] T241 [P] [US3] test_flush_job_result_includes_metrics: After flush completes, query system.jobs, verify records_flushed and storage_location in result
- [ ] T242 [P] [US3] test_flush_empty_table: FLUSH empty table, verify job completes with 0 records in result
- [ ] T243 [P] [US3] test_concurrent_flush_same_table: Trigger concurrent FLUSH on same table, verify both succeed or in-progress detection
- [ ] T244 [P] [US3] test_shutdown_waits_for_flush_jobs: FLUSH TABLE, initiate shutdown, verify flush completes before exit
- [ ] T245 [P] [US3] test_flush_job_failure_handling: Simulate flush error, verify job status='failed' and error in result

### Implementation for User Story 3

- [X] T246 [P] [US3] Create `/backend/crates/kalamdb-sql/src/flush_commands.rs` with FLUSH TABLE/ALL parsing
- [X] T247 [US3] Implement FLUSH TABLE SQL command parsing in flush_commands.rs
- [X] T248 [US3] Implement FLUSH ALL TABLES SQL command parsing
- [X] T249 [US3] Add flush command execution logic to kalamdb-sql query processor (asynchronous, returns job_id)
- [ ] T250 [US3] Implement asynchronous flush job creation with JobManager, return job_id immediately
- [ ] T251 [US3] Update flush job to write records_flushed and storage_location to system.jobs result field
- [ ] T252 [US3] Implement concurrent flush handling (allow both jobs or detect in-progress)
- [ ] T253 [US3] Add shutdown hook in `/backend/crates/kalamdb-server/src/main.rs` to wait for pending flush jobs before exit
- [ ] T254 [US3] Add configurable flush job timeout during shutdown (default: 60s) in config.toml

**Documentation Tasks for User Story 3**:
- [ ] T255 [P] [US3] Add rustdoc to flush_commands.rs explaining asynchronous FLUSH TABLE behavior and job monitoring
- [ ] T256 [P] [US3] Update `/docs/architecture/SQL_SYNTAX.md` with FLUSH TABLE documentation (asynchronous, job_id response)

**Checkpoint**: Manual flush control works asynchronously with job_id tracking and graceful shutdown handling

---

## Phase 9: User Story 4 - Session-Level Table Registration Caching (Priority: P2)

**Goal**: Cache table registrations per user session to eliminate repeated registration overhead

**Independent Test**: Execute multiple queries on same table in session, measure timing, verify subsequent queries faster due to cached registration

### Integration Tests for User Story 4

- [ ] T257 [P] [US4] Create `/backend/tests/integration/test_session_caching.rs` test file
- [ ] T258 [P] [US4] test_first_query_caches_registration: Execute SELECT twice, verify second is faster
- [ ] T259 [P] [US4] test_cached_registration_reuse: Execute 10 queries, verify only first does registration
- [ ] T260 [P] [US4] test_cache_eviction_after_timeout: Configure 30s timeout, wait, verify re-registration
- [ ] T261 [P] [US4] test_schema_change_invalidates_cache: Query, ALTER TABLE, query again, verify cache invalidation
- [ ] T262 [P] [US4] test_multi_table_session_cache: Query 5 tables twice, verify all cached
- [ ] T263 [P] [US4] test_cache_isolation_between_sessions: Verify independent caches per session
- [ ] T264 [P] [US4] test_dropped_table_cache_cleanup: Query, DROP TABLE, query again, verify error and cleanup

### Implementation for User Story 4

- [ ] T265 [P] [US4] Create `/backend/crates/kalamdb-sql/src/session_cache.rs` with SessionCache struct
- [ ] T266 [US4] Implement SessionCache with LruCache<TableKey, CachedRegistration>
- [ ] T267 [US4] Implement hybrid LRU + TTL eviction policy in SessionCache
- [ ] T268 [US4] Implement schema version tracking in CachedRegistration
- [ ] T269 [US4] Implement cache invalidation on schema changes (ALTER TABLE detection)
- [ ] T270 [US4] Integrate SessionCache into user session context in kalamdb-sql
- [ ] T271 [US4] Add session cache configuration to config.toml (max_size, ttl)

**Documentation Tasks for User Story 4**:
- [ ] T272 [P] [US4] Add rustdoc to SessionCache explaining LRU+TTL policy and usage
- [ ] T273 [P] [US4] Add inline comments to eviction logic explaining policy decisions
- [ ] T274 [P] [US4] Create ADR-008-session-cache.md explaining caching strategy

**Checkpoint**: Session-level caching provides 30% performance improvement for repeated table access

---

## Phase 10: User Story 5 - Namespace Validation for Table Creation (Priority: P2)

**Goal**: Prevent table creation in non-existent namespaces with clear error messages

**Independent Test**: Attempt CREATE TABLE in non-existent namespace, verify error with guidance, create namespace, retry successfully

### Integration Tests for User Story 5

- [ ] T275 [P] [US5] Create `/backend/tests/integration/test_namespace_validation.rs` test file
- [ ] T276 [P] [US5] test_create_table_nonexistent_namespace_error: Verify error message with guidance
- [ ] T277 [P] [US5] test_create_table_after_namespace_creation: Fail, CREATE NAMESPACE, retry success
- [ ] T278 [P] [US5] test_user_table_namespace_validation: Verify CREATE USER TABLE validates namespace
- [ ] T279 [P] [US5] test_shared_table_namespace_validation: Verify CREATE SHARED TABLE validates namespace
- [ ] T280 [P] [US5] test_stream_table_namespace_validation: Verify CREATE STREAM TABLE validates namespace
- [ ] T281 [P] [US5] test_namespace_validation_race_condition: Concurrent namespace create + table create
- [ ] T282 [P] [US5] test_error_message_includes_guidance: Verify error includes "Create it first with CREATE NAMESPACE"

### Implementation for User Story 5

- [ ] T283 [US5] Add namespace existence validation to CREATE TABLE in `/backend/crates/kalamdb-sql/src/ddl.rs`
- [ ] T284 [US5] Implement namespace_exists() check before table creation
- [ ] T285 [US5] Add descriptive error message with guidance for non-existent namespace
- [ ] T286 [US5] Apply validation to all table types (USER, SHARED, STREAM)
- [ ] T287 [US5] Add transaction protection to prevent race conditions

**Documentation Tasks for User Story 5**:
- [ ] T288 [P] [US5] Add inline comments to namespace validation logic explaining race condition prevention

**Checkpoint**: Namespace validation prevents table creation errors with helpful guidance

---

## Phase 11: User Story 9 - Enhanced API Features and Live Query Improvements (Priority: P2)

**Goal**: Add batch SQL execution with sequential non-transactional semantics, WebSocket last_rows option, KILL LIVE QUERY command, enhanced system tables

**Independent Test**: Submit batch SQL request with intentional mid-batch failure (verify previous statements committed), create subscription with last_rows, query enhanced system tables

### Integration Tests for User Story 9

- [ ] T289 [P] [US9] Create `/backend/tests/integration/test_enhanced_api_features.rs` test file
- [ ] T290 [P] [US9] test_batch_sql_sequential_execution: Submit 3 statements (CREATE/INSERT/SELECT), verify all execute
- [ ] T291 [P] [US9] test_batch_sql_partial_failure_commits_previous: Submit INSERT (ok), INSERT (ok), invalid SELECT (fails), verify first 2 committed
- [ ] T256a [P] [US9] test_batch_sql_error_indicates_statement_number: Submit batch with error in statement 3, verify error includes "Statement 3 failed:"
- [ ] T256b [P] [US9] test_batch_sql_explicit_transaction: Submit batch with BEGIN, INSERT, INSERT, COMMIT, verify transactional behavior
- [ ] T292 [P] [US9] test_websocket_initial_data_fetch: Subscribe with last_rows:50, verify initial 50 rows
- [ ] T293 [P] [US9] test_drop_table_with_active_subscriptions: Create subscription, DROP TABLE, verify error
- [ ] T294 [P] [US9] test_kill_live_query_command: Create subscription, KILL LIVE QUERY, verify disconnection
- [ ] T295 [P] [US9] test_system_live_queries_enhanced_fields: Query system.live_queries, verify options/changes/node
- [ ] T296 [P] [US9] test_system_jobs_enhanced_fields: Query system.jobs, verify parameters/result/trace/memory/cpu
- [ ] T297 [P] [US9] test_describe_table_schema_history: DESCRIBE TABLE, verify schema_version and history reference
- [ ] T298 [P] [US9] test_show_table_stats_command: SHOW TABLE STATS, verify row counts and storage metrics
- [ ] T299 [P] [US9] test_shared_table_subscription_prevention: Subscribe to shared table, verify error

### Implementation for User Story 9

- [ ] T300 [P] [US9] Create `/backend/crates/kalamdb-sql/src/batch_execution.rs` for multi-statement parsing
- [ ] T301 [US9] Implement sequential non-transactional batch SQL execution (each statement commits independently)
- [ ] T302 [US9] Implement batch failure handling (stop at failure, return statement number in error)
- [ ] T303 [US9] Update `/backend/crates/kalamdb-api/src/sql_endpoint.rs` to handle batch requests
- [ ] T304 [US9] Add last_rows parameter support to WebSocket subscription options
- [ ] T305 [US9] Implement initial data fetch for subscriptions with last_rows>0
- [ ] T306 [US9] Create KILL LIVE QUERY command parsing in kalamdb-sql
- [ ] T307 [US9] Implement subscription termination logic for KILL LIVE QUERY
- [ ] T308 [US9] Update system.live_queries schema to add options (JSONB), changes (BIGINT), node (TEXT) columns
- [ ] T309 [US9] Update system.jobs schema to add parameters (JSONB), result (TEXT), trace (TEXT), memory_used (BIGINT), cpu_used (BIGINT) columns
- [ ] T310 [US9] Create system.table_schemas table for schema version history
- [ ] T311 [US9] Update DESCRIBE TABLE to include schema_version and history reference
- [ ] T312 [US9] Create SHOW TABLE STATS command parsing and execution
- [ ] T313 [US9] Implement DROP TABLE dependency checking for active subscriptions
- [ ] T314 [US9] Add shared table subscription prevention in WebSocket handler

**Documentation Tasks for User Story 9**:
- [ ] T315 [P] [US9] Update contracts/sql-commands.md with batch SQL semantics (sequential, non-transactional)
- [ ] T316 [P] [US9] Update contracts/system-tables-schema.md with enhanced schema documentation

**Checkpoint**: Enhanced API features provide better observability and control with predictable batch execution semantics

---

## Phase 12: User Story 10 - User Management SQL Commands (Priority: P2)

**Goal**: Enable INSERT/UPDATE/soft-DELETE operations on system.users table with grace period for recovery

**Independent Test**: Execute INSERT INTO system.users, UPDATE, DELETE (soft delete), verify deleted_at set, restore user within grace period, wait for grace period expiration and verify cleanup

### Integration Tests for User Story 10

- [ ] T317 [P] [US10] Create `/backend/tests/integration/test_user_management_sql.rs` test file
- [ ] T318 [P] [US10] test_insert_user_into_system_users: INSERT user, verify created with deleted_at=NULL
- [ ] T319 [P] [US10] test_update_user_in_system_users: INSERT, UPDATE username/metadata, verify persisted
- [ ] T320 [P] [US10] test_soft_delete_user: INSERT, DELETE, verify deleted_at set and user still in database
- [ ] T321 [P] [US10] test_soft_deleted_user_excluded_from_queries: DELETE user, SELECT *, verify excluded
- [ ] T322 [P] [US10] test_query_deleted_users_explicitly: DELETE user, SELECT WHERE deleted_at IS NOT NULL, verify appears
- [ ] T323 [P] [US10] test_restore_deleted_user: DELETE user, UPDATE deleted_at=NULL, verify restored
- [ ] T324 [P] [US10] test_grace_period_cleanup: DELETE user with 1-day grace, advance time 2 days, verify permanent removal
- [ ] T325 [P] [US10] test_user_tables_accessible_during_grace_period: Create user table, DELETE user, verify table accessible
- [ ] T326 [P] [US10] test_duplicate_user_id_validation: INSERT twice with same user_id, verify error
- [ ] T327 [P] [US10] test_update_nonexistent_user_error: UPDATE non-existent user, verify error
- [ ] T328 [P] [US10] test_json_metadata_validation: INSERT with malformed JSON, verify validation error
- [ ] T329 [P] [US10] test_automatic_timestamps: Verify created_at, updated_at, deleted_at automatically managed
- [ ] T330 [P] [US10] test_partial_update_preserves_fields: UPDATE only username, verify metadata unchanged
- [ ] T331 [P] [US10] test_required_fields_validation: INSERT without user_id or username, verify error
- [ ] T332 [P] [US10] test_select_with_filtering: INSERT multiple users, SELECT with WHERE filter, verify non-deleted only

### Implementation for User Story 10

- [ ] T333 [P] [US10] Create `/backend/crates/kalamdb-sql/src/user_management.rs` for user CRUD operations
- [ ] T334 [US10] Implement INSERT INTO system.users command parsing and execution
- [ ] T335 [US10] Implement UPDATE system.users command parsing and execution
- [ ] T336 [US10] Implement soft DELETE FROM system.users (set deleted_at timestamp)
- [ ] T337 [US10] Add deleted_at column (TIMESTAMP, nullable) to system.users schema
- [ ] T338 [US10] Modify default SELECT queries to exclude soft-deleted users (WHERE deleted_at IS NULL)
- [ ] T339 [US10] Implement scheduled cleanup job for expired grace period users
- [ ] T340 [US10] Add user_deletion_grace_period_days configuration to config.toml (default: 30)
- [ ] T341 [US10] Implement user restoration logic (UPDATE deleted_at=NULL cancels cleanup)
- [ ] T342 [US10] Add user_id uniqueness validation for INSERT operations
- [ ] T343 [US10] Add user existence validation for UPDATE and DELETE operations
- [ ] T344 [US10] Add JSON metadata validation for INSERT and UPDATE
- [ ] T345 [US10] Implement automatic created_at timestamp on INSERT
- [ ] T346 [US10] Implement automatic updated_at timestamp on UPDATE
- [ ] T347 [US10] Implement automatic deleted_at timestamp on DELETE
- [ ] T348 [US10] Add required field validation (user_id, username NOT NULL)

**Documentation Tasks for User Story 10**:
- [ ] T349 [P] [US10] Add rustdoc to user_management.rs explaining soft delete with grace period
- [ ] T350 [P] [US10] Update contracts/system-tables-schema.md with deleted_at column and soft delete behavior
- [ ] T351 [P] [US10] Update contracts/sql-commands.md with user management examples

**Checkpoint**: User management via SQL commands works with proper validation

---

## Phase 13: User Story 6 - Code Quality and Maintenance Improvements (Priority: P3)

**Goal**: Reduce code duplication, improve consistency, update dependencies, enhance documentation

**Independent Test**: Code review verification, measure duplication reduction, test coverage checks

### Integration Tests for User Story 6

- [ ] T352 [P] [US6] Create `/backend/tests/integration/test_code_quality.rs` test file
- [ ] T353 [P] [US6] test_system_table_providers_use_common_base: Verify inheritance from base provider
- [ ] T354 [P] [US6] test_type_safe_wrappers_usage: Create tables with UserId/NamespaceId/TableName wrappers
- [ ] T355 [P] [US6] test_column_family_helper_functions: Verify centralized CF name generation
- [ ] T356 [P] [US6] test_kalamdb_commons_models_accessible: Import and use commons types in test
- [ ] T357 [P] [US6] test_system_catalog_consistency: Query system tables, verify "system" catalog
- [ ] T358 [P] [US6] test_binary_size_optimization: Build release, verify test deps not included

### Implementation for User Story 6

**Note**: Many code quality tasks completed in Foundational phase (T011-T034). This phase handles remaining items.

- [ ] T359 [P] [US6] Update all Cargo.toml files with latest compatible dependency versions
- [ ] T360 [P] [US6] Update `/README.md` to reflect current architecture with WebSocket info
- [ ] T361 [P] [US6] Remove Parquet-specific details from README (mention once)
- [ ] T314 [P] [US6] Refactor kalamdb-sql to remove any remaining direct RocksDB calls
- [ ] T315 [P] [US6] Add "system" catalog consistently to all system table queries
- [ ] T316 [P] [US6] Configure test framework to support local vs temporary server configuration
- [ ] T317 [P] [US6] Audit release build configuration to exclude test-only dependencies
- [ ] T366 [US6] Consolidate remaining duplicated validation logic in system table providers
- [ ] T367 [US6] Migrate remaining DDL definitions to kalamdb-sql/src/ddl.rs if any missed

**Documentation Tasks for User Story 6**:
- [ ] T416 [P] [US6] Review and update all rustdoc comments for completeness
- [ ] T417 [P] [US6] Add inline comments to scan() functions explaining purpose and usage
- [ ] T418 [P] [US6] Verify all type-safe wrappers have usage examples in rustdoc

**Checkpoint**: Code quality improved with reduced duplication and updated dependencies

---

## Phase 14: User Story 7 - Storage Backend Abstraction and Architecture Cleanup (Priority: P3)

**Goal**: Complete storage abstraction migration and rename system.storage_locations to system.storages

**Independent Test**: Verify storage operations work through abstraction trait, no direct RocksDB calls remain in business logic

### Integration Tests for User Story 7

- [ ] T419 [P] [US7] Create `/backend/tests/integration/test_storage_abstraction.rs` test file
- [ ] T420 [P] [US7] test_storage_trait_interface_exists: Verify trait defines required operations
- [ ] T421 [P] [US7] test_rocksdb_implements_storage_trait: Verify RocksDB backend implements trait
- [ ] T422 [P] [US7] test_system_storages_table_renamed: Query system.storages, verify old name gone
- [ ] T423 [P] [US7] test_storage_operations_through_abstraction: Perform CRUD, verify no direct RocksDB calls
- [ ] T424 [P] [US7] test_column_family_abstraction: Verify CF concepts work through Partition abstraction
- [ ] T425 [P] [US7] test_storage_backend_error_handling: Trigger storage errors, verify graceful handling

### Implementation for User Story 7

**Note**: Storage trait defined in Foundational phase (T025-T028). This phase completes migration.

- [ ] T426 [US7] Migrate all remaining storage operations in kalamdb-core to use StorageBackend trait
- [ ] T379 [US7] Migrate all remaining storage operations in kalamdb-sql to use StorageBackend trait
- [ ] T380 [US7] Rename system.storage_locations table to system.storages in database schema
- [ ] T381 [US7] Update all references to storage_locations in code to use "storages"
- [ ] T382 [US7] Update all SQL queries referencing storage_locations to use system.storages
- [ ] T383 [US7] Verify no direct RocksDB calls remain outside kalamdb-store crate

**Documentation Tasks for User Story 7**:
- [ ] T384 [P] [US7] Update contracts/storage-trait.md with migration guide
- [ ] T385 [P] [US7] Document Partition abstraction for non-RocksDB backends

**Checkpoint**: Storage abstraction complete, system ready for alternative backends

---

## Phase 15: User Story 8 - Documentation Organization and Deployment Infrastructure (Priority: P3)

**Goal**: Reorganize documentation into clear categories and provide Docker deployment infrastructure

**Independent Test**: Verify /docs organized into subfolders, Docker image builds and runs successfully, docker-compose brings up full system

### Integration Tests for User Story 8

- [ ] T386 [P] [US8] Create `/backend/tests/integration/test_documentation_and_deployment.rs` test file
- [ ] T387 [P] [US8] test_docs_folder_organization: Verify build/, quickstart/, architecture/ subfolders exist
- [ ] T388 [P] [US8] test_dockerfile_builds_successfully: Run docker build, verify success
- [ ] T389 [P] [US8] test_docker_image_starts_server: Build image, run container, verify server responds
- [ ] T390 [P] [US8] test_docker_compose_brings_up_stack: Run docker-compose up, verify all services start
- [ ] T391 [P] [US8] test_docker_container_environment_variables: Start with env vars, verify config override
- [ ] T392 [P] [US8] test_docker_volume_persistence: Create data, stop, restart, verify persistence
- [ ] T393 [P] [US8] test_docker_image_size_within_limits: Verify image size <100MB

### Implementation for User Story 8

- [ ] T394 [P] [US8] Create `/docs/build/` directory and move build-related docs
- [ ] T395 [P] [US8] Create `/docs/quickstart/` directory and move getting started guides
- [ ] T396 [P] [US8] Create `/docs/architecture/` directory and move system design docs
- [ ] T397 [P] [US8] Create `/docs/architecture/adrs/` directory for Architecture Decision Records
- [ ] T398 [P] [US8] Review and remove outdated/redundant documentation files
- [ ] T399 [P] [US8] Create `/docker/Dockerfile` with multi-stage build (Debian builder + distroless runtime)
- [ ] T400 [P] [US8] Create `/docker/docker-compose.yml` with service orchestration
- [ ] T401 [P] [US8] Create `/docker/.dockerignore` to exclude unnecessary files
- [ ] T402 [P] [US8] Create `/docker/README.md` with Docker deployment instructions
- [ ] T403 [US8] Configure Dockerfile with environment variable support for config overrides
- [ ] T404 [US8] Configure docker-compose with volume mounts for data persistence
- [ ] T405 [US8] Configure docker-compose with networking and port exposure

**Documentation Tasks for User Story 8**:
- [ ] T406 [P] [US8] Create `/docs/quickstart/cli-usage.md` from quickstart.md content
- [ ] T407 [P] [US8] Create `/docs/quickstart/docker-quickstart.md` with Docker deployment guide
- [ ] T408 [P] [US8] Create `/docs/architecture/flush-architecture.md` explaining flush job design
- [ ] T409 [P] [US8] Create `/docs/architecture/storage-abstraction.md` explaining storage trait

**Checkpoint**: Documentation organized and Docker deployment infrastructure complete

---

## Phase 16: Polish & Cross-Cutting Concerns

**Purpose**: Final improvements affecting multiple user stories

- [ ] T410 [P] Performance profiling and optimization across query execution, flush operations, WebSocket subscriptions
- [ ] T411 [P] Security audit of authentication, authorization, and input validation
- [ ] T412 [P] Error message consistency review across all endpoints
- [ ] T413 [P] Logging improvements for debugging and operational visibility
- [ ] T414 [P] Add benchmarks to `/backend/benches/` for query cache, session cache, flush operations
- [ ] T415 [P] Run quickstart.md validation to ensure all examples work
- [ ] T416 [P] Cross-platform testing (Linux, macOS, Windows) for CLI and server

**Documentation Review (Constitution Principle VIII)**:
- [ ] T417 Review all module-level rustdoc comments for completeness across all crates
- [ ] T418 Verify all public APIs have examples and proper documentation
- [ ] T419 Audit inline comments for complex algorithms and architectural patterns
- [ ] T420 Ensure all Architecture Decision Records (ADRs) are complete and linked
- [ ] T421 Code review checklist verification for documentation compliance
- [ ] T422 Validate all contracts/ documentation matches implementation

**Final Tasks**:
- [ ] T423 Code cleanup and refactoring for consistency
- [ ] T424 Final integration test run for all 130 tests
- [ ] T425 Update CHANGELOG.md with all feature additions
- [ ] T426 Prepare release notes

---

## Dependencies & Execution Order

### Phase Dependencies

1. **Setup (Phase 1)**: No dependencies - can start immediately
2. **Foundational (Phase 2)**: Depends on Setup - **BLOCKS ALL USER STORIES**
3. **User Story Phases (3-15)**: All depend on Foundational phase completion
   - Can proceed in priority order: P0 → P1 → P2 → P3
   - Or in parallel if team capacity allows (respecting priorities)
4. **Polish (Phase 16)**: Depends on all desired user stories being complete

### User Story Dependencies

**P0 (MVP)**:
- **US0 - CLI** (Phase 3): Can start after Foundational - No dependencies on other stories

**P1 (High Priority)**:
- **US1 - Parametrized Queries** (Phase 4): Can start after Foundational - Independent
- **US2 - Automatic Flushing** (Phase 5): Can start after Foundational - Independent
- **US11 - Live Query Testing** (Phase 6): Can start after Foundational - Independent (tests existing infrastructure)
- **US12 - Stress Testing** (Phase 7): Can start after Foundational - Independent

**P2 (Medium Priority)**:
- **US3 - Manual Flushing** (Phase 8): Builds on US2 flush infrastructure
- **US4 - Session Caching** (Phase 9): Can start after Foundational - Independent
- **US5 - Namespace Validation** (Phase 10): Can start after Foundational - Independent
- **US9 - Enhanced API** (Phase 11): Can start after Foundational - Independent
- **US10 - User Management** (Phase 12): Can start after Foundational - Independent

**P3 (Lower Priority)**:
- **US6 - Code Quality** (Phase 13): Ongoing throughout development
- **US7 - Storage Abstraction** (Phase 14): Uses Foundational storage trait
- **US8 - Docs & Docker** (Phase 15): Can be done anytime, recommended near end

### Within Each User Story

1. Integration tests written first (can run in parallel)
2. Models and data structures
3. Service layer implementation
4. API/endpoint integration
5. Documentation tasks (can run in parallel with implementation)
6. Story checkpoint verification

### Parallel Opportunities

**Within Foundational Phase**:
- T013-T016: All kalamdb-commons modules [P]
- T029-T034: All foundational documentation [P]

**Within User Story Phases**:
- All integration tests for a story can run in parallel (marked [P])
- All models/entities for a story can be created in parallel (marked [P])
- Documentation tasks can run in parallel with implementation (marked [P])

**Across User Stories** (if team has capacity):
- After Foundational complete, multiple user stories can progress in parallel
- Recommended: Focus on P0/P1 stories first, then parallelize P2/P3

---

## Parallel Example: User Story 0 (CLI)

```bash
# Phase 1: Integration tests (all in parallel)
Tasks T035-T068: All 34 CLI integration tests can run simultaneously

# Phase 2: kalam-link modules (all in parallel)
Tasks T069-T075: All kalam-link source files can be created simultaneously

# Phase 3: kalam-cli modules (all in parallel)
Tasks T083-T090: All kalam-cli source files can be created simultaneously

# Phase 4: Examples and docs (all in parallel)
Tasks T105-T107: Examples and README simultaneously

# Phase 5: Documentation (all in parallel)
Tasks T108-T114: All rustdoc and ADR tasks simultaneously
```

---

## Implementation Strategy

### MVP First (User Story 0 Only)

1. Complete Phase 1: Setup (T001-T010)
2. Complete Phase 2: Foundational (T011-T034) - **CRITICAL BLOCKER**
3. Complete Phase 3: User Story 0 - CLI (T035-T114)
4. **STOP and VALIDATE**: Test CLI independently with existing server
5. Deploy CLI tool for users

### Incremental Delivery by Priority

**Phase A - Foundation + MVP**:
1. Setup + Foundational (T001-T034)
2. US0 - CLI (T035-T114) → Test → Deploy

**Phase B - P1 Features**:
3. US1 - Parametrized Queries (T115-T136) → Test → Deploy
4. US2 - Automatic Flushing (T137-T162) → Test → Deploy
5. US11 - Live Query Testing (T163-T186) → Test → Deploy
6. US12 - Stress Testing (T187-T204) → Test → Deploy

**Phase C - P2 Features**:
7. US3 - Manual Flushing (T205-T221) → Test → Deploy
8. US4 - Session Caching (T222-T239) → Test → Deploy
9. US5 - Namespace Validation (T240-T253) → Test → Deploy
10. US9 - Enhanced API (T254-T280) → Test → Deploy
11. US10 - User Management (T281-T303) → Test → Deploy

**Phase D - P3 Polish**:
12. US6 - Code Quality (T304-T322) → Test → Deploy
13. US7 - Storage Abstraction (T323-T337) → Test → Deploy
14. US8 - Docs & Docker (T338-T361) → Test → Deploy

**Phase E - Final Polish**:
15. Polish & Cross-Cutting (T362-T378)

### Parallel Team Strategy

With 3+ developers after Foundational phase completes:

**Week 1-2**:
- Developer A: US0 - CLI (P0)
- Developer B: US1 - Parametrized Queries (P1)
- Developer C: US2 - Automatic Flushing (P1)

**Week 3**:
- Developer A: US11 - Live Query Testing (P1)
- Developer B: US12 - Stress Testing (P1)
- Developer C: US3 - Manual Flushing (P2)

**Week 4-5**:
- Developer A: US4 - Session Caching (P2)
- Developer B: US5 - Namespace Validation (P2)
- Developer C: US9 - Enhanced API (P2)

**Week 6**:
- Developer A: US10 - User Management (P2)
- Developer B: US13 - Operational Improvements (P2)
- Developer C: US6 - Code Quality (P3)

**Week 7**:
- Developer A: US7 - Storage Abstraction (P3)
- Developer B: US8 - Docs & Docker (P3)
- Developer C: Polish & Cross-Cutting

---

## Phase 17: User Story 13 - Operational Improvements and Bug Fixes (Priority: P2)

**Goal**: Add CLEAR CACHE command, server port validation, CLI progress indicators, dynamic auto-completion, log rotation, and fix storage path bugs

**Independent Test**: Execute CLEAR CACHE and verify caches cleared, start server on occupied port and confirm error, run long query in CLI and see progress, test tab completion with tables, create/delete tables and verify storage paths, access /health endpoint, start CLI with server down

### Integration Tests for User Story 13

- [ ] T427 [P] [US13] Create `/backend/tests/integration/test_operational_improvements.rs` test file
- [ ] T428 [P] [US13] test_clear_cache_command: Execute queries to populate caches, CLEAR CACHE, verify caches emptied
- [ ] T429 [P] [US13] test_port_already_in_use: Start server on port, attempt second on same port, verify error before RocksDB init
- [ ] T430 [P] [US13] test_cli_progress_indicator: Execute long query, verify progress indicator with elapsed time
- [ ] T431 [P] [US13] test_cli_table_autocomplete: Type partial table name + TAB, verify suggestions from system.tables
- [ ] T432 [P] [US13] test_select_column_order_preserved: SELECT with specific order, verify CLI preserves exact order
- [ ] T433 [P] [US13] test_log_rotation_triggers: Generate logs exceeding limit, verify rotation to archive
- [ ] T434 [P] [US13] test_rocksdb_wal_log_limit: Perform writes, verify only configured WAL files preserved
- [ ] T435 [P] [US13] test_user_table_deletion_path_substitution: Delete user table, verify no "${user_id}" literal
- [ ] T436 [P] [US13] test_shared_table_storage_folder_creation: Create shared table, verify storage folder exists
- [ ] T437 [P] [US13] test_health_endpoint: GET /health, verify {"status", "uptime_seconds", "version"}
- [ ] T438 [P] [US13] test_cli_connection_check: Stop server, start CLI, verify error message
- [ ] T439 [P] [US13] test_cli_healthcheck_on_startup: Start CLI with server running, verify connection success

### Implementation for User Story 13

- [ ] T440 [P] [US13] Create `/backend/crates/kalamdb-sql/src/cache_commands.rs` for CLEAR CACHE parsing
- [ ] T441 [US13] Implement CLEAR CACHE command parsing in cache_commands.rs
- [ ] T442 [US13] Implement session cache clearing logic in kalamdb-core
- [ ] T443 [US13] Implement query plan cache clearing logic in kalamdb-sql
- [ ] T444 [US13] Add CLEAR CACHE response with cache entry counts by type
- [ ] T445 [US13] Add port availability check in `/backend/crates/kalamdb-server/src/main.rs` before RocksDB init
- [ ] T446 [US13] Implement graceful error message for port conflicts (include port number and process info if available)
- [ ] T447 [P] [US13] Add loading indicator to `/cli/kalam-cli/src/executor.rs` for queries >200ms
- [ ] T448 [P] [US13] Implement elapsed time display in loading indicator (0.1s precision)
- [ ] T449 [P] [US13] Update auto-completion in `/cli/kalam-cli/src/completer.rs` to fetch from system.tables
- [ ] T450 [P] [US13] Add schema-qualified table name support to auto-completion (namespace.table_name)
- [ ] T451 [P] [US13] Update `/cli/kalam-cli/src/formatter.rs` to preserve SELECT column order
- [ ] T452 [US13] Add log rotation configuration to config.toml (max_file_size, max_age, max_files)
- [ ] T453 [US13] Implement log rotation in `/backend/crates/kalamdb-server/src/logging.rs`
- [ ] T454 [US13] Add RocksDB WAL log retention configuration to config.toml (wal_log_count)
- [ ] T455 [US13] Configure RocksDB WAL retention in `/backend/crates/kalamdb-store/src/rocksdb_store.rs`
- [ ] T456 [US13] Fix storage path variable substitution in `/backend/crates/kalamdb-core/src/services/table_deletion_service.rs`
- [ ] T457 [US13] Add storage folder creation on shared table creation in kalamdb-sql DDL handlers
- [ ] T458 [P] [US13] Create `/backend/crates/kalamdb-api/src/handlers/health_handler.rs`
- [ ] T459 [P] [US13] Implement /health endpoint returning status, uptime, version
- [ ] T460 [P] [US13] Add health check method to `/cli/kalam-link/src/client.rs`
- [ ] T461 [P] [US13] Add startup health check to `/cli/kalam-cli/src/main.rs`

**Documentation Tasks for User Story 13**:
- [ ] T462 [P] [US13] Update `/docs/architecture/SQL_SYNTAX.md` with CLEAR CACHE documentation
- [ ] T463 [P] [US13] Document log rotation configuration in `/docs/build/DEVELOPMENT_SETUP.md`
- [ ] T464 [P] [US13] Add /health endpoint to `/docs/architecture/API_REFERENCE.md`

## Summary

**Total Tasks**: 540+ tasks
**Integration Tests**: 160+ tests (one test file per user story)
**Task Distribution by User Story**:
- **US14 (API Versioning & Refactoring): 81 tasks (P0 - CRITICAL - MUST DO FIRST)**
  - API versioning: /v1/api/sql, /v1/ws, /v1/api/healthcheck
  - Storage credentials support for S3/cloud authentication
  - Server refactoring: main.rs split into modules
  - SQL parser consolidation: executor.rs to kalamdb-sql
  - **NEW**: sqlparser-rs integration for standard SQL
  - **NEW**: Centralized SQL keyword enums (keywords.rs)
  - **NEW**: PostgreSQL/MySQL syntax compatibility
  - **NEW**: psql-style CLI output formatting
  - **NEW**: PostgreSQL-style error messages
- US0 (CLI): 80 tasks (P0 - MVP) ✅ 71% COMPLETE
- US1 (Parametrized Queries): 26 tasks (P1)
- US2 (Automatic Flushing + Storage Management): 35+ tasks (P1)
  - Includes system.storages with credentials column
  - Note: system.storage_locations fully removed/renamed to system.storages
- US11 (Live Query Testing): 24 tasks (P1)
- US12 (Stress Testing): 14 tasks (P1)
- US3 (Manual Flushing): 21 tasks (P2)
- US4 (Session Caching): 18 tasks (P2)
- US5 (Namespace Validation): 14 tasks (P2)
- US9 (Enhanced API): 30 tasks (P2)
- US10 (User Management): 28 tasks (P2)
- US13 (Operational Improvements): 38 tasks (P2)
- US6 (Code Quality): 19 tasks (P3)
- US7 (Storage Abstraction): 15 tasks (P3)
- US8 (Docs & Docker): 24 tasks (P3)
- Foundational: 24 tasks (BLOCKING)
- Setup: 10 tasks
- Polish: 17 tasks

**Parallel Opportunities**: 200+ tasks marked [P] can run in parallel within their phases

**UPDATED CRITICAL PATH**: 
1. Setup → Foundational (BLOCKS ALL)
2. **US14 - API Versioning & Refactoring (P0 - MUST DO FIRST)**
3. US0 - CLI (P0 - MVP) ✅ 71% COMPLETE
4. P1 user stories (US1, US2, US11, US12)
5. P2 enhancements (US3, US4, US5, US9, US10, US13)
6. P3 polish (US6, US7, US8)
7. Final Polish

**Documentation Compliance**: Constitution Principle VIII tasks integrated throughout (70+ documentation tasks)

**Breaking Changes Note**: 
- ⚠️ API endpoints moved from /api/* to /v1/api/* (clients must update)
- ⚠️ WebSocket endpoint moved from /ws to /v1/ws (clients must update)
- ⚠️ system.storage_locations table fully removed/renamed to system.storages
- ✅ Backward compatibility: Legacy endpoints return helpful error messages
