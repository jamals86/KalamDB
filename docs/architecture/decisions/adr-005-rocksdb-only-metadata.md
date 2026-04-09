# ADR-005: RocksDB-Only Metadata Storage

**Date**: 2025-10-20  
**Status**: Accepted  
**Context**: Phase 9.5 (002-simple-kalamdb)

## Context

The original design (Phase 1-8) used **JSON configuration files** for metadata:

```
config/
├── namespaces.json          # Namespace definitions
├── storage_locations.json   # Storage location registry
└── tables/
    ├── app.messages.json    # Table metadata
    ├── app.events.json
    └── ...
```

This created several problems:

1. **Synchronization Issues**: JSON files out of sync with RocksDB state
2. **Concurrency Problems**: File locking required for multi-process writes
3. **No Transactional Guarantees**: Metadata changes not atomic with data changes
4. **Startup Overhead**: Parse hundreds of JSON files on server restart
5. **Backup Complexity**: Must backup both JSON files + RocksDB + Parquet
6. **Query Limitations**: Cannot SQL query metadata (e.g., "SHOW TABLES")

Example problems:
```bash
# Problem 1: Out of sync
CREATE TABLE app.messages (...);  # Creates RocksDB CF + JSON file
# Server crashes before JSON write completes
# Restart: RocksDB has CF, but no JSON metadata

# Problem 2: Backup complexity
tar -czf backup.tar.gz config/ data/rocksdb/ data/parquet/
# Must remember to backup three separate directories
```

## Decision

We will store **all metadata in RocksDB system tables** with NO JSON config files.

### System Tables

All metadata stored in dedicated RocksDB column families:

```
System Column Families:
├── system_namespaces         # Namespace definitions
├── system_tables             # Table metadata (name, type, flush policy)
├── system_table_schemas      # Schema versions per table
├── system_storage_locations  # Storage location registry
├── system_jobs               # Background job tracking
└── system_users              # User authentication and permissions
```

Active WebSocket subscriptions are now exposed through the in-memory
`system.live` view rather than a dedicated RocksDB column family.

### Unified system table access

All metadata access now goes through `kalamdb-system` providers exposed from
`AppContext` and `SystemTablesRegistry`. `kalamdb-dialect` owns SQL parsing and
statement classification; it no longer owns metadata persistence.

```rust
let system_tables = app_context.system_tables();
let namespace = system_tables.namespaces().get_namespace(&namespace_id)?;
let storages = system_tables.storages().list_storages()?;
```

## Consequences

### Positive

1. **Atomic Operations**: `kalamdb-core` coordinates metadata changes through
    `app_context.system_tables()` and hot-path writes through `kalamdb-store`,
    keeping metadata and data-path changes in the same orchestration layer.

2. **No Synchronization Issues**: Single source of truth (RocksDB)
   - No file/DB sync problems
   - No startup parsing overhead

3. **SQL Queryable Metadata**: System tables are queryable
   ```sql
   SHOW TABLES IN app;               -- Queries system_tables CF
   DESCRIBE TABLE app.messages;      -- Queries system_tables + system_table_schemas
   SHOW STATS FOR TABLE app.messages; -- Queries system_tables + counts data
   ```

4. **Simplified Backup**: Single storage system
   ```bash
   # Backup RocksDB (includes all metadata + hot data)
   BACKUP DATABASE app TO '/backups/app-20251020';
   # Copies: RocksDB system tables + Parquet files
   ```

5. **Faster Startup**: No JSON parsing
   - Large deployments: 100+ tables load instantly
   - Lazy loading: Only load metadata when accessed

6. **Consistent Error Handling**: All storage errors from same source
   - No mix of file I/O errors and DB errors

7. **Versioned Schemas**: Schema history remains stored in system tables and is
    surfaced through `kalamdb-system` providers.

8. **Job Tracking**: Background operations visible
   ```sql
   SELECT * FROM system.jobs WHERE status = 'running';
   -- Shows active flush jobs, backup jobs, etc.
   ```

### Negative

1. **Cannot Edit Config with Text Editor**: Metadata in binary format
   - Mitigation: Use SQL commands (CREATE NAMESPACE, CREATE TABLE)
   - Benefit: Enforces validation and consistency

2. **No Human-Readable Config**: Cannot inspect metadata with `cat`
   - Mitigation: Use SQL queries (SHOW TABLES, DESCRIBE TABLE)
   - Benefit: Metadata integrity guaranteed

3. **RocksDB Dependency**: Cannot use metadata without RocksDB
   - Mitigation: RocksDB is already a hard dependency
   - Benefit: Simplified architecture (one storage system)

### Trade-offs

| Aspect | JSON Config Files | RocksDB System Tables |
|--------|-------------------|----------------------|
| **Atomicity** | No (file + DB separate) | Yes (single transaction) |
| **Queryability** | No (must parse files) | Yes (SQL queries) |
| **Human-Readable** | Yes (`cat config.json`) | No (binary format) |
| **Startup Speed** | Slow (parse all files) | Fast (lazy loading) |
| **Backup** | Complex (3 directories) | Simple (1 RocksDB + Parquet) |
| **Synchronization** | Manual (error-prone) | Automatic (single source) |
| **Validation** | Manual (parse JSON) | Automatic (typed API) |

## Implementation Details

### System Table Schemas

#### system_namespaces

```rust
pub struct Namespace {
    pub namespace_id: NamespaceId,
    pub created_at: DateTime<Utc>,
    pub options: HashMap<String, String>,
}

// RocksDB Key: namespace_id
// RocksDB Value: JSON-serialized Namespace
```

#### system_tables

```rust
pub struct Table {
    pub table_id: TableId,
    pub namespace_id: NamespaceId,
    pub table_name: TableName,
    pub table_type: TableType, // User, Shared, Stream, System
    pub storage_location_name: Option<String>,
    pub flush_policy_type: Option<FlushPolicyType>,
    pub row_limit: Option<usize>,
    pub time_interval: Option<u64>,
    pub current_version: i32,
    pub last_flushed_at: Option<DateTime<Utc>>,
    pub deleted_retention_hours: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// RocksDB Key: table_id
// RocksDB Value: JSON-serialized Table
```

#### system_table_schemas

```rust
pub struct TableSchema {
    pub table_id: TableId,
    pub version: i32,
    pub schema_json: String, // Arrow schema as JSON
    pub created_at: DateTime<Utc>,
}

// RocksDB Key: {table_id}:{version}
// RocksDB Value: JSON-serialized TableSchema
```

#### system_storage_locations

```rust
pub struct StorageLocation {
    pub location_id: StorageLocationId,
    pub name: String,
    pub path: String,
    pub location_type: String, // "filesystem", "s3", etc.
    pub created_at: DateTime<Utc>,
}

// RocksDB Key: location_id
// RocksDB Value: JSON-serialized StorageLocation
```

#### system_jobs

```rust
pub struct Job {
    pub job_id: JobId,
    pub job_type: String, // "flush", "backup", "restore"
    pub table_id: Option<TableId>,
    pub namespace_id: Option<NamespaceId>,
    pub status: String, // "running", "completed", "failed"
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub result: Option<String>, // JSON result data
}

// RocksDB Key: job_id
// RocksDB Value: JSON-serialized Job
```

### kalamdb-system Implementation

Metadata persistence now lives under `backend/crates/kalamdb-system/src/providers/**`
and is exposed through `SystemTablesRegistry` on `AppContext`. The SQL surface
for these commands lives in `kalamdb-dialect`, while `kalamdb-core` bridges the
parsed statements to the provider calls.

## Migration Path

Completed in Phase 9.5:

1. Consolidated metadata persistence into `kalamdb-system` providers and registry APIs
2. Migrated all metadata operations from JSON files to RocksDB
3. Updated `kalamdb-core` services to use `app_context.system_tables()`
4. Removed all JSON config file parsing code
5. Removed `kalamdb-sql`; parsing and classification moved to `kalamdb-dialect`

## Alternatives Considered

### 1. SQLite for Metadata

Store metadata in separate SQLite database.

**Pros**:
- SQL queryable
- Transactional
- Human-readable (with sqlite3 CLI)

**Cons**:
- Additional dependency
- Separate backup (RocksDB + SQLite + Parquet)
- Synchronization issues (two databases)

**Rejected**: Adds complexity with no clear benefit over RocksDB.

### 2. PostgreSQL for Metadata

Use PostgreSQL as metadata store.

**Pros**:
- Rich SQL features
- Well-known tooling

**Cons**:
- Requires separate database server
- Network latency for metadata queries
- Complex deployment

**Rejected**: Too heavy for embedded use case. KalamDB should be standalone.

### 3. Keep JSON + Add RocksDB Mirror

Maintain JSON files as source of truth, mirror in RocksDB.

**Pros**:
- Human-readable config
- Fast queries via RocksDB

**Cons**:
- Dual writes (error-prone)
- Synchronization complexity
- Backup still complex

**Rejected**: Worst of both worlds (complexity + sync issues).

## Related ADRs

- ADR-009: Three-Layer Architecture (`kalamdb-system` and `kalamdb-store` are Layer 2)
- ADR-004: RocksDB Column Families (system_* CFs implementation)

## References

- [Tasks](../../../specs/002-simple-kalamdb/tasks.md) - Phase 9.5: System Tables Refactor
- [Session Summary](../../../specs/002-simple-kalamdb/progress/SESSION_2025-10-17_SUMMARY.md) - Metadata migration discussion
- [kalamdb-system crate](../../../backend/crates/kalamdb-system/) - System table providers
- [kalamdb-dialect crate](../../../backend/crates/kalamdb-dialect/) - SQL dialect and parser surface

## Revision History

- 2025-10-20: Initial version (accepted)
