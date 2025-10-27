# System Table Model Consolidation - Architecture Fix

**Date**: October 27, 2025  
**Issue**: Duplicate system table models scattered across multiple crates  
**Status**: IN PROGRESS - Critical architectural refactoring

## Problem Statement

We have MULTIPLE definitions of the same system table models across different crates:

### Current Duplicates

1. **Job Model** (3 locations):
   - `kalamdb-core/src/models/system.rs` → `Job` (with `JobType` and `JobStatus` enums)
   - `kalamdb-core/src/tables/system/jobs_provider.rs` → `JobRecord` (with String fields)
   - `kalamdb-sql/src/models.rs` → `Job` (with String fields)

2. **LiveQuery Model** (3 locations):
   - `kalamdb-core/src/models/system.rs` → `LiveQuery` (with typed IDs)
   - `kalamdb-core/src/tables/system/live_queries_provider.rs` → `LiveQueryRecord` (with String fields)
   - `kalamdb-sql/src/models.rs` → `LiveQuery` (with String fields)

3. **User Model** (2 locations):
   - `kalamdb-core/src/models/system.rs` → `User` (with `UserId`, `Role`, `AuthType` enums)
   - `kalamdb-sql/src/models.rs` → `User` (with String fields)

4. **Namespace Model** (3 locations):
   - `kalamdb-core/src/models/system.rs` → `Namespace` (with typed IDs)
   - `kalamdb-core/src/catalog/namespace.rs` → `Namespace` (catalog entity?)
   - `kalamdb-sql/src/models.rs` → `Namespace` (with String fields)

5. **SystemTable Model** (2 locations):
   - `kalamdb-core/src/models/system.rs` → `SystemTable`
   - `kalamdb-sql/src/models.rs` → `Table`

## Solution: Single Source of Truth in kalamdb-commons

### ✅ Completed Steps

1. **Created `kalamdb-commons/src/models/system.rs`** with canonical models:
   - `User` (with `UserId`, `Role`, `AuthType`)
   - `Job` (with `JobType`, `JobStatus`, `NamespaceId`, `TableName`)
   - `Namespace` (with `NamespaceId`, `UserId`)
   - `SystemTable` (with `TableName`, `NamespaceId`, `TableType`, `StorageId`)
   - `LiveQuery` (with `UserId`, `NamespaceId`, `TableName`)
   - `InformationSchemaTable`
   - `UserTableCounter`

2. **Updated `kalamdb-commons/Cargo.toml`**:
   - Added `bincode = "1.3"` (for RocksDB serialization)
   - Added `chrono = "0.4"` (for `Job::new()` timestamp methods)
   - Enabled default `serde` feature

3. **Updated `kalamdb-commons/src/lib.rs`**:
   - Exported `system` module
   - Re-exported as `pub use models::system;`

4. **Deleted `kalamdb-core/src/models/system.rs`**

5. **Updated `kalamdb-core/src/models/mod.rs`**:
   - Changed from `pub mod system;` to `pub use kalamdb_commons::system::{...};`

6. **Started updating `jobs_provider.rs`**:
   - Removed `JobRecord` struct definition
   - Added `use kalamdb_commons::system::Job;`
   - Updated `convert_sql_job()` to convert kalamdb_sql::Job → kalamdb_commons::system::Job
   - Updated `insert_job()` to accept `Job` parameter

### 🚧 Remaining Work

#### 1. Complete jobs_provider.rs Migration

**File**: `backend/crates/kalamdb-core/src/tables/system/jobs_provider.rs`

**Tasks**:
- [ ] Update `update_job()` method signature: `pub fn update_job(&self, job: Job)`
- [ ] Update `get_job()` return type: `Result<Option<Job>, ...>`
- [ ] Update `cancel_job()` to use `JobStatus` enum: `if job.status == JobStatus::Running`
- [ ] Update all test functions to use `Job::new()` builder pattern
- [ ] Update exports in `mod.rs`: change `pub use jobs_provider::{Job, JobsTableProvider};`

#### 2. Migrate live_queries_provider.rs

**File**: `backend/crates/kalamdb-core/src/tables/system/live_queries_provider.rs`

**Tasks**:
- [ ] Remove `LiveQueryRecord` struct definition (lines 26-42)
- [ ] Add `use kalamdb_commons::system::LiveQuery;`
- [ ] Replace all `LiveQueryRecord` → `LiveQuery` throughout file (~25 occurrences)
- [ ] Update method signatures:
  - `insert_live_query(&self, live_query: LiveQuery)`
  - `update_live_query(&self, live_query: LiveQuery)`
  - `get_live_query(&self, ...) -> Result<Option<LiveQuery>, ...>`
  - `get_by_user_id(&self, ...) -> Result<Vec<LiveQuery>, ...>`
- [ ] Update exports in `mod.rs`: change `pub use live_queries_provider::{LiveQuery, LiveQueriesTableProvider};`

#### 3. Clean Up kalamdb-sql/src/models.rs

**File**: `backend/crates/kalamdb-sql/src/models.rs`

**Tasks**:
- [ ] **DELETE** these duplicate structs:
  - `User` (lines 8-23)
  - `LiveQuery` (lines 26-41)
  - `Job` (lines 58-78)
  - `Namespace` (lines 82-89)
  - `Table` (lines 92-104)
- [ ] **KEEP** these models (not duplicated):
  - `StorageLocation` (or mark as deprecated if Storage replaces it)
  - `TableSchema`
  - `Storage`
- [ ] Add imports for system models:
  ```rust
  pub use kalamdb_commons::system::{User, Job, LiveQuery, Namespace, SystemTable};
  ```
- [ ] Update internal `KalamSql` methods to use common models

#### 4. Update All Import Statements

**Search and replace across entire backend/**:

```bash
# User model
use kalamdb_sql::models::User -> use kalamdb_commons::system::User

# Job model  
use kalamdb_sql::models::Job -> use kalamdb_commons::system::Job
use crate::tables::system::JobRecord -> use kalamdb_commons::system::Job

# LiveQuery model
use kalamdb_sql::models::LiveQuery -> use kalamdb_commons::system::LiveQuery
use crate::tables::system::LiveQueryRecord -> use kalamdb_commons::system::LiveQuery

# Namespace model
use kalamdb_sql::models::Namespace -> use kalamdb_commons::system::Namespace

# Table model
use kalamdb_sql::models::Table -> use kalamdb_commons::system::SystemTable
```

**Files to update** (14 known occurrences):
- `backend/src/lifecycle.rs` (line 369)
- `backend/src/commands/create_user.rs` (line 8)
- `backend/crates/kalamdb-core/src/sql/executor.rs` (multiple)
- `backend/crates/kalamdb-core/src/services/*.rs` (multiple)
- `backend/crates/kalamdb-core/src/scheduler.rs`
- `backend/crates/kalamdb-core/src/jobs/retention.rs`

#### 5. Reconcile Namespace Models

**Issue**: Two different `Namespace` structs exist:
- `kalamdb_commons::system::Namespace` (system table row)
- `kalamdb_core::catalog::namespace::Namespace` (catalog entity)

**Decision needed**:
- **Option A**: Catalog entity is different → Rename to `NamespaceCatalogEntry`
- **Option B**: They're the same → Delete catalog version, use system model

**Investigation**:
- Check if `catalog::Namespace` has different fields/purpose
- Check if `DateTime<Utc>` vs `i64` timestamps are incompatible
- Determine if catalog needs richer domain logic

#### 6. Update mod.rs Exports

**File**: `backend/crates/kalamdb-core/src/tables/system/mod.rs`

**Current**:
```rust
pub use jobs_provider::{JobRecord, JobsTableProvider};
pub use live_queries_provider::{LiveQueriesTableProvider, LiveQueryRecord};
```

**New**:
```rust
pub use kalamdb_commons::system::{Job, LiveQuery};
pub use jobs_provider::JobsTableProvider;
pub use live_queries_provider::LiveQueriesTableProvider;
```

#### 7. Update Serialization Logic

**Critical**: Ensure bincode compatibility across all usage:

- RocksDB storage in `kalamdb-store` must use `bincode::serialize()`
- DataFusion table providers must convert to Arrow format
- API responses use JSON via Serde

**Test files to verify**:
- `backend/tests/test_api_key_auth.rs`
- `backend/tests/test_combined_data_integrity.rs`
- `backend/crates/kalamdb-core/src/tables/system/jobs_provider.rs` (tests)
- `backend/crates/kalamdb-core/src/tables/system/live_queries_provider.rs` (tests)

#### 8. Build and Test

```bash
# From workspace root
cd backend
cargo build

# Run all tests
cargo test

# Run specific integration tests
cargo test --test test_api_key_auth
cargo test --test test_combined_data_integrity
```

#### 9. Update Documentation

**Files to update**:
- `.github/copilot-instructions.md`: Add note about system models in kalamdb-commons
- `docs/architecture/CODE_ORGANIZATION.md`: Document single source of truth
- `backend/crates/kalamdb-commons/README.md`: Document system module

## Implementation Order

**CRITICAL**: Follow this order to minimize breakage:

1. ✅ Create kalamdb-commons/src/models/system.rs
2. ✅ Update kalamdb-commons exports
3. ✅ Delete kalamdb-core/src/models/system.rs
4. ✅ Update kalamdb-core/src/models/mod.rs
5. 🚧 Complete jobs_provider.rs migration
6. ⬜ Complete live_queries_provider.rs migration
7. ⬜ Update all usage sites (14+ files)
8. ⬜ Delete duplicates from kalamdb-sql/src/models.rs
9. ⬜ Reconcile Namespace models
10. ⬜ Run cargo build and fix compilation errors
11. ⬜ Run cargo test and fix test failures
12. ⬜ Update documentation

## Testing Strategy

### Unit Tests

Each system model has serialization tests in `kalamdb-commons/src/models/system.rs`:
- `test_user_serialization()`
- `test_job_serialization()`
- `test_namespace_serialization()`
- `test_live_query_serialization()`
- etc.

### Integration Tests

Run existing tests to verify no regression:
```bash
cargo test --test test_api_key_auth
cargo test --test test_combined_data_integrity
cargo test --test test_row_count_behavior
cargo test --test test_soft_delete
cargo test --test test_stream_ttl
```

### Table Provider Tests

Each provider has embedded tests:
- `backend/crates/kalamdb-core/src/tables/system/jobs_provider.rs::tests`
- `backend/crates/kalamdb-core/src/tables/system/live_queries_provider.rs::tests`

## Risks and Mitigation

### Risk 1: Bincode Deserialization Failure

**Issue**: Existing RocksDB data may not deserialize with new models

**Mitigation**:
- System models have same field order and types
- Only String→Enum changes (both are String-serializable)
- Test with production-like data first

### Risk 2: Type Conversion Overhead

**Issue**: Converting between kalamdb_sql::Job and kalamdb_commons::system::Job

**Mitigation**:
- Conversion only happens at storage boundary
- Most code uses common models directly
- Consider migrating kalamdb_sql to use common models internally

### Risk 3: Circular Dependencies

**Issue**: kalamdb-commons depends on nothing, but imports from it everywhere

**Mitigation**:
- kalamdb-commons has zero external dependencies (only serde, bincode)
- All other crates can safely depend on it
- This is the correct dependency direction

## Success Criteria

- ✅ Only ONE definition per system table model (in kalamdb-commons)
- ✅ All crates import from kalamdb_commons::system::*
- ✅ `cargo build` succeeds without errors
- ✅ All tests pass (`cargo test`)
- ✅ No RocksDB deserialization errors
- ✅ Documentation updated with new architecture

## Follow-Up Work

After consolidation is complete:

1. **Migrate kalamdb-sql internally**: Make kalamdb_sql use common models instead of maintaining parallel String-based versions
2. **Consolidate TableSchema and Storage**: Check if these models also have duplicates
3. **Add runtime validation**: Ensure String→Enum parsing has fallback values
4. **Performance profiling**: Measure conversion overhead (should be negligible)

## Questions for Resolution

1. **Namespace models**: Are `catalog::Namespace` and `system::Namespace` the same?
2. **StorageLocation**: Is this duplicate of `Storage` model? Can we remove it?
3. **Table vs SystemTable**: Should we rename `SystemTable` to `Table` for simplicity?
4. **Migration strategy**: Do we need data migration for existing RocksDB entries?

---

**Next Steps**: 
1. Complete jobs_provider.rs migration
2. Run partial build to catch compilation errors early
3. Update live_queries_provider.rs
4. Update all import statements
5. Full build and test cycle
