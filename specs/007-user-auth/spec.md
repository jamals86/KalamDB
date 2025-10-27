# Feature Specification: User Authentication

**Feature Branch**: `007-user-auth`  
**Created**: October 27, 2025  
**Status**: Draft  
**Input**: User description: "User Authentication - Comprehensive authentication system with HTTP Basic Auth, JWT tokens, role-based access control, system user management, and CLI integration"

## Clarifications

### Session 2025-10-27

- Q: What JSON error response format should be used for authentication/authorization errors? → A: Custom error object: `{"error": "ERROR_CODE", "message": "...", "request_id": "..."}`
- Q: What observability/logging requirements are needed for authentication? → A: Log authentication failures, role changes, and admin operations (balanced approach)
- Q: How should JWT token refresh/renewal be handled when tokens expire? → A: Manual re-authentication only (stateless, delegates to external providers)
- Q: What rate limiting strategy should be used for authentication attempts? → A: Per-IP + Per-Username combined with dedicated logs/auth.log file for all authentication events
- Q: What password complexity policy should be enforced beyond min/max length? → A: Length-focused with common password blocking (NIST SP 800-63B aligned)

## User Scenarios & Testing *(mandatory)*

<!--
  IMPORTANT: User stories are prioritized as user journeys ordered by importance.
  Each user story/journey is independently testable and can be developed/deployed standalone.
-->

### User Story 1 - Basic User Authentication (Priority: P1)

A user needs to authenticate to KalamDB to access their data securely. They should be able to log in using their username and password through standard HTTP authentication methods that work with any client library.

**Why this priority**: Core security requirement - no other functionality is possible without user authentication. This is the foundation of the entire security model.

**Independent Test**: Can be fully tested by creating a user account with a password and successfully authenticating via HTTP Basic Auth to execute a simple SQL query. Delivers immediate value by securing the database.

**Acceptance Scenarios**:

1. **Given** a new user account with username "alice" and password "secret123" exists, **When** the user sends a request with HTTP Basic Auth credentials, **Then** the system authenticates the user and allows access to their data.

2. **Given** a user provides incorrect credentials, **When** they attempt to authenticate, **Then** the system rejects the request with an authentication error and does not grant access.

3. **Given** a user is already authenticated, **When** they make subsequent requests within the session, **Then** the system maintains their authenticated state without requiring re-authentication.

4. **Given** no authentication credentials are provided, **When** a user attempts to access protected resources, **Then** the system denies access and prompts for authentication.

---

### User Story 2 - Token-Based Authentication (Priority: P1)

Modern applications and services need to authenticate using bearer tokens (JWT) instead of sending passwords with every request. This allows for better security, token expiration, and integration with external authentication providers.

**Why this priority**: Essential for production deployments and service integrations. Supports standard OAuth2/OpenID Connect flows and allows external identity providers.

**Independent Test**: Can be fully tested by issuing a JWT token for a user and successfully using it to authenticate API requests without sending the password. Delivers value by enabling secure, stateless authentication.

**Acceptance Scenarios**:

1. **Given** a valid JWT token containing user identity in the "sub" claim, **When** the token is sent in the Authorization header, **Then** the system authenticates the user based on the token claims.

2. **Given** an expired JWT token, **When** the user attempts to authenticate, **Then** the system rejects the request with a token expiration error.

3. **Given** a JWT token with an invalid signature, **When** the user attempts to authenticate, **Then** the system rejects the request and denies access.

4. **Given** a JWT token from an untrusted issuer, **When** the user attempts to authenticate, **Then** the system validates the issuer against the configured allowlist and rejects unauthorized issuers.

5. **Given** an externally-issued JWT token, **When** validated by KalamDB, **Then** the system can optionally verify the user exists in the system.users table before granting access.

---

### User Story 3 - Role-Based Access Control (Priority: P1)

Different types of users require different levels of access. A regular user should only access their own data, while administrative users need broader access to manage the system. Service accounts need cross-user access for integrations.

**Why this priority**: Core authorization requirement - must be implemented alongside authentication to prevent privilege escalation and unauthorized access.

**Independent Test**: Can be fully tested by creating users with different roles (user, service, dba, system) and verifying each role can only perform allowed operations. Delivers value by enforcing the principle of least privilege.

**Acceptance Scenarios**:

1. **Given** a user with role "user", **When** they attempt to access their own tables, **Then** the system grants full access (SELECT, INSERT, UPDATE, DELETE).

2. **Given** a user with role "user", **When** they attempt to access another user's tables, **Then** the system denies access.

3. **Given** a user with role "service", **When** they attempt to access any user table or shared table, **Then** the system grants full access for integration purposes.

4. **Given** a user with role "dba", **When** they attempt to create/drop tables or manage users, **Then** the system grants full administrative access.

5. **Given** a user with role "system" connecting from localhost, **When** they attempt any operation, **Then** the system grants full access for internal processes.

6. **Given** a user with role "user", **When** they attempt to create a namespace or manage other users, **Then** the system denies access and returns an authorization error.

---

### User Story 4 - Shared Table Access Control (Priority: P2)

Organizations need to share certain tables across users (like analytics data or reference tables) while keeping other data private. Access levels should be configurable per table.

**Why this priority**: Enables data sharing use cases without compromising security. Important for multi-tenant applications and organizational data management.

**Independent Test**: Can be fully tested by creating shared tables with different access levels (public, private, restricted) and verifying users can only access tables matching their role and the table's access level. Delivers value by enabling controlled data sharing.

**Acceptance Scenarios**:

1. **Given** a shared table with access level "public", **When** any authenticated user queries it, **Then** the system allows read-only access (SELECT).

2. **Given** a shared table with access level "private", **When** a regular user attempts to access it, **Then** the system denies access.

3. **Given** a shared table with access level "private", **When** a service account or DBA accesses it, **Then** the system grants full access.

4. **Given** a shared table without specified access level, **When** the table is created, **Then** the system defaults to "private" access.

5. **Given** a regular user attempts to modify a public shared table, **When** they execute INSERT/UPDATE/DELETE, **Then** the system denies modification access (read-only for non-privileged users).

---

### User Story 5 - System User Management (Priority: P2)

Internal system processes (background jobs, replication, cleanup tasks) need authenticated access to perform administrative tasks. These system users should be secure by default (localhost-only) but allow remote access when explicitly configured for emergency administration.

**Why this priority**: Critical for operational reliability - background jobs and internal processes must authenticate securely. Default security (localhost-only) prevents unauthorized remote access.

**Independent Test**: Can be fully tested by creating a system user, verifying it can authenticate from localhost without a password, and confirming remote connections are blocked unless explicitly enabled with a password. Delivers value by securing internal operations while maintaining convenience.

**Acceptance Scenarios**:

1. **Given** a system user with auth_type "internal" and no password, **When** connecting from localhost, **Then** the system grants access without requiring a password.

2. **Given** a system user with auth_type "internal" and no password, **When** connecting from a remote address, **Then** the system denies access.

3. **Given** a system user with remote access enabled and a password configured, **When** connecting from a remote address with correct credentials, **Then** the system grants access.

4. **Given** a system user with remote access enabled but no password, **When** any connection attempt is made, **Then** the system denies access until a password is set.

5. **Given** the configuration has allow_remote_access = false globally, **When** any system user attempts remote connection, **Then** the system denies access regardless of individual user settings.

---

### User Story 6 - CLI Tool Authentication (Priority: P2)

When users initialize a new database or use the CLI tool, a system user should be automatically created and configured for the CLI to use. Users should be able to interact with the database immediately without manual user setup.

**Why this priority**: Essential for developer experience and operational workflows. CLI tools are primary interfaces for database administration and development.

**Independent Test**: Can be fully tested by running database initialization, verifying a system user is created, and confirming the CLI can authenticate and execute commands using this user. Delivers value by providing frictionless initial setup.

**Acceptance Scenarios**:

1. **Given** a new database is initialized, **When** the initialization completes, **Then** a default system user is created for CLI access with appropriate credentials stored securely.

2. **Given** the CLI tool starts, **When** it connects to the database, **Then** it automatically authenticates using the default system user without prompting for credentials.

3. **Given** the CLI is running on the same machine as the database, **When** a user executes SQL commands, **Then** the CLI uses localhost authentication with the system user.

4. **Given** multiple database instances exist, **When** the CLI connects, **Then** it uses the appropriate system user credentials for the target database instance.

5. **Given** the system user credentials need to be rotated, **When** an administrator updates them, **Then** the CLI configuration is updated to use the new credentials.

---

### User Story 7 - Password Security (Priority: P2)

User passwords must be stored securely and never exposed in plaintext. The system should use industry-standard hashing with appropriate cost factors to prevent rainbow table and brute force attacks.

**Why this priority**: Fundamental security requirement - weak password storage undermines all other security measures. Must be implemented correctly from the start.

**Independent Test**: Can be fully tested by creating a user, verifying the password is hashed in storage (not plaintext), and confirming authentication works via hash comparison. Delivers value by protecting user credentials.

**Acceptance Scenarios**:

1. **Given** a user creates an account with a password, **When** the password is stored, **Then** it is hashed using bcrypt with appropriate cost factor (never stored as plaintext).

2. **Given** a user authenticates with their password, **When** the system validates it, **Then** it compares the hash without ever storing or logging the plaintext password.

3. **Given** a password hash is stored, **When** an attacker gains read access to the database, **Then** they cannot reverse the hash to obtain the original password in reasonable time.

4. **Given** a user provides a weak password (too short, common), **When** they attempt to create an account, **Then** the system rejects it and requires a stronger password.

5. **Given** password hashing takes computational time, **When** multiple authentication requests arrive, **Then** the system handles concurrent authentication without blocking.

---

### User Story 8 - OAuth Integration (Priority: P3)

Service accounts and automated systems should be able to authenticate using OAuth providers (Google, GitHub, Azure) instead of maintaining separate passwords. This allows centralized identity management.

**Why this priority**: Enables enterprise SSO integration and reduces password management overhead. Not critical for initial deployment but important for enterprise adoption.

**Independent Test**: Can be fully tested by configuring an OAuth provider, creating a user with OAuth authentication, and verifying they can authenticate using an OAuth token. Delivers value by enabling federated identity.

**Acceptance Scenarios**:

1. **Given** a user account configured for OAuth with provider "google", **When** they present a valid Google OAuth token, **Then** the system authenticates them based on the token subject.

2. **Given** an OAuth user attempts to authenticate with a password, **When** the system checks their auth_type, **Then** it rejects password authentication and requires OAuth token.

3. **Given** an OAuth provider issues a token for a new user, **When** they first authenticate, **Then** the system can optionally auto-provision their account based on token claims.

4. **Given** an OAuth token's subject claim, **When** the system validates it, **Then** it matches the subject to the stored auth_data to identify the user.

---

### Edge Cases

- **Empty or Missing Credentials**: What happens when a user sends a request with no Authorization header or empty credentials? System must return 401 Unauthorized with clear error message.

- **Malformed Authorization Header**: What happens when the Authorization header has invalid format (not "Basic ..." or "Bearer ...")? System must reject with 400 Bad Request.

- **Concurrent Authentication Requests**: How does the system handle multiple simultaneous authentication attempts for the same user? Must handle concurrently without race conditions or deadlocks.

- **Password Hash Collision**: What happens if by chance two different passwords produce the same hash? Bcrypt's design makes this astronomically unlikely, but system should handle gracefully.

- **System User Remote Access Misconfiguration**: What happens if remote access is enabled for a system user but no password is set? System must deny all access until password is configured.

- **JWT Token Without 'sub' Claim**: What happens when a JWT token is valid but missing the required 'sub' (subject) claim? System must reject with clear error about missing user identity.

- **Deleted User Authentication**: What happens when a soft-deleted user attempts to authenticate? System must deny access and treat as non-existent user.

- **Role Change During Active Session**: What happens when a DBA changes a user's role while they have an active session? Behavior depends on session implementation - may require re-authentication.

- **Maximum Password Length**: What happens when a user provides an extremely long password (e.g., 10MB)? System must enforce reasonable maximum length to prevent DoS.

- **CLI Multiple Database Instances**: What happens when the CLI connects to different database instances with different system users? CLI must maintain separate credential configurations per instance.

- **Localhost Detection Edge Cases**: How does the system detect localhost (127.0.0.1, ::1, Unix sockets, "localhost" hostname)? Must handle all common localhost representations.

- **Shared Table Default Access**: What happens when a shared table is created without specifying access level? Must default to "private" for security.

## Requirements *(mandatory)*

### Functional Requirements

#### Authentication

- **FR-AUTH-001**: System MUST support HTTP Basic Authentication with format "Authorization: Basic base64(username:password)"
- **FR-AUTH-002**: System MUST support Bearer JWT token authentication with format "Authorization: Bearer <token>"
- **FR-AUTH-003**: System MUST validate JWT signatures using configured public keys or shared secrets
- **FR-AUTH-004**: System MUST extract user identity from JWT "sub" claim
- **FR-AUTH-005**: System MUST verify JWT token expiration and reject expired tokens
- **FR-AUTH-006**: System MUST validate JWT issuer against configured allowlist
- **FR-AUTH-007**: System MUST hash all passwords using bcrypt with configurable cost factor (default: 12)
- **FR-AUTH-008**: System MUST never store or log passwords in plaintext
- **FR-AUTH-009**: System MUST reject requests without valid authentication credentials (401 Unauthorized)
- **FR-AUTH-010**: System MUST return clear error messages for authentication failures without revealing whether username exists
- **FR-AUTH-011**: System MUST return error responses in JSON format: `{"error": "ERROR_CODE", "message": "descriptive message", "request_id": "unique_request_id"}`
- **FR-AUTH-012**: System MUST use error codes: MISSING_AUTHORIZATION, INVALID_CREDENTIALS, MALFORMED_AUTHORIZATION, TOKEN_EXPIRED, INVALID_SIGNATURE, UNTRUSTED_ISSUER, MISSING_CLAIM, WEAK_PASSWORD
- **FR-AUTH-013**: System MUST support optional user existence verification in system.users table for JWT authentication
- **FR-AUTH-014**: System MUST validate password minimum length (8 characters)
- **FR-AUTH-015**: System MUST enforce maximum password length to prevent DoS attacks (suggested: 1024 characters)
- **FR-AUTH-016**: System MUST NOT implement refresh token functionality (tokens require full re-authentication upon expiration)
- **FR-AUTH-017**: System SHOULD support configurable JWT expiration times (recommended: 1-24 hours based on security requirements)
- **FR-AUTH-018**: When JWT tokens expire, system MUST return TOKEN_EXPIRED error and require client re-authentication via HTTP Basic Auth or external OAuth provider
- **FR-AUTH-019**: System MUST block common passwords by validating against embedded list of top 10,000 common passwords (e.g., "password", "12345678", "qwerty")
- **FR-AUTH-020**: System MUST NOT enforce character composition requirements (uppercase, numbers, special characters) - length-focused approach per NIST SP 800-63B
- **FR-AUTH-021**: System MUST return WEAK_PASSWORD error code when password is found in common password list
- **FR-AUTH-022**: System SHOULD support configuration option to disable common password checking for internal/development deployments

#### Authorization (Role-Based Access Control)

- **FR-AUTHZ-001**: System MUST support four user roles: "user", "service", "dba", "system"
- **FR-AUTHZ-002**: Users with role "user" MUST have full access to their own user tables only
- **FR-AUTHZ-003**: Users with role "user" MUST have read-only access to shared tables with access level "public"
- **FR-AUTHZ-004**: Users with role "user" MUST NOT have access to other users' tables
- **FR-AUTHZ-005**: Users with role "user" MUST NOT be able to create/drop tables or namespaces
- **FR-AUTHZ-006**: Users with role "service" MUST have full access to all user tables and all shared tables
- **FR-AUTHZ-007**: Users with role "service" MUST be able to execute FLUSH, BACKUP, and CLEANUP operations
- **FR-AUTHZ-008**: Users with role "service" MUST have read access to system.jobs, system.live_queries, system.tables
- **FR-AUTHZ-009**: Users with role "service" MUST NOT be able to create/drop tables or manage users
- **FR-AUTHZ-010**: Users with role "dba" MUST have full access to all tables (system, shared, user)
- **FR-AUTHZ-011**: Users with role "dba" MUST be able to create/drop/alter namespaces, tables, and storages
- **FR-AUTHZ-012**: Users with role "dba" MUST be able to manage users (create, update, delete, restore)
- **FR-AUTHZ-013**: Users with role "system" MUST have full access to all operations (same as dba)
- **FR-AUTHZ-014**: System MUST deny unauthorized operations with 403 Forbidden status and error response: `{"error": "FORBIDDEN", "message": "...", "required_role": "...", "user_role": "...", "request_id": "..."}`
- **FR-AUTHZ-015**: System MUST check authorization after successful authentication and before executing operations

#### Shared Table Access Control

- **FR-SHARED-001**: System MUST support three access levels for shared tables: "public", "private", "restricted"
- **FR-SHARED-002**: Shared tables with access level "public" MUST allow read-only access (SELECT) to any authenticated user
- **FR-SHARED-003**: Shared tables with access level "public" MUST only allow modifications by service, dba, and system roles
- **FR-SHARED-004**: Shared tables with access level "private" or "restricted" MUST only be accessible by service, dba, and system roles
- **FR-SHARED-005**: System MUST default shared tables to "private" access when no access level is specified
- **FR-SHARED-006**: System MUST allow changing a shared table's access level after creation (service/dba/system roles only)
- **FR-SHARED-007**: User tables and system tables MUST NOT have an access level (NULL value)

#### System User Management

- **FR-SYSTEM-001**: System MUST support auth_type "internal" for system users (localhost-only by default)
- **FR-SYSTEM-002**: System users with auth_type "internal" and no password MUST authenticate from localhost without credentials
- **FR-SYSTEM-003**: System users with auth_type "internal" MUST be denied remote access by default
- **FR-SYSTEM-004**: System MUST support global configuration flag "allow_remote_access" to enable remote system user access
- **FR-SYSTEM-005**: System users MUST support per-user "allow_remote" metadata flag to enable remote access
- **FR-SYSTEM-006**: System users with remote access enabled MUST have a password configured (cannot be null/empty)
- **FR-SYSTEM-007**: System MUST deny all access to system users with remote access enabled but no password
- **FR-SYSTEM-008**: System MUST detect localhost connections by checking for 127.0.0.1, ::1, Unix socket, or "localhost" hostname
- **FR-SYSTEM-009**: System MUST store connection source (remote IP address) for auditing purposes

#### CLI Integration

- **FR-CLI-001**: Database initialization MUST automatically create a default system user for CLI access
- **FR-CLI-002**: Default CLI system user MUST be configured with auth_type "internal" for localhost-only access
- **FR-CLI-003**: CLI tool MUST automatically authenticate using the default system user credentials
- **FR-CLI-004**: CLI tool MUST store system user credentials securely in configuration file
- **FR-CLI-005**: CLI tool MUST support connecting to multiple database instances with separate credentials per instance
- **FR-CLI-006**: CLI tool MUST provide commands to view and update system user credentials
- **FR-CLI-007**: CLI tool MUST handle authentication errors gracefully with clear user messages

#### OAuth Support

- **FR-OAUTH-001**: System MUST support auth_type "oauth" for OAuth-based authentication
- **FR-OAUTH-002**: OAuth users MUST store provider and subject in auth_data as JSON: {"provider": "...", "subject": "..."}
- **FR-OAUTH-003**: System MUST validate OAuth tokens with configured providers
- **FR-OAUTH-004**: OAuth users MUST NOT be able to authenticate with passwords
- **FR-OAUTH-005**: System MUST match OAuth token subject claim to stored auth_data subject to identify user

#### User Management

- **FR-USER-001**: System MUST store user records in system.users table with columns: user_id, username, email, auth_type, auth_data, role, storage_mode, storage_id, metadata, created_at, updated_at, deleted_at
- **FR-USER-002**: System MUST enforce unique user_id constraint
- **FR-USER-003**: System MUST enforce unique username constraint  
- **FR-USER-004**: System MUST auto-generate created_at timestamp on user creation
- **FR-USER-005**: System MUST auto-update updated_at timestamp on user modification
- **FR-USER-006**: System MUST validate auth_type as one of: "password", "oauth", "internal"
- **FR-USER-007**: System MUST validate role as one of: "user", "service", "dba", "system"
- **FR-USER-008**: System MUST support optional email and metadata fields
- **FR-USER-009**: System MUST validate metadata as valid JSON when provided
- **FR-USER-010**: Soft-deleted users (deleted_at IS NOT NULL) MUST be hidden from default queries
- **FR-USER-011**: Only dba and system roles MUST be able to query soft-deleted users

#### Testing Requirements

- **FR-TEST-001**: System MUST include integration tests for HTTP Basic Auth authentication flow
- **FR-TEST-002**: System MUST include integration tests for JWT token authentication flow
- **FR-TEST-003**: System MUST include integration tests for each role's access permissions
- **FR-TEST-004**: System MUST include integration tests for shared table access control at each access level
- **FR-TEST-005**: System MUST include integration tests for system user localhost vs remote access
- **FR-TEST-006**: System MUST include integration tests for CLI system user creation and authentication
- **FR-TEST-007**: System MUST include integration tests for password hashing and verification
- **FR-TEST-008**: System MUST include integration tests for OAuth authentication flow
- **FR-TEST-009**: Existing integration tests MUST be updated to authenticate properly with new authentication requirements
- **FR-TEST-010**: System MUST include integration tests for edge cases (malformed headers, missing credentials, etc.)

### Non-Functional Requirements

#### Performance
- **FR-PERF-001**: Authentication checks must complete within 10ms for 95th percentile requests
- **FR-PERF-002**: System must support at least 100 concurrent authenticated connections
- **FR-PERF-003**: Password hashing must use adaptive algorithms (bcrypt) with appropriate cost factors

#### Security
- **FR-SEC-001**: Passwords must never be logged or exposed in error messages
- **FR-SEC-002**: System MUST implement rate limiting on authentication attempts with dual strategy: 5 failed attempts per username per 5 minutes (account lockout) and 20 failed attempts per IP per 5 minutes (IP throttling)
- **FR-SEC-003**: System MUST enforce 5-minute lockout period with exponential backoff for repeated violations
- **FR-SEC-004**: System MUST exempt localhost connections and system users from rate limiting (CLI convenience)
- **FR-SEC-005**: System MUST reset failure counter upon successful authentication
- **FR-SEC-006**: JWT tokens must have configurable expiration times
- **FR-SEC-007**: System must validate JWT signatures and claims
- **FR-SEC-008**: Admin operations must be logged for audit purposes

#### Observability
- **FR-OBS-001**: System MUST maintain dedicated authentication log at logs/auth.log separate from application logs
- **FR-OBS-002**: System MUST log to logs/auth.log for all authentication attempts (failures, rate limit violations, lockouts) with: timestamp, username (if provided), source IP, failure reason (invalid credentials, missing authorization, expired token, rate limit exceeded, etc.)
- **FR-OBS-003**: System MUST log role changes with: timestamp, target user_id, old_role, new_role, admin_user_id (who made the change)
- **FR-OBS-004**: System MUST log admin operations with: timestamp, admin_user_id, operation (create_user, delete_user, update_user, change_role), target_user_id, operation_result (success/failure)
- **FR-OBS-005**: System SHOULD NOT log successful authentication attempts to logs/auth.log in production (minimize log volume) - only failures and security events
- **FR-OBS-006**: All security logs MUST include request_id for correlation with application logs
- **FR-OBS-007**: System MUST support log rotation for logs/auth.log with configurable size/time-based policies

### Key Entities

- **User**: Represents an authenticated entity (person, service account, or system process) with identity (user_id, username), authentication credentials (auth_type, auth_data), authorization role, and optional metadata. Each user has creation/update timestamps and soft-delete capability.

- **Authentication Credential**: Represents how a user proves their identity - either password hash (bcrypt), OAuth provider/subject pair, or internal system marker. Stored in auth_data column, format depends on auth_type.

- **User Role**: Represents authorization level (user, service, dba, system) that determines what operations and data access the user is permitted. Roles form a hierarchy with increasing privileges.

- **Shared Table Access Level**: Represents visibility and access permissions for shared tables (public = readable by all authenticated users, private/restricted = only service/dba/system). Controls data sharing across user boundaries.

- **System User**: Special user type for internal processes and CLI tools, configured for localhost-only access by default with optional remote access when explicitly enabled with password.

- **JWT Token**: Bearer token containing claims (sub for user_id, iss for issuer, exp for expiration) used for stateless authentication. Can be issued by KalamDB or external providers.

- **CLI Credentials**: Configuration data stored by CLI tool to automatically authenticate as system user, supporting multiple database instances.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Users can successfully authenticate using username and password via HTTP Basic Auth with response time under 100ms for 95% of requests

- **SC-002**: Users can successfully authenticate using JWT tokens with validation time under 50ms for 95% of requests

- **SC-003**: System correctly enforces role-based permissions with 100% accuracy across all four role types (user, service, dba, system)

- **SC-004**: Shared tables with "public" access are readable by all authenticated users while remaining protected from unauthorized modifications

- **SC-005**: System users can authenticate from localhost without passwords while remote access is blocked unless explicitly configured

- **SC-006**: CLI tool connects to newly initialized databases without requiring manual user setup or credential configuration

- **SC-007**: Password authentication attempts with incorrect credentials are rejected within 500ms (bcrypt verification time)

- **SC-008**: System handles 1000 concurrent authentication requests without degradation or errors

- **SC-009**: Authentication failures return clear error messages that don't reveal whether a username exists (prevent user enumeration)

- **SC-010**: All integration tests pass with 100% success rate, including authentication, authorization, shared table access, system users, and CLI integration

- **SC-011**: Existing integration tests are successfully updated to work with new authentication requirements without breaking existing functionality

- **SC-012**: Database initialization creates a working system user and configures CLI in under 5 seconds

- **SC-013**: OAuth-authenticated users can access the system using tokens from configured providers without requiring password management
