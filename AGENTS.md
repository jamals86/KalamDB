# KalamDB Development Guidelines

## 🎯 Core Coding Principles

**ALWAYS follow these essential guidelines:**

1. **Model Separation**: Each model MUST be in its own separate file
   ```rust
   // ✅ CORRECT: Separate files
   models/user.rs        // User model only
   models/job.rs         // Job model only
   models/namespace.rs   // Namespace model only
   
   // ❌ WRONG: Multiple models in one file
   models/all.rs         // Contains User, Job, Namespace together
   ```

2. **AppContext-First Pattern**: Use `Arc<AppContext>` parameter instead of individual fields

3. **Performance & Memory Optimization**: Focus on lightweight memory usage and high concurrency
   - Use `Arc<T>` for zero-copy sharing (no cloning data)
   - DashMap for lock-free concurrent access
   - Memoize expensive computations (e.g., Arrow schema construction)
   - Separate large structs from hot-path metadata (LRU timestamps in separate map)
   - Cache singleton instances (e.g., UserTableShared per table, not per user)

4. instead than doing user_id.map(kalamdb_commons::models::UserId::from); always add it to the head of the file:
```rust
use kalamdb_commons::models::UserId;
```

5. Well-organized code with minimal duplication
6. Leveraging DataFusion for query processing, version resolution, and deletion filtering
7. WHEN FINIDNG MULTIPLE ERRORS dont kleep running cargo check or cargo build run one time and output to a file and fix all of them at once!
8. Instead of passing both Namespaceid and TableName pass TableId which contains both
9. Don't import use inside methods always add them to the top of the rust file instead
10. EntityStore is used instead of using the EntityStorev2 alias

11. Filesystem vs RocksDB separation of concerns
   - Filesystem/file storage logic (cold storage, Parquet files, directory management, file size accounting) must live in `backend/crates/kalamdb-filestore`
   - Key-value storage engines and partition/column family logic must live in `backend/crates/kalamdb-store`
   - Orchestration layers in `kalamdb-core` (DDL/DML handlers, job executors) should delegate to these crates and avoid embedding filesystem or RocksDB specifics directly
   - When adding cleanup/compaction/file lifecycle functionality, implement it in `kalamdb-filestore` and call it from `kalamdb-core`

12. **Smoke Tests Priority**: Always ensure smoke tests are passing before committing changes. If smoke tests fail, fix them or the underlying backend issue immediately. Run `cargo test --test smoke` in the `cli` directory to verify.

> **⚠️ IMPORTANT**: Smoke tests require a running KalamDB server! Start the server first with `cargo run` in the `backend` directory before running smoke tests. The tests will fail if no server is running.

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

**Job Management System** (Phase 9 + Phase 8.5 Complete):
- **UnifiedJobManager**: Typed JobIds (FL/CL/RT/SE/UC/CO/BK/RS), idempotency, retry logic (3× default with exponential backoff)
- **8 Job Executors**: FlushExecutor (complete), CleanupExecutor (✅), RetentionExecutor (✅), StreamEvictionExecutor (✅), UserCleanupExecutor (✅), CompactExecutor (placeholder), BackupExecutor (placeholder), RestoreExecutor (placeholder)
- **Status Transitions**: New → Queued → Running → Completed/Failed/Retrying/Cancelled
- **Crash Recovery**: Marks Running jobs as Failed on server restart
- **Idempotency Keys**: Format "{job_type}:{namespace}:{table}:{date}" prevents duplicate jobs

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
backend/crates/
├── kalamdb-core/               # Core library (embeddable)
│   ├── app_context.rs          # Singleton AppContext
│   ├── schema_registry/        # Schema management (consolidated from kalamdb-registry)
│   │   ├── registry.rs         # Unified cache + Arrow memoization + persistence
│   │   ├── arrow_schema.rs     # Arrow schema utilities
│   │   ├── stats.rs            # Stats table provider
│   │   ├── views/              # Virtual views (information_schema)
│   │   └── error.rs            # Registry error types
│   ├── live/                   # Live query management (consolidated from kalamdb-live)
│   │   ├── manager.rs          # LiveQueryManager
│   │   ├── connection_registry.rs # Connection tracking
│   │   ├── filter.rs           # Filter cache
│   │   ├── initial_data.rs     # Initial data fetcher
│   │   └── error.rs            # Live query error types
│   ├── tables/                 # Table implementations
│   │   ├── base_table_provider.rs # Common interfaces
│   │   ├── user_tables/        # User table provider
│   │   ├── shared_tables/      # Shared table provider
│   │   ├── stream_tables/      # Stream table provider
│   │   └── system/             # System tables (10 providers)
│   │       ├── registry.rs     # SystemTablesRegistry
│   │       └── [users, jobs, namespaces, storages, live_queries, tables, audit_logs]/
│   ├── sql/executor/           # Handler-based executor
│   │   ├── mod.rs              # Routing orchestrator
│   │   └── handlers/           # DDL, DML, Query, Flush, etc.
│   ├── jobs/                   # Job management
│   │   ├── jobs_manager.rs     # UnifiedJobManager
│   │   └── executors/          # 8 job executors
│   ├── flush/                  # Flush operations
│   └── providers/              # Additional table providers
├── kalamdb-store/              # RocksDB storage layer
├── kalamdb-commons/            # Shared models and utilities
│   ├── system_tables.rs        # System table models (User, Job, etc.)
│   ├── constants.rs            # System constants
│   └── models/schemas/         # TableDefinition, ColumnDefinition, etc.
├── kalamdb-sql/                # SQL parsing
├── kalamdb-auth/               # Authentication/authorization
└── kalamdb-api/                # REST API and WebSocket

specs/010-core-architecture-v2/ # CURRENT: Arrow memoization, views
```

## Code Style

- **Rust 2021 edition**: Follow standard Rust conventions
- **Type-safe wrappers**: Use `NamespaceId`, `TableName`, `UserId`, `StorageId`, `TableType` enum, `UserRole` enum, `TableAccessLevel` enum instead of raw strings
- **Error handling**: Use `Result<T, KalamDbError>` for all fallible operations
- **Async**: Use `tokio` runtime, prefer `async/await` over raw futures
- **Logging**: Use `log` macros (`info!`, `debug!`, `warn!`, `error!`)
- **Serialization**: Use `serde` with `#[derive(Serialize, Deserialize)]`

## Build & Check Cadence (MUST)

- Prefer batching compilation feedback to avoid slow edit-run loops.
- When iterating on multi-file changes, run a single workspace-wide check and capture output to a file, fix all issues, then re-check:
  - Example: `cargo check > batch_compile_output.txt 2>&1`
  - Parse and address all errors/warnings in one pass; avoid running `cargo check` repeatedly after each tiny edit.
- Re-run `cargo check` only after a meaningful batch of fixes. This keeps feedback fast and focused, and prevents thrashing CI and local builds.

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

**Phase 13 - Crate Consolidation** (Complete):
- **Eliminated kalamdb-registry**: All schema management code moved to `kalamdb-core/src/schema_registry/`
- **Eliminated kalamdb-live**: All live query code moved to `kalamdb-core/src/live/`
- **Circular Dependency Resolution**: Direct integration into kalamdb-core eliminates circular dependencies
- **Import Updates**: Changed from `kalamdb_registry::` and `kalamdb_live::` to `crate::schema_registry::` and `crate::live::`
- **Error Type Consolidation**: RegistryError and LiveError now part of kalamdb-core error module
- **Workspace Size**: Reduced from 11 crates to 9 crates (more maintainable)
- **Stats Provider**: Moved from kalamdb-system to kalamdb-core/src/schema_registry/stats.rs
