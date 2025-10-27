# Tasks: User Authentication

**Feature Branch**: `007-user-auth`  
**Input**: Design documents from `/specs/007-user-auth/` + User-Management.md  
**Prerequisites**: plan.md, spec.md, research.md, data-model.md

**⚠️ ARCHITECTURAL COMPLIANCE**: All tasks MUST follow existing KalamDB patterns:
- SQL parsers follow `ExtensionStatement` pattern (see `parser/extensions.rs`)
- Storage follows `UserTableStore` pattern with column families (see `kalamdb-store/src/user_table_store.rs`)
- Jobs follow `JobManager` + `RetentionPolicy` pattern (see `kalamdb-core/src/jobs/`)
- All code must match style and organization of existing similar components

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

**Total Tasks**: 322 tasks across 15 phases
- **Phase 0.5**: Storage Backend Abstraction & Store Consolidation (106 tasks) - CRITICAL, DO FIRST
- **Phase 1**: Setup (10 tasks)
- **Phase 2**: Foundational (32 tasks) - BLOCKING all user stories, follow existing patterns
- **Phase 3-10**: User Stories (98 tasks) - 8 independent stories
- **Phase 5.5**: SQL Parser Extensions (18 tasks) - BLOCKING US4-US8, follow ExtensionStatement pattern
- **Phase 11**: Testing & Migration (17 tasks)
- **Phase 12**: Polish (12 tasks)
- **Phase 13**: Additional Features from User-Management.md (26 tasks) - follow job/index patterns

**Tests Coverage**: 
- **Integration Tests**: 57+ tests covering storage abstraction, authentication, SQL commands, RBAC
- **Unit Tests**: 19+ tests for parser, password hashing, JWT validation, storage backend
- **Edge Cases**: 7+ tests for malformed input, concurrent access, deleted users
- **End-to-End**: Full authentication flow test
- **Storage Abstraction**: 8 tests for mock backend, EntityStore trait, dependency verification

**SQL Commands**: CREATE USER, ALTER USER, DROP USER with password/OAuth/internal auth modes

**Storage Refactoring**: Isolate RocksDB to kalamdb-store only; all other crates use StorageBackend trait

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and basic structure for authentication system

- [ ] T001 Create kalamdb-auth crate directory at backend/crates/kalamdb-auth/ with Cargo.toml
- [ ] T002 Add kalamdb-auth to workspace members in root Cargo.toml
- [ ] T003 [P] Add dependencies to backend/crates/kalamdb-auth/Cargo.toml (bcrypt 0.15, base64 0.21, jsonwebtoken 9.2, kalamdb-commons, kalamdb-store, serde, thiserror, log, tokio)
- [ ] T004 [P] Add UserRole enum to backend/crates/kalamdb-commons/src/models.rs (user, service, dba, system)
- [ ] T005 [P] Add TableAccess enum to backend/crates/kalamdb-commons/src/models.rs (public, private, restricted)
- [ ] T006 [P] Export UserRole and TableAccess from backend/crates/kalamdb-commons/src/lib.rs
- [ ] T007 Create kalamdb-auth crate structure: backend/crates/kalamdb-auth/src/lib.rs
- [ ] T008 [P] Create common passwords list file at backend/crates/kalamdb-auth/data/common-passwords.txt (top 10,000)
- [ ] T009 [P] Create authentication log directory at backend/logs/auth.log (with .gitkeep)
- [ ] T010 [P] Add configuration section for authentication in backend/config.example.toml (bcrypt_cost, min_password_length, max_password_length, jwt_issuers, allow_remote_access)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core authentication/authorization infrastructure that MUST be complete before ANY user story implementation

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

### Core Authentication Modules

- [ ] T011 [P] Implement password hashing module in backend/crates/kalamdb-auth/src/password.rs (hash_password, verify_password with bcrypt cost 12, async spawn_blocking)
- [ ] T012 [P] Implement common password validation in backend/crates/kalamdb-auth/src/password.rs (load_common_passwords, is_common_password)
- [ ] T013 [P] Implement HTTP Basic Auth parser in backend/crates/kalamdb-auth/src/basic_auth.rs (parse_basic_auth_header, extract_credentials)
- [ ] T014 [P] Implement JWT validation in backend/crates/kalamdb-auth/src/jwt_auth.rs (validate_jwt_token, extract_claims, verify_issuer)
- [ ] T015 [P] Implement connection info detection in backend/crates/kalamdb-auth/src/connection.rs (ConnectionInfo struct, is_localhost method)
- [ ] T016 [P] Implement AuthenticatedUser context struct in backend/crates/kalamdb-auth/src/context.rs (user_id, username, role, email, connection_info)
- [ ] T017 [P] Implement AuthError types in backend/crates/kalamdb-auth/src/error.rs (MissingAuthorization, InvalidCredentials, MalformedAuthorization, TokenExpired, InvalidSignature, UntrustedIssuer, MissingClaim, WeakPassword)
- [ ] T018 Implement AuthService orchestrator in backend/crates/kalamdb-auth/src/service.rs (authenticate method, supports Basic Auth and JWT)
- [ ] T019 Export all public APIs from backend/crates/kalamdb-auth/src/lib.rs

### Storage Layer

- [ ] T020 Create system_users column family initialization in backend/crates/kalamdb-store/src/lib.rs (add to ColumnFamilyNames constants, follow pattern for system_tables, system_storages)
- [ ] T021 Create UsersStore struct in backend/crates/kalamdb-store/src/users.rs (follow UserTableStore pattern with db: Arc<DB>, ensure_cf() method)
- [ ] T022 Implement User struct with serde in backend/crates/kalamdb-store/src/users.rs (user_id, username, email, auth_type, auth_data, role, storage_mode, storage_id, metadata, created_at, updated_at, last_seen, deleted_at)
- [ ] T023 [P] Implement create_user function in backend/crates/kalamdb-store/src/users.rs (follow create_column_family pattern, return Result<(), anyhow::Error>)
- [ ] T024 [P] Implement get_user_by_id function in backend/crates/kalamdb-store/src/users.rs (use ensure_cf(), db.get_cf() pattern)
- [ ] T025 [P] Implement get_user_by_username function in backend/crates/kalamdb-store/src/users.rs (create username_index secondary index, follow existing index patterns)
- [ ] T026 [P] Implement update_user function in backend/crates/kalamdb-store/src/users.rs (use db.put_cf() pattern)
- [ ] T027 [P] Implement soft_delete_user function in backend/crates/kalamdb-store/src/users.rs (set deleted_at timestamp, follow soft delete pattern)
- [ ] T028 [P] Implement list_users function in backend/crates/kalamdb-store/src/users.rs (with role filter, deleted filter, use IteratorMode pattern)
- [ ] T029 [P] Implement update_last_seen function in backend/crates/kalamdb-store/src/users.rs (daily granularity, async update)
- [ ] T030 Create username_index column family for fast username lookups in backend/crates/kalamdb-store/src/users.rs (follow secondary index pattern)
- [ ] T031 Export UsersStore from backend/crates/kalamdb-store/src/lib.rs (follow module export pattern)

### Authorization Layer

- [ ] T031 [P] Implement RBAC permission checking in backend/crates/kalamdb-core/src/auth/roles.rs (can_access_table, can_create_table, can_manage_users)
- [ ] T032 [P] Implement table access control logic in backend/crates/kalamdb-core/src/auth/access.rs (check_shared_table_access, evaluate_access_level)
- [ ] T033 Add access_level column to system.tables in backend/crates/kalamdb-store/src/tables.rs
- [ ] T034 Export authorization functions from backend/crates/kalamdb-core/src/auth/mod.rs

### Authentication Logging

- [ ] T035 Implement dedicated auth logger in backend/src/logging.rs (create auth_file_appender for logs/auth.log)
- [ ] T036 [P] Add log_auth_failure function in backend/src/logging.rs (timestamp, username, source_ip, failure_reason, request_id)
- [ ] T037 [P] Add log_role_change function in backend/src/logging.rs (timestamp, target_user_id, old_role, new_role, admin_user_id)
- [ ] T038 [P] Add log_admin_operation function in backend/src/logging.rs (timestamp, admin_user_id, operation, target_user_id, result)
- [ ] T039 Implement log rotation for auth.log in backend/src/logging.rs (size-based or time-based)

### Rate Limiting

- [ ] T040 [P] Implement per-username rate limiter in backend/crates/kalamdb-auth/src/rate_limit.rs (5 failures per 5 minutes)
- [ ] T041 [P] Implement per-IP rate limiter in backend/crates/kalamdb-auth/src/rate_limit.rs (20 failures per 5 minutes)
- [ ] T042 Implement lockout logic with exponential backoff in backend/crates/kalamdb-auth/src/rate_limit.rs
- [ ] T043 Add localhost/system user exemption to rate limiting in backend/crates/kalamdb-auth/src/rate_limit.rs
- [ ] T044 Implement reset_on_success for rate limit counters in backend/crates/kalamdb-auth/src/rate_limit.rs

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 1 - Basic User Authentication (Priority: P1) 🎯 MVP

**Goal**: Users can authenticate to KalamDB using username and password via HTTP Basic Auth to access their data securely

**Independent Test**: Create a user account with password, authenticate via HTTP Basic Auth, execute a SQL query

### Tests for User Story 1

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [ ] T045 [P] [US1] Create test helper module in backend/tests/common/auth_helper.rs (create_test_user, authenticate_basic)
- [ ] T046 [P] [US1] Integration test for successful Basic Auth in backend/tests/test_basic_auth.rs (test_basic_auth_success)
- [ ] T047 [P] [US1] Integration test for invalid credentials in backend/tests/test_basic_auth.rs (test_basic_auth_invalid_credentials)
- [ ] T048 [P] [US1] Integration test for missing authorization header in backend/tests/test_basic_auth.rs (test_basic_auth_missing_header)
- [ ] T049 [P] [US1] Integration test for malformed authorization header in backend/tests/test_basic_auth.rs (test_basic_auth_malformed_header)
- [ ] T050 [P] [US1] Unit test for password hashing in backend/crates/kalamdb-auth/tests/password_tests.rs (test_hash_password, test_verify_password)
- [ ] T051 [P] [US1] Unit test for common password blocking in backend/crates/kalamdb-auth/tests/password_tests.rs (test_common_password_rejected)

### Implementation for User Story 1

- [ ] T052 [US1] Implement authentication middleware in backend/src/middleware.rs (extract Authorization header, call AuthService, attach AuthenticatedUser to request)
- [ ] T053 [US1] Update main.rs to register authentication middleware in backend/src/main.rs
- [ ] T054 [US1] Add authentication requirement to SQL handler in backend/crates/kalamdb-api/src/handlers/sql_handler.rs (extract AuthenticatedUser from request)
- [ ] T055 [US1] Implement user creation endpoint POST /v1/users in backend/crates/kalamdb-api/src/handlers/user_handler.rs (requires dba/system role)
- [ ] T056 [US1] Add user management routes to backend/crates/kalamdb-api/src/routes.rs
- [ ] T057 [US1] Update ExecutionContext with user_role in backend/crates/kalamdb-sql/src/models.rs
- [ ] T058 [US1] Add authorization check before query execution in backend/crates/kalamdb-core/src/sql/executor.rs (verify user can access tables)

**Checkpoint**: User Story 1 complete - users can authenticate with HTTP Basic Auth and execute queries on their own tables

---

## Phase 4: User Story 2 - Token-Based Authentication (Priority: P1)

**Goal**: Applications authenticate using JWT bearer tokens instead of sending passwords with every request

**Independent Test**: Issue a JWT token for a user, use it to authenticate API requests without password

### Tests for User Story 2

- [ ] T059 [P] [US2] Integration test for successful JWT auth in backend/tests/test_jwt_auth.rs (test_jwt_auth_success)
- [ ] T060 [P] [US2] Integration test for expired JWT token in backend/tests/test_jwt_auth.rs (test_jwt_auth_expired_token)
- [ ] T061 [P] [US2] Integration test for invalid JWT signature in backend/tests/test_jwt_auth.rs (test_jwt_auth_invalid_signature)
- [ ] T062 [P] [US2] Integration test for untrusted issuer in backend/tests/test_jwt_auth.rs (test_jwt_auth_untrusted_issuer)
- [ ] T063 [P] [US2] Integration test for missing sub claim in backend/tests/test_jwt_auth.rs (test_jwt_auth_missing_sub_claim)
- [ ] T064 [P] [US2] Unit test for JWT validation in backend/crates/kalamdb-auth/tests/jwt_tests.rs (test_validate_jwt, test_extract_claims)

### Implementation for User Story 2

- [ ] T065 [P] [US2] Add JWT issuer allowlist configuration in backend/config.toml
- [ ] T066 [US2] Update AuthService to support JWT authentication in backend/crates/kalamdb-auth/src/service.rs (check for Bearer token, validate, extract user_id)
- [ ] T067 [US2] Implement optional user existence verification in backend/crates/kalamdb-auth/src/service.rs (lookup user_id in system.users if configured)
- [ ] T068 [US2] Update authentication middleware to support Bearer token in backend/src/middleware.rs
- [ ] T069 [P] [US2] Implement JWKS caching for public key rotation in backend/crates/kalamdb-auth/src/jwt_auth.rs (background refresh, 1 hour TTL)
- [ ] T070 [US2] Add TOKEN_EXPIRED error response handling in backend/src/middleware.rs (return 401 with clear message)

**Checkpoint**: User Stories 1 AND 2 complete - users can authenticate with both HTTP Basic Auth and JWT tokens

---

## Phase 5: User Story 3 - Role-Based Access Control (Priority: P1)

**Goal**: Different user roles (user, service, dba, system) have appropriate access levels to enforce principle of least privilege

**Independent Test**: Create users with different roles, verify each role can only perform allowed operations

### Tests for User Story 3

- [ ] T071 [P] [US3] Integration test for user role permissions in backend/tests/test_rbac.rs (test_user_role_own_tables_access, test_user_role_cannot_access_others)
- [ ] T072 [P] [US3] Integration test for service role permissions in backend/tests/test_rbac.rs (test_service_role_cross_user_access, test_service_role_flush_operations)
- [ ] T073 [P] [US3] Integration test for dba role permissions in backend/tests/test_rbac.rs (test_dba_role_create_tables, test_dba_role_manage_users)
- [ ] T074 [P] [US3] Integration test for system role permissions in backend/tests/test_rbac.rs (test_system_role_all_access)
- [ ] T075 [P] [US3] Integration test for forbidden operations in backend/tests/test_rbac.rs (test_user_cannot_create_namespace, test_user_cannot_manage_users)
- [ ] T076 [P] [US3] Unit test for permission checking in backend/crates/kalamdb-core/tests/auth_tests.rs (test_can_access_table, test_can_create_table)

### Implementation for User Story 3

- [ ] T077 [US3] Implement user table ownership check in backend/crates/kalamdb-core/src/auth/roles.rs (user can only access tables where owner = user_id)
- [ ] T078 [US3] Implement service role cross-user access in backend/crates/kalamdb-core/src/auth/roles.rs (service can access any user table)
- [ ] T079 [US3] Implement dba role administrative operations in backend/crates/kalamdb-core/src/auth/roles.rs (can create/drop tables, manage users)
- [ ] T080 [US3] Add authorization checks to table creation in backend/crates/kalamdb-core/src/sql/executor.rs (only dba/system can create tables)
- [ ] T081 [US3] Add authorization checks to namespace operations in backend/crates/kalamdb-core/src/sql/executor.rs (only dba/system)
- [ ] T082 [US3] Add authorization checks to user management endpoints in backend/crates/kalamdb-api/src/handlers/user_handler.rs (only dba/system)
- [ ] T083 [US3] Implement 403 Forbidden error response with role info in backend/src/middleware.rs (error, message, required_role, user_role, request_id)
- [ ] T084 [US3] Add read access to system tables for service role in backend/crates/kalamdb-core/src/auth/roles.rs (system.jobs, system.live_queries, system.tables)

**Checkpoint**: All P1 user stories complete - authentication and authorization fully functional

---

## Phase 5.5: SQL Parser Extensions (Foundational for User Stories 4-8)

**Purpose**: Add SQL command parsing for user management (CREATE USER, ALTER USER, DROP USER)

**⚠️ BLOCKING**: Required before User Stories 4-8 implementation

**⚠️ ARCHITECTURAL COMPLIANCE**: MUST follow existing parser patterns from `backend/crates/kalamdb-sql/src/parser/extensions.rs`

### SQL Parser Implementation (Following ExtensionStatement Pattern)

- [ ] T084A [P] Add CreateUserStatement struct in backend/crates/kalamdb-sql/src/ddl/user_commands.rs (follow CreateStorageStatement pattern with parse() method)
- [ ] T084B [P] Add AlterUserStatement struct in backend/crates/kalamdb-sql/src/ddl/user_commands.rs (follow AlterStorageStatement pattern)
- [ ] T084C [P] Add DropUserStatement struct in backend/crates/kalamdb-sql/src/ddl/user_commands.rs (follow DropStorageStatement pattern)
- [ ] T084D Add CreateUser, AlterUser, DropUser variants to ExtensionStatement enum in backend/crates/kalamdb-sql/src/parser/extensions.rs (same pattern as CreateStorage, FlushTable)
- [ ] T084E Update ExtensionStatement::parse() to handle CREATE USER, ALTER USER, DROP USER in backend/crates/kalamdb-sql/src/parser/extensions.rs (follow if-statement pattern)
- [ ] T084F Export user_commands module from backend/crates/kalamdb-sql/src/ddl/mod.rs

### SQL Executor Implementation (Following Existing Executor Pattern)

- [ ] T084G Create user_executor.rs in backend/crates/kalamdb-core/src/sql/ (follow table_executor.rs pattern)
- [ ] T084H [P] Implement execute_create_user() in backend/crates/kalamdb-core/src/sql/user_executor.rs (call kalamdb-store create_user, return ExecutionResult)
- [ ] T084I [P] Implement execute_alter_user() in backend/crates/kalamdb-core/src/sql/user_executor.rs (call kalamdb-store update_user)
- [ ] T084J [P] Implement execute_drop_user() in backend/crates/kalamdb-core/src/sql/user_executor.rs (soft delete via kalamdb-store)
- [ ] T084K Add authorization checks to user management executors (only dba/system can execute, follow pattern from CREATE TABLE authorization)

### Unit Tests for SQL Parser (Following Existing Test Patterns)

- [ ] T084L [P] Unit test for parse CREATE USER WITH PASSWORD in backend/crates/kalamdb-sql/tests/parser/user_commands_tests.rs
- [ ] T084M [P] Unit test for parse CREATE USER WITH OAUTH in backend/crates/kalamdb-sql/tests/parser/user_commands_tests.rs
- [ ] T084N [P] Unit test for parse CREATE USER WITH INTERNAL in backend/crates/kalamdb-sql/tests/parser/user_commands_tests.rs
- [ ] T084O [P] Unit test for parse ALTER USER SET PASSWORD in backend/crates/kalamdb-sql/tests/parser/user_commands_tests.rs
- [ ] T084P [P] Unit test for parse ALTER USER SET ROLE in backend/crates/kalamdb-sql/tests/parser/user_commands_tests.rs
- [ ] T084Q [P] Unit test for parse DROP USER in backend/crates/kalamdb-sql/tests/parser/user_commands_tests.rs

**Checkpoint**: SQL parser ready - user management commands follow same architecture as storage/flush commands

---

## Phase 6: User Story 4 - Shared Table Access Control (Priority: P2)

**Goal**: Enable controlled data sharing via shared tables with configurable access levels (public, private, restricted)

**Independent Test**: Create shared tables with different access levels, verify users can only access tables matching their role and access level

### Tests for User Story 4

- [ ] T085 [P] [US4] Integration test for public shared table access in backend/tests/test_shared_access.rs (test_public_table_read_only_for_users)
- [ ] T086 [P] [US4] Integration test for private shared table access in backend/tests/test_shared_access.rs (test_private_table_service_dba_only)
- [ ] T087 [P] [US4] Integration test for default access level in backend/tests/test_shared_access.rs (test_shared_table_defaults_to_private)
- [ ] T088 [P] [US4] Integration test for access level modification in backend/tests/test_shared_access.rs (test_change_access_level_requires_privileges)
- [ ] T089 [P] [US4] Integration test for read-only enforcement in backend/tests/test_shared_access.rs (test_user_cannot_modify_public_table)

### Implementation for User Story 4

- [ ] T090 [P] [US4] Add access_level field to shared table creation SQL in backend/crates/kalamdb-sql/src/parser.rs (CREATE SHARED TABLE ... ACCESS LEVEL ...)
- [ ] T091 [US4] Implement default access level (private) in backend/crates/kalamdb-store/src/tables.rs (set during table creation)
- [ ] T092 [US4] Implement public table read-only access for users in backend/crates/kalamdb-core/src/auth/access.rs (check_shared_table_access)
- [ ] T093 [US4] Implement private/restricted table access for service/dba/system only in backend/crates/kalamdb-core/src/auth/access.rs
- [ ] T094 [US4] Add ALTER TABLE SET ACCESS LEVEL command in backend/crates/kalamdb-sql/src/parser.rs (requires service/dba/system role)
- [ ] T095 [US4] Implement access level change logic in backend/crates/kalamdb-store/src/tables.rs (update_table_access_level)
- [ ] T096 [US4] Enforce user tables and system tables have NULL access_level in backend/crates/kalamdb-store/src/tables.rs

**Checkpoint**: User Story 4 complete - shared table access control working

---

## Phase 7: User Story 5 - System User Management (Priority: P2)

**Goal**: Internal processes authenticate securely as system users with localhost-only access by default, optional remote access with password

**Independent Test**: Create system user, verify localhost authentication without password, confirm remote connections blocked unless explicitly enabled

### Tests for User Story 5

- [ ] T097 [P] [US5] Integration test for system user localhost access in backend/tests/test_system_users.rs (test_system_user_localhost_no_password)
- [ ] T098 [P] [US5] Integration test for system user remote access denied in backend/tests/test_system_users.rs (test_system_user_remote_denied_by_default)
- [ ] T099 [P] [US5] Integration test for system user remote access with password in backend/tests/test_system_users.rs (test_system_user_remote_with_password)
- [ ] T100 [P] [US5] Integration test for system user remote access without password denied in backend/tests/test_system_users.rs (test_system_user_remote_no_password_denied)
- [ ] T101 [P] [US5] Integration test for global allow_remote_access config in backend/tests/test_system_users.rs (test_global_remote_access_flag)
- [ ] T102 [P] [US5] Unit test for localhost detection in backend/crates/kalamdb-auth/tests/connection_tests.rs (test_localhost_detection_127_0_0_1, test_localhost_detection_ipv6, test_localhost_detection_unix_socket)

### Implementation for User Story 5

- [ ] T103 [US5] Implement localhost-only authentication for internal auth_type in backend/crates/kalamdb-auth/src/service.rs (check connection.is_localhost)
- [ ] T104 [US5] Implement per-user allow_remote metadata check in backend/crates/kalamdb-auth/src/service.rs (read from metadata JSON)
- [ ] T105 [US5] Implement global allow_remote_access configuration in backend/config.toml and backend/src/config.rs
- [ ] T106 [US5] Add password requirement validation for remote system users in backend/crates/kalamdb-auth/src/service.rs (deny if remote enabled but no password)
- [ ] T107 [US5] Implement connection source storage in backend/crates/kalamdb-auth/src/context.rs (AuthenticatedUser.connection_info)
- [ ] T108 [US5] Add localhost detection for IPv4, IPv6, Unix socket in backend/crates/kalamdb-auth/src/connection.rs (is_localhost method)

**Checkpoint**: User Story 5 complete - system users working with localhost/remote access control

---

## Phase 8: User Story 6 - CLI Tool Authentication (Priority: P2)

**Goal**: CLI tool automatically authenticates using a default system user created during database initialization

**Independent Test**: Run database initialization, verify system user created, confirm CLI can authenticate and execute commands

**⚠️ ARCHITECTURAL COMPLIANCE**: Authentication logic MUST be in `kalamdb-link` crate for sharing across CLI, WASM, and other clients

### Tests for User Story 6

- [ ] T109 [P] [US6] Integration test for database initialization creating system user in backend/tests/test_cli_auth.rs (test_init_creates_system_user)
- [ ] T110 [P] [US6] Integration test for CLI automatic authentication in cli/tests/test_cli_auth.rs (test_cli_auto_auth)
- [ ] T111 [P] [US6] Integration test for CLI credential storage in cli/tests/test_cli_auth.rs (test_cli_credentials_stored_securely)
- [ ] T112 [P] [US6] Integration test for multiple database instances in cli/tests/test_cli_auth.rs (test_cli_multiple_instances)
- [ ] T113 [P] [US6] Integration test for credential rotation in cli/tests/test_cli_auth.rs (test_cli_credential_rotation)

### Implementation for User Story 6

#### Shared Authentication Logic (kalamdb-link crate)

- [ ] T114 [P] [US6] Update AuthProvider enum in link/src/auth.rs to support HTTP Basic Auth (add BasicAuth(username, password) variant)
- [ ] T115 [P] [US6] Implement BasicAuth header formatting in link/src/auth.rs (base64 encode username:password, follow HTTP Basic Auth RFC 7617)
- [ ] T116 [P] [US6] Add system_user_auth() helper in link/src/auth.rs (convenience method for system user credentials)
- [ ] T117 [P] [US6] Update apply_to_request() in link/src/auth.rs to handle BasicAuth variant (set Authorization: Basic header)
- [ ] T118 [P] [US6] Add credential storage abstraction in link/src/credentials.rs (trait CredentialStore with get_credentials(), set_credentials(), supports multiple storage backends)

#### CLI-Specific Implementation (cli crate)

- [ ] T119 [US6] Implement FileCredentialStore in cli/src/config.rs (store at ~/.config/kalamdb/credentials.toml with 0600 permissions, implements CredentialStore trait from link)
- [ ] T120 [US6] Implement automatic authentication in CLI session in cli/src/session.rs (read credentials via FileCredentialStore, create BasicAuth provider, pass to KalamLinkClient)
- [ ] T121 [US6] Add CLI commands to view system user credentials in cli/src/commands/credentials.rs (show-credentials command, uses FileCredentialStore)
- [ ] T122 [US6] Add CLI commands to update system user credentials in cli/src/commands/credentials.rs (update-credentials command, uses FileCredentialStore)
- [ ] T123 [US6] Implement per-instance credential management in cli/src/config.rs (support multiple database configurations in credentials.toml)
- [ ] T124 [US6] Add authentication error handling in CLI with clear messages in cli/src/error.rs (handle 401, 403 responses, suggest credential check)

#### Backend System User Initialization

- [ ] T125 [US6] Add system user creation to database initialization in backend/src/lifecycle.rs (create default system user on first startup, username: "kalamdb_cli", auth_type: "internal", role: "system")
- [ ] T126 [US6] Generate and store system user credentials during init in backend/src/lifecycle.rs (create random password for emergency remote access, store in secure location)
- [ ] T127 [US6] Log system user credentials to stdout during first init in backend/src/lifecycle.rs (display username and credentials path, remind user to save securely)

**Checkpoint**: User Story 6 complete - CLI authentication working seamlessly, reusable in WASM and other clients

---

## Phase 9: User Story 7 - Password Security (Priority: P2)

**Goal**: Passwords stored securely with bcrypt hashing, never exposed in plaintext or logs

**Independent Test**: Create user with password, verify hash stored (not plaintext), confirm authentication works via hash comparison

### Tests for User Story 7

- [ ] T121 [P] [US7] Integration test for password hashing in backend/tests/test_password_security.rs (test_password_never_plaintext)
- [ ] T122 [P] [US7] Integration test for concurrent authentication in backend/tests/test_password_security.rs (test_concurrent_bcrypt_non_blocking)
- [ ] T123 [P] [US7] Integration test for weak password rejection in backend/tests/test_password_security.rs (test_weak_password_rejected)
- [ ] T124 [P] [US7] Integration test for minimum password length in backend/tests/test_password_security.rs (test_min_password_length_8)
- [ ] T125 [P] [US7] Integration test for maximum password length in backend/tests/test_password_security.rs (test_max_password_length_1024)

### Implementation for User Story 7

- [ ] T126 [US7] Ensure password never logged in backend/src/logging.rs (filter password from all log output)
- [ ] T127 [US7] Ensure password never exposed in error messages in backend/crates/kalamdb-auth/src/error.rs (generic "invalid credentials" message)
- [ ] T128 [US7] Implement password length validation in backend/crates/kalamdb-auth/src/password.rs (min 8, max 1024)
- [ ] T129 [US7] Implement common password blocking in user creation endpoint in backend/crates/kalamdb-api/src/handlers/user_handler.rs (call is_common_password)
- [ ] T130 [US7] Add WEAK_PASSWORD error response in backend/src/middleware.rs
- [ ] T131 [US7] Implement configurable common password check disable in backend/config.toml (disable_common_password_check flag)

**Checkpoint**: User Story 7 complete - password security fully implemented

---

## Phase 10: User Story 8 - OAuth Integration (Priority: P3)

**Goal**: Users can authenticate using OAuth providers (Google, GitHub, Azure) for centralized identity management

**Independent Test**: Configure OAuth provider, create user with OAuth auth, verify authentication using OAuth token

### Tests for User Story 8

- [ ] T132 [P] [US8] Integration test for OAuth authentication in backend/tests/test_oauth.rs (test_oauth_google_success)
- [ ] T133 [P] [US8] Integration test for OAuth user cannot use password in backend/tests/test_oauth.rs (test_oauth_user_password_rejected)
- [ ] T134 [P] [US8] Integration test for OAuth token subject matching in backend/tests/test_oauth.rs (test_oauth_subject_matching)
- [ ] T135 [P] [US8] Integration test for OAuth auto-provisioning in backend/tests/test_oauth.rs (test_oauth_auto_provision_disabled_by_default)

### Implementation for User Story 8

- [ ] T136 [P] [US8] Add OAuth provider configuration in backend/config.toml (providers list with Google, GitHub, Azure)
- [ ] T137 [US8] Implement OAuth token validation in backend/crates/kalamdb-auth/src/oauth.rs (validate_oauth_token, extract_provider_and_subject)
- [ ] T138 [US8] Update AuthService to support OAuth authentication in backend/crates/kalamdb-auth/src/service.rs (check auth_type = "oauth", match subject)
- [ ] T139 [US8] Implement OAuth user creation with auth_data JSON in backend/crates/kalamdb-api/src/handlers/user_handler.rs (store {"provider": "...", "subject": "..."})
- [ ] T140 [US8] Prevent OAuth users from password authentication in backend/crates/kalamdb-auth/src/service.rs (check auth_type, reject password)
- [ ] T141 [US8] Implement optional auto-provisioning for OAuth users in backend/crates/kalamdb-auth/src/service.rs (create user on first OAuth login if configured)

**Checkpoint**: User Story 8 complete - OAuth integration working

---

## Phase 0.5: Storage Backend Abstraction & Store Consolidation (Priority: P0 - CRITICAL)

**⚠️ CRITICAL**: This phase MUST be completed FIRST before ANY authentication implementation. Estimated 5 days solo / 3 days team.

**Goal**: Establish two-layer storage abstraction (StorageBackend + EntityStore traits) and consolidate ALL stores into kalamdb-core/src/stores/ with strongly-typed entity models

**Independent Test**: Verify kalamdb-core, kalamdb-sql, backend have zero rocksdb imports; mock storage backend passes integration tests; all existing tests still pass

**Why This Is Critical**:
- Authentication depends on UserStore which must follow this new pattern
- All existing stores (UserTableStore, SharedTableStore, StreamTableStore) must be migrated
- Touching these files after auth is added would require rewriting auth code
- Affects 20+ files, 300-500 lines of code changes

### Sub-Phase 0.5.1: Storage Infrastructure (kalamdb-store)

**Purpose**: Create two-layer abstraction foundation

- [ ] T001A [P] [US9] Create backend/crates/kalamdb-store/src/backend.rs with StorageBackend trait (put, get, delete, scan_prefix, scan_range, delete_batch methods)
- [ ] T001B [P] [US9] Implement RocksDbBackend struct in backend/crates/kalamdb-store/src/backend.rs (wraps Arc<rocksdb::DB>, implements StorageBackend)
- [ ] T001C [P] [US9] Create backend/crates/kalamdb-store/src/traits.rs with EntityStore<T> trait (backend(), partition(), serialize(), deserialize(), put(), get(), delete(), scan_prefix())
- [ ] T001D [P] [US9] Add default JSON serialization/deserialization to EntityStore<T> trait in backend/crates/kalamdb-store/src/traits.rs
- [ ] T001E [P] [US9] Create backend/crates/kalamdb-store/src/mock_backend.rs with MockStorageBackend (HashMap-based implementation for testing)
- [ ] T001F [US9] Update backend/crates/kalamdb-store/src/lib.rs to export StorageBackend, EntityStore traits, RocksDbBackend, MockStorageBackend
- [ ] T001G [P] [US9] Add integration test for MockStorageBackend in backend/crates/kalamdb-store/src/tests/mock_backend_tests.rs (verify all trait methods work)

### Sub-Phase 0.5.2: Domain Models (kalamdb-core)

**Purpose**: Create strongly-typed entity models for all storage

- [ ] T002A [US9] Create backend/crates/kalamdb-core/src/models/mod.rs directory
- [ ] T002B [P] [US9] Define User struct in backend/crates/kalamdb-core/src/models/system.rs (id, username, password_hash, role, email, auth_type, auth_data, created_at, updated_at, last_seen, deleted_at) with Serialize, Deserialize, Clone, Debug
- [ ] T002C [P] [US9] Define Job struct in backend/crates/kalamdb-core/src/models/system.rs (job_id, job_type, namespace_id, table_name, status, created_at, completed_at) with Serialize, Deserialize
- [ ] T002D [P] [US9] Define Namespace struct in backend/crates/kalamdb-core/src/models/system.rs (namespace_id, name, owner_id, created_at) with Serialize, Deserialize
- [ ] T002E [P] [US9] Define UserTableRow struct in backend/crates/kalamdb-core/src/models/tables.rs (fields: Map<String, Value> with #[serde(flatten)], _updated: String, _deleted: bool) with Serialize, Deserialize
- [ ] T002F [P] [US9] Define SharedTableRow struct in backend/crates/kalamdb-core/src/models/tables.rs (similar to UserTableRow with access_level field) with Serialize, Deserialize
- [ ] T002G [P] [US9] Define StreamTableRow struct in backend/crates/kalamdb-core/src/models/tables.rs (fields + ttl fields) with Serialize, Deserialize
- [ ] T002H [US9] Export all models from backend/crates/kalamdb-core/src/models/mod.rs (pub use system::*, pub use tables::*)

### Sub-Phase 0.5.3: System Stores (kalamdb-core)

**Purpose**: Create new stores for system tables with EntityStore<T> implementation

**Note**: All system tables (users, jobs, namespaces) share the same pattern - they all extend EntityStore

- [ ] T003A [P] [US9] Create backend/crates/kalamdb-core/src/stores/mod.rs directory structure
- [ ] T003B [P] [US9] Create UserStore in backend/crates/kalamdb-core/src/stores/user_store.rs (backend: Arc<dyn StorageBackend>, partition: "system_users")
- [ ] T003C [P] [US9] Implement EntityStore<User> for UserStore in backend/crates/kalamdb-core/src/stores/user_store.rs (override serialize/deserialize to use bincode for performance)
- [ ] T003D [P] [US9] Add custom methods to UserStore: get_by_username(), list_all(), list_active() in backend/crates/kalamdb-core/src/stores/user_store.rs
- [ ] T003E [P] [US9] Create JobStore in backend/crates/kalamdb-core/src/stores/job_store.rs (backend: Arc<dyn StorageBackend>, partition: "system_jobs")
- [ ] T003F [P] [US9] Implement EntityStore<Job> for JobStore in backend/crates/kalamdb-core/src/stores/job_store.rs (bincode serialization)
- [ ] T003G [P] [US9] Add custom methods to JobStore: get_by_status(), get_by_namespace() in backend/crates/kalamdb-core/src/stores/job_store.rs
- [ ] T003H [P] [US9] Create NamespaceStore in backend/crates/kalamdb-core/src/stores/namespace_store.rs (backend: Arc<dyn StorageBackend>, partition: "system_namespaces")
- [ ] T003I [P] [US9] Implement EntityStore<Namespace> for NamespaceStore in backend/crates/kalamdb-core/src/stores/namespace_store.rs (bincode serialization)
- [ ] T003J [P] [US9] Add custom methods to NamespaceStore: get_by_owner(), list_all() in backend/crates/kalamdb-core/src/stores/namespace_store.rs
- [ ] T003K [US9] Export all system stores from backend/crates/kalamdb-core/src/stores/mod.rs

### Sub-Phase 0.5.4: Migrate UserTableStore

**Purpose**: Move UserTableStore from kalamdb-store to kalamdb-core with new architecture

- [ ] T004A [US9] Copy backend/crates/kalamdb-store/src/user_table_store.rs to backend/crates/kalamdb-core/src/stores/user_table_store.rs
- [ ] T004B [US9] Change db: Arc<DB> to backend: Arc<dyn StorageBackend> in UserTableStore struct in backend/crates/kalamdb-core/src/stores/user_table_store.rs
- [ ] T004C [US9] Add partition_name: String field to UserTableStore in backend/crates/kalamdb-core/src/stores/user_table_store.rs
- [ ] T004D [US9] Update UserTableStore::new() to accept Arc<dyn StorageBackend>, namespace_id, table_name in backend/crates/kalamdb-core/src/stores/user_table_store.rs
- [ ] T004E [US9] Implement EntityStore<UserTableRow> for UserTableStore in backend/crates/kalamdb-core/src/stores/user_table_store.rs
- [ ] T004F [US9] Override serialize() method to inject _updated and _deleted system columns in backend/crates/kalamdb-core/src/stores/user_table_store.rs
- [ ] T004G [US9] Update all self.db.put_cf() → self.backend.put(self.partition(), ...) in backend/crates/kalamdb-core/src/stores/user_table_store.rs
- [ ] T004H [US9] Update all self.db.get_cf() → self.backend.get(self.partition(), ...) in backend/crates/kalamdb-core/src/stores/user_table_store.rs
- [ ] T004I [US9] Update all self.db.delete_cf() → self.backend.delete(self.partition(), ...) in backend/crates/kalamdb-core/src/stores/user_table_store.rs
- [ ] T004J [US9] Update scan methods to use self.backend.scan_prefix() in backend/crates/kalamdb-core/src/stores/user_table_store.rs
- [ ] T004K [US9] Remove use rocksdb::* imports from backend/crates/kalamdb-core/src/stores/user_table_store.rs
- [ ] T004L [US9] Fix compilation errors from RocksDB API changes in backend/crates/kalamdb-core/src/stores/user_table_store.rs
- [ ] T004M [US9] Delete original backend/crates/kalamdb-store/src/user_table_store.rs file
- [ ] T004N [US9] Update imports in backend/crates/kalamdb-core to use kalamdb_core::stores::UserTableStore

### Sub-Phase 0.5.5: Migrate SharedTableStore

**Purpose**: Move SharedTableStore from kalamdb-store to kalamdb-core with new architecture

- [ ] T005A [US9] Copy backend/crates/kalamdb-store/src/shared_table_store.rs to backend/crates/kalamdb-core/src/stores/shared_table_store.rs
- [ ] T005B [US9] Change db: Arc<DB> to backend: Arc<dyn StorageBackend> in SharedTableStore struct in backend/crates/kalamdb-core/src/stores/shared_table_store.rs
- [ ] T005C [US9] Add partition_name: String field to SharedTableStore in backend/crates/kalamdb-core/src/stores/shared_table_store.rs
- [ ] T005D [US9] Update SharedTableStore::new() to accept Arc<dyn StorageBackend>, namespace_id, table_name in backend/crates/kalamdb-core/src/stores/shared_table_store.rs
- [ ] T005E [US9] Implement EntityStore<SharedTableRow> for SharedTableStore in backend/crates/kalamdb-core/src/stores/shared_table_store.rs
- [ ] T005F [US9] Update all RocksDB calls to use StorageBackend trait in backend/crates/kalamdb-core/src/stores/shared_table_store.rs (put_cf → put, get_cf → get)
- [ ] T005G [US9] Remove use rocksdb::* imports from backend/crates/kalamdb-core/src/stores/shared_table_store.rs
- [ ] T005H [US9] Delete original backend/crates/kalamdb-store/src/shared_table_store.rs file
- [ ] T005I [US9] Update imports in backend/crates/kalamdb-core to use kalamdb_core::stores::SharedTableStore

### Sub-Phase 0.5.6: Migrate StreamTableStore

**Purpose**: Move StreamTableStore from kalamdb-store to kalamdb-core with new architecture

- [ ] T006A [US9] Copy backend/crates/kalamdb-store/src/stream_table_store.rs to backend/crates/kalamdb-core/src/stores/stream_table_store.rs
- [ ] T006B [US9] Change db: Arc<DB> to backend: Arc<dyn StorageBackend> in StreamTableStore struct in backend/crates/kalamdb-core/src/stores/stream_table_store.rs
- [ ] T006C [US9] Add partition_name: String field to StreamTableStore in backend/crates/kalamdb-core/src/stores/stream_table_store.rs
- [ ] T006D [US9] Update StreamTableStore::new() to accept Arc<dyn StorageBackend>, namespace_id, table_name in backend/crates/kalamdb-core/src/stores/stream_table_store.rs
- [ ] T006E [US9] Implement EntityStore<StreamTableRow> for StreamTableStore in backend/crates/kalamdb-core/src/stores/stream_table_store.rs
- [ ] T006F [US9] Update all RocksDB calls to use StorageBackend trait in backend/crates/kalamdb-core/src/stores/stream_table_store.rs
- [ ] T006G [US9] Remove use rocksdb::* imports from backend/crates/kalamdb-core/src/stores/stream_table_store.rs
- [ ] T006H [US9] Delete original backend/crates/kalamdb-store/src/stream_table_store.rs file
- [ ] T006I [US9] Update imports in backend/crates/kalamdb-core to use kalamdb_core::stores::StreamTableStore

### Sub-Phase 0.5.7: Refactor kalamdb-core Storage Layer

**Purpose**: Remove RocksDB from kalamdb-core completely

- [ ] T007A [US9] Delete backend/crates/kalamdb-core/src/storage/rocksdb_store.rs file
- [ ] T007B [US9] Delete backend/crates/kalamdb-core/src/storage/rocksdb_init.rs file
- [ ] T007C [US9] Delete backend/crates/kalamdb-core/src/storage/rocksdb_config.rs file
- [ ] T007D [US9] Replace all Arc<rocksdb::DB> with Arc<dyn StorageBackend> in backend/crates/kalamdb-core/src/catalog/mod.rs
- [ ] T007E [US9] Replace all Arc<rocksdb::DB> with Arc<dyn StorageBackend> in backend/crates/kalamdb-core/src/storage/mod.rs
- [ ] T007F [US9] Update KalamCore::new() constructor to accept Arc<dyn StorageBackend> in backend/crates/kalamdb-core/src/lib.rs
- [ ] T007G [US9] Pass StorageBackend to all store constructors (UserTableStore, SharedTableStore, etc.) in backend/crates/kalamdb-core/src/lib.rs
- [ ] T007H [US9] Remove all use rocksdb::* imports from backend/crates/kalamdb-core/src/**/*.rs
- [ ] T007I [US9] Remove rocksdb = "0.24" from backend/crates/kalamdb-core/Cargo.toml dependencies
- [ ] T007J [US9] Add bincode = "1.3" to backend/crates/kalamdb-core/Cargo.toml (for system table serialization)
- [ ] T007K [US9] Fix all compilation errors in kalamdb-core from RocksDB removal
- [ ] T007L [US9] Run cargo check on kalamdb-core to verify no RocksDB dependencies

### Sub-Phase 0.5.8: Refactor kalamdb-sql Adapter

**Purpose**: Update SQL layer to use StorageBackend instead of RocksDB

- [ ] T008A [US9] Rename RocksDbAdapter to StorageAdapter in backend/crates/kalamdb-sql/src/adapter.rs
- [ ] T008B [US9] Change db: Arc<rocksdb::DB> to backend: Arc<dyn StorageBackend> in StorageAdapter struct in backend/crates/kalamdb-sql/src/adapter.rs
- [ ] T008C [US9] Update StorageAdapter::new() to accept Arc<dyn StorageBackend> in backend/crates/kalamdb-sql/src/adapter.rs
- [ ] T008D [US9] Update all RocksDB-specific calls to use StorageBackend trait methods in backend/crates/kalamdb-sql/src/adapter.rs
- [ ] T008E [US9] Update KalamSql::new() constructor to accept Arc<dyn StorageBackend> in backend/crates/kalamdb-sql/src/lib.rs
- [ ] T008F [US9] Remove all use rocksdb::* imports from backend/crates/kalamdb-sql/src/**/*.rs
- [ ] T008G [US9] Remove rocksdb = "0.24" from backend/crates/kalamdb-sql/Cargo.toml dependencies
- [ ] T008H [US9] Fix all compilation errors in kalamdb-sql from RocksDB removal
- [ ] T008I [US9] Run cargo check on kalamdb-sql to verify no RocksDB dependencies

### Sub-Phase 0.5.9: Refactor Backend Initialization

**Purpose**: Backend creates RocksDbBackend and passes Arc<dyn StorageBackend> to all crates

- [ ] T009A [US9] Update backend/src/lifecycle.rs to create RocksDbBackend from kalamdb_store::RocksDbBackend::new()
- [ ] T009B [US9] Wrap RocksDbBackend in Arc<dyn StorageBackend> in backend/src/lifecycle.rs
- [ ] T009C [US9] Pass Arc<dyn StorageBackend> to KalamCore::new() in backend/src/lifecycle.rs
- [ ] T009D [US9] Pass Arc<dyn StorageBackend> to KalamSql::new() in backend/src/lifecycle.rs
- [ ] T009E [US9] Update backend/src/main.rs to use new initialization pattern
- [ ] T009F [US9] Remove direct Arc<rocksdb::DB> passing from backend/src/lifecycle.rs
- [ ] T009G [US9] Remove all use rocksdb::* imports from backend/src/**/*.rs
- [ ] T009H [US9] Remove rocksdb = "0.24" from backend/Cargo.toml workspace dependencies (keep ONLY in kalamdb-store)
- [ ] T009I [US9] Fix all compilation errors in backend from initialization changes
- [ ] T009J [US9] Run cargo check on backend to verify no RocksDB dependencies

### Sub-Phase 0.5.10: Verification & Testing

**Purpose**: Ensure refactoring is complete and nothing breaks

- [ ] T010A [P] [US9] Run cargo check on entire workspace and verify it compiles
- [ ] T010B [P] [US9] Run grep -r "use rocksdb" backend/crates/kalamdb-core/src and verify zero results
- [ ] T010C [P] [US9] Run grep -r "use rocksdb" backend/crates/kalamdb-sql/src and verify zero results
- [ ] T010D [P] [US9] Run grep -r "use rocksdb" backend/src and verify zero results
- [ ] T010E [P] [US9] Run cargo tree -i rocksdb and verify ONLY kalamdb-store depends on it
- [ ] T010F [P] [US9] Run all existing integration tests in backend/tests/ and verify they pass
- [ ] T010G [P] [US9] Create integration test with MockStorageBackend in backend/tests/test_mock_storage.rs (verify UserStore, JobStore work with mock)
- [ ] T010H [P] [US9] Document two-layer architecture in docs/architecture/STORAGE_ABSTRACTION.md
- [ ] T010I [US9] Update README.md to mention storage backend abstraction capability
- [ ] T010J [US9] Add architecture diagram showing StorageBackend and EntityStore layers

**Checkpoint**: Phase 0.5 complete - All stores migrated, RocksDB isolated to kalamdb-store only, ready for authentication implementation

---

## Phase 11: Testing & Migration

**Purpose**: Integration tests, edge case coverage, and backward compatibility

### Edge Case Tests

- [ ] T143A [P] Integration test for empty credentials in backend/tests/test_edge_cases.rs (test_empty_credentials_401)
- [ ] T143B [P] Integration test for malformed Basic Auth header in backend/tests/test_edge_cases.rs (test_malformed_basic_auth_400)
- [ ] T143C [P] Integration test for concurrent auth requests in backend/tests/test_edge_cases.rs (test_concurrent_auth_no_race_conditions)
- [ ] T143D [P] Integration test for deleted user authentication in backend/tests/test_edge_cases.rs (test_deleted_user_denied)
- [ ] T143E [P] Integration test for role change during session in backend/tests/test_edge_cases.rs (test_role_change_applies_next_request)
- [ ] T143F [P] Integration test for maximum password length in backend/tests/test_edge_cases.rs (test_max_password_10mb_rejected)
- [ ] T143G [P] Integration test for shared table default access in backend/tests/test_edge_cases.rs (test_shared_table_defaults_private)

### Backward Compatibility & Migration

- [ ] T144 [P] Update ALL existing integration tests to use auth helper in backend/tests/ (scan for all test files, add authenticate() calls)
- [ ] T145 Implement backward compatibility for X-API-KEY header in backend/src/middleware.rs (support old and new auth simultaneously)
- [ ] T146 Implement backward compatibility for X-USER-ID header in backend/src/middleware.rs (honor if present)
- [ ] T147 Add deprecation warnings for old auth headers in backend/src/logging.rs (log warning when X-API-KEY used)
- [ ] T148 Create migration documentation in docs/migration/OLD_AUTH_TO_NEW_AUTH.md (timeline, steps, examples)

### User Management Endpoints

- [ ] T149 [P] Implement GET /v1/users/{user_id} endpoint in backend/crates/kalamdb-api/src/handlers/user_handler.rs (auth: dba/system or self)
- [ ] T150 [P] Implement PUT /v1/users/{user_id} endpoint in backend/crates/kalamdb-api/src/handlers/user_handler.rs (auth: dba/system or self with limited fields)
- [ ] T151 [P] Implement DELETE /v1/users/{user_id} endpoint in backend/crates/kalamdb-api/src/handlers/user_handler.rs (soft delete, auth: dba/system)
- [ ] T152 [P] Implement GET /v1/users endpoint in backend/crates/kalamdb-api/src/handlers/user_handler.rs (list users with filters, auth: dba/system)
- [ ] T153 [P] Implement POST /v1/users/restore/{user_id} endpoint in backend/crates/kalamdb-api/src/handlers/user_handler.rs (restore soft-deleted, auth: dba/system)

---

## Phase 12: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, performance optimization, security hardening

- [ ] T154 [P] Create API contracts documentation in specs/007-user-auth/contracts/auth.yaml (POST /v1/auth/login, POST /v1/auth/validate)
- [ ] T155 [P] Create API contracts documentation in specs/007-user-auth/contracts/users.yaml (POST /v1/users, GET /v1/users/{user_id}, PUT, DELETE, GET list)
- [ ] T156 [P] Create API contracts documentation in specs/007-user-auth/contracts/errors.yaml (401, 403 error response schemas)
- [ ] T157 [P] Create quickstart guide in specs/007-user-auth/quickstart.md (database init, create first user, Basic Auth example, JWT example, RBAC examples, CLI auth, troubleshooting)
- [ ] T158 [P] Update agent context with new technologies in .github/copilot-instructions.md (bcrypt, HTTP Basic Auth, JWT, RBAC, Actix-Web auth middleware, StorageBackend abstraction)
- [ ] T159 Code cleanup and refactoring across all modified files
- [ ] T160 [P] Performance benchmarking for authentication endpoints (measure p50, p95, p99 latency)
- [ ] T161 [P] Security audit of authentication code (password handling, timing attacks, error messages)
- [ ] T162 Implement user record caching for performance (moka cache, 99%+ hit rate, saves 1-5ms RocksDB lookup)
- [ ] T163 Implement JWT token claim caching for performance (moka cache, 5-10x speedup, <1ms p95 latency)
- [ ] T164 [P] Add request_id to all authentication error responses in backend/src/middleware.rs (for troubleshooting)
- [ ] T165 Final end-to-end test in backend/tests/test_e2e_auth_flow.rs (create user → authenticate → execute query → soft delete → restore)

---

## Phase 13: Additional Features from User-Management.md

**Purpose**: Complete features specified in User-Management.md but not in original spec

### Scheduled Cleanup Job

- [ ] T166 Create UserCleanupJob struct in backend/crates/kalamdb-core/src/jobs/user_cleanup.rs (follow RetentionPolicy pattern from retention.rs)
- [ ] T167 Implement UserCleanupConfig with grace_period_days in backend/crates/kalamdb-core/src/jobs/user_cleanup.rs (follow RetentionConfig pattern)
- [ ] T168 Implement enforce() method in UserCleanupJob (find expired users where deleted_at < now - grace_period, delete tables, delete users, follow RetentionPolicy::enforce() pattern)
- [ ] T169 Export UserCleanupJob from backend/crates/kalamdb-core/src/jobs/mod.rs (add to pub use statements)
- [ ] T170 Integrate cleanup job into scheduler in backend/src/lifecycle.rs (use TokioJobManager::start_job(), register in system.jobs table, follow flush job pattern)
- [ ] T171 [P] Add cleanup job configuration in backend/config.toml (deletion_grace_period_days, cleanup_job_schedule cron expression)
- [ ] T172 [P] Integration test for cleanup job in backend/tests/test_user_cleanup.rs (test_cleanup_deletes_expired_users, test_cleanup_cascade_deletes_tables, verify system.jobs entries)

### Database Indexes

- [ ] T173 Add idx_users_username index constant to ColumnFamilyNames in backend/crates/kalamdb-commons/src/constants.rs (follow existing index naming pattern)
- [ ] T174 Implement create_username_index() in backend/crates/kalamdb-store/src/users.rs (secondary index column family, follow existing index patterns)
- [ ] T175 [P] Create idx_users_role index for role filtering in backend/crates/kalamdb-store/src/users.rs (use db.put_cf() for index updates)
- [ ] T176 [P] Create idx_users_deleted_at index for cleanup job efficiency in backend/crates/kalamdb-store/src/users.rs
- [ ] T177 [P] Create idx_tables_access index in backend/crates/kalamdb-store/src/tables.rs (for shared table access level queries)

### Comprehensive Audit Logging

- [ ] T178 [P] Create system.audit_log table schema in backend/crates/kalamdb-store/src/audit_log.rs (audit_id, timestamp, user_id, action, target, details, ip_address)
- [ ] T179 [P] Implement audit logging for CREATE USER in backend/crates/kalamdb-core/src/sql/user_executor.rs
- [ ] T180 [P] Implement audit logging for ALTER USER in backend/crates/kalamdb-core/src/sql/user_executor.rs
- [ ] T181 [P] Implement audit logging for DROP USER in backend/crates/kalamdb-core/src/sql/user_executor.rs
- [ ] T182 [P] Implement audit logging for ALTER TABLE SET ACCESS in backend/crates/kalamdb-core/src/sql/table_executor.rs

### Migration Scripts

- [ ] T183 [P] Create schema migration script in backend/migrations/001_add_auth_columns.sql (ALTER TABLE system.users ADD COLUMN role, auth_type, auth_data, metadata, deleted_at)
- [ ] T184 [P] Create data migration script in backend/migrations/002_migrate_existing_users.sql (SET auth_type='password', generate placeholder hashes)
- [ ] T185 [P] Create index creation script in backend/migrations/003_create_auth_indexes.sql (CREATE INDEX idx_users_username, idx_users_role, idx_users_deleted_at, idx_tables_access)
- [ ] T186 [P] Create migration runner in backend/src/migrations/runner.rs (execute migrations in order, track applied migrations)

### Password Complexity Validation

- [ ] T187 [P] Implement password complexity validation in backend/crates/kalamdb-auth/src/password.rs (uppercase, lowercase, digit, special char)
- [ ] T188 [P] Add enforce_password_complexity configuration flag in backend/config.toml
- [ ] T189 [P] Integration test for password complexity in backend/tests/test_password_complexity.rs (test_complexity_uppercase_required, test_complexity_lowercase_required, test_complexity_digit_required, test_complexity_special_char_required)

### Integration Tests for SQL Commands (from User-Management.md)

- [ ] T190 [P] Integration test for CREATE USER WITH PASSWORD in backend/tests/test_user_sql.rs
- [ ] T191 [P] Integration test for CREATE USER WITH OAUTH in backend/tests/test_user_sql.rs
- [ ] T192 [P] Integration test for CREATE USER WITH INTERNAL in backend/tests/test_user_sql.rs
- [ ] T193 [P] Integration test for CREATE USER duplicate error in backend/tests/test_user_sql.rs
- [ ] T194 [P] Integration test for ALTER USER SET PASSWORD in backend/tests/test_user_sql.rs
- [ ] T195 [P] Integration test for ALTER USER SET ROLE in backend/tests/test_user_sql.rs
- [ ] T196 [P] Integration test for ALTER USER SET EMAIL in backend/tests/test_user_sql.rs
- [ ] T197 [P] Integration test for ALTER USER not found error in backend/tests/test_user_sql.rs
- [ ] T198 [P] Integration test for DROP USER soft delete in backend/tests/test_user_sql.rs
- [ ] T199 [P] Integration test for DROP USER IF EXISTS in backend/tests/test_user_sql.rs
- [ ] T200 [P] Integration test for restore deleted user (UPDATE deleted_at = NULL) in backend/tests/test_user_sql.rs
- [ ] T201 [P] Integration test for SELECT users excludes deleted in backend/tests/test_user_sql.rs
- [ ] T202 [P] Integration test for SELECT deleted users explicit in backend/tests/test_user_sql.rs

### System User Tests

- [ ] T203 [P] Integration test for system user localhost access without password in backend/tests/test_system_user.rs
- [ ] T204 [P] Integration test for system user remote access blocked by default in backend/tests/test_system_user.rs
- [ ] T205 [P] Integration test for system user remote access with password in backend/tests/test_system_user.rs
- [ ] T206 [P] Integration test for system user remote access without password rejected in backend/tests/test_system_user.rs

### Last Seen Tracking

- [ ] T207 [P] Implement last_seen column in system.users in backend/crates/kalamdb-store/src/users.rs
- [ ] T208 [P] Implement daily last_seen update in backend/crates/kalamdb-auth/src/service.rs (async, non-blocking, once per day)
- [ ] T209 [P] Integration test for last_seen tracking in backend/tests/test_last_seen.rs (test_last_seen_updated_once_per_day)

**Checkpoint**: All features from User-Management.md fully implemented

---

## Dependencies & Execution Order

### Phase Dependencies

**⚠️ CRITICAL**: Phase 0.5 (Storage Refactoring) MUST be completed FIRST before ANY other work

- **Phase 0.5 (Storage Refactoring)**: No dependencies - MUST START IMMEDIATELY - BLOCKS everything else
- **Setup (Phase 1)**: Depends on Phase 0.5 completion (needs stores to be migrated)
- **Foundational (Phase 2)**: Depends on Setup (Phase 1) + Phase 0.5 - BLOCKS all user stories
- **User Stories (Phase 3-10)**: All depend on Phase 0.5 + Foundational (Phase 2) completion
  - US1 (Basic Auth): Depends on Phase 0.5 (uses UserStore with EntityStore<User>)
  - US2 (JWT): Depends on Phase 0.5 (uses UserStore)
  - US3 (RBAC): Depends on Phase 0.5 (uses all stores with StorageBackend)
  - US4 (Shared Tables): Depends on Phase 0.5 + US3 (RBAC) for role checks
  - US5 (System Users): Depends on Phase 0.5 (uses UserStore)
  - US6 (CLI): Depends on Phase 0.5 + US5 (System Users) for CLI system user creation
  - US7 (Password Security): Depends on Phase 0.5 (uses UserStore)
  - US8 (OAuth): Depends on Phase 0.5 (uses UserStore)
- **Testing & Migration (Phase 11)**: Depends on Phase 0.5 + all desired user stories being complete
- **Polish (Phase 12)**: Depends on Phase 0.5 + all user stories being complete

### User Story Dependencies

```
Phase 0.5: Storage Refactoring (CRITICAL - DO FIRST)
    │
    ├─> Setup (Phase 1)
    │       │
    │       └─> Foundational (Phase 2)
    │               ├─ US1 (Basic Auth) ────────────┐
    │               ├─ US2 (JWT) ───────────────────┤
    │               ├─ US3 (RBAC) ──────────────────┼─> Testing & Migration (Phase 11)
    │               │       └─ US4 (Shared Tables) ─┤
    │               ├─ US5 (System Users) ──────────┤
    │               │       └─ US6 (CLI) ───────────┤
    │               ├─ US7 (Password Security) ─────┤
    │               └─ US8 (OAuth) ─────────────────┤
    │                                                └─> Polish (Phase 12)
    └─> (All phases depend on Phase 0.5)
```

**CRITICAL**: Phase 0.5 is the foundational refactoring that establishes the storage architecture. ALL authentication work depends on it because:
- UserStore (system.users) uses `EntityStore<User>` trait
- JobStore, NamespaceStore use same pattern
- All table stores (UserTableStore, SharedTableStore, StreamTableStore) use `EntityStore<T>`
- No crate except kalamdb-store can import rocksdb

### Critical Path (Correct Order)

**YOU MUST FOLLOW THIS ORDER**:

1. **Phase 0.5**: Storage Refactoring (5 days solo / 3 days team) - **MANDATORY FIRST**
   - Create StorageBackend + EntityStore traits
   - Create all domain models (User, Job, Namespace, UserTableRow, etc.)
   - Migrate all stores from kalamdb-store to kalamdb-core/src/stores/
   - Remove RocksDB from kalamdb-core, kalamdb-sql, backend
   - Verify with grep checks and integration tests

2. **Phase 1**: Setup (0.5 day)
   - Create kalamdb-auth crate
   - Add enums to kalamdb-commons

3. **Phase 2**: Foundational (2 days)
   - Implement password hashing, JWT validation, auth middleware
   - Build on top of Phase 0.5's UserStore

4. **Phase 3+**: User Stories (8-12 days)
   - Implement authentication features using the new storage architecture

**DO NOT skip Phase 0.5** - attempting authentication without it will require complete rewrite later.

### Parallel Opportunities

**Within Phase 0.5 Storage Refactoring**:

**Sub-Phase 0.5.1** (Storage Infrastructure):
- T001A, T001B, T001C, T001D, T001E can run in parallel (different files in kalamdb-store)
- T001G can run after infrastructure is created

**Sub-Phase 0.5.2** (Domain Models):
- All model creation tasks (T002A-T002H) can run in parallel (different model files)

**Sub-Phase 0.5.3** (System Stores):
- T003B-T003D (UserStore), T003E-T003G (JobStore), T003H-T003J (NamespaceStore) can run in parallel
- Each store is independent, different files

**Sub-Phases 0.5.4-0.5.6** (Store Migrations):
- UserTableStore, SharedTableStore, StreamTableStore migrations can run in parallel (different team members)
- Each sub-phase is self-contained

**Sub-Phase 0.5.10** (Verification):
- All grep checks (T010B-T010E) can run in parallel
- Documentation (T010H-T010J) can happen in parallel with tests

**Within Setup (Phase 1)**:
- T003, T004, T005, T006, T008, T009, T010 can all run in parallel (different files)

**Within Foundational (Phase 2)**:
- All core modules (T011-T019) can run in parallel (different source files)
- All storage functions (T022-T028) can run in parallel (different functions in same file)
- Authorization modules (T031-T032) can run in parallel
- Logging functions (T036-T038) can run in parallel
- Rate limiting modules (T040-T041) can run in parallel

**Across User Stories (Phase 3-10)** (ONLY AFTER Phase 0.5 complete):
- If team capacity allows, multiple user stories can be worked on in parallel:
  - Team Member 1: US1 (Basic Auth)
  - Team Member 2: US2 (JWT)
  - Team Member 3: US3 (RBAC)
  - Team Member 4: US5 (System Users)
  - Team Member 5: US7 (Password Security)
  - Team Member 6: US8 (OAuth)
- US4 must wait for US3, US6 must wait for US5

**Within Each User Story**:
- All tests marked [P] can run in parallel (different test files)
- Different implementation files can be worked on in parallel where marked [P]

### Recommended Execution Strategy

**Solo Developer (Sequential) - CORRECT ORDER**:
1. **Phase 0.5: Storage Refactoring (5 days) - MANDATORY FIRST**
   - Day 1-2: Create infrastructure, models, system stores
   - Day 3-4: Migrate existing stores (UserTableStore, SharedTableStore, StreamTableStore)
   - Day 5: Refactor kalamdb-core, kalamdb-sql, backend + verification
2. Phase 1: Setup (0.5 day)
3. Phase 2: Foundational (2 days)
4. Phase 3: US1 Basic Auth (1 day)
5. Phase 4: US2 JWT (1 day)
6. Phase 5: US3 RBAC (1-2 days)
7. Phase 6: US4 Shared Tables (1 day)
8. Phase 7: US5 System Users (1 day)
9. Phase 8: US6 CLI (1 day)
10. Phase 9: US7 Password Security (0.5 day)
11. Phase 10: US8 OAuth (1 day)
12. Phase 11: Testing & Migration (1-2 days)
13. Phase 12: Polish (1 day)

**Total Estimated Time**: 16-20 days solo (including mandatory storage refactoring)

**Team of 3+ (Parallel) - CORRECT ORDER**:

**Week 1 (Days 1-3): MANDATORY Storage Refactoring**
- **Day 1-3: Phase 0.5 - All team members work on storage refactoring**
  - Team Member 1: Sub-Phases 0.5.1-0.5.2 (infrastructure + models)
  - Team Member 2: Sub-Phase 0.5.3 (system stores)
  - Team Member 3: Sub-Phase 0.5.4 (UserTableStore migration)
  - Team Member 4: Sub-Phases 0.5.5-0.5.6 (SharedTableStore + StreamTableStore)
  - Team Member 5: Sub-Phases 0.5.7-0.5.9 (kalamdb-core, kalamdb-sql, backend refactoring)
  - All: Sub-Phase 0.5.10 (verification + testing)

**Week 1 (Days 4-5): Setup + Foundational**
- Phase 1-2: Setup + Foundational (1-2 days, all team members work together)

**Week 2 (Days 6-10): Parallel User Stories**
- Team Member 1: US1 (Basic Auth)
- Team Member 2: US2 (JWT)
- Team Member 3: US3 (RBAC) → then US4 (Shared Tables)
- Team Member 4: US5 (System Users) → then US6 (CLI)
- Team Member 5: US7 (Password Security) + US8 (OAuth)

**Week 3 (Days 11-12): Testing & Polish**
- Phase 11: Testing & Migration (1 day, all team members)
- Phase 12: Polish (1 day, all team members)

**Total Estimated Time**: 12 days with 5-person team (3 days storage refactoring + 9 days features)
5. Phase 11: Testing & Migration (1 day, all team members)
6. Phase 12: Polish (1 day)

**Total Estimated Time**: 7-9 days with team (including storage refactoring)

---

## Parallel Example: Foundational Phase

```bash
# Developer 1: Core authentication modules
git checkout -b feat/auth-core
# Work on T011-T019 (password, basic_auth, jwt_auth, connection, context, error, service)

# Developer 2: Storage layer
git checkout -b feat/auth-storage
# Work on T020-T030 (system_users column family, user CRUD operations)

# Developer 3: Authorization layer
git checkout -b feat/auth-authz
# Work on T031-T034 (RBAC permission checking, table access control)

# Developer 4: Logging & rate limiting
git checkout -b feat/auth-logging-ratelimit
# Work on T035-T044 (dedicated auth.log, rate limiting)

# All merge to main after completion, enabling user story work to begin
```

---

## Parallel Example: User Story Phase

```bash
# Developer 1: Basic Auth (US1)
git checkout -b feat/us1-basic-auth
# Work on T045-T058 (tests + implementation for HTTP Basic Auth)

# Developer 2: JWT (US2)
git checkout -b feat/us2-jwt
# Work on T059-T070 (tests + implementation for JWT authentication)

# Developer 3: RBAC (US3)
git checkout -b feat/us3-rbac
# Work on T071-T084 (tests + implementation for role-based access control)

# Developer 4: System Users (US5)
git checkout -b feat/us5-system-users
# Work on T097-T108 (tests + implementation for system user localhost/remote access)

# All stories are independently testable and can be merged separately
```

---

## Implementation Strategy

### MVP First Approach

**Minimum Viable Product**: US1 (Basic Auth) + US3 (RBAC)

This delivers:
- Users can authenticate with username/password
- Role-based permissions enforce access control
- Database is secure and functional

**Why this is sufficient for MVP**:
- HTTP Basic Auth works with all clients (curl, Postman, SDKs)
- RBAC prevents unauthorized access
- Can be deployed to production immediately
- JWT, OAuth, CLI can be added incrementally

### Incremental Delivery

After MVP, add features in this priority order:
1. **US2 (JWT)**: Enable stateless token authentication for modern apps
2. **US5 (System Users)**: Secure internal processes
3. **US6 (CLI)**: Improve developer experience
4. **US4 (Shared Tables)**: Enable data sharing use cases
5. **US7 (Password Security)**: Harden password validation
6. **US8 (OAuth)**: Enterprise SSO integration

Each increment is independently deployable and adds clear value.

---

## Success Metrics

**Task Completion**: 170 total tasks

**Test Coverage**:
- Unit tests: 15+ tests in kalamdb-auth crate
- Integration tests: 50+ tests across all user stories
- Edge case tests: 7 tests for edge cases
- All existing tests updated and passing

**Performance Targets** (from spec.md):
- HTTP Basic Auth: <100ms (95th percentile) ✅ SC-001
- JWT validation: <50ms (95th percentile) ✅ SC-002
- RBAC check: <5ms per operation ✅ SC-003
- 1000 concurrent auth requests without degradation ✅ SC-008

**Security Validation**:
- Zero passwords in logs ✅ SC-009, FR-SEC-001
- Generic error messages (no user enumeration) ✅ SC-009, FR-AUTH-010
- Bcrypt cost factor 12 ✅ FR-AUTH-007
- Rate limiting implemented ✅ FR-SEC-002 through FR-SEC-005
- Common password blocking ✅ FR-AUTH-019 through FR-AUTH-022

**Functional Completeness**:
- All 93 functional requirements implemented ✅
- All 8 user stories independently testable ✅
- All 12 edge cases handled ✅
- All 13 success criteria met ✅

---

## Task Summary

| Phase | Task Count | Estimated Time (Solo) | Parallel Opportunities |
|-------|------------|----------------------|------------------------|
| Phase 1: Setup | 10 tasks | 0.5 day | 7 tasks (70%) |
| Phase 2: Foundational | 34 tasks | 2 days | 25 tasks (74%) |
| Phase 3: US1 Basic Auth | 14 tasks | 1 day | 6 tasks (43%) |
| Phase 4: US2 JWT | 12 tasks | 1 day | 6 tasks (50%) |
| Phase 5: US3 RBAC | 14 tasks | 1.5 days | 6 tasks (43%) |
| Phase 6: US4 Shared Tables | 12 tasks | 1 day | 5 tasks (42%) |
| Phase 7: US5 System Users | 12 tasks | 1 day | 6 tasks (50%) |
| Phase 8: US6 CLI | 12 tasks | 1 day | 5 tasks (42%) |
| Phase 9: US7 Password Security | 11 tasks | 0.5 day | 5 tasks (45%) |
| Phase 10: US8 OAuth | 10 tasks | 1 day | 4 tasks (40%) |
| Phase 11: Testing & Migration | 17 tasks | 1.5 days | 8 tasks (47%) |
| Phase 12: Polish | 12 tasks | 1 day | 9 tasks (75%) |
| **TOTAL** | **170 tasks** | **12-14 days** | **92 parallel (54%)** |

**Parallel Work Efficiency**: 54% of tasks can run in parallel with proper team coordination

**Critical Path**: Setup → Foundational → US1 → US3 → Testing → Polish (minimum 6-7 days with parallelization)

**Recommended Team Size**: 3-4 developers for optimal parallel execution
