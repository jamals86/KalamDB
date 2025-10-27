# Tasks: User Authentication

**Feature Branch**: `007-user-auth`  
**Input**: Design documents from `/specs/007-user-auth/`  
**Prerequisites**: plan.md, spec.md, research.md, data-model.md

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

**Tests**: Integration tests are included per FR-TEST-001 through FR-TEST-010 requirements.

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

- [ ] T020 Create system_users column family initialization in backend/crates/kalamdb-store/src/lib.rs
- [ ] T021 Implement User struct with serde in backend/crates/kalamdb-store/src/users.rs (user_id, username, email, auth_type, auth_data, role, storage_mode, storage_id, metadata, created_at, updated_at, last_seen, deleted_at)
- [ ] T022 [P] Implement create_user function in backend/crates/kalamdb-store/src/users.rs (with UserId auto-generation)
- [ ] T023 [P] Implement get_user_by_id function in backend/crates/kalamdb-store/src/users.rs
- [ ] T024 [P] Implement get_user_by_username function in backend/crates/kalamdb-store/src/users.rs
- [ ] T025 [P] Implement update_user function in backend/crates/kalamdb-store/src/users.rs
- [ ] T026 [P] Implement soft_delete_user function in backend/crates/kalamdb-store/src/users.rs
- [ ] T027 [P] Implement list_users function in backend/crates/kalamdb-store/src/users.rs (with role filter, deleted filter)
- [ ] T028 [P] Implement update_last_seen function in backend/crates/kalamdb-store/src/users.rs (daily granularity)
- [ ] T029 Create username_index for fast username lookups in backend/crates/kalamdb-store/src/users.rs
- [ ] T030 Export user storage functions from backend/crates/kalamdb-store/src/lib.rs

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

### Tests for User Story 6

- [ ] T109 [P] [US6] Integration test for database initialization creating system user in cli/tests/test_cli_auth.rs (test_init_creates_system_user)
- [ ] T110 [P] [US6] Integration test for CLI automatic authentication in cli/tests/test_cli_auth.rs (test_cli_auto_auth)
- [ ] T111 [P] [US6] Integration test for CLI credential storage in cli/tests/test_cli_auth.rs (test_cli_credentials_stored_securely)
- [ ] T112 [P] [US6] Integration test for multiple database instances in cli/tests/test_cli_auth.rs (test_cli_multiple_instances)
- [ ] T113 [P] [US6] Integration test for credential rotation in cli/tests/test_cli_auth.rs (test_cli_credential_rotation)

### Implementation for User Story 6

- [ ] T114 [US6] Add system user creation to database initialization in backend/src/lifecycle.rs (create default system user on first startup)
- [ ] T115 [US6] Implement credentials.toml storage in cli/src/config.rs (store at ~/.config/kalamdb/credentials.toml with 0600 permissions)
- [ ] T116 [US6] Implement automatic authentication in CLI session in cli/src/session.rs (read credentials, authenticate on connect)
- [ ] T117 [US6] Add CLI commands to view system user credentials in cli/src/commands/credentials.rs (show-credentials command)
- [ ] T118 [US6] Add CLI commands to update system user credentials in cli/src/commands/credentials.rs (update-credentials command)
- [ ] T119 [US6] Implement per-instance credential management in cli/src/config.rs (support multiple database configurations)
- [ ] T120 [US6] Add authentication error handling in CLI with clear messages in cli/src/error.rs (handle 401, 403 responses)

**Checkpoint**: User Story 6 complete - CLI authentication working seamlessly

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

**Checkpoint**: All user stories complete - full authentication system implemented

---

## Phase 11: Testing & Migration

**Purpose**: Integration tests, edge case coverage, and backward compatibility

### Edge Case Tests

- [ ] T142 [P] Integration test for empty credentials in backend/tests/test_edge_cases.rs (test_empty_credentials_401)
- [ ] T143 [P] Integration test for malformed Basic Auth header in backend/tests/test_edge_cases.rs (test_malformed_basic_auth_400)
- [ ] T144 [P] Integration test for concurrent auth requests in backend/tests/test_edge_cases.rs (test_concurrent_auth_no_race_conditions)
- [ ] T145 [P] Integration test for deleted user authentication in backend/tests/test_edge_cases.rs (test_deleted_user_denied)
- [ ] T146 [P] Integration test for role change during session in backend/tests/test_edge_cases.rs (test_role_change_applies_next_request)
- [ ] T147 [P] Integration test for maximum password length in backend/tests/test_edge_cases.rs (test_max_password_10mb_rejected)
- [ ] T148 [P] Integration test for shared table default access in backend/tests/test_edge_cases.rs (test_shared_table_defaults_private)

### Backward Compatibility & Migration

- [ ] T149 [P] Update ALL existing integration tests to use auth helper in backend/tests/ (scan for all test files, add authenticate() calls)
- [ ] T150 Implement backward compatibility for X-API-KEY header in backend/src/middleware.rs (support old and new auth simultaneously)
- [ ] T151 Implement backward compatibility for X-USER-ID header in backend/src/middleware.rs (honor if present)
- [ ] T152 Add deprecation warnings for old auth headers in backend/src/logging.rs (log warning when X-API-KEY used)
- [ ] T153 Create migration documentation in docs/migration/OLD_AUTH_TO_NEW_AUTH.md (timeline, steps, examples)

### User Management Endpoints

- [ ] T154 [P] Implement GET /v1/users/{user_id} endpoint in backend/crates/kalamdb-api/src/handlers/user_handler.rs (auth: dba/system or self)
- [ ] T155 [P] Implement PUT /v1/users/{user_id} endpoint in backend/crates/kalamdb-api/src/handlers/user_handler.rs (auth: dba/system or self with limited fields)
- [ ] T156 [P] Implement DELETE /v1/users/{user_id} endpoint in backend/crates/kalamdb-api/src/handlers/user_handler.rs (soft delete, auth: dba/system)
- [ ] T157 [P] Implement GET /v1/users endpoint in backend/crates/kalamdb-api/src/handlers/user_handler.rs (list users with filters, auth: dba/system)
- [ ] T158 [P] Implement POST /v1/users/restore/{user_id} endpoint in backend/crates/kalamdb-api/src/handlers/user_handler.rs (restore soft-deleted, auth: dba/system)

---

## Phase 12: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, performance optimization, security hardening

- [ ] T159 [P] Create API contracts documentation in specs/007-user-auth/contracts/auth.yaml (POST /v1/auth/login, POST /v1/auth/validate)
- [ ] T160 [P] Create API contracts documentation in specs/007-user-auth/contracts/users.yaml (POST /v1/users, GET /v1/users/{user_id}, PUT, DELETE, GET list)
- [ ] T161 [P] Create API contracts documentation in specs/007-user-auth/contracts/errors.yaml (401, 403 error response schemas)
- [ ] T162 [P] Create quickstart guide in specs/007-user-auth/quickstart.md (database init, create first user, Basic Auth example, JWT example, RBAC examples, CLI auth, troubleshooting)
- [ ] T163 [P] Update agent context with new technologies in .github/copilot-instructions.md (bcrypt, HTTP Basic Auth, JWT, RBAC, Actix-Web auth middleware)
- [ ] T164 Code cleanup and refactoring across all modified files
- [ ] T165 [P] Performance benchmarking for authentication endpoints (measure p50, p95, p99 latency)
- [ ] T166 [P] Security audit of authentication code (password handling, timing attacks, error messages)
- [ ] T167 Implement user record caching for performance (moka cache, 99%+ hit rate, saves 1-5ms RocksDB lookup)
- [ ] T168 Implement JWT token claim caching (LRU cache, 95%+ hit rate, saves 1-2ms signature verification)
- [ ] T169 [P] Add Bruno API test collection in docs/API-Kalam/ (CREATE USER, LOGIN, JWT AUTH, RBAC tests)
- [ ] T170 Run quickstart.md validation (execute all examples, verify they work)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup (Phase 1) - BLOCKS all user stories
- **User Stories (Phase 3-10)**: All depend on Foundational (Phase 2) completion
  - US1 (Basic Auth): Can start after Foundational - No dependencies on other stories
  - US2 (JWT): Can start after Foundational - No dependencies on other stories (independent)
  - US3 (RBAC): Can start after Foundational - No dependencies on other stories (independent)
  - US4 (Shared Tables): Depends on US3 (RBAC) for role checks
  - US5 (System Users): Can start after Foundational - No dependencies on other stories (independent)
  - US6 (CLI): Depends on US5 (System Users) for CLI system user creation
  - US7 (Password Security): Can start after Foundational - Enhances US1 (independent)
  - US8 (OAuth): Can start after Foundational - No dependencies on other stories (independent)
- **Testing & Migration (Phase 11)**: Depends on all desired user stories being complete
- **Polish (Phase 12)**: Depends on all user stories being complete

### User Story Dependencies

```
Foundational (Phase 2)
    ├─ US1 (Basic Auth) ────────────┐
    ├─ US2 (JWT) ───────────────────┤
    ├─ US3 (RBAC) ──────────────────┼─> Testing & Migration (Phase 11)
    │       └─ US4 (Shared Tables) ─┤
    ├─ US5 (System Users) ──────────┤
    │       └─ US6 (CLI) ───────────┤
    ├─ US7 (Password Security) ─────┤
    └─ US8 (OAuth) ─────────────────┘
            └─> Polish (Phase 12)
```

### Critical Path (Minimum MVP)

For fastest time-to-value, implement in this order:
1. **Phase 1-2**: Setup + Foundational (required)
2. **Phase 3**: US1 - Basic User Authentication (core authentication)
3. **Phase 5**: US3 - RBAC (core authorization)
4. **Phase 11**: Update existing tests (backward compatibility)

This delivers a working authenticated database with role-based access control.

### Parallel Opportunities

**Within Setup (Phase 1)**:
- T003, T004, T005, T006, T008, T009, T010 can all run in parallel (different files)

**Within Foundational (Phase 2)**:
- All core modules (T011-T019) can run in parallel (different source files)
- All storage functions (T022-T028) can run in parallel (different functions in same file)
- Authorization modules (T031-T032) can run in parallel
- Logging functions (T036-T038) can run in parallel
- Rate limiting modules (T040-T041) can run in parallel

**Across User Stories (Phase 3-10)**:
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

**Solo Developer (Sequential)**:
1. Phase 1-2: Setup + Foundational (1-2 days)
2. Phase 3: US1 Basic Auth (1 day)
3. Phase 4: US2 JWT (1 day)
4. Phase 5: US3 RBAC (1-2 days)
5. Phase 6: US4 Shared Tables (1 day)
6. Phase 7: US5 System Users (1 day)
7. Phase 8: US6 CLI (1 day)
8. Phase 9: US7 Password Security (0.5 day)
9. Phase 10: US8 OAuth (1 day)
10. Phase 11: Testing & Migration (1-2 days)
11. Phase 12: Polish (1 day)

**Total Estimated Time**: 10-14 days solo

**Team of 3+ (Parallel)**:
1. Phase 1-2: Setup + Foundational (1-2 days, all team members)
2. Parallel work on US1, US2, US3, US5, US7, US8 (2-3 days)
3. Sequential work on US4 (after US3), US6 (after US5) (1 day)
4. Phase 11: Testing & Migration (1 day, all team members)
5. Phase 12: Polish (1 day)

**Total Estimated Time**: 5-7 days with team

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
