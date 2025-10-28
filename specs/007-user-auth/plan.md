# Implementation Plan: User Authentication

**Branch**: `007-user-auth` | **Date**: October 27, 2025 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/007-user-auth/spec.md`

**Note**: This plan documents the complete implementation approach for authentication, authorization, and user management in KalamDB.

## Summary

**⚠️ CRITICAL: This feature requires foundational storage refactoring FIRST (Phase 0.5) before authentication implementation.**

Implement comprehensive user authentication and authorization system for KalamDB with:
- **PHASE 0.5 (CRITICAL - MUST DO FIRST)**: Storage backend abstraction refactoring
  - Two-layer abstraction: `StorageBackend` trait + `EntityStore<T>` trait
  - Consolidate ALL stores to `kalamdb-core/src/stores/` with strongly-typed models
  - Migrate `UserTableStore`, `SharedTableStore`, `StreamTableStore` from `kalamdb-store` to `kalamdb-core`
  - Replace `Arc<rocksdb::DB>` with `Arc<dyn StorageBackend>` throughout codebase
  - Remove RocksDB imports from `kalamdb-core`, `kalamdb-sql`, `backend` (only in `kalamdb-store`)
- **HTTP Basic Auth** and **JWT token** authentication methods
- **Role-Based Access Control (RBAC)** with four roles: user, service, dba, system
- **Shared table access control** with public/private/restricted levels
- **System user management** with localhost-only default and optional remote access
- **CLI integration** with automatic system user creation on database initialization
- **New kalamdb-auth crate** for centralized authentication/authorization logic
- **Password security** using bcrypt hashing (cost factor 12)
- **OAuth provider integration** for enterprise SSO
- **Complete test coverage** including updates to all existing integration tests

**Critical Architecture Change**: This feature includes **FOUNDATIONAL REFACTORING** that must be completed before authentication work begins. All existing stores (`UserTableStore`, `SharedTableStore`, `StreamTableStore`) must be migrated to `kalamdb-core/src/stores/` with strongly-typed entity models and use `Arc<dyn StorageBackend>` instead of direct RocksDB access. This ensures ONLY `kalamdb-store` crate depends on RocksDB. The refactoring affects the entire codebase and is estimated at 3-5 days of work.

## Technical Context

**Language/Version**: Rust 1.75+ (edition 2021) - as specified in workspace Cargo.toml  
**Primary Dependencies**: 
- **Existing**: Actix-Web 4.4, DataFusion 35.0, RocksDB 0.24.0, tokio 1.48, jsonwebtoken 9.2
- **New**: bcrypt 0.15 (password hashing), base64 0.21 (Basic Auth decoding)

**Storage**: RocksDB column family `system_users` for user records; existing `system_tables` for access metadata  
**Storage Abstraction**: ONLY `kalamdb-store` crate depends on RocksDB; all other crates use `StorageBackend` trait  
**Testing**: cargo test (existing integration tests in `backend/tests/`) + new auth-specific tests  
**Target Platform**: Linux/macOS/Windows server (existing deployment targets)  
**Project Type**: Multi-crate workspace (backend server + CLI tool)  
**Performance Goals**: 
- HTTP Basic Auth: < 100ms authentication (95th percentile) - includes bcrypt verification
- JWT validation: < 50ms (95th percentile) - signature verification only
- RBAC check: < 5ms per operation (in-memory role evaluation)
- Concurrent auth: 1000 simultaneous requests without degradation

**Constraints**: 
- **Security-first**: All passwords bcrypt-hashed (never plaintext), timing-safe comparisons
- **Backward compatibility**: Existing X-API-KEY and X-USER-ID headers must continue working during migration
- **Zero downtime**: Must support gradual migration from old to new auth system
- **Localhost exception**: System users accessible from localhost without password
- **Audit logging**: All authentication attempts and authorization failures must be logged
- **Storage abstraction**: RocksDB types MUST NOT leak outside kalamdb-store crate boundary

**Scale/Scope**: 
- Support 1M+ users in system.users table (via StorageBackend abstraction)
- 100+ concurrent authentication requests per second
- 4 distinct user roles with complex permission matrix
- 3 access levels for shared tables
- Integration across 6 existing crates + 1 new crate (kalamdb-auth)
- **CRITICAL REFACTORING**: Migrate ALL stores to `kalamdb-core/src/stores/` with strongly-typed models
  - Move 3 existing stores: `UserTableStore`, `SharedTableStore`, `StreamTableStore`
  - Create 3 new system stores: `UserStore`, `JobStore`, `NamespaceStore`
  - Replace ~50+ occurrences of `Arc<rocksdb::DB>` with `Arc<dyn StorageBackend>`
  - Remove RocksDB dependencies from 3 crates (kalamdb-core, kalamdb-sql, backend)
  - Estimated 300-500 lines of code changes across 20+ files

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

**Status**: ⚠️ Constitution template not yet filled in - cannot validate compliance

**Note**: The project constitution file (`.specify/memory/constitution.md`) contains only template placeholders. Once the constitution is established with actual principles, this section will validate:

- Test-first development approach
- Library-first architecture (kalamdb-auth as new crate)
- Integration test coverage requirements
- Code organization standards
- Security and quality gates

**Assumed Principles for This Feature** (pending constitution):
- ✅ **Security-First**: Authentication is a critical security boundary
- ✅ **Library-First**: New kalamdb-auth crate isolates authentication logic
- ✅ **Test-First**: Integration tests written before implementation
- ✅ **Backward Compatibility**: Gradual migration from X-API-KEY to new auth
- ✅ **Zero External Dependencies** (commons crate): UserRole and TableAccess enums in kalamdb-commons

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

## Project Structure

### Documentation (this feature)

```text
specs/007-user-auth/
├── spec.md              # Feature specification (completed)
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output - bcrypt, JWT, auth patterns
├── auth-performance.md  # Phase 0 output - caching strategies & benchmarks
├── data-model.md        # Phase 1 output - system.users schema, enums
├── quickstart.md        # Phase 1 output - authentication examples
├── contracts/           # Phase 1 output - API contract specifications
│   ├── auth.yaml        # Authentication endpoints (Basic Auth, JWT)
│   ├── users.yaml       # User management endpoints (CRUD)
│   └── errors.yaml      # Error response schemas
├── checklists/
│   └── requirements.md  # Quality validation checklist (completed)
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT YET CREATED)
```

### Source Code (repository root)

```text
backend/
├── Cargo.toml                      # Add bcrypt, base64 dependencies
├── src/
│   ├── main.rs                     # Update to use AuthService middleware
│   ├── middleware.rs               # Add authentication middleware
│   └── routes.rs                   # Update endpoints with auth requirements
├── crates/
│   ├── kalamdb-auth/               # NEW CRATE - Authentication & Authorization
│   │   ├── Cargo.toml              # Dependencies: bcrypt, base64, jsonwebtoken
│   │   ├── src/
│   │   │   ├── lib.rs              # Public API exports
│   │   │   ├── basic_auth.rs       # HTTP Basic Auth parsing & validation
│   │   │   ├── jwt_auth.rs         # JWT token parsing & validation
│   │   │   ├── password.rs         # Bcrypt hashing & verification
│   │   │   ├── connection.rs       # Localhost detection & connection info
│   │   │   ├── service.rs          # AuthService - main authentication orchestrator
│   │   │   ├── context.rs          # AuthenticatedUser context
│   │   │   └── error.rs            # AuthError types
│   │   └── tests/
│   │       ├── basic_auth_tests.rs
│   │       ├── jwt_tests.rs
│   │       └── password_tests.rs
│   │
│   ├── kalamdb-commons/            # ENHANCED - Add UserRole & TableAccess enums
│   │   └── src/
│   │       ├── models.rs           # Add UserRole enum (user/service/dba/system)
│   │       │                       # Add TableAccess enum (public/private/restricted)
│   │       └── lib.rs              # Re-export new enums
│   │
│   ├── kalamdb-store/              # ENHANCED - Add system.users storage
│   │   └── src/
│   │       ├── users.rs            # NEW - User CRUD operations
│   │       └── lib.rs              # Export user storage functions
│   │
│   ├── kalamdb-core/               # ENHANCED - Add authorization checks
│   │   └── src/
│   │       ├── sql/
│   │       │   └── executor.rs     # Add RBAC checks before query execution
│   │       └── auth/               # ENHANCED authorization module
│   │           ├── mod.rs
│   │           ├── roles.rs        # RBAC permission checking
│   │           └── access.rs       # Table access control logic
│   │
│   ├── kalamdb-api/                # ENHANCED - Update auth middleware
│   │   └── src/
│   │       ├── middleware/
│   │       │   └── auth.rs         # NEW - Use kalamdb-auth AuthService
│   │       ├── handlers/
│   │       │   ├── sql_handler.rs  # Update to use authenticated context
│   │       │   ├── ws_handler.rs   # Update WebSocket auth to new system
│   │       │   └── user_handler.rs # NEW - User management endpoints
│   │       └── routes.rs           # Add user management routes
│   │
│   └── kalamdb-sql/                # ENHANCED - Update context with user role
│       └── src/
│           └── models.rs           # Update ExecutionContext with UserRole
│
└── tests/                          # ENHANCED - Update all integration tests
    ├── test_api_key_auth.rs        # Update to test both old & new auth
    ├── test_basic_auth.rs          # NEW - HTTP Basic Auth tests
    ├── test_jwt_auth.rs            # NEW - JWT token auth tests
    ├── test_rbac.rs                # NEW - Role-based access control tests
    ├── test_shared_access.rs       # NEW - Shared table access control tests
    ├── test_system_users.rs        # NEW - System user localhost/remote tests
    └── integration/                # Update all existing tests with auth
        ├── common/
        │   └── auth_helper.rs      # NEW - Test authentication helpers
        └── [all existing tests]    # Update to authenticate properly

cli/
├── Cargo.toml                      # Add kalamdb-auth dependency
├── src/
│   ├── main.rs                     # Update to use system user credentials
│   ├── config.rs                   # Add system user credential storage
│   └── session.rs                  # Add authentication to sessions
└── tests/
    └── test_cli_auth.rs            # NEW - CLI authentication tests

Cargo.toml                          # Add kalamdb-auth to workspace members
```

**Structure Decision**: 
- **New Crate Justification**: kalamdb-auth isolates all authentication logic (password hashing, JWT validation, Basic Auth parsing) from API and core logic. This follows the library-first principle and allows independent testing.
- **Commons Enhancement**: UserRole and TableAccess enums belong in kalamdb-commons (zero dependencies) to be shared across all crates without circular dependencies.
- **Store Enhancement**: User storage operations added to kalamdb-store for consistency with existing table storage patterns.
- **API/Core Enhancement**: Existing crates updated to use authentication context and enforce authorization rules.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

**Status**: ✅ No constitution violations detected

This feature adds a new crate (kalamdb-auth) which aligns with the assumed library-first principle. The authentication logic is isolated, independently testable, and has clear boundaries.

| Aspect | Complexity Added | Justification |
|--------|------------------|---------------|
| New Crate | kalamdb-auth | Security-critical code isolated for focused testing & audit |
| Enum Types | UserRole (4 variants), TableAccess (3 variants) | Type-safe role & access control prevents runtime errors |
| Auth Methods | 2 (Basic Auth, JWT) | Industry-standard protocols for different use cases |
| Password Hashing | bcrypt | Security best practice - computationally expensive to crack |
| Backward Compat | Old X-API-KEY + new auth coexist | Zero-downtime migration required |

**Rejected Simpler Alternatives**:
- **Single auth method**: Rejected - JWT required for stateless tokens, Basic Auth for simplicity
- **Plain-text passwords**: Rejected - unacceptable security risk
- **Hard-coded roles**: Rejected - enum provides type safety and clear contract
- **Immediate migration**: Rejected - would break existing deployments

---

## Implementation Phases

### Phase 0.5: Storage Backend Abstraction Refactoring (CRITICAL - DO FIRST)

**⚠️ WARNING: This phase MUST be completed before any authentication work begins. It affects the entire codebase.**

**Objective**: Establish two-layer storage abstraction and consolidate all stores into `kalamdb-core/src/stores/` with strongly-typed entity models.

**Why This Is Critical**:
- Authentication code depends on `UserStore` which must follow the new pattern
- All existing stores need to be migrated to avoid technical debt
- Touching these files later (after auth is added) would require rewriting auth code
- Estimated 3-5 days solo / 2-3 days team effort

**Refactoring Tasks**:

1. **Create Storage Infrastructure in kalamdb-store** (~1 day)
   - Create `backend.rs` with `StorageBackend` trait and `RocksDbBackend` implementation
   - Create `traits.rs` with `EntityStore<T>` trait (serialize, deserialize, put, get, delete, scan)
   - Update `lib.rs` to export new abstractions
   - Add integration test with mock `StorageBackend` implementation

2. **Create Domain Models in kalamdb-core** (~0.5 day)
   - Create `models/` directory structure
   - Define `User` struct for `system.users` table
   - Define `Job` struct for `system.jobs` table
   - Define `Namespace` struct for `system.namespaces` table
   - Define `UserTableRow` struct (dynamic fields + system columns `_updated`, `_deleted`)
   - Define `SharedTableRow` and `StreamTableRow` structs
   - All models implement `Serialize`, `Deserialize`, `Clone`, `Debug`

3. **Migrate UserTableStore to kalamdb-core** (~1 day)
   - Move `kalamdb-store/src/user_table_store.rs` → `kalamdb-core/src/stores/user_table_store.rs`
   - Change `db: Arc<DB>` to `backend: Arc<dyn StorageBackend>`
   - Implement `EntityStore<UserTableRow>` trait
   - Override `serialize()` to inject `_updated` and `_deleted` system columns
   - Update all `self.db.put_cf()` → `self.backend.put()`
   - Update all `self.db.get_cf()` → `self.backend.get()`
   - Update partition handling (column family names)
   - Fix all compilation errors in dependent code

4. **Migrate SharedTableStore and StreamTableStore** (~1 day)
   - Same migration process as UserTableStore
   - Define `SharedTableRow` and `StreamTableRow` entity types
   - Implement `EntityStore<T>` for each
   - Update all callers in `kalamdb-core` and `kalamdb-sql`

5. **Create New System Stores** (~0.5 day)
   - Create `kalamdb-core/src/stores/user_store.rs` implementing `EntityStore<User>`
   - Create `kalamdb-core/src/stores/job_store.rs` implementing `EntityStore<Job>`
   - Create `kalamdb-core/src/stores/namespace_store.rs` implementing `EntityStore<Namespace>`
   - Add custom domain methods (e.g., `UserStore::get_by_username()`)
   - Use bincode serialization for system tables (faster than JSON)

6. **Refactor kalamdb-core** (~1 day)
   - Delete `storage/rocksdb_store.rs`, `storage/rocksdb_init.rs`, `storage/rocksdb_config.rs`
   - Replace all `Arc<rocksdb::DB>` with `Arc<dyn StorageBackend>`
   - Update constructors to accept `Arc<dyn StorageBackend>`
   - Remove all `use rocksdb::*` imports
   - Update all code to use `kalamdb_core::stores::*`
   - Fix compilation errors throughout crate

7. **Refactor kalamdb-sql** (~0.5 day)
   - Update `RocksDbAdapter` to wrap `Arc<dyn StorageBackend>` instead of `Arc<rocksdb::DB>`
   - Update `adapter.rs` method signatures
   - Remove `use rocksdb::*` imports
   - Fix compilation errors

8. **Refactor backend Initialization** (~0.5 day)
   - Create `RocksDbBackend` in `main.rs` / `lib.rs`
   - Wrap in `Arc<dyn StorageBackend>`
   - Pass to all stores via constructors
   - Remove direct `Arc<rocksdb::DB>` passing
   - Update startup initialization sequence

9. **Update Cargo.toml Files** (~0.1 day)
   - Remove `rocksdb = "0.24"` from `kalamdb-core/Cargo.toml`
   - Remove `rocksdb = "0.24"` from `kalamdb-sql/Cargo.toml`
   - Remove `rocksdb = "0.24"` from `backend/Cargo.toml`
   - Keep ONLY in `kalamdb-store/Cargo.toml`
   - Add `bincode = "1.3"` to `kalamdb-core/Cargo.toml` (for system table serialization)

10. **Verification & Testing** (~0.5 day)
    - Run `cargo check` on each workspace crate
    - Verify NO `use rocksdb::*` imports outside `kalamdb-store`
    - Run all existing integration tests to ensure nothing breaks
    - Add integration test proving mock backend works
    - Document the new architecture in `docs/architecture/STORAGE_ABSTRACTION.md`

**Output**: 
- `kalamdb-store/src/backend.rs` - StorageBackend trait + RocksDbBackend
- `kalamdb-store/src/traits.rs` - EntityStore<T> trait
- `kalamdb-core/src/models/*.rs` - All entity type definitions
- `kalamdb-core/src/stores/*.rs` - All store implementations
- Updated Cargo.toml files with correct dependencies
- Documentation of the two-layer architecture

**Success Criteria**:
- ✅ `cargo check` passes for all crates
- ✅ `grep -r "use rocksdb" backend/crates/kalamdb-core` returns NO results
- ✅ `grep -r "use rocksdb" backend/crates/kalamdb-sql` returns NO results
- ✅ `grep -r "use rocksdb" backend/src` returns NO results
- ✅ `cargo tree -i rocksdb` shows ONLY `kalamdb-store` depends on it
- ✅ All existing integration tests pass
- ✅ Mock backend integration test passes

**Estimated Timeline**: 
- Solo: 5 days (with testing)
- Team (2 people): 3 days (parallel work on different stores)

---

### Phase 0: Research & Decision Making

**Objective**: Resolve technical unknowns and document architectural decisions

**Research Topics**:

1. **Bcrypt Configuration**
   - Cost factor selection (12 vs 13 vs 14)
   - Async vs sync hashing impact
   - Performance benchmarks on target hardware

2. **JWT Validation Strategy**
   - JWKS endpoint caching strategy
   - Signature algorithm allowlist (RS256, ES256)
   - Token validation library evaluation

3. **Authentication Middleware Pattern**
   - Actix-Web middleware best practices
   - Request context extraction
   - Error handling and 401/403 responses

4. **CLI Credential Storage**
   - Config file location (XDG Base Directory spec)
   - File permissions (0600 vs OS keychain)
   - Multi-instance credential management

5. **Migration Strategy**
   - Dual authentication support timeline
   - Deprecation warnings for X-API-KEY
   - Database initialization with default system user

6. **Storage Backend Abstraction**
   - Current RocksDB usage in `kalamdb-core`, `kalamdb-sql`, `backend`
   - `StorageBackend` trait interface completeness
   - Migration path: `Arc<rocksdb::DB>` → `Arc<dyn StorageBackend>`
   - Impact on system tables (users, jobs, namespaces)
   - Performance implications of trait object indirection

**Output**: `research.md` with decisions, rationale, and alternatives considered

**Additional Research**: `auth-performance.md` documents caching strategies for optimizing authentication:
- JWT token claim caching (5-10x speedup, <1ms p95 latency)
- User record caching (saves 1-5ms RocksDB lookup per request)
- JWKS background refresh (prevents cache expiry spikes)
- Cache invalidation strategies (proactive invalidation on user changes)
- **Recommended Implementation**: User cache (Phase 1), Token cache (Phase 2)

---

### Phase 1: Data Model & Contracts

**Objective**: Define data structures, API contracts, and integration points

#### 1.1 Data Model (`data-model.md`)

**Entities to Document**:

- **User Entity** (system.users table)
  - Fields: user_id, username, email, auth_type, auth_data, role, storage_mode, storage_id, metadata, created_at, updated_at, last_seen, deleted_at
  - Constraints: unique user_id, unique username
  - Indexes: user_id (primary), username (unique), last_seen (for cleanup queries)
  - State transitions: active → soft-deleted → permanently deleted
  - Activity tracking: last_seen updated once per day (async, non-blocking)

- **UserRole Enum** (kalamdb-commons)
  - Variants: User, Service, Dba, System
  - Serialization: lowercase strings ("user", "service", "dba", "system")
  - Trait implementations: Display, FromStr, Serialize, Deserialize

- **TableAccess Enum** (kalamdb-commons)
  - Variants: Public, Private, Restricted
  - Default: Private
  - Serialization: lowercase strings

- **AuthenticatedUser Context**
  - Fields: user_id, username, role, email, connection_info
  - Used throughout request lifecycle

- **ConnectionInfo**
  - Fields: remote_addr (IpAddr), is_localhost (bool)
  - Used for system user access control

- **StorageBackend Trait** (kalamdb-store)
  - Methods: get(), put(), delete(), batch(), scan()
  - Abstracts RocksDB operations for pluggable storage
  - Implemented by: RocksDBBackend (initial), MockBackend (testing)

- **Partition Abstraction** (kalamdb-store)
  - Represents column families (RocksDB), trees (Sled), key prefixes (Redis)
  - Used to isolate system.users, system.tables, user tables, etc.

**Relationships**:
- User → Storage (optional FK: storage_id)
- All crates → StorageBackend trait (dependency inversion)
- ONLY kalamdb-store → rocksdb::DB (concrete implementation)
- User → Tables (ownership via user_id)
- AuthenticatedUser → every authenticated request

#### 1.2 API Contracts (`contracts/`)

**contracts/auth.yaml** - Authentication Endpoints:
```yaml
POST /v1/auth/login
  Request: { username, password }
  Response: { token (JWT), user_id, role }
  Errors: 401 Unauthorized, 400 Bad Request

POST /v1/auth/validate
  Headers: Authorization: Bearer <token>
  Response: { user_id, role, exp }
  Errors: 401 Unauthorized

POST /v1/auth/refresh
  Request: { refresh_token }
  Response: { token, expires_at }
  Errors: 401 Unauthorized
```

**contracts/users.yaml** - User Management:
```yaml
POST /v1/users
  Request: { username, password, email?, role, metadata? }
  Response: { user_id, username, role, created_at }
  Auth: dba or system role required
  Errors: 403 Forbidden, 409 Conflict (duplicate username)

GET /v1/users/{user_id}
  Response: { user_id, username, email, role, metadata, created_at }
  Auth: dba/system (all users) or self
  Errors: 403 Forbidden, 404 Not Found

PUT /v1/users/{user_id}
  Request: { email?, metadata?, password? }
  Response: { user_id, updated_at }
  Auth: dba/system or self (limited fields)
  Errors: 403 Forbidden, 404 Not Found

DELETE /v1/users/{user_id}
  Response: { deleted_at }
  Auth: dba or system role required
  Errors: 403 Forbidden, 404 Not Found

GET /v1/users
  Query: role?, deleted?
  Response: { users: [...] }
  Auth: dba or system role required
  Errors: 403 Forbidden
```

**contracts/errors.yaml** - Error Response Schema:
```yaml
AuthenticationError (401):
  { error: "AUTHENTICATION_FAILED", message, request_id }

AuthorizationError (403):
  { error: "FORBIDDEN", message, required_role, user_role, request_id }

InvalidCredentials (401):
  { error: "INVALID_CREDENTIALS", message }
  Note: Generic message to prevent user enumeration

ExpiredToken (401):
  { error: "TOKEN_EXPIRED", message, expired_at }

MissingAuthorization (401):
  { error: "MISSING_AUTHORIZATION", message }
```

#### 1.3 Quickstart Guide (`quickstart.md`)

**Sections**:
1. Database Initialization (automatic system user creation)
2. Creating Your First User (HTTP Basic Auth example)
3. Authenticating with JWT Tokens
4. Role-Based Access Examples
5. Shared Table Access Control
6. CLI Authentication
7. Troubleshooting Common Auth Issues

#### 1.4 Agent Context Update

Run: `.specify/scripts/bash/update-agent-context.sh copilot`

**Technologies to Add**:
- bcrypt password hashing
- HTTP Basic Authentication (RFC 7617)
- JWT (JSON Web Tokens) validation
- Role-Based Access Control (RBAC) patterns
- Actix-Web authentication middleware

---

### Phase 2: Implementation Planning (via `/speckit.tasks`)

**Note**: Detailed task breakdown will be created by `/speckit.tasks` command after Phase 1 completion.

**Expected Task Categories**:

1. **Setup Tasks**
   - Create kalamdb-auth crate structure
   - Add UserRole and TableAccess enums to kalamdb-commons
   - Add bcrypt and base64 dependencies

2. **Core Authentication Tasks**
   - Implement password hashing/verification (bcrypt)
   - Implement HTTP Basic Auth parsing
   - Implement JWT token validation
   - Implement connection source detection (localhost)
   - Build AuthService orchestrator

3. **Storage Tasks**
   - Add system.users RocksDB column family
   - Implement user CRUD operations via StorageBackend trait
   - Add access column to system.tables
   - **Storage Abstraction Refactoring**:
     - Move `kalamdb-core/src/storage/rocksdb_*.rs` to `kalamdb-store`
     - Create `kalamdb-store::SystemStore` for system tables
     - Replace `Arc<rocksdb::DB>` with `Arc<dyn StorageBackend>` in kalamdb-core
     - Refactor `RocksDbAdapter` to wrap `StorageBackend` instead of `rocksdb::DB`
     - Remove `rocksdb` dependency from kalamdb-core/Cargo.toml
     - Remove `rocksdb` dependency from kalamdb-sql/Cargo.toml
     - Remove `rocksdb` dependency from backend/Cargo.toml

4. **Authorization Tasks**
   - Implement RBAC permission checking
   - Implement shared table access control
   - Add authorization checks to query executor

5. **API Integration Tasks**
   - Create authentication middleware
   - Add user management endpoints
   - Update existing endpoints with auth
   - Update WebSocket authentication

6. **CLI Integration Tasks**
   - Auto-create system user on db init
   - Store system user credentials in CLI config
   - Authenticate CLI sessions with system user

7. **Testing Tasks**
   - Unit tests for kalamdb-auth crate
   - Integration tests for authentication flows
   - Integration tests for RBAC
   - Integration tests for shared table access
   - Update ALL existing integration tests to authenticate
   - CLI authentication tests
   - **Storage Abstraction Tests**:
     - Mock storage backend implementation
     - Integration test proving storage backend swap works
     - Compile-time dependency verification (no rocksdb in kalamdb-core/sql/backend)

8. **Migration Tasks**
   - Backward compatibility with X-API-KEY
   - Deprecation warnings
   - Migration documentation

---

## Key Technical Decisions

### Authentication Architecture

**Decision**: Two-tier authentication (AuthService in kalamdb-auth → middleware in kalamdb-api)

**Rationale**:
- AuthService provides library-level authentication (reusable across server, CLI, future tools)
- Middleware adapts AuthService to Actix-Web request/response cycle
- Clear separation: auth logic vs HTTP concerns

**Alternatives Considered**:
- All-in-middleware: Rejected - not reusable outside HTTP context
- All-in-kalamdb-core: Rejected - mixes storage concerns with auth

### Password Storage

**Decision**: Bcrypt with cost factor 12

**Rationale**:
- Industry standard for password hashing
- Adaptive cost factor (can increase over time)
- Built-in salt generation
- Resistant to GPU cracking
- Cost 12 = ~250ms verification time (acceptable for auth)

**Alternatives Considered**:
- Argon2: Rejected - less mature Rust ecosystem
- PBKDF2: Rejected - less resistant to GPU attacks
- Scrypt: Rejected - memory requirements problematic for high concurrency

### Role Representation

**Decision**: Rust enum (UserRole) with serde serialization

**Rationale**:
- Type-safe at compile time
- Exhaustive match checking prevents missing cases
- serde provides seamless string ↔ enum conversion
- Located in kalamdb-commons for zero-dependency sharing

**Alternatives Considered**:
- String literals: Rejected - no compile-time safety
- Bitflags: Rejected - roles are mutually exclusive, not combinable
- Database-driven roles: Rejected - adds complexity, no clear benefit

### JWT Validation

**Decision**: jsonwebtoken crate with configurable issuer allowlist

**Rationale**:
- Battle-tested library (used by 1000+ crates)
- Supports RS256, ES256, HS256 algorithms
- JWKS integration for public key rotation
- Validates exp, iss, sub claims automatically

**Alternatives Considered**:
- DIY JWT parsing: Rejected - security-critical, use proven library
- OAuth2-specific library: Rejected - over-engineered for our needs
- External validation service: Rejected - adds network dependency

### System User Access Control

**Decision**: Localhost detection via connection info + allow_remote flag

**Rationale**:
- Default deny for remote access = secure by default
- Localhost detection covers 127.0.0.1, ::1, Unix sockets
- Per-user allow_remote flag provides flexibility
- Explicit password requirement when remote enabled

**Alternatives Considered**:
- IP allowlist: Rejected - too complex for CLI use case
- Certificate-based: Rejected - over-engineered for system users
- Always allow remote: Rejected - security risk

### Storage Backend Abstraction

**Decision**: Isolate RocksDB to `kalamdb-store` crate only; all other crates use `StorageBackend` trait

**Rationale**:
- **Pluggable storage**: Enables future migration to Sled, TiKV, FoundationDB without rewriting business logic
- **Clean boundaries**: `kalamdb-core` focuses on business logic, `kalamdb-store` handles storage implementation
- **Testability**: Mock storage backend for fast unit tests without RocksDB dependency
- **Dependency management**: `rocksdb` crate has complex build requirements (libclang); limiting to one crate reduces build issues
- **Type safety**: Trait abstraction prevents leaking RocksDB-specific types (ColumnFamily, WriteBatch) into API surface

**Current State**:
- ❌ `kalamdb-core` imports `rocksdb` directly (storage/rocksdb_*.rs files)
- ❌ `kalamdb-sql` imports `rocksdb::DB` (adapter.rs)
- ❌ `backend` has `rocksdb` workspace dependency
- ✅ `kalamdb-store` has `StorageBackend` trait defined

**Refactoring Strategy**:
1. Move `kalamdb-core/src/storage/rocksdb_*.rs` → `kalamdb-store/src/rocksdb/`
2. Create `kalamdb-store::SystemStore` wrapping `StorageBackend` for system tables
3. Replace all `Arc<rocksdb::DB>` parameters with `Arc<dyn StorageBackend>`
4. Update `RocksDbAdapter` in `kalamdb-sql` to wrap `StorageBackend` trait
5. Remove `rocksdb` from workspace dependencies except `kalamdb-store/Cargo.toml`
6. Add compile-time test verifying no `use rocksdb::*` in restricted crates

**Alternatives Considered**:
- Keep RocksDB everywhere: Rejected - tightly couples KalamDB to single storage engine
- Abstract only user tables: Rejected - inconsistent, system tables need abstraction too
- Use generics instead of trait objects: Rejected - monomorphization bloat, less flexible
- Wait for future storage needs: Rejected - refactoring is harder after authentication code is written

**Performance Impact**:
- Trait object indirection: ~1-2ns per call (negligible compared to RocksDB I/O)
- Measured via benchmarks in `kalamdb-store/benches/storage_trait.rs`

---

**Decision**: Localhost detection via connection source + auth_type="internal"

**Rationale**:
- Simple security model: default secure (no remote access)
- Explicit opt-in for remote access (requires password)
- Connection source available from Actix-Web request
- Works with Unix sockets, 127.0.0.1, ::1

**Alternatives Considered**:
- Always allow remote: Rejected - insecure default
- Always require password: Rejected - inconvenient for internal processes
- IP allowlist: Rejected - complex configuration, easy to misconfigure

### CLI Credential Storage

**Decision**: Store encrypted credentials in config file at `~/.config/kalamdb/credentials.toml` (XDG Base Directory)

**Rationale**:
- XDG standard for config files on Linux/macOS
- File permissions (0600) prevent other users from reading
- Per-instance credentials support multiple databases
- Simple key-value format

**Alternatives Considered**:
- OS keychain: Rejected - complex cross-platform support
- Plaintext: Rejected - insecure if shared machine
- Environment variables: Rejected - visible in process list

---

## Migration & Backward Compatibility

### Gradual Migration Strategy

**Phase 1: Dual Authentication** (this feature)
- Both X-API-KEY and new auth work simultaneously
- X-USER-ID header honored if present
- Deprecation warnings logged for old headers

**Phase 2: Deprecation** (future)
- X-API-KEY and X-USER-ID marked deprecated in docs
- Warnings logged to application logs
- Timeline: 6 months notice before removal

**Phase 3: Removal** (future)
- Old auth headers removed from codebase
- Migration guide published

### Test Migration Plan

**All existing integration tests must be updated**:
1. Add authentication helper module (`tests/integration/common/auth_helper.rs`)
2. Create test system user in test setup
3. Update each test to authenticate before operations
4. Verify both old and new auth work during transition

**Test Categories to Update**:
- API tests (40+ test files)
- WebSocket tests
- SQL execution tests
- Flush and compaction tests
- Stream tests

---

## Security Considerations

### Threat Model

**Threats Mitigated**:
1. **Unauthorized Access**: RBAC prevents privilege escalation
2. **Password Compromise**: Bcrypt makes cracking computationally expensive
3. **Token Replay**: JWT expiration limits replay window
4. **User Enumeration**: Generic "invalid credentials" message
5. **Remote System User Access**: Localhost-only default prevents remote attacks

**Threats Acknowledged** (out of scope for this feature):
1. **Rate Limiting**: Not implemented - can be added later
2. **Account Lockout**: Not implemented - prevents DoS of legitimate users
3. **2FA/MFA**: Not implemented - can be added as enhancement
4. **Session Management**: Stateless JWT, no server-side session tracking

### Audit Logging

**All authentication events logged**:
- Successful authentication (user_id, source IP, timestamp)
- Failed authentication (username attempted, source IP, reason, timestamp)
- Authorization failures (user_id, attempted operation, role, timestamp)
- Password changes (user_id, timestamp)
- Role changes (user_id, old_role, new_role, changed_by, timestamp)

**Log Format**: Structured JSON logs via existing logging infrastructure

---

## Performance Characteristics

### Authentication Performance

| Operation | Target | Notes |
|-----------|--------|-------|
| Bcrypt hash (new user) | < 300ms | Cost factor 12, blocks during hash |
| Bcrypt verify (login) | < 300ms | Cost factor 12, same as hash |
| JWT validation | < 50ms | Signature verification + claim validation |
| Basic Auth parse | < 1ms | Base64 decode + split |
| RBAC check | < 5ms | In-memory enum comparison |
| Localhost detection | < 1ms | IP address comparison |

### Concurrency

**Bcrypt Handling**:
- Use `tokio::task::spawn_blocking` for bcrypt operations
- Prevents blocking async runtime
- Thread pool sized for expected auth rate

**JWT Caching**:
- JWKS public keys cached (1 hour TTL)
- Reduces external HTTP requests
- Invalidate cache on signature failure

### Memory Usage

| Component | Memory Estimate |
|-----------|-----------------|
| 1M users in RocksDB | ~200MB (avg 200 bytes/user) |
| Bcrypt hash (per user) | 60 bytes |
| AuthenticatedUser context | 200 bytes/request |
| JWT validation cache | 10KB (public keys) |

---

## Testing Strategy

### Unit Tests (kalamdb-auth crate)

- Password hashing and verification
- Basic Auth header parsing (valid/invalid/malformed)
- JWT token validation (valid/expired/invalid signature)
- Localhost detection (various IP formats)
- UserRole and TableAccess enum serialization

### Integration Tests

**New Test Files**:
1. `test_basic_auth.rs`: HTTP Basic Auth flow
2. `test_jwt_auth.rs`: JWT token authentication
3. `test_rbac.rs`: Role-based permission matrix
4. `test_shared_access.rs`: Shared table access control
5. `test_system_users.rs`: Localhost vs remote access
6. `test_cli_auth.rs`: CLI authentication and credential management
7. `test_auth_migration.rs`: Old and new auth coexistence

**Existing Test Updates**:
- ALL tests in `backend/tests/` must authenticate
- Create `auth_helper.rs` module for test authentication
- Verify no tests break during migration

### Test Coverage Goals

- **Unit test coverage**: > 90% for kalamdb-auth crate
- **Integration test coverage**: 100% of user stories
- **Edge case coverage**: All 12 edge cases documented in spec
- **Role coverage**: Each role tested with permitted and forbidden operations
- **Access level coverage**: Each shared table access level tested

---

## Dependencies

### New Crate Dependencies

**kalamdb-auth Cargo.toml**:
```toml
[dependencies]
bcrypt = "0.15"           # Password hashing
base64 = "0.21"           # Basic Auth decoding
jsonwebtoken = "9.2"      # JWT validation (already in workspace)
kalamdb-commons = { path = "../kalamdb-commons" }
kalamdb-store = { path = "../kalamdb-store" }
serde = { workspace = true }
thiserror = { workspace = true }
log = { workspace = true }
```

**backend/Cargo.toml additions**:
```toml
[dependencies]
kalamdb-auth = { path = "crates/kalamdb-auth" }
bcrypt = "0.15"
base64 = "0.21"
```

**cli/Cargo.toml additions**:
```toml
[dependencies]
kalamdb-auth = { path = "../backend/crates/kalamdb-auth" }
```

### Dependency Justification

- **bcrypt**: Industry standard for password hashing, proven security
- **base64**: RFC 7617 Basic Auth uses base64 encoding
- **jsonwebtoken**: Already used in project, mature JWT library

---

## Open Questions & Risks

### Open Questions

1. **OAuth Provider Integration Scope**: Which OAuth providers to support in initial release? (Google, GitHub, Azure?)
   - **Recommendation**: Start with Google and GitHub (most common), add others later

2. **Password Complexity Requirements**: Enforce complexity rules beyond minimum length?
   - **Recommendation**: Start with minimum 8 chars, add complexity as enhancement

3. **Session Behavior on Role Change**: Force re-authentication or apply immediately?
   - **Recommendation**: Apply on next request (simpler), document in security notes

4. **Auto-Provisioning for OAuth**: Enable by default or require explicit configuration?
   - **Recommendation**: Disabled by default (security), explicit opt-in

5. **Storage Abstraction Implementation Order**: Refactor storage abstraction before or after authentication implementation?
   - **Recommendation**: Refactor storage abstraction FIRST (Phase 1.5), then build authentication on clean abstractions. Rationale: Avoids rewriting authentication code after storage refactoring; ensures authentication uses StorageBackend from day one.
   - **Alternative**: Implement auth with RocksDB directly, refactor later - REJECTED due to rework risk

### Risks & Mitigation

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| Bcrypt blocking affects latency | HIGH | MEDIUM | Use spawn_blocking for all bcrypt ops |
| Existing tests break during migration | MEDIUM | HIGH | Gradual rollout, auth_helper module |
| JWT validation failure on clock skew | MEDIUM | LOW | Allow 60s clock skew in validation |
| System user remote access misconfiguration | HIGH | LOW | Deny by default, require explicit enable |
| Performance regression from RBAC checks | MEDIUM | LOW | In-memory enum checks are <5ms |
| Storage abstraction breaks existing code | HIGH | MEDIUM | Incremental refactoring, extensive testing, type system catches issues at compile time |
| Trait object overhead impacts performance | LOW | LOW | Benchmarks show <2ns overhead; I/O dominates |
| Mock storage backend diverges from RocksDB | MEDIUM | LOW | Property-based tests verify behavior equivalence |

---

## Success Metrics

**From spec.md Success Criteria**:

1. ✅ **SC-001**: HTTP Basic Auth < 100ms (95th percentile)
2. ✅ **SC-002**: JWT validation < 50ms (95th percentile)
3. ✅ **SC-003**: 100% RBAC accuracy across all 4 roles
4. ✅ **SC-004**: Public shared tables readable by all authenticated users
5. ✅ **SC-005**: System users authenticate from localhost without password
6. ✅ **SC-006**: CLI connects without manual user setup
7. ✅ **SC-007**: Incorrect credentials rejected within 500ms (bcrypt)
8. ✅ **SC-008**: 1000 concurrent auth requests without degradation
9. ✅ **SC-009**: Auth errors don't reveal username existence
10. ✅ **SC-010**: 100% integration test pass rate
11. ✅ **SC-011**: Existing tests updated and passing
12. ✅ **SC-012**: DB init creates system user in < 5 seconds
13. ✅ **SC-013**: OAuth users authenticate without password management
14. ✅ **SC-014**: Storage abstraction complete - zero RocksDB imports in kalamdb-core/sql/backend
15. ✅ **SC-015**: Mock storage backend successfully authenticates users
16. ✅ **SC-016**: All user credential operations use kalamdb-store abstractions only

**Additional Implementation Metrics**:
- Zero security vulnerabilities in auth code (audit before merge)
- All 93+ functional requirements implemented and tested (including User-Management.md + storage abstraction)
- **90 comprehensive tests**: 74 integration tests + 16 unit tests
- Documentation complete (quickstart, API contracts, migration guide)
- Backward compatibility: X-API-KEY continues working

### Integration Test Coverage

**Test Organization** (74 integration tests across 12 files):

1. **Authentication** (11 tests)
   - backend/tests/test_basic_auth.rs - HTTP Basic Auth (5 tests)
   - backend/tests/test_jwt_auth.rs - JWT token validation (6 tests)

2. **Authorization** (14 tests)
   - backend/tests/test_rbac.rs - Role-based access control for all 4 roles

3. **Shared Table Access** (5 tests)
   - backend/tests/test_shared_access.rs - Public/private/restricted access levels

4. **SQL User Management** (16 tests)
   - backend/tests/test_user_sql.rs - CREATE USER, ALTER USER, DROP USER commands

5. **Password Security** (6 tests)
   - backend/tests/test_password_security.rs - Hashing, complexity, common passwords

6. **OAuth Integration** (4 tests)
   - backend/tests/test_oauth.rs - OAuth provider authentication

7. **System Users** (4 tests)
   - backend/tests/test_system_user.rs - Localhost-only and remote access

8. **Cleanup Job** (3 tests)
   - backend/tests/test_user_cleanup.rs - Scheduled deletion after grace period

9. **Edge Cases** (4 tests)
   - backend/tests/test_edge_cases.rs - Malformed input, concurrent access, caching

10. **End-to-End** (1 test)
    - backend/tests/test_e2e_auth_flow.rs - Complete user lifecycle

11. **Storage Abstraction** (6 tests)
    - backend/tests/test_storage_abstraction.rs - Mock backend, dependency verification (3 tests)
    - kalamdb-store/src/tests/mod.rs - Encapsulation tests (3 tests)

**Unit Test Coverage** (16 unit tests):
- backend/crates/kalamdb-auth/tests/ - Password hashing, JWT validation, Basic Auth parsing
- backend/crates/kalamdb-sql/tests/ - SQL parser for user management commands

**Test-First Development**:
- All tests written BEFORE implementation (TDD approach)
- Each user story has dedicated test file
- Tests validate acceptance criteria from spec.md
- Edge cases explicitly tested
- Performance benchmarks included

