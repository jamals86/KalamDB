# Architectural Consistency Updates Summary

**Date**: October 27, 2025  
**Feature**: User Authentication (007-user-auth)  
**Purpose**: Ensure all implementation follows existing KalamDB architectural patterns

---

## Changes Made

### 1. spec.md - Added Architectural Consistency Requirements Section ✅

Added new section "**Architectural Consistency Requirements**" with 6 critical requirements:

#### FR-ARCH-001: SQL Command Parsing
- **Requirement**: User management commands MUST follow `ExtensionStatement` enum pattern
- **Reference**: `backend/crates/kalamdb-sql/src/parser/extensions.rs`
- **Pattern**: Same as `CreateStorageStatement`, `FlushTableStatement`
- **Key Points**:
  - Add to `ExtensionStatement` enum
  - Implement `parse()` method for each statement
  - Return `Result<Statement, String>`
  - Export from `parser/mod.rs`

#### FR-ARCH-002: Background Job Management
- **Requirement**: Cleanup jobs MUST follow existing job infrastructure
- **Reference**: `backend/crates/kalamdb-core/src/jobs/`
- **Pattern**: Same as `RetentionPolicy` in `retention.rs`
- **Key Points**:
  - Use `JobManager` trait for execution
  - Implement using `JobExecutor` pattern
  - Register in `system.jobs` table
  - Use `TokioJobManager::start_job()`
  - Follow lifecycle: created → running → completed/failed/cancelled

#### FR-ARCH-003: Storage Layer Organization
- **Requirement**: `system.users` storage MUST follow existing store patterns
- **Reference**: `backend/crates/kalamdb-store/src/user_table_store.rs`
- **Pattern**: Same as `UserTableStore`, `SharedTableStore`
- **Key Points**:
  - Create dedicated `users.rs` module
  - Use RocksDB column family: `system_users`
  - Implement CRUD functions
  - Use `ensure_cf()` pattern for lazy creation
  - Follow `create_column_family()` pattern
  - Return `Result<T, anyhow::Error>`

#### FR-ARCH-004: SQL Execution Flow
- **Requirement**: User management execution MUST follow existing executor patterns
- **Reference**: `backend/crates/kalamdb-core/src/sql/`
- **Key Points**:
  - Add executor in `sql/` directory
  - Integrate with `ExecutionContext`
  - Authorization checks before execution
  - Return via `system.jobs` for async ops
  - Return `Result<ExecutionResult, KalamDbError>`

#### FR-ARCH-005: Data Model Consistency
- **Requirement**: `system.users` table MUST follow schema patterns
- **Reference**: `system.tables`, `system.storages`, `system.jobs`
- **Key Points**:
  - Consistent column naming: `created_at`, `updated_at`, `deleted_at`
  - Timestamps in milliseconds
  - JSON metadata in `metadata` column
  - Enum serialization as lowercase strings
  - Soft delete: `deleted_at IS NULL` for active

#### FR-ARCH-006: Error Handling
- **Requirement**: Errors MUST use `KalamDbError` enum
- **Reference**: `backend/crates/kalamdb-core/src/error.rs`
- **Key Points**:
  - Add new variants: `AuthenticationFailed`, `AuthorizationFailed`, `InvalidCredentials`
  - Follow API handler error response pattern
  - Structured JSON: `{"error": "...", "message": "...", "request_id": "..."}`
  - Use `anyhow::Context` for propagation

---

### 2. tasks.md - Updated Tasks to Follow Architectural Patterns ✅

#### Phase 2: Foundational Storage Layer (T020-T031)
**Updated to follow UserTableStore pattern**:

- T020: Create `system_users` column family in `ColumnFamilyNames` constants (follow existing pattern)
- T021: Create `UsersStore` struct with `db: Arc<DB>` and `ensure_cf()` method
- T022: Implement `User` struct with serde (13 fields)
- T023-T029: CRUD functions following `db.get_cf()`, `db.put_cf()`, `IteratorMode` patterns
- T030: Create `username_index` column family (secondary index pattern)
- T031: Export `UsersStore` from `lib.rs` (module export pattern)

**Before**: Generic "implement functions"  
**After**: Specific patterns to follow with file paths and method names

#### Phase 5.5: SQL Parser Extensions (T084A-T084Q)
**Updated to follow ExtensionStatement pattern**:

- T084A-C: Create `CreateUserStatement`, `AlterUserStatement`, `DropUserStatement` in `ddl/user_commands.rs`
- T084D-E: Add to `ExtensionStatement` enum, update `parse()` method in `parser/extensions.rs`
- T084F: Export from `ddl/mod.rs`
- T084G-K: Create `user_executor.rs` following `table_executor.rs` pattern
- T084L-Q: Unit tests in `tests/parser/user_commands_tests.rs`

**Before**: Generic "add parser" tasks  
**After**: Specific files, structs, and patterns matching existing DDL commands

#### Phase 13: Scheduled Cleanup Job (T171-T177)
**Updated to follow RetentionPolicy pattern**:

- T171: Create `UserCleanupJob` struct following `RetentionPolicy` pattern from `retention.rs`
- T172: Implement `UserCleanupConfig` following `RetentionConfig` pattern
- T173: Implement `enforce()` method (same signature as `RetentionPolicy::enforce()`)
- T174: Export from `jobs/mod.rs` (add to `pub use` statements)
- T175: Integrate with `TokioJobManager::start_job()`, register in `system.jobs`
- T176-177: Config and tests

**Before**: Generic "create job" tasks  
**After**: Specific struct names, methods, and patterns matching existing job infrastructure

#### Phase 13: Database Indexes (T178-T182)
**Updated to follow existing index patterns**:

- T178: Add index constants to `ColumnFamilyNames` (follow naming convention)
- T179-182: Create secondary index column families using `db.put_cf()` pattern

**Before**: Generic "create indexes"  
**After**: Specific column family creation following existing index patterns

---

### 3. Task Count Adjustments

**Previous**: 209 tasks  
**Current**: 213 tasks (+4 tasks)

**Changes**:
- Phase 2: 34 → 32 tasks (consolidated, made more specific)
- Phase 5.5: 16 → 18 tasks (added more granular parser tasks)
- Phase 13: 42 → 26 tasks (consolidated, removed duplicates, made more specific)
- **Net change**: More focused tasks with clear architectural guidance

---

## Architectural Patterns Referenced

### 1. SQL Parser Pattern (from `extensions.rs`)
```rust
pub enum ExtensionStatement {
    CreateStorage(CreateStorageStatement),
    FlushTable(FlushTableStatement),
    CreateUser(CreateUserStatement),  // NEW
}

impl CreateUserStatement {
    pub fn parse(sql: &str) -> Result<Self, String> {
        // Pattern: uppercase check, parse fields, return struct
    }
}
```

### 2. Job Management Pattern (from `retention.rs`)
```rust
pub struct UserCleanupJob {
    users_provider: Arc<UsersTableProvider>,
    config: UserCleanupConfig,
}

impl UserCleanupJob {
    pub fn enforce(&self) -> Result<usize, KalamDbError> {
        // Pattern: find expired, delete cascade, return count
    }
}
```

### 3. Storage Layer Pattern (from `user_table_store.rs`)
```rust
pub struct UsersStore {
    db: Arc<DB>,
}

impl UsersStore {
    fn ensure_cf(&self) -> Result<&ColumnFamily> {
        // Pattern: check exists, create if missing, return handle
    }
    
    pub fn create_user(&self, user: User) -> Result<()> {
        // Pattern: ensure_cf(), db.put_cf(), return Result
    }
}
```

---

## Validation Checklist

- ✅ All SQL parser tasks reference `ExtensionStatement` pattern
- ✅ All job tasks reference `JobManager` + `RetentionPolicy` pattern
- ✅ All storage tasks reference `UserTableStore` pattern with column families
- ✅ All executor tasks reference existing SQL executor patterns
- ✅ All index tasks reference existing secondary index patterns
- ✅ Error handling tasks reference `KalamDbError` enum
- ✅ All tasks include specific file paths
- ✅ All tasks include method/struct names to match
- ✅ All tasks specify patterns to follow with code references

---

## Benefits of Architectural Consistency

1. **Reduced Implementation Risk**: Clear patterns to follow reduce mistakes
2. **Faster Code Review**: Reviewers can verify against referenced patterns
3. **Better Maintainability**: Consistent code style across entire codebase
4. **Easier Onboarding**: New developers see pattern references in tasks
5. **Quality Gates**: Architectural compliance is now a requirement (FR-ARCH-001 through FR-ARCH-006)

---

## Key Takeaways

### For Implementation
1. **Before coding**: Read the referenced pattern file (e.g., `extensions.rs`, `retention.rs`)
2. **During coding**: Match struct names, method signatures, error handling
3. **After coding**: Verify against pattern checklist in spec.md
4. **Code review**: Reviewers check FR-ARCH-001 through FR-ARCH-006 compliance

### For Testing
- Integration tests must follow existing test patterns in `backend/tests/`
- Unit tests must follow existing test organization in `tests/parser/`, `tests/`
- Test helpers must use patterns from `tests/common/`

### For Documentation
- API contracts must match existing contract patterns
- SQL syntax must match existing DDL command documentation
- Error responses must match existing error formats

---

## Status

✅ **Complete**: All architectural consistency requirements documented  
✅ **Complete**: All tasks updated with specific patterns to follow  
✅ **Complete**: Validation checklist created  
⏭️ **Next**: Begin Phase 1 implementation following documented patterns
