# Phase 8 Progress Summary: Legacy Services Removal

**Date**: 2025-11-05  
**Status**: 🔄 **PARTIALLY COMPLETE** (NamespaceService ✅, 3 table services remaining)

## Overview

Phase 8 aims to remove the legacy service layer (NamespaceService, UserTableService, SharedTableService, StreamTableService, TableDeletionService) by inlining their business logic directly into DDL handlers and using providers from AppContext.

## ✅ Completed: NamespaceService Replacement

### Changes Made

**1. Updated `handlers/ddl.rs`**:
- **Removed import**: `use crate::services::NamespaceService;`
- **Added import**: `use kalamdb_commons::system::Namespace;`
- **Updated `execute_create_namespace()`**:
  - Changed parameter from `namespace_service: &NamespaceService` to `namespaces_provider: &Arc<crate::tables::system::NamespacesTableProvider>`
  - Inlined validation: `Namespace::validate_name(name)?`
  - Inlined existence check: `namespaces_provider.get_namespace(&namespace_id)?`
  - Inlined creation: `namespaces_provider.create_namespace(namespace)?`
- **Updated `execute_drop_namespace()`**:
  - Changed parameter to use `namespaces_provider`
  - Inlined existence check and table count validation: `namespace.can_delete()`
  - Inlined deletion: `namespaces_provider.delete_namespace(&namespace_id)?`

**2. Updated `sql/executor/mod.rs` Routing**:
```rust
SqlStatement::CreateNamespace => {
    let namespaces_provider = self.app_context.system_tables().namespaces();
    DDLHandler::execute_create_namespace(&namespaces_provider, session, sql, exec_ctx).await
},
SqlStatement::DropNamespace => {
    let namespaces_provider = self.app_context.system_tables().namespaces();
    DDLHandler::execute_drop_namespace(&namespaces_provider, session, sql, exec_ctx).await
},
```

**3. Updated Tests in `handlers/ddl.rs`**:
- Replaced `create_test_namespace_service()` with `get_namespaces_provider()` 
- Updated all 5 test methods to use provider instead of service
- Tests now get provider via: `test_helpers::get_app_context().system_tables().namespaces()`

### Business Logic Inlined

The NamespaceService was a thin wrapper. All logic successfully inlined:
- **Validation**: `Namespace::validate_name()`
- **Existence checks**: Provider's `get_namespace()` method
- **Table count validation**: `namespace.can_delete()` 
- **CRUD operations**: Provider's `create_namespace()` and `delete_namespace()`

### Pattern Established

This provides the template for remaining services:
1. Replace service parameter with provider from AppContext
2. Inline validation and business logic
3. Use provider methods for data operations
4. Update routing to get provider from `app_context.system_tables()`
5. Update tests to use provider

## 🚧 Remaining Work: Table Creation Services

### UserTableService.create_table()

**Location**: `services/user_table_service.rs:67-131`

**Business Logic to Migrate** (~150 lines):
1. **Validation**:
   - `validate_table_name()` - check table name validity
   - `table_exists()` check with IF NOT EXISTS handling
   
2. **Schema Transformations**:
   - `inject_auto_increment_field()` - adds `id INT64` column if missing
   - `inject_system_columns()` - adds `_updated TIMESTAMP` and `_deleted BOOLEAN`
   
3. **Column Defaults**:
   - Inject `DEFAULT SNOWFLAKE_ID()` for auto-generated id column
   
4. **Persistence**:
   - `save_table_definition()` - save to information_schema_tables (atomic write, ~80 lines)
   
5. **RocksDB Setup**:
   - `user_table_store.create_column_family()` - create CF for table

**Current Usage** (line 370 in ddl.rs):
```rust
user_table_service.create_table(stmt)?;
```

**Migration Strategy**:
- Create helper methods in DDLHandler or inline directly in `create_user_table()`
- Schema transformations can be extracted as private methods
- Replace service call with direct provider operations + helper methods

### SharedTableService.create_table()

**Location**: `services/shared_table_service.rs`

**Business Logic** (similar pattern to UserTableService):
- Table name validation
- Existence checking with IF NOT EXISTS
- Schema injection (id, _updated, _deleted columns)
- Default column injection
- save_table_definition() 
- create_column_family()

**Current Usage** (line 524 in ddl.rs):
```rust
let was_created = shared_table_service.create_table(stmt)?;
```

**Migration Strategy**: Same as UserTableService

### StreamTableService.create_table()

**Location**: `services/stream_table_service.rs`

**Business Logic** (simpler than user/shared):
- Table name validation
- Existence checking
- **NO** system columns (_updated, _deleted) - streams are ephemeral
- TTL/retention configuration
- save_table_definition()
- create_column_family()

**Current Usage** (line 447 in ddl.rs):
```rust
stream_table_service.create_table(stmt)?;
```

**Migration Strategy**: Similar to others, but skip system column injection

### TableDeletionService

**Investigation Needed**: Grep for usage in codebase

```bash
grep -r "TableDeletionService::" backend/
```

If unused, can be deleted immediately. If used, inline logic into DROP TABLE handler.

## 📋 Detailed Migration Steps

### Step 1: Inline UserTableService (T108)

1. **Extract helper methods** in `handlers/ddl.rs`:
```rust
impl DDLHandler {
    fn validate_table_name(name: &str) -> Result<(), KalamDbError> { ... }
    fn inject_auto_increment_field(schema: Arc<Schema>) -> Result<Arc<Schema>, KalamDbError> { ... }
    fn inject_system_columns(schema: Arc<Schema>, table_type: TableType) -> Result<Arc<Schema>, KalamDbError> { ... }
    fn save_table_definition(stmt: &CreateTableStatement, schema: &Arc<Schema>) -> Result<(), KalamDbError> { ... }
}
```

2. **Update `create_user_table()` in ddl.rs** (line 310-400):
```rust
// Replace line 370: user_table_service.create_table(stmt)?;
// With:

// Validate table name
Self::validate_table_name(table_name.as_str())?;

// Check if table exists
let table_id = TableId::from_strings(namespace_id.as_str(), table_name.as_str());
if tables_provider.table_exists(&table_id)? {
    if stmt.if_not_exists {
        return Ok(ExecutionResult::Success(format!("Table {} already exists", table_id)));
    } else {
        return Err(KalamDbError::AlreadyExists(format!("Table {} already exists", table_id)));
    }
}

// Transform schema
let schema = Self::inject_auto_increment_field(schema)?;
let schema = Self::inject_system_columns(schema, TableType::User)?;

// Inject column defaults
let mut modified_stmt = stmt.clone();
if !modified_stmt.column_defaults.contains_key("id") {
    modified_stmt.column_defaults.insert(
        "id".to_string(),
        kalamdb_commons::schemas::ColumnDefault::function("SNOWFLAKE_ID", vec![]),
    );
}

// Save table definition
Self::save_table_definition(&modified_stmt, &schema)?;

// Create column family
let app_ctx = AppContext::get();
let user_table_store = app_ctx.user_table_store();
user_table_store.create_column_family(namespace_id.as_str(), table_name.as_str())?;
```

3. **Remove `user_table_service` parameter** from `create_user_table()` signature

4. **Update SqlExecutor routing** to not pass service

### Step 2: Inline SharedTableService (T109)

Follow same pattern as UserTableService:
- Extract/reuse helper methods
- Update `create_shared_table()` at line 490-600 in ddl.rs
- Remove service parameter
- Update routing

### Step 3: Inline StreamTableService (T110)

Similar to above, but **skip system column injection**:
- Update `create_stream_table()` at line 410-480 in ddl.rs
- Use same validation and save_table_definition helpers
- Skip `inject_system_columns()` call (streams don't have _updated/_deleted)

### Step 4: Handle TableDeletionService (T111)

**Investigation Required**: Check if service is actually used.

If used in DROP TABLE logic, inline into `execute_drop_table()` handler.

### Step 5: Delete Service Files (T107-T111)

```bash
rm backend/crates/kalamdb-core/src/services/namespace_service.rs
rm backend/crates/kalamdb-core/src/services/user_table_service.rs
rm backend/crates/kalamdb-core/src/services/shared_table_service.rs
rm backend/crates/kalamdb-core/src/services/stream_table_service.rs
rm backend/crates/kalamdb-core/src/services/table_deletion_service.rs
```

### Step 6: Update services/mod.rs (T112)

```rust
// Remove these lines:
// pub mod namespace_service;
// pub mod user_table_service;
// pub mod shared_table_service;
// pub mod stream_table_service;
// pub mod table_deletion_service;

// pub use namespace_service::NamespaceService;
// pub use user_table_service::UserTableService;
// pub use shared_table_service::SharedTableService;
// pub use stream_table_service::StreamTableService;
// pub use table_deletion_service::{TableDeletionResult, TableDeletionService};

// Keep these:
pub mod backup_service;
pub mod restore_service;
pub mod schema_evolution_service;

pub use backup_service::{BackupManifest, BackupResult, BackupService, BackupStatistics};
pub use restore_service::{RestoreResult, RestoreService};
pub use schema_evolution_service::{SchemaEvolutionResult, SchemaEvolutionService};
```

### Step 7: Remove SqlExecutor Service Fields (T113)

In `sql/executor/mod.rs`, remove from SqlExecutor struct:
```rust
// Remove these fields:
namespace_service: Arc<NamespaceService>,
user_table_service: Arc<UserTableService>,
shared_table_service: Arc<SharedTableService>,
stream_table_service: Arc<StreamTableService>,
table_deletion_service: Arc<TableDeletionService>,
```

And from `SqlExecutor::new()`:
```rust
// Remove these initializations:
let namespace_service = Arc::new(NamespaceService::new(kalam_sql));
let user_table_service = Arc::new(UserTableService::new());
let shared_table_service = Arc::new(SharedTableService::new());
let stream_table_service = Arc::new(StreamTableService::new());
let table_deletion_service = Arc::new(TableDeletionService::new());
```

### Step 8: Update All Imports (T114)

Remove service imports from:
- `sql/executor/handlers/ddl.rs` - **DONE for NamespaceService**
- `sql/executor/mod.rs`
- `sql/executor/handlers/tests/ddl_tests.rs`
- Any other files found by grep

### Step 9: Validation (T117-T119)

```bash
# Verify zero service references
grep -r "NamespaceService\|UserTableService\|SharedTableService\|StreamTableService\|TableDeletionService" backend/ --include="*.rs"

# Build workspace
cargo build

# Run tests
cargo test
```

## 🎯 Next Actions

1. **Immediate**: Inline UserTableService.create_table() logic (T108)
2. **Then**: Inline SharedTableService.create_table() logic (T109)  
3. **Then**: Inline StreamTableService.create_table() logic (T110)
4. **Then**: Check and handle TableDeletionService (T111)
5. **Then**: Delete all 5 service files (T107-T111)
6. **Then**: Update services/mod.rs (T112)
7. **Then**: Remove service fields from SqlExecutor (T113)
8. **Then**: Update all imports (T114)
9. **Finally**: Run validation (T117-T119)

## ⚠️ Known Blockers

**Pre-existing kalamdb-auth compilation errors** will prevent full workspace build until fixed:
```
error[E0432]: unresolved import `kalamdb_sql::RocksDbAdapter`
```

These errors are NOT related to Phase 8 work and exist from incomplete Phase 5/6 cleanup.

## 📊 Progress Tracking

- ✅ T107: NamespaceService - **COMPLETE**
- ⏳ T108: UserTableService - **IN PROGRESS** (logic identified, migration strategy documented)
- ⏳ T109: SharedTableService - **READY** (same pattern as T108)
- ⏳ T110: StreamTableService - **READY** (same pattern, skip system columns)
- ❓ T111: TableDeletionService - **INVESTIGATION NEEDED**
- ⏸️ T112: services/mod.rs - **BLOCKED** (waiting for T107-T111)
- ⏸️ T113: SqlExecutor fields - **BLOCKED** (waiting for T107-T111)
- ⏸️ T114: Import cleanup - **BLOCKED** (waiting for T107-T111)
- ⏸️ T115-T116: Verify retained services - **BLOCKED** (waiting for T112)
- ⏸️ T117-T119: Validation - **BLOCKED** (waiting for T114, kalamdb-auth fix)

**Estimated Remaining Effort**:
- T108-T110: ~4-6 hours (inlining 3 services with ~150-200 lines of logic each)
- T111: ~30 minutes (investigation + handling)
- T112-T114: ~30 minutes (cleanup)
- T115-T119: ~1 hour (validation + fixing any issues)

**Total**: ~6-8 hours of focused development work
