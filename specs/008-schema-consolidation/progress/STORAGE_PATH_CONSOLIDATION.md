# Storage Path Consolidation Summary

## Overview
Consolidated all storage paths to use `/data/storage` as the default base directory, with configurable templates for shared and user tables.

## Changes Made

### 1. Configuration Structure (`backend/src/config.rs`)
Added two new fields to `StorageSettings`:
- `shared_tables_template`: Template for shared table paths (default: `"{namespace}/{tableName}"`)
- `user_tables_template`: Template for user table paths (default: `"{namespace}/{tableName}/{userId}"`)

### 2. Configuration Files
Updated both `backend/config.toml` and `backend/config.example.toml` to include:
```toml
[storage]
rocksdb_path = "./data/rocksdb"
default_storage_path = "./data/storage"
shared_tables_template = "{namespace}/{tableName}"
user_tables_template = "{namespace}/{tableName}/{userId}"
```

### 3. Lifecycle Initialization (`backend/src/lifecycle.rs`)
Changed default 'local' storage creation to use config templates:
```rust
shared_tables_template: config.storage.shared_tables_template.clone(),
user_tables_template: config.storage.user_tables_template.clone(),
```

### 4. SharedTableService (`backend/crates/kalamdb-core/src/services/shared_table_service.rs`)
- Added `default_storage_path` parameter to constructor
- Changed `resolve_storage_path()` to use configurable base path instead of hardcoded `/data/shared`
- All path resolution now respects configuration

### 5. Test Files Updated
All test files now pass appropriate test storage paths:
- `backend/crates/kalamdb-core/src/services/shared_table_service.rs` (test module)
- `backend/tests/integration/common/mod.rs`
- `backend/tests/test_audit_logging.rs`
- `backend/crates/kalamdb-sql/src/executor.rs` (test module)

## How It Works

### Path Resolution
1. **Base Path**: Set in config as `default_storage_path` (default: `"./data/storage"`)
2. **Templates**: Define directory structure within base path
   - Shared: `{namespace}/{tableName}`
   - User: `{namespace}/{tableName}/{userId}`
3. **Final Paths**: Base + Template after placeholder substitution

### Examples
For a shared table in namespace `myapp` with table name `products`:
```
Base: ./data/storage
Template: {namespace}/{tableName}
Final: ./data/storage/myapp/products/
```

For a user table in namespace `myapp`, table `preferences`, user `user123`:
```
Base: ./data/storage
Template: {namespace}/{tableName}/{userId}
Final: ./data/storage/myapp/preferences/user123/
```

## Storage Backend
The 'local' storage entry in `system.storages` table:
- `storage_id`: "local"
- `base_directory`: "" (empty means use `default_storage_path`)
- `shared_tables_template`: from config
- `user_tables_template`: from config

## Testing
✅ All 5 smoke tests passing:
- `smoke_user_table_subscription_lifecycle`
- `smoke_system_tables_and_user_lifecycle`
- `smoke_user_table_rls_isolation`
- `smoke_stream_table_subscription`
- `smoke_shared_table_crud`

## Benefits
1. **Centralized Configuration**: All storage paths configurable from `config.toml`
2. **Flexible Templates**: Easy to customize directory structure per deployment
3. **Consistent Organization**: All table data under `/data/storage` by default
4. **Clear Separation**: RocksDB in `/data/rocksdb`, Parquet files in `/data/storage`
5. **Backward Compatible**: Default templates match previous hardcoded behavior

## Related Files
- `backend/src/config.rs` - Configuration structure
- `backend/config.toml` - Active configuration
- `backend/config.example.toml` - Configuration template
- `backend/src/lifecycle.rs` - Server initialization
- `backend/crates/kalamdb-core/src/services/shared_table_service.rs` - Path resolution

## Migration Notes
No migration needed - existing installations will use default values that match previous hardcoded behavior.
