# System Model Consolidation - Current Status

**Date**: October 27, 2025  
**Branch**: `007-user-auth`  
**Status**: IN PROGRESS - Core consolidation complete, fixing remaining integration issues

## ✅ Completed Steps

### 1. Core Infrastructure
- ✅ Created `kalamdb-commons/src/models/system.rs` with all canonical system models:
  - `User` (with `UserId`, `Role`, `AuthType` enums)
  - `Job` (with `JobType`, `JobStatus` enums)
  - `LiveQuery`
  - `Namespace`
  - `SystemTable`
  - `Storage`
  - `TableSchema`
  - `InformationSchemaTable`
  - `UserTableCounter`

### 2. Deleted Duplicate Definitions
- ✅ Deleted `kalamdb-core/src/models/system.rs` (duplicated User, Job, etc.)
- ✅ Deleted `kalamdb-sql/src/models.rs` entirely (all models moved to commons)

### 3. Import Updates
- ✅ Updated `kalamdb-core/src/models/mod.rs` to re-export from commons
- ✅ Updated `kalamdb-sql/src/lib.rs` to re-export system models from commons
- ✅ Updated `kalamdb-sql/src/adapter.rs` to use commons models
- ✅ Updated `stream_table_service.rs` to import `TableSchema` from commons
- ✅ Updated `shared_table_service.rs` to import `TableSchema` from commons
- ✅ Fixed syntax error in `jobs_provider.rs` (missing closing brace)
- ✅ Removed deprecated `storage_location` module from catalog

## 🚧 Remaining Work

### 1. Update users_provider.rs

**File**: `backend/crates/kalamdb-core/src/tables/system/users_provider.rs`

**Issues Found**:
- ❌ References non-existent fields: `user.storage_mode`, `user.storage_id` (lines 145-146)
- ❌ Uses string literal for `Role` enum: `role: "user".to_string()` should be `role: Role::User` (line 60)
- ❌ Likely has additional issues with User model structure

**Required Actions**:
1. Remove `storage_mode` and `storage_id` field access (these don't exist in User model)
2. Change all string role assignments to use `Role` enum:
   - `"user"` → `Role::User`
   - `"service"` → `Role::Service`
   - `"dba"` → `Role::Dba`
   - `"system"` → `Role::System`
3. Ensure all User struct creation uses correct field types from `kalamdb_commons::system::User`

### 2. Fix Remaining Compilation Errors

Currently 159 compilation errors in `kalamdb-core` related to:
- User model field mismatches
- Role enum vs String type mismatches
- Missing field references

### 3. Test and Verify

After compilation errors are fixed:
```bash
cd backend
cargo build        # Must succeed with no errors
cargo test         # Must pass all tests
```

## Current User Model Definition

**Single Source of Truth**: `kalamdb-commons/src/models/system.rs` (lines 94-107)

```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct User {
    pub id: UserId,
    pub username: String,
    pub password_hash: String,
    pub role: Role,
    pub email: Option<String>,
    pub auth_type: AuthType,
    pub auth_data: Option<String>, // JSON blob for OAuth provider/subject
    pub api_key: Option<String>,   // API key for authentication
    pub created_at: i64,            // Unix timestamp in milliseconds
    pub updated_at: i64,            // Unix timestamp in milliseconds
    pub last_seen: Option<i64>,     // Unix timestamp in milliseconds (daily granularity)
    pub deleted_at: Option<i64>,    // Unix timestamp in milliseconds for soft delete
}
```

**Fields that DO NOT exist**:
- ❌ `storage_mode` (removed)
- ❌ `storage_id` (removed)

## Catalog Models vs System Table Models

**IMPORTANT DISTINCTION**: The models in `kalamdb-core/src/catalog/` are NOT duplicates - they serve different purposes:

### Catalog Models (Runtime Entities)
Located in: `backend/crates/kalamdb-core/src/catalog/`

**catalog::Namespace** - Runtime catalog entity
- `name: NamespaceId`
- `created_at: DateTime<Utc>`
- `options: HashMap<String, serde_json::Value>`
- `table_count: u32`
- **Purpose**: In-memory catalog management, validation logic, domain methods

**catalog::TableMetadata** - Runtime table metadata
- `table_name: TableName`
- `table_type: TableType`
- `namespace: NamespaceId`
- `created_at: DateTime<Utc>`
- `storage_location: String`
- `flush_policy: FlushPolicy`
- `schema_version: u32`
- `deleted_retention_hours: Option<u32>`
- **Purpose**: In-memory table metadata, column family naming, schema paths

### System Table Models (Persisted Rows)
Located in: `backend/crates/kalamdb-commons/src/models/system.rs`

**system::Namespace** - System table row
- `namespace_id: NamespaceId`
- `name: String`
- `owner_id: UserId`
- `created_at: i64` (milliseconds)
- `options: Option<String>` (JSON string)
- `table_count: i32`
- **Purpose**: Persisted in `system.namespaces` table (RocksDB)

**system::SystemTable** - System table row
- `table_id: String`
- `table_name: TableName`
- `namespace: NamespaceId`
- `table_type: TableType`
- `created_at: i64`
- `storage_location: String`
- `storage_id: Option<StorageId>`
- `use_user_storage: bool`
- `flush_policy: String` (JSON)
- `schema_version: i32`
- `deleted_retention_hours: i32`
- **Purpose**: Persisted in `system.tables` table (RocksDB)

**Why Both Exist**:
- Catalog models: Rich domain logic, validation, runtime operations
- System models: Simple DTOs for database persistence (bincode serialization)
- Converting between them is intentional (catalog ↔ system table)

**Action**: No consolidation needed for catalog models - they are architecturally separate.

## Spec.md Integration Status

✅ **Completed**: Added "User Story 9 - System Model Consolidation" to `specs/007-user-auth/spec.md`

**Location**: Lines 319-388

**Key Content**:
- Problem statement (duplicate models in 3+ locations)
- Solution (single source of truth in kalamdb-commons)
- Acceptance scenarios (10 test cases)
- Current state assessment
- Migration checklist

## Next Actions (Priority Order)

1. **Fix users_provider.rs** (high priority - blocking compilation)
   - Remove `storage_mode` and `storage_id` field accesses
   - Convert all role strings to `Role` enum values
   - Update User struct initialization to match commons model

2. **Verify all imports are correct** (medium priority)
   - Search for any remaining `use kalamdb_sql::models::` imports
   - Ensure all code uses `kalamdb_commons::system::*` or crate re-exports

3. **Run full build and test suite** (verification)
   ```bash
   cd backend
   cargo build
   cargo test
   cargo test --test test_api_key_auth
   cargo test --test test_combined_data_integrity
   ```

4. **Update documentation** (final step)
   - Update `.github/copilot-instructions.md` with consolidation notes
   - Mark consolidation as complete in SYSTEM_MODEL_CONSOLIDATION.md

## Impact on 007-user-auth Feature

This consolidation work is **CRITICAL** for the user authentication feature because:

1. **User model is foundation**: Authentication logic depends on `User` struct
2. **Type safety**: Using strongly-typed `Role`, `AuthType` enums prevents errors
3. **No duplication**: Single source of truth prevents divergence
4. **Clean imports**: All auth code will import from `kalamdb_commons::system::User`

Once this consolidation is complete, authentication implementation can proceed cleanly without model conflicts.

## Files Modified (Summary)

### Created
- `kalamdb-commons/src/models/system.rs` (canonical models)
- `docs/architecture/SYSTEM_MODEL_CONSOLIDATION_STATUS.md` (this file)

### Deleted
- `kalamdb-core/src/models/system.rs`
- `kalamdb-sql/src/models.rs`

### Modified
- `kalamdb-commons/src/lib.rs` (added system module export)
- `kalamdb-core/src/models/mod.rs` (re-export from commons)
- `kalamdb-core/src/catalog/mod.rs` (removed storage_location)
- `kalamdb-core/src/tables/system/jobs_provider.rs` (syntax fix)
- `kalamdb-core/src/services/stream_table_service.rs` (import from commons)
- `kalamdb-core/src/services/shared_table_service.rs` (import from commons)
- `kalamdb-sql/src/lib.rs` (re-export from commons)
- `kalamdb-sql/src/adapter.rs` (import from commons)
- `specs/007-user-auth/spec.md` (added User Story 9)

### Needs Fixing
- ❌ `kalamdb-core/src/tables/system/users_provider.rs` (field mismatches)
- ❌ Various other files with compilation errors (159 total)

---

**Conclusion**: Core consolidation infrastructure is complete. The remaining work is fixing integration points that referenced old model structures. Once `users_provider.rs` is fixed and compilation succeeds, we can proceed with authentication implementation using the clean, consolidated models.
