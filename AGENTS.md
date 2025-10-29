# KalamDB Development Guidelines

## ⚠️ CRITICAL: System Table Models Architecture

**SINGLE SOURCE OF TRUTH**: All system table models are defined in `kalamdb-commons/src/models/system.rs`

**Authentication Constants**: System user constants defined in `kalamdb-commons/src/constants.rs`
**DO NOT create duplicate model definitions**. Always import from:
```rust
use kalamdb_commons::system::{User, Job, LiveQuery, Namespace, SystemTable, InformationSchemaTable, UserTableCounter};
```

**ALWAYS prefer using enums instead of string**


## ⚠️ CRITICAL: Dependency Management

**SINGLE SOURCE OF TRUTH**: All Rust dependencies are defined in the root `Cargo.toml` under `[workspace.dependencies]`

**NEVER specify versions in individual crate Cargo.toml files**. Always use `workspace = true`:

```toml
# ✅ CORRECT: In backend/crates/kalamdb-core/Cargo.toml
[dependencies]
serde = { workspace = true }
tokio = { workspace = true, features = ["sync", "time", "rt"] }

# ❌ WRONG: Don't specify versions directly
serde = { version = "1.0", features = ["derive"] }
tokio = { version = "1.48.0", features = ["full"] }
```

**When adding a new dependency:**
1. Add it to `Cargo.toml` (root) under `[workspace.dependencies]` with version
2. Reference it in individual crates using `{ workspace = true }`
3. Add crate-specific features if needed: `{ workspace = true, features = ["..."] }`

**To update a dependency version:**
- Only edit the version in root `Cargo.toml`
- All crates will automatically use the new version

## Active Technologies
- Rust 1.90+ (stable toolchain, edition 2021)
- RocksDB 0.24, Apache Arrow 52.0, Apache Parquet 52.0, DataFusion 40.0, Actix-Web 4.4
- RocksDB for write path (<1ms), Parquet for flushed storage (compressed columnar format)
- TypeScript/JavaScript ES2020+ (frontend SDKs)
- WASM for browser-based client library

**Authentication & Authorization**:
- **bcrypt 0.15**: Password hashing (cost factor 12, min 8 chars, max 72 chars)
- **jsonwebtoken 9.2**: JWT token generation and validation (HS256 algorithm)
- **HTTP Basic Auth**: Base64-encoded username:password (Authorization: Basic header)
- **JWT Bearer Tokens**: Stateless authentication (Authorization: Bearer header)
- **OAuth 2.0 Integration**: Google Workspace, GitHub, Microsoft Azure AD
- **RBAC (Role-Based Access Control)**: Four roles (user, service, dba, system)
- **Actix-Web Middleware**: Custom authentication extractors and guards
- **StorageBackend Abstraction**: `Arc<dyn StorageBackend>` isolates RocksDB dependencies

## Project Structure
```
backend/                         # Server binary and core crates
├── src/main.rs                  # kalamdb-server entry point
└── crates/                      # Supporting libraries
    ├── kalamdb-core/            # Core library (embeddable)
    ├── kalamdb-api/             # REST API and WebSocket
    ├── kalamdb-sql/             # SQL execution and DataFusion
    ├── kalamdb-store/           # RocksDB storage layer
    ├── kalamdb-live/            # Real-time subscriptions
    ├── kalamdb-auth/            # Authentication and authorization
    └── kalamdb-commons/         # Shared utilities and models

cli/                             # CLI tool binary
├── src/main.rs                  # kalam-cli entry point
└── tests/                       # CLI integration tests

link/                            # WASM-compiled client library
├── src/lib.rs                   # Rust library
├── src/wasm.rs                  # WASM bindings
└── sdks/                        # Multi-language SDKs
    └── typescript/              # TypeScript/JavaScript SDK
        ├── package.json         # npm package (@kalamdb/client)
        ├── build.sh             # Rust→WASM compilation
        ├── tests/               # SDK tests (14 passing)
        └── README.md            # API documentation

examples/                        # Example applications
└── simple-typescript/           # React TODO app
    ├── package.json             # Uses link/sdks/typescript/ as dependency
    └── src/                     # Imports from '@kalamdb/client'

specs/                           # Feature specifications
└── 007-user-auth/               # Current feature
    ├── plan.md                  # Implementation plan
    └── tasks.md                 # Task breakdown
```

## SDK Architecture Principles

**CRITICAL**: Examples MUST use SDKs as dependencies, NOT implement their own clients

- SDKs at `link/sdks/{language}/` are complete, publishable npm/PyPI packages
- Examples import SDKs as local dependencies: `"@kalamdb/client": "file:../../link/sdks/typescript"`
- Examples MUST NOT create mock client implementations (e.g., `kalamClient.ts`)
- If examples need functionality, add it to the SDK for all users
- SDKs include: build system, tests, docs, package config, .gitignore
- Always keep the: docs\architecture\SQL_SYNTAX.md updated with the latest SQL syntax we have
- Check the README.md if there is anything not acurate

**Example Usage**:
```typescript
// ✅ CORRECT: examples/simple-typescript/src/App.tsx
import { KalamClient } from '@kalamdb/client'; // From SDK

// ❌ WRONG: examples/simple-typescript/src/services/kalamClient.ts
export class KalamClient { ... } // Don't implement your own!
```

## Commands
```bash
# Build entire workspace
cargo build

# Test entire workspace
cargo test

# Run server
cargo run --bin kalamdb-server

# Run CLI
cargo run --bin kalam

# Build specific crate
cargo build -p kalamdb-core

# Run tests for specific crate
cargo test -p kalamdb-sql
```

## Code Style

- **Rust 2021 edition**: Follow standard Rust conventions
- **Type-safe wrappers**: Use `NamespaceId`, `TableName`, `UserId`, `StorageId`, `TableType` enum, `UserRole` enum, `TableAccessLevel` enum instead of raw strings
- **Error handling**: Use `Result<T, KalamDbError>` for all fallible operations
- **Async**: Use `tokio` runtime, prefer `async/await` over raw futures
- **Logging**: Use `log` macros (`info!`, `debug!`, `warn!`, `error!`)
- **Serialization**: Use `serde` with `#[derive(Serialize, Deserialize)]`

**Authentication Patterns**:
- **Password Security**: ALWAYS use `bcrypt::hash()` for password storage, NEVER store plaintext
- **Timing-Safe Comparisons**: Use `bcrypt::verify()` for constant-time password verification
- **JWT Claims**: Include `user_id`, `role`, `exp` (expiration) in token payload
- **Role Hierarchy**: system > dba > service > user (enforced in authorization middleware)
- **Generic Error Messages**: Use "Invalid username or password" (NEVER "user not found" vs "wrong password")
- **Soft Deletes**: Set `deleted_at` timestamp, return same error as invalid credentials
- **Authorization Checks**: Verify role permissions BEFORE executing database operations
- **Storage Abstraction**: Use `Arc<dyn StorageBackend>` instead of `Arc<rocksdb::DB>` (except in kalamdb-store)

## Recent Changes
- 2025-10-29: **Phase 13 & 14: Index Infrastructure & EntityStore Foundation** - Completed foundational work:
  - **Phase 13 Index Infrastructure**: Created generic SecondaryIndex<T,K> infrastructure (kalamdb-store/src/index/mod.rs)
    - User indexes: username (unique), role (non-unique), deleted_at (non-unique)
    - UserIndexManager: Unified API for all 3 indexes
    - Total: ~980 lines of code, 26 comprehensive tests
  - **Phase 14 Foundation (Steps 1-3)**: Type-safe entity storage infrastructure
    - Type-safe key models: RowId, UserRowId, TableId, JobId, LiveQueryId, UserName (6 models, 62 tests)
    - EntityStore<K,V> and CrossUserTableStore<K,V> traits (350+ lines, 4 tests)
    - SystemTableStore<K,V> generic implementation (400+ lines, 9 tests)
    - Total: ~1,640 lines of foundational code, 75+ unit tests
  - **Phase 14 Migration (Steps 4-12)**: DEFERRED - Can be adopted incrementally
    - Foundation is production-ready for NEW features
    - Existing tables can be migrated one at a time as needed
    - See PHASE_14_MIGRATION_ROADMAP.md for strategy
- 2025-10-28: **Dependency Management** - Migrated all dependencies to workspace dependencies pattern
- 2025-10-28: **User Authentication & Authorization** - Implemented comprehensive auth system:
  - HTTP Basic Auth and JWT token authentication
  - RBAC with 4 roles (user, service, dba, system)
  - SQL-based user management (CREATE USER, ALTER USER, DROP USER)
  - bcrypt password hashing (cost 12)
  - OAuth 2.0 integration (Google, GitHub, Azure AD)
  - Shared table access control (public, private, restricted)
  - System user isolation (localhost-only default)
  - New kalamdb-auth crate for authentication/authorization logic
  - StorageBackend abstraction pattern (Arc<dyn StorageBackend>)


<!-- MANUAL ADDITIONS START -->
<!-- MANUAL ADDITIONS END -->
