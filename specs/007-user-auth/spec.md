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

## Architectural Consistency Requirements

**CRITICAL**: All implementation MUST follow existing KalamDB patterns and architecture:

### 1. SQL Command Parsing (FR-ARCH-001)

User management SQL commands (CREATE USER, ALTER USER, DROP USER) **MUST**:
- Follow the same parser structure as existing DDL commands in `backend/crates/kalamdb-sql/src/parser/`
- Use `ExtensionStatement` enum pattern (see `extensions.rs`) for KalamDB-specific SQL
- Follow the `parse()` method pattern used in `CreateStorageStatement`, `FlushTableStatement`, etc.
- Add to `parser/mod.rs` exports like other custom commands
- Return `Result<Statement, String>` for error handling consistency

**Example Pattern** (from existing `extensions.rs`):
```rust
// In parser/extensions.rs
pub enum ExtensionStatement {
    CreateStorage(CreateStorageStatement),
    FlushTable(FlushTableStatement),
    CreateUser(CreateUserStatement),  // NEW - follow same pattern
    AlterUser(AlterUserStatement),    // NEW - follow same pattern
    DropUser(DropUserStatement),      // NEW - follow same pattern
}

// Each statement has parse() method
impl CreateUserStatement {
    pub fn parse(sql: &str) -> Result<Self, String> {
        // Follow same pattern as CreateStorageStatement
    }
}
```

### 2. Background Job Management (FR-ARCH-002)

User cleanup jobs **MUST** follow existing job infrastructure in `backend/crates/kalamdb-core/src/jobs/`:
- Use `JobManager` trait for job execution (see `job_manager.rs`)
- Implement job using `JobExecutor` pattern (see `executor.rs`)
- Follow `RetentionPolicy` pattern for cleanup logic (see `retention.rs`)
- Register jobs in `system.jobs` table via `JobsTableProvider`
- Use `TokioJobManager::start_job()` for async execution
- Follow job lifecycle: created → running → completed/failed/cancelled

**Example Pattern** (from existing `retention.rs`):
```rust
// User cleanup job should follow RetentionPolicy pattern
pub struct UserCleanupJob {
    users_provider: Arc<UsersTableProvider>,
    config: UserCleanupConfig,
}

impl UserCleanupJob {
    pub fn enforce(&self) -> Result<usize, KalamDbError> {
        // Follow same pattern as RetentionPolicy::enforce()
        // Delete users where deleted_at < (now - grace_period)
    }
}
```

### 3. Storage Layer Organization (FR-ARCH-003)

`system.users` storage **MUST** follow existing patterns in `backend/crates/kalamdb-store/src/`:
- Create dedicated module `users.rs` (similar to `user_table_store.rs`, `shared_table_store.rs`)
- Use RocksDB column family pattern: `system_users` (follow `ColumnFamilyNames` constants)
- Implement CRUD functions: `create_user()`, `get_user()`, `update_user()`, `delete_user()` (similar to table stores)
- Use `ensure_cf()` pattern for lazy column family creation
- Follow `create_column_family()` pattern from `user_table_store.rs`
- Return `Result<T, anyhow::Error>` for error handling

**Example Pattern** (from existing `user_table_store.rs`):
```rust
// In kalamdb-store/src/users.rs
pub struct UsersStore {
    db: Arc<DB>,
}

impl UsersStore {
    fn ensure_cf(&self) -> Result<&ColumnFamily> {
        // Follow pattern from user_table_store.rs
        let cf_name = "system_users";
        if self.db.cf_handle(cf_name).is_none() {
            self.create_column_family()?;
        }
        // ... return handle
    }

    pub fn create_user(&self, user: User) -> Result<()> {
        // Follow CRUD pattern from existing stores
    }
}
```

### 4. SQL Execution Flow (FR-ARCH-004)

User management command execution **MUST**:
- Add executor in `backend/crates/kalamdb-core/src/sql/` (similar to table executors)
- Integrate with existing `ExecutionContext` pattern
- Use authorization checks before execution (same pattern as CREATE TABLE, DROP TABLE)
- Return execution results via `system.jobs` table for async operations
- Follow error propagation pattern: `Result<ExecutionResult, KalamDbError>`

### 5. Data Model Consistency (FR-ARCH-005)

`system.users` table **MUST**:
- Follow same schema patterns as `system.tables`, `system.storages`, `system.jobs`
- Use consistent column naming: `created_at`, `updated_at`, `deleted_at` (timestamps in milliseconds)
- Store JSON metadata in `metadata` column (same as `system.tables.metadata`)
- Use enum serialization patterns from `kalamdb-commons` (lowercase strings)
- Follow soft delete pattern: `deleted_at IS NULL` for active records

### 6. Error Handling (FR-ARCH-006)

Authentication/authorization errors **MUST**:
- Use `KalamDbError` enum from `backend/crates/kalamdb-core/src/error.rs`
- Add new error variants: `AuthenticationFailed`, `AuthorizationFailed`, `InvalidCredentials`
- Follow error response pattern from existing API handlers
- Return structured JSON errors with `error`, `message`, `request_id` fields
- Use `anyhow::Context` for error context propagation

**Validation**: All new code must be reviewed against existing patterns before PR submission.

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

### User Story 9 - System Model Consolidation (Priority: P0 - CRITICAL PREREQUISITE)

**CRITICAL ARCHITECTURAL CONSOLIDATION**: Before implementing any authentication logic, we MUST consolidate all duplicate system table models into a single source of truth in `kalamdb-commons`. This is a prerequisite for all other work.

**Problem**: We currently have MULTIPLE definitions of the same system table models scattered across different crates:
- `User` model exists in 2+ places
- `Job` model exists in 3 places (kalamdb-core, kalamdb-sql, jobs_provider)
- `LiveQuery` model exists in 3 places
- `Namespace` model exists in 3 places
- This creates maintenance nightmares, serialization issues, and type conversion overhead

**Solution**: All system models MUST be defined ONCE in `kalamdb-commons/src/models/system.rs` and imported everywhere else.

**Why this priority**: This is P0 (CRITICAL PREREQUISITE) because:
1. User authentication depends on `User` model - must use canonical version
2. Every crate importing duplicate models needs to be updated
3. Serialization/deserialization must work consistently across all layers
4. Doing this AFTER implementing auth would require rewriting all auth code
5. This fixes architectural debt that blocks clean development

**Independent Test**: Can be fully tested by:
1. Verifying `kalamdb-commons/src/models/system.rs` contains all models
2. Confirming NO duplicate model definitions exist in `kalamdb-sql/src/models.rs`
3. Ensuring all imports use `kalamdb_commons::system::*`
4. Running existing tests to verify serialization compatibility
5. Checking `cargo build` succeeds after cleanup

**Acceptance Scenarios**:

1. **Given** the file `backend/crates/kalamdb-sql/src/models.rs` exists, **When** it is examined, **Then** it contains NO duplicate definitions of `User`, `Job`, `LiveQuery`, `Namespace`, or `SystemTable` (these must be deleted and replaced with re-exports from kalamdb-commons).

2. **Given** the file `kalamdb-commons/src/models/system.rs` exists, **When** it is examined, **Then** it contains canonical definitions for: `User`, `Job`, `LiveQuery`, `Namespace`, `SystemTable`, `Storage`, `TableSchema`, `InformationSchemaTable`, and `UserTableCounter`.

3. **Given** the file `kalamdb-core/src/tables/system/jobs_provider.rs` exists, **When** it is examined, **Then** it uses `use kalamdb_commons::system::Job;` and does NOT define `JobRecord` locally.

4. **Given** the file `kalamdb-core/src/tables/system/live_queries_provider.rs` exists, **When** it is examined, **Then** it uses `use kalamdb_commons::system::LiveQuery;` and does NOT define `LiveQueryRecord` locally.

5. **Given** any file in `backend/crates/` needs to work with system tables, **When** it imports models, **Then** it uses `use kalamdb_commons::system::{User, Job, ...};` (not from kalamdb-sql or local definitions).

6. **Given** the authentication code needs to store user credentials, **When** it creates a `User` entity, **Then** it uses `kalamdb_commons::system::User` with strongly-typed fields (`UserId`, `Role`, `AuthType` enums).

7. **Given** existing tests rely on system models, **When** they run after consolidation, **Then** all tests pass without serialization/deserialization errors.

8. **Given** the file `kalamdb-sql/src/models.rs` exists, **When** examined, **Then** it ONLY contains models NOT in commons: currently it should either be deleted entirely OR only re-export from commons.

9. **Given** the codebase is searched for model definitions, **When** looking for `pub struct User`, **Then** it appears ONLY in `kalamdb-commons/src/models/system.rs` (nowhere else).

10. **Given** the two services using `TableSchema` exist (`stream_table_service.rs`, `shared_table_service.rs`), **When** examined, **Then** they import `TableSchema` from `kalamdb_commons::system::TableSchema` (not from kalamdb-sql).

11. **Given** the catalog models exist in `kalamdb-core/src/catalog/` (Namespace, TableMetadata), **When** examined, **Then** they are confirmed to be DIFFERENT from system table models - catalog models are runtime entities with domain logic, system models are simple DTOs for persistence.

12. **Given** the `Storage` system table model is needed, **When** code references it, **Then** it imports from `kalamdb_commons::system::Storage` (already available in commons).

**Current State (as of 2025-10-27)**:
- ✅ `kalamdb-commons/src/models/system.rs` created with all canonical models (User, Job, LiveQuery, Namespace, SystemTable, Storage, TableSchema, InformationSchemaTable, UserTableCounter)
- ✅ `kalamdb-core/src/models/system.rs` deleted
- ✅ `kalamdb-core/src/models/mod.rs` updated to re-export from commons
- ✅ `kalamdb-sql/src/models.rs` DELETED - lib.rs now re-exports from commons
- ✅ `kalamdb-sql/src/adapter.rs` updated to import from commons
- ✅ `stream_table_service.rs` updated to import `TableSchema` from commons
- ✅ `shared_table_service.rs` updated to import `TableSchema` from commons
- ✅ `jobs_provider.rs` syntax error fixed (missing closing brace)
- ✅ Catalog models (Namespace, TableMetadata) confirmed as separate from system models
- ❌ `users_provider.rs` has field mismatches (storage_mode, storage_id don't exist; role strings should be Role enum)
- ❌ `live_queries_provider.rs` still has `LiveQueryRecord` (needs migration)

**Migration Checklist**:
- [x] Verify `kalamdb-sql/src/models.rs` is completely removed
- [x] Update `kalamdb-sql/src/lib.rs` to re-export from commons
- [x] Update `kalamdb-sql/src/adapter.rs` imports
- [x] Update `stream_table_service.rs` to import from commons
- [x] Update `shared_table_service.rs` to import from commons
- [x] Fix `jobs_provider.rs` syntax error
- [ ] Fix `users_provider.rs` field mismatches and Role enum usage
- [ ] Complete `live_queries_provider.rs` migration (remove `LiveQueryRecord`, use `LiveQuery`)
- [ ] Update `stream_table_service.rs` to import `TableSchema` from commons
- [ ] Update `shared_table_service.rs` to import `TableSchema` from commons
- [ ] Search for any remaining `use kalamdb_sql::models::` imports and replace
- [ ] Run `cargo build` in backend/
- [ ] Run `cargo test` to verify no regressions
- [ ] Update `.github/copilot-instructions.md` to document single source of truth

---

### User Story 10 - Storage Backend Abstraction & Store Consolidation (Priority: P0 - CRITICAL)

**CRITICAL ARCHITECTURAL REFACTORING**: This is the second foundational change that MUST be completed BEFORE implementing authentication. All existing storage code must be refactored to use a two-layer abstraction pattern, and all stores must be consolidated into `kalamdb-core/src/stores/` with strongly-typed entity models.

The authentication system must store user credentials and metadata in a way that doesn't tightly couple to RocksDB. All database interactions should go through the `kalamdb-store` crate abstraction layer, enabling future migration to other storage backends (Sled, TiKV, FoundationDB, etc.) without rewriting authentication logic.

**Why this priority**: This is P0 (CRITICAL) because it affects the entire codebase architecture. All existing table stores (UserTableStore, SharedTableStore, StreamTableStore) must be migrated to the new pattern. Authentication code depends on this foundation. Doing this refactoring after authentication would require rewriting authentication code.

**Independent Test**: Can be fully tested by verifying that `kalamdb-core`, `kalamdb-sql`, and `backend` crates never import `rocksdb` directly and only use `kalamdb-store` traits/types. Mock storage backend can be implemented to prove abstraction works. Delivers value by making storage backend a pluggable implementation detail and establishing proper architectural layering.

**Two-Layer Architecture**:

**Layer 1: Storage Backend Abstraction** (`kalamdb-store/src/backend.rs`)
```rust
/// Low-level key-value storage backend (RocksDB, Sled, TiKV, etc.)
pub trait StorageBackend: Send + Sync {
    fn put(&self, partition: &str, key: &[u8], value: &[u8]) -> Result<()>;
    fn get(&self, partition: &str, key: &[u8]) -> Result<Option<Vec<u8>>>;
    fn delete(&self, partition: &str, key: &[u8]) -> Result<()>;
    fn scan_prefix(&self, partition: &str, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;
    fn scan_range(&self, partition: &str, start: &[u8], end: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;
    fn delete_batch(&self, partition: &str, keys: &[Vec<u8>]) -> Result<()>;
}

/// RocksDB implementation
pub struct RocksDbBackend {
    db: Arc<rocksdb::DB>,
}
```

**Layer 2: Entity Store Trait** (`kalamdb-store/src/traits.rs`)
```rust
/// Generic CRUD operations for strongly-typed entities
/// Each store implements this for their specific entity type
pub trait EntityStore<T>
where
    T: Serialize + for<'de> Deserialize<'de>,
{
    fn backend(&self) -> &Arc<dyn StorageBackend>;
    fn partition(&self) -> &str;
    
    // Default JSON serialization (override for bincode/protobuf/etc.)
    fn serialize(&self, entity: &T) -> Result<Vec<u8>> {
        serde_json::to_vec(entity).map_err(Into::into)
    }
    
    fn deserialize(&self, bytes: &[u8]) -> Result<T> {
        serde_json::from_slice(bytes).map_err(Into::into)
    }
    
    // Standard CRUD operations
    fn put(&self, key: &str, entity: &T) -> Result<()>;
    fn get(&self, key: &str) -> Result<Option<T>>;
    fn delete(&self, key: &str) -> Result<()>;
    fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, T)>>;
}
```

**Store Consolidation Plan**:

All stores MUST be moved to `kalamdb-core/src/stores/` with strongly-typed entity models:

**Before (Current State)**:
```
kalamdb-store/
├── user_table_store.rs      # Uses JsonValue
├── shared_table_store.rs    # Uses JsonValue
└── stream_table_store.rs    # Uses JsonValue

kalamdb-core/
└── (no stores, uses kalamdb-store directly)
```

**After (Target State)**:
```
kalamdb-store/
├── src/
│   ├── backend.rs           # StorageBackend trait + RocksDbBackend
│   ├── traits.rs            # EntityStore<T> trait
│   └── lib.rs

kalamdb-core/
├── src/
│   ├── models/
│   │   ├── mod.rs
│   │   ├── system.rs        # User, Job, Namespace structs (all system tables together)
│   │   └── tables.rs        # UserTableRow, SharedTableRow, StreamTableRow structs
│   ├── stores/
│   │   ├── mod.rs
│   │   ├── user_store.rs        # impl EntityStore<User>
│   │   ├── job_store.rs         # impl EntityStore<Job>
│   │   ├── namespace_store.rs   # impl EntityStore<Namespace>
│   │   ├── user_table_store.rs  # impl EntityStore<UserTableRow>
│   │   ├── shared_table_store.rs # impl EntityStore<SharedTableRow>
│   │   └── stream_table_store.rs # impl EntityStore<StreamTableRow>
```

**Entity Type Examples**:

```rust
// kalamdb-core/src/models/system.rs - All system table models together
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub role: UserRole,
    pub created_at: String,
    // ... other fields
}

// kalamdb-core/src/models/user_table_row.rs
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UserTableRow {
    #[serde(flatten)]
    pub fields: serde_json::Map<String, serde_json::Value>,  // Dynamic user data
    pub _updated: String,   // System column
    pub _deleted: bool,     // System column
}

// kalamdb-core/src/stores/user_store.rs
pub struct UserStore {
    backend: Arc<dyn StorageBackend>,  // ← Generic backend, no RocksDB!
}

impl EntityStore<User> for UserStore {
    fn backend(&self) -> &Arc<dyn StorageBackend> { &self.backend }
    fn partition(&self) -> &str { "system_users" }
    
    // Override for bincode (faster than JSON)
    fn serialize(&self, entity: &User) -> Result<Vec<u8>> {
        bincode::serialize(entity).map_err(Into::into)
    }
    fn deserialize(&self, bytes: &[u8]) -> Result<User> {
        bincode::deserialize(bytes).map_err(Into::into)
    }
}

impl UserStore {
    // Custom domain methods
    pub fn get_by_username(&self, username: &str) -> Result<Option<User>> { ... }
    pub fn list_all(&self) -> Result<Vec<User>> { ... }
}
```

**Acceptance Scenarios**:

1. **Given** the authentication system needs to store user credentials, **When** it writes to storage, **Then** it uses `UserStore` methods which internally use `StorageBackend` trait (never `rocksdb::DB` directly).

2. **Given** the `system.users` table needs to be queried, **When** the SQL layer executes the query, **Then** it uses `UserStore` from `kalamdb-core/src/stores/` (not RocksDB-specific APIs).

3. **Given** a developer wants to implement a new storage backend (e.g., Sled), **When** they implement the `StorageBackend` trait in `kalamdb-store`, **Then** all stores work without code changes (only backend initialization changes).

4. **Given** `kalamdb-core` needs database access, **When** it performs operations, **Then** it receives `Arc<dyn StorageBackend>` from `kalamdb-store` (not `Arc<rocksdb::DB>`).

5. **Given** the codebase is analyzed for dependencies, **When** checking `kalamdb-core/Cargo.toml`, `kalamdb-sql/Cargo.toml`, and `backend/Cargo.toml`, **Then** these files do NOT list `rocksdb` as a dependency (only `kalamdb-store/Cargo.toml` has it).

6. **Given** RocksDB-specific operations are needed (column families, compaction, snapshots), **When** the feature is implemented, **Then** it is encapsulated inside `kalamdb-store/src/backend.rs` with generic methods exposed via `StorageBackend` trait.

7. **Given** the authentication middleware needs to verify credentials, **When** it queries the users table, **Then** it calls `UserStore::get()` methods (which implement `EntityStore<User>` trait).

8. **Given** system initialization creates default users and tables, **When** this happens during startup, **Then** all stores are created with `Arc<dyn StorageBackend>` and all writes go through `EntityStore<T>` trait methods.

9. **Given** existing user table operations need to continue working, **When** migrated to `UserTableStore` in `kalamdb-core/src/stores/`, **Then** it implements `EntityStore<UserTableRow>` with system column injection in `serialize()` method.

10. **Given** all stores are in `kalamdb-core/src/stores/`, **When** business logic needs database access, **Then** it imports stores from `kalamdb_core::stores::*` (not from `kalamdb_store`).

**Current State Assessment**:
- ✅ `kalamdb-store` crate exists with basic infrastructure
- ❌ `UserTableStore`, `SharedTableStore`, `StreamTableStore` are in `kalamdb-store` (should be in `kalamdb-core`)
- ❌ These stores use `Arc<DB>` directly instead of `Arc<dyn StorageBackend>`
- ❌ `EntityStore<T>` trait doesn't exist yet
- ❌ `StorageBackend` trait doesn't exist yet
- ❌ `kalamdb-core` imports `rocksdb` directly (see `storage/rocksdb_store.rs`, `storage/rocksdb_init.rs`, `storage/rocksdb_config.rs`)
- ❌ `kalamdb-sql` imports `rocksdb::DB` (see `adapter.rs`, `lib.rs`)
- ❌ `backend` passes `Arc<rocksdb::DB>` to other crates

**Refactoring Scope (MUST BE DONE FIRST)**:
1. **Create infrastructure in kalamdb-store**:
   - Create `backend.rs` with `StorageBackend` trait and `RocksDbBackend` implementation
   - Create `traits.rs` with `EntityStore<T>` trait
   
2. **Create domain models in kalamdb-core**:
   - Create `models/` directory with entity types (User, Job, Namespace, UserTableRow, etc.)
   
3. **Migrate existing stores to kalamdb-core**:
   - Move `UserTableStore` from `kalamdb-store/src/user_table_store.rs` to `kalamdb-core/src/stores/user_table_store.rs`
   - Move `SharedTableStore` to `kalamdb-core/src/stores/shared_table_store.rs`
   - Move `StreamTableStore` to `kalamdb-core/src/stores/stream_table_store.rs`
   - Refactor each to use `Arc<dyn StorageBackend>` instead of `Arc<DB>`
   - Implement `EntityStore<T>` for each with appropriate entity types
   
4. **Create new system stores in kalamdb-core**:
   - Create `stores/user_store.rs` implementing `EntityStore<User>` for `system.users`
   - Create `stores/job_store.rs` implementing `EntityStore<Job>` for `system.jobs`
   - Create `stores/namespace_store.rs` implementing `EntityStore<Namespace>` for `system.namespaces`
   
5. **Refactor kalamdb-core to remove RocksDB**:
   - Delete `storage/rocksdb_store.rs`, `storage/rocksdb_init.rs`, `storage/rocksdb_config.rs`
   - Replace `Arc<rocksdb::DB>` with `Arc<dyn StorageBackend>` throughout
   - Update all code to use stores from `kalamdb_core::stores::*`
   
6. **Refactor kalamdb-sql**:
   - Update `RocksDbAdapter` to wrap `Arc<dyn StorageBackend>` instead of `Arc<rocksdb::DB>`
   - Remove `use rocksdb::*` imports
   
7. **Refactor backend initialization**:
   - Backend creates `RocksDbBackend` and wraps in `Arc<dyn StorageBackend>`
   - Pass backend to all stores via constructors
   - Remove direct RocksDB passing
   
8. **Update Cargo.toml files**:
   - Remove `rocksdb` from `kalamdb-core/Cargo.toml`
   - Remove `rocksdb` from `kalamdb-sql/Cargo.toml`
   - Remove `rocksdb` from `backend/Cargo.toml` (only in `kalamdb-store/Cargo.toml`)

---

### Edge Cases

- **Empty or Missing Credentials**: What happens when a user sends a request with no Authorization header or empty credentials? System must return 401 Unauthorized with clear error message.

- **Malformed Authorization Header**: What happens when the Authorization header has invalid format (not "Basic ..." or "Bearer ...")? System must reject with 400 Bad Request.

- **Concurrent Authentication Requests**: How does the system handle multiple simultaneous authentication attempts for the same user? Must handle concurrently without race conditions or deadlocks.

- **Password Hash Collision**: What happens if by chance two different passwords produce the same hash? Bcrypt's design makes this astronomically unlikely, but system should handle gracefully.

- **System User Remote Access Misconfiguration**: What happens if remote access is enabled for a system user but no password is set? System must deny all access until password is configured.

- **JWT Token Without 'sub' Claim**: What happens when a JWT token is valid but missing the required 'sub' (subject) claim? System must reject with clear error about missing user identity.

- **Deleted User Authentication**: What happens when a soft-deleted user attempts to authenticate? System must deny access and treat as non-existent user.

---

## Integration Test Requirements

### Required Integration Tests (60+ tests total)

All integration tests must be written **BEFORE** implementation (TDD approach) and organized by feature area:

#### 1. Authentication Tests (backend/tests/test_basic_auth.rs, test_jwt_auth.rs)

**HTTP Basic Auth**:
- test_basic_auth_success - Valid username/password authenticates successfully
- test_basic_auth_invalid_credentials - Wrong password returns 401
- test_basic_auth_missing_header - No Authorization header returns 401
- test_basic_auth_malformed_header - Invalid header format returns 400
- test_basic_auth_deleted_user - Soft-deleted user authentication denied

**JWT Token Auth**:
- test_jwt_auth_success - Valid JWT authenticates successfully
- test_jwt_auth_expired_token - Expired token returns 401 TOKEN_EXPIRED
- test_jwt_auth_invalid_signature - Invalid signature returns 401
- test_jwt_auth_untrusted_issuer - Untrusted issuer returns 401
- test_jwt_auth_missing_sub_claim - Token without 'sub' claim returns 401
- test_jwt_auth_user_not_exists - JWT with unknown user_id (if verify_user_exists=true)

#### 2. Authorization Tests (backend/tests/test_rbac.rs)

**User Role Permissions**:
- test_user_role_own_tables_access - User can SELECT/INSERT/UPDATE/DELETE own tables
- test_user_role_cannot_access_others - User cannot access other users' tables
- test_user_role_cannot_create_namespace - User role cannot create namespaces
- test_user_role_cannot_manage_users - User role cannot create/update/delete users
- test_user_role_public_shared_table_read - User can SELECT from public shared tables
- test_user_role_private_shared_table_denied - User cannot access private shared tables

**Service Role Permissions**:
- test_service_role_cross_user_access - Service can access any user table
- test_service_role_flush_operations - Service can execute FLUSH commands
- test_service_role_read_system_tables - Service can read system.jobs, system.live_queries
- test_service_role_cannot_manage_users - Service cannot create/update/delete users

**DBA Role Permissions**:
- test_dba_role_create_tables - DBA can CREATE/DROP tables
- test_dba_role_manage_users - DBA can CREATE/ALTER/DROP users
- test_dba_role_all_access - DBA has full access to all tables
- test_dba_role_restore_deleted_users - DBA can restore soft-deleted users

**System Role Permissions**:
- test_system_role_all_access - System role has unrestricted access
- test_system_role_localhost_no_password - System user from localhost needs no password
- test_system_role_remote_access_blocked - System user remote access blocked by default
- test_system_role_remote_with_password - System user remote access works with password

#### 3. Shared Table Access Control Tests (backend/tests/test_shared_access.rs)

- test_public_table_read_only_for_users - User role can only SELECT from public tables
- test_private_table_service_dba_only - Only service/dba/system can access private tables
- test_shared_table_defaults_to_private - Shared table without access level defaults to private
- test_change_access_level_requires_privileges - Only dba/system can ALTER TABLE SET ACCESS
- test_user_cannot_modify_public_table - User cannot INSERT/UPDATE/DELETE public tables

#### 4. SQL User Management Tests (backend/tests/test_user_sql.rs)

**CREATE USER**:
- test_create_user_with_password - CREATE USER with password auth
- test_create_user_with_oauth - CREATE USER with OAuth provider
- test_create_user_with_internal - CREATE USER with internal auth (system user)
- test_create_user_duplicate_error - Duplicate username returns error
- test_create_user_weak_password - Weak password rejected (too short, common)
- test_create_user_system_remote_requires_password - System user with allow_remote needs password

**ALTER USER**:
- test_alter_user_set_password - ALTER USER SET PASSWORD updates hash
- test_alter_user_set_role - ALTER USER SET ROLE (dba only)
- test_alter_user_set_email - ALTER USER SET EMAIL updates metadata
- test_alter_user_not_found - ALTER non-existent user returns error
- test_alter_user_self_password - User can change own password
- test_alter_user_self_role_denied - User cannot change own role

**DROP USER**:
- test_drop_user_soft_delete - DROP USER sets deleted_at
- test_drop_user_if_exists - DROP USER IF EXISTS handles non-existent
- test_restore_deleted_user - UPDATE deleted_at = NULL restores user
- test_select_users_excludes_deleted - Default SELECT hides deleted users
- test_select_deleted_users_explicit - WHERE deleted_at IS NOT NULL shows deleted

#### 5. Password Security Tests (backend/tests/test_password_security.rs)

- test_password_never_plaintext - Password stored as bcrypt hash, never plaintext
- test_concurrent_bcrypt_non_blocking - Concurrent authentication doesn't block
- test_weak_password_rejected - Password < 8 chars rejected
- test_common_password_rejected - Top 10,000 common passwords blocked
- test_max_password_length - Password > 1024 chars rejected
- test_password_complexity_enforced - Uppercase/lowercase/digit/special char required (if configured)

#### 6. OAuth Integration Tests (backend/tests/test_oauth.rs)

- test_oauth_google_success - OAuth user authenticates with Google token
- test_oauth_user_password_rejected - OAuth user cannot use password auth
- test_oauth_subject_matching - OAuth token subject matches auth_data
- test_oauth_auto_provision_disabled - Auto-provisioning disabled by default

#### 7. System User Tests (backend/tests/test_system_user.rs)

- test_system_user_localhost_no_password - Localhost access without password
- test_system_user_remote_blocked_default - Remote access blocked by default
- test_system_user_remote_with_password - Remote access enabled with password
- test_system_user_remote_without_password_rejected - Remote without password rejected

#### 8. Cleanup Job Tests (backend/tests/test_user_cleanup.rs)

- test_cleanup_deletes_expired_users - Cleanup job deletes users after grace period
- test_cleanup_cascade_deletes_tables - User deletion cascades to user tables
- test_cleanup_job_logging - Cleanup job logs to system.jobs

#### 9. Edge Case Tests (backend/tests/test_edge_cases.rs)

- test_empty_credentials_401 - Empty Authorization header returns 401
- test_concurrent_auth_no_race_conditions - Concurrent auth requests handle correctly
- test_role_change_during_session - Role change applies to next request
- test_jwt_token_claim_caching - JWT claims cached for performance

#### 10. End-to-End Flow Test (backend/tests/test_e2e_auth_flow.rs)

- test_complete_user_lifecycle - Create user → authenticate → query → soft delete → restore

#### 11. Storage Abstraction Tests (backend/tests/test_storage_abstraction.rs, kalamdb-store/src/tests/mod.rs)

**Storage Backend Abstraction** (backend/tests/test_storage_abstraction.rs):
- test_auth_uses_storage_backend_only - Authentication never imports rocksdb directly
- test_mock_storage_backend_authentication - Mock StorageBackend implementation can authenticate users
- test_storage_backend_swap_no_code_changes - Swap RocksDB→Mock without changing kalamdb-core/backend code

**kalamdb-store Encapsulation** (kalamdb-store/src/tests/mod.rs):
- test_users_store_crud_operations - UsersStore (or SystemStore) provides create/read/update/delete for users
- test_partition_abstraction - Column families accessed via Partition abstraction
- test_rocksdb_specific_features_hidden - RocksDB types (DB, ColumnFamily) never exposed in public API

**Dependency Analysis** (compile-time check via CI):
- test_kalamdb_core_no_rocksdb_dependency - `kalamdb-core/Cargo.toml` does NOT have `rocksdb` dependency
- test_kalamdb_sql_no_rocksdb_dependency - `kalamdb-sql/Cargo.toml` does NOT have `rocksdb` dependency
- test_backend_no_rocksdb_dependency - `backend/Cargo.toml` does NOT have `rocksdb` dependency (only `kalamdb-store`)

### Test Organization

```
backend/tests/
├── test_basic_auth.rs          # 5 tests - HTTP Basic Auth
├── test_jwt_auth.rs            # 6 tests - JWT authentication
├── test_rbac.rs                # 14 tests - Role-based access control
├── test_shared_access.rs       # 5 tests - Shared table access levels
├── test_user_sql.rs            # 16 tests - SQL user management commands
├── test_password_security.rs   # 6 tests - Password hashing & validation
├── test_oauth.rs               # 4 tests - OAuth integration
├── test_system_user.rs         # 4 tests - System user security
├── test_user_cleanup.rs        # 3 tests - Scheduled cleanup job
├── test_edge_cases.rs          # 4 tests - Edge cases
├── test_e2e_auth_flow.rs       # 1 test - End-to-end lifecycle
├── test_storage_abstraction.rs # 3 tests - Storage backend abstraction
└── common/
    └── auth_helper.rs          # Test utilities (create_test_user, authenticate_basic, etc.)

kalamdb-store/src/tests/
└── mod.rs                      # 3 tests - kalamdb-store encapsulation
```

**Total**: 74 integration tests + 16 unit tests = **90 comprehensive tests**

---

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

#### Storage Backend Abstraction

- **FR-STORAGE-001**: ONLY the `kalamdb-store` crate MUST have `rocksdb` as a direct dependency
- **FR-STORAGE-002**: All crates (`kalamdb-core`, `kalamdb-sql`, `backend`) MUST access storage through `kalamdb-store` abstractions only
- **FR-STORAGE-003**: System MUST NOT expose `rocksdb::DB` or other RocksDB-specific types outside `kalamdb-store` crate
- **FR-STORAGE-004**: All storage operations MUST use `kalamdb-store::StorageBackend` trait interface
- **FR-STORAGE-005**: System users table MUST be accessed via `kalamdb-store::SystemStore` (or equivalent abstraction, not direct RocksDB calls)
- **FR-STORAGE-006**: `kalamdb-sql::RocksDbAdapter` MUST be refactored to wrap `StorageBackend` trait instead of `Arc<rocksdb::DB>`
- **FR-STORAGE-007**: `kalamdb-core` storage modules (`storage/rocksdb_store.rs`, `storage/rocksdb_init.rs`, `storage/rocksdb_config.rs`) MUST be moved to `kalamdb-store` crate
- **FR-STORAGE-008**: All constructors accepting database handles MUST accept `Arc<dyn StorageBackend>` instead of `Arc<rocksdb::DB>`
- **FR-STORAGE-009**: RocksDB-specific features (column families, compaction, snapshots) MUST be encapsulated in `kalamdb-store` with generic interfaces
- **FR-STORAGE-010**: System MUST support pluggable storage backends - switching from RocksDB to another backend MUST only require changes in `kalamdb-store` crate and configuration
- **FR-STORAGE-011**: Authentication middleware MUST query user credentials via `kalamdb-store::UsersStore` (or `SystemStore`) methods, not direct RocksDB APIs
- **FR-STORAGE-012**: Partition abstraction (column families in RocksDB, trees in Sled) MUST be mapped through `kalamdb-store::Partition` type

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
- **FR-TEST-011**: System MUST include integration test proving storage backend abstraction works (mock backend implementation authenticates successfully)

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

- **Storage Backend**: Abstraction layer (defined in `kalamdb-store::StorageBackend` trait) that encapsulates all database operations. Allows KalamDB to switch between different key-value stores (RocksDB, Sled, TiKV, FoundationDB) without changing business logic in `kalamdb-core`, `kalamdb-sql`, or `backend` crates. Provides methods for get/put/delete, batch operations, range scans, and partition management.

- **Partition**: Generic abstraction for data organization within a storage backend. Maps to column families in RocksDB, trees in Sled, key prefixes in Redis, or namespace buckets in other stores. Used to isolate different table types and system metadata.

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

- **SC-010**: All integration tests pass with 100% success rate, including authentication, authorization, shared table access, system users, CLI integration, and storage abstraction

- **SC-011**: Existing integration tests are successfully updated to work with new authentication requirements without breaking existing functionality

- **SC-012**: Database initialization creates a working system user and configures CLI in under 5 seconds

- **SC-013**: OAuth-authenticated users can access the system using tokens from configured providers without requiring password management

- **SC-014**: Storage backend abstraction is complete - `kalamdb-core`, `kalamdb-sql`, and `backend` crates have ZERO direct imports of `rocksdb` types

- **SC-015**: Mock storage backend implementation can successfully authenticate users, proving storage abstraction is properly decoupled

- **SC-016**: All user credential operations (create, read, update, delete, authenticate) go through `kalamdb-store` abstractions only

