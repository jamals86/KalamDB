# KalamDB Code Organization

**Last Updated**: October 25, 2025  
**Version**: 0.1.0

## Overview

KalamDB is organized as a Rust workspace with multiple crates following a layered architecture. This document describes the codebase structure, module hierarchy, and architectural decisions.

## Workspace Structure

```
backend/
├── crates/
│   ├── kalamdb-commons/      # Shared types and utilities
│   ├── kalamdb-store/        # Storage layer (RocksDB/Parquet)
│   ├── kalamdb-sql/          # SQL parsing and system catalog
│   ├── kalamdb-core/         # Core business logic
│   ├── kalamdb-live/         # Live query subscriptions
│   ├── kalamdb-api/          # REST API and WebSocket handlers
│   └── kalamdb-server/       # Server binary and lifecycle
├── tests/
│   ├── integration/          # Integration tests by category
│   └── test_*.rs             # Top-level integration tests
└── benches/                  # Performance benchmarks
```

## Crate Dependency Graph

```
kalamdb-server
    ├── kalamdb-api
    │   ├── kalamdb-core
    │   └── kalamdb-live
    ├── kalamdb-core
    │   ├── kalamdb-sql
    │   ├── kalamdb-store
    │   └── kalamdb-commons
    ├── kalamdb-sql
    │   └── kalamdb-commons
    ├── kalamdb-store
    │   └── kalamdb-commons
    └── kalamdb-commons (base layer)
```

### Dependency Rules

1. **No circular dependencies**: Each crate can only depend on crates below it
2. **Commons is base**: All crates can depend on `kalamdb-commons`
3. **Core is central**: Business logic in `kalamdb-core` orchestrates other layers
4. **API is presentation**: `kalamdb-api` only handles HTTP/WebSocket, no business logic

## Crate Descriptions

### kalamdb-commons (Base Layer)

**Purpose**: Shared types, utilities, and error handling used across all crates.

**Key Modules**:
- `models/` - Type-safe wrappers and core domain models
  - `user_id.rs` - UserId newtype wrapper
  - `namespace_id.rs` - NamespaceId newtype wrapper
  - `table_name.rs` - TableName newtype wrapper
  - `storage_id.rs` - StorageId newtype wrapper
  - `mod.rs` - Enums (TableType, JobStatus, JobType, StorageType)
- `errors.rs` - Common error types

**Design Principles**:
- Type-safe wrappers prevent string confusion
- No dependencies on other kalamdb crates
- Minimal external dependencies

**Key Types**:
```rust
pub struct UserId(String);           // User identifier
pub struct NamespaceId(String);      // Namespace identifier
pub struct TableName(String);        // Table name
pub struct StorageId(String);        // Storage location identifier

pub enum TableType { User, Shared, Stream, System }
pub enum JobStatus { Pending, Running, Completed, Failed }
pub enum JobType { Flush, Compact, Cleanup }
```

### kalamdb-store (Storage Layer)

**Purpose**: Abstractions for RocksDB (write path) and Parquet (read path).

**Key Modules**:
- `common.rs` - Shared column family helpers
- `user_table_store.rs` - User table storage operations
- `shared_table_store.rs` - Shared table storage operations
- `stream_table_store.rs` - Stream table storage operations
- `storage_trait.rs` - Storage abstraction trait
- `rocksdb_impl.rs` - RocksDB implementation

**Design Principles**:
- Common base module for code reuse
- Type-safe partition/CF naming
- Separation of concerns (write vs read paths)

**Architecture**:
- **Write Path**: RocksDB column families (<1ms latency)
- **Read Path**: Parquet files (compressed columnar format)
- **Flush Mechanism**: Async flush from RocksDB → Parquet

**Unsafe Code**:
- `common.rs`: 2 unsafe blocks for CF creation/deletion (RocksDB API limitation)
- `rocksdb_impl.rs`: 2 unsafe blocks for partition management
- All unsafe blocks have SAFETY comments explaining rationale

### kalamdb-sql (SQL Layer)

**Purpose**: SQL parsing, DDL/DML execution, and system catalog.

**Key Modules**:
- `ddl/` - DDL statement parsing and execution
  - `create_table.rs` - CREATE TABLE parsing
  - `alter_table.rs` - ALTER TABLE parsing
  - `drop_table.rs` - DROP TABLE parsing
- `dml/` - DML statement execution
- `models.rs` - System table models (User, Namespace, Table, Storage)
- `executor.rs` - SQL execution engine
- `kalam_sql.rs` - Main SQL coordinator

**Design Principles**:
- Separation of parsing (sqlparser) and execution
- System catalog stored in RocksDB
- Type-safe models with From/Into conversions

**System Tables**:
- `system.users` - User accounts
- `system.namespaces` - Namespace metadata
- `system.tables` - Table definitions
- `system.storage_locations` - Storage configurations
- `system.jobs` - Background jobs
- `system.live_queries` - Active subscriptions

### kalamdb-core (Business Logic Layer)

**Purpose**: Core orchestration of table operations, schema evolution, and services.

**Key Modules**:
- `services/` - Business logic services
  - `user_table_service.rs` - User table CRUD
  - `shared_table_service.rs` - Shared table CRUD
  - `stream_table_service.rs` - Stream table CRUD
  - `schema_evolution_service.rs` - ALTER TABLE logic
  - `table_deletion_service.rs` - DROP TABLE logic
- `sql/` - SQL execution and DataFusion integration
- `flush/` - Flush policy and scheduling
- `jobs/` - Background job management
- `live_query/` - Live query infrastructure
  - `manager.rs` - Subscription management
  - `filter.rs` - WHERE clause compilation
  - `initial_data.rs` - Initial data fetching
- `tables/system/` - System table providers for DataFusion

**Design Principles**:
- Services encapsulate business rules
- DataFusion for complex SQL queries
- Async-first design with Tokio
- Event-driven notifications

### kalamdb-live (Live Query Layer)

**Purpose**: Real-time subscription and notification system.

**Key Modules**:
- `subscription.rs` - Subscription lifecycle
- `notification.rs` - Change notification format
- `websocket.rs` - WebSocket protocol

**Design Principles**:
- Decoupled from core business logic
- Efficient notification batching
- Filter compilation for performance

### kalamdb-api (Presentation Layer)

**Purpose**: HTTP REST API and WebSocket handlers.

**Key Modules**:
- `handlers/` - HTTP endpoint handlers
  - `sql.rs` - SQL execution endpoint
  - `health.rs` - Health check
- `websocket/` - WebSocket connection management
- `middleware/` - CORS, authentication, logging

**Design Principles**:
- Thin layer - no business logic
- Actix-Web framework
- JSON request/response format
- JWT authentication

### kalamdb-server (Application Layer)

**Purpose**: Server binary, configuration, and lifecycle management.

**Key Modules**:
- `main.rs` - Application entry point
- `lifecycle.rs` - Startup and shutdown logic
- `config.rs` - Configuration management

**Design Principles**:
- Graceful shutdown
- Signal handling (SIGTERM, SIGINT)
- Default configurations for development

## Module Organization Patterns

### Type-Safe Wrapper Pattern

**Location**: `kalamdb-commons/src/models/`

**Pattern**:
```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeName(String);

impl TypeName {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    
    pub fn as_ref(&self) -> &str {
        &self.0
    }
    
    pub fn into_string(self) -> String {
        self.0
    }
}

impl From<String> for TypeName { ... }
impl From<&str> for TypeName { ... }
impl AsRef<str> for TypeName { ... }
```

**Benefits**:
- Compile-time type checking prevents string confusion
- Clear intent in function signatures
- Consistent API across all wrappers

### Common Base Module Pattern

**Location**: `kalamdb-store/src/common.rs`

**Pattern**:
```rust
// Shared helper functions used by all table stores
pub fn create_column_family(db: &Arc<DB>, cf_name: &str) -> Result<()> {
    // Common implementation
}

pub fn drop_column_family(db: &Arc<DB>, cf_name: &str) -> Result<()> {
    // Common implementation
}
```

**Usage**:
```rust
// In user_table_store.rs
use crate::common::{create_column_family, drop_column_family};

impl UserTableStore {
    pub fn create_column_family(&self, namespace: &str, table: &str) -> Result<()> {
        let cf_name = format!("user_table:{}:{}", namespace, table);
        common::create_column_family(&self.db, &cf_name)
    }
}
```

**Benefits**:
- Eliminates code duplication
- Consistent error handling
- Easier to test and maintain

### Service Layer Pattern

**Location**: `kalamdb-core/src/services/`

**Pattern**:
```rust
pub struct SomeService {
    kalam_sql: Arc<KalamSql>,
    db: Arc<DB>,
    store: Arc<SomeTableStore>,
}

impl SomeService {
    pub fn new(kalam_sql: Arc<KalamSql>, db: Arc<DB>) -> Self {
        // Initialize
    }
    
    pub async fn create_something(&self, params: Params) -> Result<Response> {
        // Business logic
        // 1. Validate
        // 2. Transform
        // 3. Persist
        // 4. Return
    }
}
```

**Benefits**:
- Clear separation of concerns
- Testable in isolation
- Consistent API patterns

## Testing Organization

### Integration Tests

**Location**: `backend/tests/`

**Structure**:
```
tests/
├── integration/
│   ├── combined/         # Multi-feature tests
│   ├── tables/
│   │   ├── user/         # User table tests
│   │   ├── shared/       # Shared table tests
│   │   ├── stream/       # Stream table tests
│   │   └── system/       # System table tests
│   ├── flush/            # Flush operation tests
│   ├── jobs/             # Job management tests
│   ├── api/              # API endpoint tests
│   ├── storage_management/ # Storage tests
│   └── common/           # Test utilities
└── test_*.rs             # Top-level test harnesses
```

**Registration**: Integration tests are registered in `kalamdb-server/Cargo.toml`:
```toml
[[test]]
name = "test_name"
path = "../../tests/path/to/test.rs"
```

**Utilities**: Common test helpers in `tests/integration/common/`:
- `mod.rs` - Test server setup
- `fixtures.rs` - Common test data

### Unit Tests

**Location**: Co-located with source files in `#[cfg(test)] mod tests`

**Pattern**:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_something() {
        // Arrange
        // Act
        // Assert
    }
}
```

## Naming Conventions

### Files and Modules

- **Snake_case**: All file and module names
- **Descriptive**: `user_table_service.rs` not `service.rs`
- **Singular**: `user_table_store.rs` not `user_tables_store.rs`

### Types

- **PascalCase**: Structs, Enums, Traits
- **Descriptive suffixes**:
  - `Service` - Business logic services
  - `Store` - Storage layer implementations
  - `Provider` - DataFusion table providers
  - `Error` - Error types
  - `Id` - Type-safe wrappers for identifiers

### Functions

- **Snake_case**: All function names
- **Action verbs**: `create_table`, `drop_column`, `fetch_data`
- **Async suffix**: Optional `_async` for clarity in mixed sync/async contexts

### Constants

- **SCREAMING_SNAKE_CASE**: `DEFAULT_FLUSH_THRESHOLD`
- **Grouped by prefix**: `CF_PREFIX_USER`, `CF_PREFIX_SHARED`

## Error Handling

### Error Type Hierarchy

```
CommonError (kalamdb-commons)
    ├── InvalidInput
    ├── NotFound
    └── AlreadyExists

StorageError (kalamdb-store)
    ├── NotFound
    ├── IoError
    └── SerializationError

KalamDbError (kalamdb-core)
    ├── NotFound
    ├── InvalidOperation
    ├── InvalidSql
    ├── Other(String)
    └── From<anyhow::Error>
```

### Error Conversion Pattern

```rust
impl From<StorageError> for KalamDbError {
    fn from(err: StorageError) -> Self {
        match err {
            StorageError::NotFound(msg) => KalamDbError::NotFound(msg),
            StorageError::IoError(msg) => KalamDbError::Other(msg),
            // ...
        }
    }
}
```

## Documentation Standards

### Rustdoc Comments

**Module-level**:
```rust
//! Brief module description
//!
//! Detailed explanation of module purpose, architecture, and usage patterns.
//!
//! # Examples
//!
//! ```rust
//! // Example code
//! ```
```

**Function-level**:
```rust
/// Brief one-line description
///
/// Detailed explanation if needed.
///
/// # Arguments
///
/// * `param1` - Description
/// * `param2` - Description
///
/// # Returns
///
/// * `Ok(T)` - Success case description
/// * `Err(E)` - Error case description
///
/// # Examples
///
/// ```rust
/// // Example usage
/// ```
```

**Type-level**:
```rust
/// Brief description of the type
///
/// Detailed explanation of:
/// - Purpose and role in architecture
/// - Key invariants and constraints
/// - Usage patterns
pub struct TypeName { ... }
```

## Configuration

### Workspace Configuration

**Location**: `backend/Cargo.toml`

**Shared Dependencies**: Defined once in `[workspace.dependencies]`:
- `serde`, `tokio`, `actix-web`, `rocksdb`, `arrow`, `parquet`, `datafusion`

**Version Pinning**: Critical dependencies pinned to avoid conflicts:
- `chrono = "=0.4.39"` (compatibility with arrow-arith 52.2.0)

### Build Profiles

**Development** (`dev`):
- No optimizations
- Debug symbols included
- Fast compilation

**Release** (`release`):
```toml
[profile.release]
opt-level = 3           # Maximum optimization
lto = true              # Link-time optimization
codegen-units = 1       # Better optimization, slower build
strip = true            # Remove debug symbols
```

## Future Refactoring Opportunities

### Deferred Tasks

1. **T364-T367**: Split `kalamdb-sql/src/models.rs` into `models/` directory
   - Low priority - current monolithic structure works well
   - Would improve navigability for large teams

2. **T373-T375**: Refactor DDL parser to reduce duplication
   - Medium complexity
   - Consider when adding more DDL statements

3. **Additional opportunities**:
   - Extract common DataFusion provider patterns
   - Consolidate error message formatting
   - Shared test fixtures across integration tests

## References

- [ADR-014: Type-Safe Wrappers](../adrs/ADR-014-type-safe-wrappers.md)
- [ADR-015: Enum Usage Policy](../adrs/ADR-015-enum-usage-policy.md)
- [Testing Strategy](./testing-strategy.md)
- [Architecture Overview](./README.md)

## Changelog

- **2025-10-25**: Initial documentation created (US6 Code Quality)
- Document reflects codebase state at v0.1.0
