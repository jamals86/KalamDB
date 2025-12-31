# ADR-007: Multi-Storage Architecture with Template Validation

**Status**: Accepted  
**Date**: 2025-10-22  
**Deciders**: KalamDB Core Team  
**Context**: User Story 2 - Automatic Table Flushing (Phase 5)

## Context and Problem Statement

KalamDB needs to support multiple storage backends (local filesystem, S3, etc.) for flushing table data to Parquet files. Each storage backend has different path structures and requirements. We need a flexible, extensible architecture that:

1. Allows users to define custom storage backends via SQL
2. Validates storage path templates to prevent data collisions
3. Supports both shared tables and user-specific tables with different path requirements
4. Enables storage selection at table creation time or via user preferences

## Decision Drivers

- **Flexibility**: Users should be able to add new storage backends without code changes
- **Safety**: Template validation must prevent data loss from incorrect path configurations
- **Performance**: Storage lookups should be fast (cached in memory)
- **Consistency**: Path templates must follow predictable, documented ordering rules
- **Extensibility**: Architecture should support future storage types (Azure Blob, GCS, etc.)

## Considered Options

### Option 1: Hardcoded Storage Paths
- Store all data in a single `data/` directory with fixed path structure
- **Pros**: Simple, no configuration needed
- **Cons**: Not flexible, doesn't support multi-cloud, hard to migrate data

### Option 2: Configuration File Storage Registry
- Define storage backends in `config.toml`
- **Pros**: Centralized configuration, easy to manage
- **Cons**: Requires server restart to add storage, not user-facing

### Option 3: SQL-Managed Storage Registry (CHOSEN)
- Store backends in `system.storages` table
- Manage via CREATE/ALTER/DROP STORAGE SQL commands
- Validate templates using pluggable StorageRegistry
- **Pros**: Dynamic management, SQL-based, supports multi-tenancy, no restarts
- **Cons**: More complex, requires template validation logic

## Decision Outcome

**Chosen option: Option 3 - SQL-Managed Storage Registry**

We implement a `StorageRegistry` that validates path templates against a set of ordering rules, and store storage configurations in the `system.storages` table. Users manage storage backends via SQL commands.

### Architecture Components

#### 1. `system.storages` Table Schema

```sql
CREATE TABLE system.storages (
    storage_id TEXT PRIMARY KEY,           -- User-defined ID (e.g., 's3-prod', 'local')
    storage_name TEXT NOT NULL,            -- Human-readable name
    description TEXT,                      -- Optional description
    storage_type TEXT NOT NULL,            -- 'filesystem' or 's3'
    base_directory TEXT NOT NULL,          -- Root path or S3 bucket
    shared_tables_template TEXT NOT NULL,  -- Path template for shared tables
    user_tables_template TEXT NOT NULL,    -- Path template for user tables
    created_at BIGINT NOT NULL,            -- Unix timestamp (ms)
    updated_at BIGINT NOT NULL             -- Unix timestamp (ms)
);
```

#### 2. StorageRegistry Component

Located in `/backend/crates/kalamdb-core/src/storage/storage_registry.rs`:

```rust
pub struct StorageRegistry {
    kalam_sql: Arc<KalamSql>,
}

impl StorageRegistry {
    /// Validate template variable ordering
    pub fn validate_template(
        &self,
        template: &str,
        table_type: &str,
    ) -> Result<(), String> {
        // Enforce ordering rules (see below)
    }
}
```

#### 3. SQL Commands

**CREATE STORAGE**
```sql
CREATE STORAGE s3_prod
TYPE s3
NAME 'S3 Production Storage'
DESCRIPTION 'Primary production S3 bucket'
BUCKET 'kalamdb-prod'
REGION 'us-west-2'
SHARED_TABLES_TEMPLATE 's3://kalamdb-prod/shared/{namespace}/{tableName}'
USER_TABLES_TEMPLATE 's3://kalamdb-prod/users/{userId}/{namespace}/{tableName}';
```

**ALTER STORAGE**
```sql
ALTER STORAGE s3_prod
SET DESCRIPTION = 'Updated description'
SET SHARED_TABLES_TEMPLATE = 's3://kalamdb-prod/v2/shared/{namespace}/{tableName}';
```

**DROP STORAGE**
```sql
DROP STORAGE old_storage;  -- Fails if tables reference it
```

**SHOW STORAGES**
```sql
SHOW STORAGES;  -- Lists all registered storages
```

### Template Validation Rules

The `StorageRegistry` enforces strict ordering of template variables to prevent path collisions:

#### Shared Tables Template

**Required ordering**: `{namespace}` → `{tableName}`

- ✅ Valid: `/data/{namespace}/{tableName}`
- ✅ Valid: `/data/shared/{namespace}/tables/{tableName}/data`
- ❌ Invalid: `/data/{tableName}/{namespace}` (reversed order)

**Rationale**: Namespaces provide isolation, so namespace must come before table name to prevent cross-namespace access.

#### User Tables Template

**Required ordering**: `{namespace}` → `{tableName}` → `{userId}` → `{shard}`

- ✅ Valid: `/data/{namespace}/{tableName}/{userId}`
- ✅ Valid: `/data/{namespace}/{tableName}/{userId}/{shard}`
- ❌ Invalid: `/data/{userId}/{namespace}/{tableName}` (userId before namespace)
- ❌ Invalid: `/data/{namespace}/{userId}/{tableName}` (userId before tableName)

**Rationale**:
1. **Namespace first**: Provides organizational isolation
2. **Table name second**: Groups all user data for a specific table together
3. **User ID third**: Allows per-user partitioning within a table
4. **Shard last** (optional): Enables horizontal scaling within a user's data

**Note**: User tables MUST include `{userId}` in the template (validated by StorageRegistry).

### Storage Lookup Chain

When flushing a table, KalamDB resolves the storage backend using this priority order:

1. **Table-level storage**: If table has `use_user_storage` set, use that storage_id
2. **User-level storage**: If user has a preferred storage_id, use it
3. **Default storage**: Use the 'local' storage (always present)

```rust
fn resolve_storage_for_table(
    table: &Table,
    user: Option<&User>,
) -> String {
    if let Some(storage_id) = &table.use_user_storage {
        return storage_id.clone();
    }
    
    if let Some(user) = user {
        if let Some(storage_id) = &user.storage_id {
            return storage_id.clone();
        }
    }
    
    "local".to_string()  // Default fallback
}
```

### Default 'local' Storage

On server startup, if `system.storages` is empty, a default 'local' storage is created:

```rust
Storage {
    storage_id: "local",
    storage_name: "Local Filesystem",
    description: Some("Default local filesystem storage"),
    storage_type: "filesystem",
    base_directory: "",  // Uses default from config
    shared_tables_template: "{namespace}/{tableName}",
    user_tables_template: "{namespace}/{tableName}/{userId}",
    created_at: now,
    updated_at: now,
}
```

### Referential Integrity

`DROP STORAGE` is protected by referential integrity checks:

1. Query all tables in `system.tables` for `use_user_storage = storage_id`
2. If any tables reference the storage, return error with table count
3. Special case: `storage_id = 'local'` cannot be deleted (hardcoded protection)

```rust
let tables = kalam_sql.scan_all_tables()?;
let referencing_tables: Vec<_> = tables
    .iter()
    .filter(|t| t.use_user_storage.as_deref() == Some(&storage_id))
    .collect();

if !referencing_tables.is_empty() {
    return Err(format!(
        "Cannot delete storage '{}': {} table(s) still reference it",
        storage_id,
        referencing_tables.len()
    ));
}
```

## Consequences

### Positive

- **Dynamic Management**: Add/modify storage backends without server restarts
- **Multi-Cloud Support**: Seamlessly support S3, filesystem, and future backends
- **Safety**: Template validation prevents data loss from misconfiguration
- **Flexibility**: Users control storage selection per table or per user
- **Auditability**: All storage changes tracked via SQL commands
- **Testability**: Easy to test with temporary storage backends

### Negative

- **Complexity**: More moving parts than hardcoded paths
- **Migration**: Existing deployments need to migrate to new storage schema
- **Validation Overhead**: Template validation adds slight overhead (mitigated by caching)

### Neutral

- **Learning Curve**: Users need to understand template syntax and ordering rules
- **Documentation**: Requires comprehensive docs for template configuration

## Implementation Status

- ✅ `system.storages` table schema defined
- ✅ StorageRegistry with template validation implemented
- ✅ CREATE/ALTER/DROP/SHOW STORAGE SQL parsers implemented
- ✅ Referential integrity checks for DROP STORAGE
- ✅ Default 'local' storage auto-creation
- ✅ Integration into SqlExecutor
- ✅ Unit tests for all commands (8/8 passing)
- 🔄 Integration tests (19/35 passing - debugging in progress)

## References

- **Specification**: `/specs/004-system-improvements-and/spec.md` (FR-021aa through FR-021ah)
- **Tasks**: `/specs/004-system-improvements-and/tasks.md` (T164-T193)
- **Code**:
  - StorageRegistry: `/backend/crates/kalamdb-core/src/storage/storage_registry.rs`
  - SQL Commands: `/backend/crates/kalamdb-sql/src/storage_commands.rs`
  - Executor Integration: `/backend/crates/kalamdb-core/src/sql/executor.rs`
  - System Table: `/backend/crates/kalamdb-sql/src/models/storage.rs`

## Related ADRs

- ADR-003: RocksDB for System Metadata
- ADR-004: Automatic Flushing Architecture
- ADR-006: Multi-Storage Backend Design (if exists)

## Notes

This architecture positions KalamDB for future enhancements:
- **Cloud-Native Deployments**: Each tenant can have dedicated S3 buckets
- **Data Locality**: Route tables to region-specific storage
- **Tiered Storage**: Hot data on SSD, cold data on S3 Glacier
- **Backup Integration**: Replicate to secondary storage for disaster recovery
