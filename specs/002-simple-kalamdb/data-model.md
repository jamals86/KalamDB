# Data Model: Simple KalamDB

**Feature**: Simple KalamDB - User-Based Database with Live Queries  
**Branch**: `002-simple-kalamdb`  
**Date**: 2025-10-15

## Overview

This document defines all entities, relationships, and validation rules for KalamDB's metadata and system tables.

## Core Entities

### Namespace

Logical container for organizing tables.

**Fields**:
- `name` (String, PK): Unique namespace identifier (e.g., "app", "analytics")
- `created_at` (Timestamp): When namespace was created
- `options` (JSON): Custom namespace configuration
- `table_count` (Integer): Number of tables in this namespace

**Validation Rules**:
- `name` must be unique across all namespaces
- `name` must match regex: `^[a-z][a-z0-9_]*$` (lowercase, start with letter)
- `name` cannot be "system" (reserved)
- Cannot delete namespace if `table_count > 0`

**Storage**: RocksDB column family `system_namespaces` (managed via kalamdb-sql)

**Key Format**: `{namespace_id}` (UUID)

**Example**:
```json
{
  "namespace_id": "ns_abc123",
  "name": "app",
  "created_at": "2025-10-14T10:00:00Z",
  "options": {},
  "table_count": 3
}
```

---

### TableMetadata

Metadata for user/shared/stream tables.

**Fields**:
- `table_name` (String, PK): Table identifier within namespace
- `table_type` (Enum): User | Shared | Stream
- `namespace` (String, FK → Namespace.name): Parent namespace
- `created_at` (Timestamp): When table was created
- `storage_location` (String): Either path template or location reference name
- `flush_policy` (FlushPolicy): When to flush buffered data to Parquet
- `schema_version` (Integer): Current schema version number
- `deleted_retention_hours` (Integer, optional): How long to keep deleted rows

**Validation Rules**:
- `table_name` must be unique within namespace
- `table_name` must match regex: `^[a-z][a-z0-9_]*$`
- `namespace` must exist
- `table_type` determines key format in RocksDB
- System tables (system.*) managed separately

**Derived Values**:
- Column family name: `{table_type}_table:{namespace}:{table_name}`
- Schema stored in: `system_table_schemas` (RocksDB)

**Storage**: RocksDB column family `system_tables` (managed via kalamdb-sql)

**Key Format**: `{table_id}` (UUID)

**Example**:
```json
{
  "table_id": "tbl_xyz789",
  "table_name": "messages",
  "table_type": "User",
  "namespace": "app",
  "created_at": "2025-10-14T11:00:00Z",
  "storage_location": "/data/${user_id}/messages",
  "flush_policy": {"row_limit": 10000},
  "schema_version": 1,
  "deleted_retention_hours": 72
}
```

---

### FlushPolicy

Determines when to flush RocksDB buffer to Parquet.

**Type**: Enum with variants

**Variants**:
- `RowLimit { row_limit: Integer }`: Flush after N rows inserted
- `TimeInterval { interval_seconds: Integer }`: Flush every N seconds

**Validation Rules**:
- `row_limit` must be > 0 and < 1,000,000
- `interval_seconds` must be > 0 and < 86400 (24 hours)

**Examples**:
```json
{"row_limit": 10000}
{"interval_seconds": 300}
```

---

### StorageLocation

Predefined storage locations for table data.

**Fields**:
- `location_name` (String, PK): Unique location identifier
- `location_type` (Enum): Filesystem | S3
- `path` (String): Path template (may include `${user_id}`)
- `credentials_ref` (String, optional): Reference to credentials (for S3)
- `usage_count` (Integer): Number of tables using this location

**Validation Rules**:
- `location_name` must be unique
- `location_name` must match regex: `^[a-z][a-z0-9_]*$`
- Cannot delete location if `usage_count > 0`
- `path` must be accessible (test on creation)

**Storage**: RocksDB column family `system_storage_locations` (managed via kalamdb-sql)

**Key Format**: `{location_name}`

**Example**:
```json
{
  "location_name": "local_disk",
  "location_type": "Filesystem",
  "path": "/data/${user_id}/tables",
  "credentials_ref": null,
  "usage_count": 5
}
```

---

## System Tables

### system.users

User registry for authentication and permissions.

**Fields**:
- `user_id` (String, PK): Unique user identifier (from JWT)
- `username` (String, unique): Human-readable username
- `email` (String, unique): User email
- `created_at` (Timestamp): When user was created

**Validation Rules**:
- `user_id` must be unique
- `username` must be unique
- `email` must be unique and valid format

**Storage**: RocksDB column family `system_users` (managed via kalamdb-sql)

**Key Format**: `{user_id}`

**Example Row**:
```json
{
  "user_id": "user123",
  "username": "alice",
  "email": "alice@example.com",
  "created_at": "2025-10-14T09:00:00Z"
}
```

---

### system.live_queries

Active live query subscriptions.

**Fields**:
- `live_id` (String, PK): `{connection_id}-{query_id}` (unique per subscription)
- `connection_id` (String): WebSocket connection identifier
- `table_id` (String): Table being monitored
- `query_id` (String): Client-provided query identifier
- `user_id` (String, FK → system.users.user_id): Owner of subscription
- `query` (String): SQL query being executed
- `options` (JSON): Subscription options (e.g., `{"last_rows": 100}`)
- `created_at` (Timestamp): When subscription was created
- `updated_at` (Timestamp): Last notification sent
- `changes` (Integer): Number of notifications sent
- `node` (String): Which node owns this subscription

**Validation Rules**:
- `live_id` must be unique
- `user_id` must exist in system.users
- Deleted automatically when WebSocket disconnects

**Storage**: RocksDB column family `system_live_queries` (managed via kalamdb-sql)

**Key Format**: `{live_id}`

**Example Row**:
```json
{
  "live_id": "conn123-messages",
  "connection_id": "conn123",
  "table_id": "messages",
  "query_id": "messages",
  "user_id": "user456",
  "query": "SELECT * FROM messages WHERE conversation_id = 'conv789'",
  "options": {"last_rows": 100},
  "created_at": "2025-10-15T10:00:00Z",
  "updated_at": "2025-10-15T10:05:30Z",
  "changes": 12,
  "node": "node1"
}
```

---

### system.storage_locations

Predefined storage locations.

**Purpose**: Queryable via SQL, managed by kalamdb-sql crate.

**Fields**: Same as StorageLocation entity (see above)

**Storage**: RocksDB column family `system_storage_locations` (managed via kalamdb-sql)

**Key Format**: `{location_name}`

---

### system.jobs

Background job execution tracking.

**Fields**:
- `job_id` (String, PK): Unique job identifier (UUID)
- `job_type` (Enum): FlushUser | FlushShared | Cleanup | Schema Evolution
- `table_name` (String): Fully qualified table name (e.g., "app.messages")
- `status` (Enum): Running | Completed | Failed
- `start_time` (Timestamp): When job started
- `end_time` (Timestamp, optional): When job finished
- `parameters` (Array<String>): Job input parameters
- `result` (String, optional): Job outcome summary
- `trace` (String, optional): Execution trace/logs
- `memory_used_mb` (Integer, optional): Peak memory usage
- `cpu_used_percent` (Float, optional): Average CPU usage
- `node_id` (String): Which node executed the job
- `error_message` (String, optional): Error details if failed

**Validation Rules**:
- `job_id` must be unique
- `status` transitions: Running → (Completed | Failed)
- `end_time` must be after `start_time`

**Storage**: RocksDB column family `system_jobs` (managed via kalamdb-sql)

**Key Format**: `{job_id}`

**Example Row**:
```json
{
  "job_id": "job_abc123",
  "job_type": "FlushUser",
  "table_name": "app.messages",
  "status": "Completed",
  "start_time": "2025-10-15T10:00:00Z",
  "end_time": "2025-10-15T10:00:15Z",
  "parameters": ["user123", "batch-001"],
  "result": "Flushed 10000 rows to /data/user123/messages/batch-001.parquet",
  "trace": "...",
  "memory_used_mb": 128,
  "cpu_used_percent": 45.2,
  "node_id": "node1",
  "error_message": null
}
```

---

### system.namespaces

Namespace registry (metadata for all namespaces).

**Fields**:
- `namespace_id` (String, PK): Unique namespace identifier (UUID)
- `name` (String, unique): Human-readable namespace name
- `created_at` (Timestamp): When namespace was created
- `options` (JSON): Custom namespace configuration
- `table_count` (Integer): Number of tables in this namespace

**Validation Rules**:
- `namespace_id` must be unique (UUID)
- `name` must be unique across all namespaces
- `name` must match regex: `^[a-z][a-z0-9_]*$`
- `name` cannot be "system" (reserved)
- Cannot delete namespace if `table_count > 0`

**Storage**: RocksDB column family `system_namespaces` (managed via kalamdb-sql)

**Key Format**: `{namespace_id}`

**Example Row**:
```json
{
  "namespace_id": "ns_abc123",
  "name": "app",
  "created_at": "2025-10-14T10:00:00Z",
  "options": {},
  "table_count": 3
}
```

**Purpose**: Replaces `conf/namespaces.json` - all namespace metadata now in RocksDB.

---

### system.tables

Table metadata registry (metadata for all user/shared/stream tables).

**Fields**:
- `table_id` (String, PK): Unique table identifier (UUID)
- `table_name` (String): Table identifier within namespace
- `namespace` (String, FK → system.namespaces.name): Parent namespace
- `table_type` (Enum): User | Shared | Stream
- `created_at` (Timestamp): When table was created
- `storage_location` (String): Either path template or location reference name
- `flush_policy` (JSON): Flush policy configuration (row_limit or interval_seconds)
- `schema_version` (Integer): Current schema version number
- `deleted_retention_hours` (Integer, optional): How long to keep deleted rows

**Validation Rules**:
- `table_id` must be unique (UUID)
- `(namespace, table_name)` must be unique
- `table_name` must match regex: `^[a-z][a-z0-9_]*$`
- `namespace` must exist in system.namespaces
- `table_type` determines column family name
- Cannot drop table if active live queries exist

**Storage**: RocksDB column family `system_tables` (managed via kalamdb-sql)

**Key Format**: `{table_id}`

**Example Row**:
```json
{
  "table_id": "tbl_xyz789",
  "table_name": "messages",
  "namespace": "app",
  "table_type": "User",
  "created_at": "2025-10-14T11:00:00Z",
  "storage_location": "/data/${user_id}/messages",
  "flush_policy": {"row_limit": 10000},
  "schema_version": 1,
  "deleted_retention_hours": 72
}
```

**Purpose**: Replaces `conf/{namespace}/schemas/{table_name}/manifest.json` - all table metadata now in RocksDB.

---

### system.table_schemas

Arrow schema versions for all tables.

**Fields**:
- `schema_id` (String, PK): Unique schema identifier (UUID)
- `table_id` (String, FK → system.tables.table_id): Parent table
- `version` (Integer): Schema version number (1, 2, 3, ...)
- `arrow_schema` (JSON): Arrow schema in JSON format
- `created_at` (Timestamp): When this schema version was created
- `changes` (String, optional): Description of what changed (e.g., "ADD COLUMN user_id STRING")

**Validation Rules**:
- `schema_id` must be unique (UUID)
- `(table_id, version)` must be unique
- `version` must increment sequentially (1, 2, 3, ...)
- `arrow_schema` must be valid Arrow JSON format
- Cannot delete schema version if it's the current version

**Storage**: RocksDB column family `system_table_schemas` (managed via kalamdb-sql)

**Key Format**: `{table_id}:{version}` (allows efficient range queries for schema history)

**Example Row**:
```json
{
  "schema_id": "sch_def456",
  "table_id": "tbl_xyz789",
  "version": 2,
  "arrow_schema": {
    "fields": [
      {"name": "message_id", "type": "Utf8", "nullable": false},
      {"name": "content", "type": "Utf8", "nullable": false},
      {"name": "user_id", "type": "Utf8", "nullable": false},
      {"name": "_updated", "type": "Timestamp", "nullable": false},
      {"name": "_deleted", "type": "Boolean", "nullable": false}
    ]
  },
  "created_at": "2025-10-15T11:00:00Z",
  "changes": "ADD COLUMN user_id STRING"
}
```

**Purpose**: Replaces `conf/{namespace}/schemas/{table_name}/schema_v{N}.json` - all schema versions now in RocksDB.

**Query Patterns**:
```sql
-- Get current schema for a table
SELECT arrow_schema FROM system.table_schemas 
WHERE table_id = 'tbl_xyz789' 
ORDER BY version DESC 
LIMIT 1;

-- Get schema history
SELECT version, created_at, changes FROM system.table_schemas
WHERE table_id = 'tbl_xyz789'
ORDER BY version ASC;
```

---

## kalamdb-sql Crate Architecture

### Overview

The `kalamdb-sql` crate provides a unified SQL interface for all 7 system tables, eliminating code duplication and providing consistent CRUD operations.

### Crate Structure

```rust
kalamdb-sql/
├── src/
│   ├── lib.rs          // Public API exports
│   ├── models.rs       // Rust structs for 7 system tables
│   ├── parser.rs       // SQL parsing (sqlparser-rs)
│   ├── executor.rs     // SQL execution logic
│   └── adapter.rs      // RocksDB read/write operations
```

### Models (models.rs)

```rust
// Rust types for system tables
pub struct User {
    pub user_id: String,
    pub username: String,
    pub email: String,
    pub created_at: Timestamp,
}

pub struct LiveQuery {
    pub live_id: String,
    pub connection_id: String,
    pub table_id: String,
    pub query_id: String,
    pub user_id: String,
    pub query: String,
    pub options: serde_json::Value,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub changes: i64,
    pub node: String,
}

pub struct StorageLocation {
    pub location_name: String,
    pub location_type: LocationType,
    pub path: String,
    pub credentials_ref: Option<String>,
    pub usage_count: i32,
}

pub struct Job {
    pub job_id: String,
    pub job_type: JobType,
    pub table_name: String,
    pub status: JobStatus,
    pub start_time: Timestamp,
    pub end_time: Option<Timestamp>,
    pub parameters: Vec<String>,
    pub result: Option<String>,
    pub trace: Option<String>,
    pub memory_used_mb: Option<i32>,
    pub cpu_used_percent: Option<f64>,
    pub node_id: String,
    pub error_message: Option<String>,
}

pub struct Namespace {
    pub namespace_id: String,
    pub name: String,
    pub created_at: Timestamp,
    pub options: serde_json::Value,
    pub table_count: i32,
}

pub struct Table {
    pub table_id: String,
    pub table_name: String,
    pub namespace: String,
    pub table_type: TableType,
    pub created_at: Timestamp,
    pub storage_location: String,
    pub flush_policy: serde_json::Value,
    pub schema_version: i32,
    pub deleted_retention_hours: Option<i32>,
}

pub struct TableSchema {
    pub schema_id: String,
    pub table_id: String,
    pub version: i32,
    pub arrow_schema: serde_json::Value,
    pub created_at: Timestamp,
    pub changes: Option<String>,
}

// Enums
pub enum TableType { User, Shared, Stream }
pub enum JobType { FlushUser, FlushShared, Cleanup, SchemaEvolution }
pub enum JobStatus { Running, Completed, Failed }
pub enum LocationType { Filesystem, S3 }
```

### Public API (lib.rs)

```rust
pub struct KalamSql {
    db: Arc<DB>, // RocksDB handle
}

impl KalamSql {
    pub fn new(db: Arc<DB>) -> Self { ... }
    
    // Unified SQL execution
    pub fn execute(&self, sql: &str) -> Result<Vec<RecordBatch>> { ... }
    
    // Typed helpers for common operations
    pub fn get_user(&self, user_id: &str) -> Result<Option<User>> { ... }
    pub fn insert_namespace(&self, ns: &Namespace) -> Result<()> { ... }
    pub fn get_table_schema(&self, table_id: &str, version: i32) -> Result<Option<TableSchema>> { ... }
}
```

### Usage Example

```rust
// In kalamdb-core
use kalamdb_sql::KalamSql;

let sql_engine = KalamSql::new(rocksdb_handle);

// Query system tables
let users = sql_engine.execute("SELECT * FROM system.users WHERE username = 'alice'")?;

// Insert namespace
sql_engine.execute("INSERT INTO system.namespaces (namespace_id, name, options, table_count) VALUES (?, ?, ?, ?)")?;

// Get current schema
let schema = sql_engine.get_table_schema("tbl_xyz789", 2)?;
```

### Benefits

1. **No code duplication**: Single implementation for 7 tables
2. **Type safety**: Rust models with compile-time validation
3. **Consistent API**: All tables use same SQL interface
4. **Easy testing**: Test SQL engine once, all tables validated
5. **Future-ready**: SQL operations can be replicated via Raft

---

## Relationships

### Namespace (system.namespaces) → TableMetadata (system.tables) (1:N)
- One namespace contains many tables
- Cascade: Cannot delete namespace if tables exist (`table_count > 0`)
- Integrity: `table_count` updated on table create/drop

### TableMetadata (system.tables) → TableSchema (system.table_schemas) (1:N)
- One table has multiple schema versions
- Cascade: Schema versions retained even after table drop (for audit)
- Current schema: Query `WHERE table_id = ? ORDER BY version DESC LIMIT 1`

### Namespace → StorageLocation (N:M via table references)
- Namespaces don't directly reference locations
- Tables (system.tables) reference locations via `storage_location` field
- Cascade: Cannot delete location if `usage_count > 0`

### TableMetadata → FlushPolicy (1:1 embedded)
- Each table has exactly one flush policy
- Embedded as JSON in system.tables.flush_policy

### system.users → system.live_queries (1:N)
- One user can have many live subscriptions
- Cascade: Subscriptions deleted when WebSocket disconnects

### system.tables → system.jobs (1:N)
- One table can have many flush/cleanup jobs
- No cascade: Jobs retained for audit even after table drop

---

## State Transitions

### Namespace Lifecycle
```
[CREATE via INSERT INTO system.namespaces] → Active
Active → [DROP if table_count = 0] → Deleted
Active → [DROP if table_count > 0] → ERROR
```

### Table Lifecycle
```
[CREATE via INSERT INTO system.tables] → Active
Active → [ALTER adds new row to system.table_schemas] → Active (schema_version++)
Active → [DROP if no live_queries] → Deleted
Active → [DROP if live_queries exist] → ERROR
```

### Job Lifecycle
```
[INSERT INTO system.jobs with status=Running] → Running
Running → [UPDATE status=Completed] → Completed
Running → [UPDATE status=Failed] → Failed
```

### Live Query Lifecycle
```
[SUBSCRIBE inserts into system.live_queries] → Active
Active → [WebSocket disconnect] → DELETE FROM system.live_queries
Active → [KILL LIVE QUERY] → DELETE FROM system.live_queries
```

### Schema Version Lifecycle
```
[CREATE TABLE inserts version 1] → Active
[ALTER TABLE inserts version 2] → Active (version 1 still retained)
[ALTER TABLE inserts version 3] → Active (versions 1-2 retained)
```

---

## RocksDB Key Formats

### User Tables
**Column Family**: `user_table:{namespace}:{table_name}`  
**Key Format**: `{user_id}:{row_id}`  
**Example**: `user123:msg001` in `user_table:app:messages`

### Shared Tables
**Column Family**: `shared_table:{namespace}:{table_name}`  
**Key Format**: `{row_id}`  
**Example**: `conv001` in `shared_table:app:conversations`

### Stream Tables
**Column Family**: `stream_table:{namespace}:{table_name}`  
**Key Format**: `{timestamp_micros}:{row_id}`  
**Example**: `1697299200000000:evt001` in `stream_table:app:events`

### System Tables (7 total)
All managed by kalamdb-sql crate:

1. **system_users**
   - **Key Format**: `{user_id}`
   - **Example**: `user123`

2. **system_live_queries**
   - **Key Format**: `{live_id}` (format: `{connection_id}-{query_id}`)
   - **Example**: `conn123-messages`

3. **system_storage_locations**
   - **Key Format**: `{location_name}`
   - **Example**: `local_disk`

4. **system_jobs**
   - **Key Format**: `{job_id}` (UUID)
   - **Example**: `job_abc123`

5. **system_namespaces**
   - **Key Format**: `{namespace_id}` (UUID)
   - **Example**: `ns_abc123`

6. **system_tables**
   - **Key Format**: `{table_id}` (UUID)
   - **Example**: `tbl_xyz789`

7. **system_table_schemas**
   - **Key Format**: `{table_id}:{version}` (allows efficient range queries)
   - **Example**: `tbl_xyz789:2`

### Flush Tracking
**Column Family**: `user_table_counters`  
**Key Format**: `{user_id}:{namespace}:{table_name}`  
**Value**: Row count (i64)  
**Example**: `user123:app:messages` → `8547`  
**Purpose**: Track per-user-per-table row counts for flush policy enforcement

---

## Schema Versioning

### Storage Strategy

**All schema versions stored in `system_table_schemas`** (RocksDB column family).

**Key Benefits**:
- Transactional consistency with table metadata
- Efficient range queries for schema history
- No file system dependencies
- Simplified backup/restore

### Schema Version Queries

```sql
-- Get current schema for a table
SELECT arrow_schema FROM system.table_schemas 
WHERE table_id = 'tbl_xyz789' 
ORDER BY version DESC 
LIMIT 1;

-- Get schema history
SELECT version, created_at, changes FROM system.table_schemas
WHERE table_id = 'tbl_xyz789'
ORDER BY version ASC;

-- Check if column exists in current schema
SELECT arrow_schema FROM system.table_schemas 
WHERE table_id = 'tbl_xyz789' 
  AND arrow_schema LIKE '%"name": "user_id"%'
ORDER BY version DESC 
LIMIT 1;
```

### Schema Evolution Example

```json
// Version 1 (initial)
{
  "schema_id": "sch_v1_abc",
  "table_id": "tbl_xyz789",
  "version": 1,
  "arrow_schema": {
    "fields": [
      {"name": "message_id", "type": "Utf8", "nullable": false},
      {"name": "content", "type": "Utf8", "nullable": false},
      {"name": "_updated", "type": "Timestamp", "nullable": false},
      {"name": "_deleted", "type": "Boolean", "nullable": false}
    ]
  },
  "created_at": "2025-10-14T10:00:00Z",
  "changes": "Initial schema"
}

// Version 2 (after ALTER TABLE ADD COLUMN)
{
  "schema_id": "sch_v2_def",
  "table_id": "tbl_xyz789",
  "version": 2,
  "arrow_schema": {
    "fields": [
      {"name": "message_id", "type": "Utf8", "nullable": false},
      {"name": "content", "type": "Utf8", "nullable": false},
      {"name": "user_id", "type": "Utf8", "nullable": false},
      {"name": "_updated", "type": "Timestamp", "nullable": false},
      {"name": "_deleted", "type": "Boolean", "nullable": false}
    ]
  },
  "created_at": "2025-10-15T11:00:00Z",
  "changes": "ADD COLUMN user_id STRING"
}
```

### System Columns (Automatic)
All user/shared tables automatically get:
- `_updated` (TIMESTAMP): Last modification time
- `_deleted` (BOOLEAN): Soft delete flag

Stream tables do NOT get system columns.

---

## Type-Safe Wrappers

To prevent string-based bugs, use newtype pattern:

```rust
struct NamespaceId(String);
struct TableName(String);
struct UserId(String);
struct ConnectionId(String);
struct LiveId(String);

enum TableType {
    User,
    Shared,
    System,
    Stream,
}
```

These types enforce correct usage at compile time and prevent accidentally passing wrong identifiers.

---

## Summary

**Core Entities**: 4 (Namespace, TableMetadata, FlushPolicy, StorageLocation)  
**System Tables**: 7 (users, live_queries, storage_locations, jobs, namespaces, tables, table_schemas)  
**RocksDB Column Families**: 
- Dynamic per table (user_table:*, shared_table:*, stream_table:*)
- System tables: 7 CFs (system_users, system_live_queries, system_storage_locations, system_jobs, system_namespaces, system_tables, system_table_schemas)
- Flush tracking: 1 CF (user_table_counters)

**Unified SQL Engine**: kalamdb-sql crate manages all 7 system tables via SQL interface

**Storage Architecture**: 
- ✅ RocksDB-only for all metadata (no JSON config files)
- ✅ All system tables managed by kalamdb-sql
- ✅ Schema versions stored in system_table_schemas
- ✅ Per-user flush tracking in user_table_counters

**Next Step**: API contracts (REST + WebSocket) - no changes needed, already completed
