# ADR-004: kalamdb-commons Crate for Shared Types

**Status**: Accepted  
**Date**: 2025-10-21  
**Decision Makers**: KalamDB Team  
**Related**: Phase 2 (Foundational) - System Improvements Feature

## Context

KalamDB has multiple crates (kalamdb-core, kalamdb-sql, kalamdb-store, kalamdb-api, kalamdb-live) that need to share common types, constants, and configuration structures. Without a shared crate, we face several problems:

1. **Circular Dependencies**: kalamdb-core depends on kalamdb-sql for DDL types, but kalamdb-sql needs core types like UserId and TableName
2. **Code Duplication**: System table names, column family constants, and type definitions are duplicated across crates
3. **Type Safety**: String-based identifiers (user IDs, namespace IDs, table names) can be accidentally mixed, causing runtime errors
4. **Maintenance Burden**: Updating a constant or type requires changes in multiple crates

## Decision

We will create a new `kalamdb-commons` crate with the following characteristics:

### Zero External Dependencies
The crate has **no external dependencies** (not even serde or thiserror). This prevents any possibility of circular dependencies and ensures it can be imported by any other crate without conflicts.

### Type-Safe Wrappers
Provide newtype wrappers around String for compile-time type safety:
```rust
pub struct UserId(String);
pub struct NamespaceId(String);
pub struct TableName(String);
pub enum TableType { User, Shared, Stream }
```

### Centralized Constants
All system-wide constants in one place:
```rust
pub const SYSTEM_TABLES: SystemTableNames = ...;
pub const COLUMN_FAMILIES: ColumnFamilyNames = ...;
pub const SYSTEM_COLUMNS: SystemColumnNames = ...;
```

### Shared Configuration Models
Configuration structures used across crates:
```rust
pub enum FlushPolicy { Rows(u64), Interval(u64), ... }
pub struct StorageLocation { ... }
pub struct RetentionPolicy { ... }
pub struct StreamConfig { ... }
```

### Basic Error Types
Simple error enum without external dependencies:
```rust
pub enum CommonError {
    InvalidInput(String),
    NotFound(String),
    AlreadyExists(String),
    ...
}
```

## Consequences

### Positive
- **No Circular Dependencies**: All other crates can safely depend on kalamdb-commons
- **Type Safety**: Compile-time prevention of identifier mixing (UserId vs NamespaceId)
- **Single Source of Truth**: Constants defined once, used everywhere
- **Reduced Coupling**: Crates only depend on commons, not on each other
- **Better Maintainability**: Changes to shared types happen in one place
- **Clear Ownership**: Commons crate has clear responsibility and minimal scope

### Negative
- **Additional Crate**: Adds one more crate to the workspace (mitigated by clear benefits)
- **Limited Features**: No serde, no external dependencies (mitigated by simplicity)
- **Conversion Overhead**: External crates must convert to/from commons types (mitigated by From/Into implementations)

### Neutral
- **Migration Path**: Existing code must be updated to use commons types (one-time cost)
- **Testing**: Commons crate needs unit tests for all types (minimal effort due to simplicity)

## Alternatives Considered

### 1. Keep Status Quo (Duplicated Types)
**Rejected**: Leads to maintenance burden and type safety issues

### 2. Consolidate All Crates
**Rejected**: Would create a monolithic crate and lose separation of concerns

### 3. Use Feature Flags
**Rejected**: Adds complexity and doesn't solve circular dependency problem

### 4. External Crate Dependencies
**Rejected**: Would allow circular dependencies through transitive dependencies

## Implementation Notes

### Crate Structure
```
kalamdb-commons/
├── Cargo.toml (NO dependencies)
└── src/
    ├── lib.rs (module exports)
    ├── models.rs (type-safe wrappers)
    ├── constants.rs (system constants)
    ├── errors.rs (basic error types)
    └── config.rs (configuration models)
```

### Dependency Graph (After)
```
kalamdb-commons (no dependencies)
    ↑
    ├─ kalamdb-sql
    ├─ kalamdb-store
    ├─ kalamdb-core (also depends on sql, store)
    ├─ kalamdb-api (depends on core)
    ├─ kalamdb-live (depends on core)
    └─ kalamdb-server (depends on api, core)
```

### Migration Strategy
1. ✅ Create kalamdb-commons crate with zero dependencies
2. ✅ Implement type-safe wrappers with From/Into/AsRef traits
3. ✅ Define all system constants
4. ✅ Add kalamdb-commons to workspace
5. ✅ Add dependency in all backend crates
6. ⏳ Gradually migrate existing code to use commons types
7. ⏳ Remove duplicated constants and types from other crates

## References

- [Rust API Guidelines: C-REEXPORT](https://rust-lang.github.io/api-guidelines/interoperability.html#crate-level-docs-explicitly-document-all-intended-uses-c-reexport)
- [Cargo Book: Workspace](https://doc.rust-lang.org/cargo/reference/workspaces.html)
- Feature Spec: `specs/004-system-improvements-and/spec.md` (User Story 6, FR-077 to FR-082)

## Validation

### Success Criteria
- [X] kalamdb-commons compiles with zero external dependencies
- [X] All backend crates successfully depend on kalamdb-commons
- [X] Type-safe wrappers prevent identifier mixing at compile time
- [X] Constants are defined once and reused everywhere
- [ ] All existing code migrated to use commons types (ongoing)
- [ ] Binary size impact <1% (to be measured after full migration)

### Testing
- [X] Unit tests for type-safe wrapper conversions
- [X] Unit tests for constant values
- [X] Unit tests for error type creation
- [X] Unit tests for configuration models
- [ ] Integration tests verify no circular dependencies (cargo build success)

---

**Decision Status**: ✅ Accepted and Implemented (Phase 2)  
**Last Updated**: 2025-10-21
