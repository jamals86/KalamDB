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

**Storage**: `conf/namespaces.json` (JSON file)

**Example**:
```json
{
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
- Schema directory: `conf/{namespace}/schemas/{table_name}/`

**Storage**: `conf/{namespace}/schemas/{table_name}/manifest.json`

**Example**:
```json
{
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

**Storage**: `conf/storage_locations.json`

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

**Storage**: RocksDB column family `system_table:users`

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

**Storage**: RocksDB column family `system_table:live_queries`

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

Predefined storage locations (same as StorageLocation entity above).

**Purpose**: Queryable via SQL instead of reading JSON file.

**Fields**: Same as StorageLocation entity

**Storage**: RocksDB column family `system_table:storage_locations`

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

**Storage**: RocksDB column family `system_table:jobs`

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

## Relationships

### Namespace → TableMetadata (1:N)
- One namespace contains many tables
- Cascade: Cannot delete namespace if tables exist
- Integrity: `table_count` updated on table create/drop

### Namespace → StorageLocation (N:M via table references)
- Namespaces don't directly reference locations
- Tables reference locations via `storage_location` field
- Cascade: Cannot delete location if `usage_count > 0`

### TableMetadata → FlushPolicy (1:1 embedded)
- Each table has exactly one flush policy
- Embedded in table metadata (not separate entity)

### system.users → system.live_queries (1:N)
- One user can have many live subscriptions
- Cascade: Subscriptions deleted when WebSocket disconnects

### TableMetadata → system.jobs (1:N)
- One table can have many flush/cleanup jobs
- No cascade: Jobs retained for audit even after table drop

---

## State Transitions

### Namespace Lifecycle
```
[CREATE] → Active
Active → [DROP if table_count = 0] → Deleted
Active → [DROP if table_count > 0] → ERROR
```

### Table Lifecycle
```
[CREATE] → Active
Active → [ALTER] → Active (schema_version++)
Active → [DROP if no live_queries] → Deleted
Active → [DROP if live_queries exist] → ERROR
```

### Job Lifecycle
```
[START] → Running
Running → [SUCCESS] → Completed
Running → [FAILURE] → Failed
```

### Live Query Lifecycle
```
[SUBSCRIBE] → Active
Active → [WebSocket disconnect] → Deleted
Active → [KILL LIVE QUERY] → Deleted
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

### System Tables
**Column Family**: `system_table:{table_name}`  
**Key Format**: Varies by table
- `system.users`: `{user_id}`
- `system.live_queries`: `{live_id}`
- `system.storage_locations`: `{location_name}`
- `system.jobs`: `{job_id}`

---

## Schema Versioning

### Schema Directory Structure
```
conf/{namespace}/schemas/{table_name}/
├── manifest.json              # Version history
├── schema_v1.json            # Initial schema
├── schema_v2.json            # After ALTER TABLE
├── schema_v3.json            # After another ALTER
└── current.json              # Symlink → schema_v3.json
```

### manifest.json Format
```json
{
  "table_name": "messages",
  "namespace": "app",
  "current_version": 3,
  "versions": [
    {
      "version": 1,
      "created_at": "2025-10-14T10:00:00Z",
      "changes": "Initial schema"
    },
    {
      "version": 2,
      "created_at": "2025-10-15T11:00:00Z",
      "changes": "ADD COLUMN user_id STRING"
    },
    {
      "version": 3,
      "created_at": "2025-10-15T12:00:00Z",
      "changes": "ADD COLUMN metadata JSON"
    }
  ]
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
**System Tables**: 4 (users, live_queries, storage_locations, jobs)  
**RocksDB Column Families**: Dynamic (per table) + 4 system tables  
**Configuration Files**: 2 (namespaces.json, storage_locations.json) + per-table schema dirs

**Next Step**: Create API contracts (REST + WebSocket)
