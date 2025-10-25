# Enhanced System Tables Schema Contract

**Version**: 1.0.0  
**Feature**: System Improvements and Performance Optimization  
**Language**: SQL (KalamDB system tables)

## Purpose

This document defines the enhanced schema for system tables with new columns for observability, cluster awareness, and resource tracking.

---

## system.jobs (Enhanced)

### Purpose
Track background jobs (flush operations, compactions, maintenance) with enhanced resource and execution metadata.

### Schema

```sql
CREATE TABLE system.jobs (
    job_id UUID PRIMARY KEY,
    job_type TEXT NOT NULL,              -- Job category: 'flush', 'compact', 'backup', etc.
    status TEXT NOT NULL,                -- 'pending', 'running', 'completed', 'failed', 'cancelled'
    parameters JSONB,                    -- NEW: Job input parameters as JSON array
    result TEXT,                         -- NEW: Job outcome/summary
    trace TEXT,                          -- NEW: Execution context (node, actor, thread)
    memory_used BIGINT,                  -- NEW: Memory consumed (bytes)
    cpu_used BIGINT,                     -- NEW: CPU time (microseconds)
    created_at TIMESTAMP DEFAULT now(),
    started_at TIMESTAMP,
    completed_at TIMESTAMP,
    error TEXT
);

CREATE INDEX idx_jobs_status ON system.jobs(status);
CREATE INDEX idx_jobs_type ON system.jobs(job_type);
CREATE INDEX idx_jobs_created ON system.jobs(created_at DESC);
```

### Column Definitions

| Column | Type | Nullable | Description |
|--------|------|----------|-------------|
| `job_id` | UUID | No | Unique job identifier |
| `job_type` | TEXT | No | Job category: 'flush', 'compact', 'backup', 'migrate' |
| `status` | TEXT | No | Current state: 'pending', 'running', 'completed', 'failed', 'cancelled' |
| `parameters` | JSONB | Yes | **NEW**: Input parameters as JSON array (e.g., `["namespace", "table"]`) |
| `result` | TEXT | Yes | **NEW**: Human-readable summary of job outcome |
| `trace` | TEXT | Yes | **NEW**: Execution location (node ID, actor ID, thread ID) |
| `memory_used` | BIGINT | Yes | **NEW**: Peak memory usage in bytes |
| `cpu_used` | BIGINT | Yes | **NEW**: Total CPU time in microseconds |
| `created_at` | TIMESTAMP | No | Job creation time (default: now()) |
| `started_at` | TIMESTAMP | Yes | Job execution start time |
| `completed_at` | TIMESTAMP | Yes | Job completion time |
| `error` | TEXT | Yes | Error message if status = 'failed' |

### New Fields Details

#### parameters (JSONB)
JSON array containing job input parameters. Structure varies by `job_type`:

**Flush job**:
```json
["namespace_id", "table_name", "user_id"]
```

**Compact job**:
```json
["partition_name"]
```

**Backup job**:
```json
["namespace_id", "backup_location", "include_system_tables"]
```

#### result (TEXT)
Human-readable summary of job execution:

**Examples**:
```
"Flushed 10,000 records to ./data/storage/chat/users/jamal/messages/j/batch-2025-10-21-143022.parquet"
"Compacted 12 files into 3 files, saved 15.3 MB"
"Backup completed: 3 namespaces, 15 tables, 5.2 GB"
```

#### trace (TEXT)
Execution context for debugging and cluster coordination:

**Format**: `<node-id>:<actor-id>:<thread-id>`

**Examples**:
```
"node-1:flush-actor-3:thread-42"
"node-2:compact-actor-1:thread-18"
"localhost:scheduler:main"
```

#### memory_used (BIGINT)
Peak memory allocation in bytes during job execution. Measured via system allocator tracking.

**Examples**:
```
52428800   (50 MB)
1073741824 (1 GB)
```

#### cpu_used (BIGINT)
Total CPU time consumed in microseconds. Includes all threads spawned by the job.

**Examples**:
```
1500000    (1.5 seconds)
30000000   (30 seconds)
```

### Query Examples

```sql
-- Get all running jobs
SELECT * FROM system.jobs WHERE status = 'running';

-- Get recent flush jobs with metrics
SELECT 
    job_id,
    parameters->0 AS namespace,
    parameters->1 AS table_name,
    result,
    memory_used / 1024 / 1024 AS memory_mb,
    cpu_used / 1000000 AS cpu_seconds
FROM system.jobs
WHERE job_type = 'flush'
ORDER BY created_at DESC
LIMIT 10;

-- Find jobs consuming most memory
SELECT 
    job_type,
    AVG(memory_used) / 1024 / 1024 AS avg_memory_mb,
    MAX(memory_used) / 1024 / 1024 AS max_memory_mb
FROM system.jobs
WHERE status = 'completed'
GROUP BY job_type;

-- Get job execution time distribution
SELECT 
    job_type,
    AVG(EXTRACT(EPOCH FROM (completed_at - started_at))) AS avg_duration_seconds,
    MIN(EXTRACT(EPOCH FROM (completed_at - started_at))) AS min_duration_seconds,
    MAX(EXTRACT(EPOCH FROM (completed_at - started_at))) AS max_duration_seconds
FROM system.jobs
WHERE status = 'completed'
GROUP BY job_type;

-- Find failed jobs on specific node
SELECT * FROM system.jobs
WHERE status = 'failed' AND trace LIKE 'node-1:%'
ORDER BY created_at DESC;
```

---

## system.live_queries (Enhanced)

### Purpose
Track active WebSocket subscriptions with subscription options, notification counters, and cluster node information.

### Schema

```sql
CREATE TABLE system.live_queries (
    live_id UUID PRIMARY KEY,
    user_id TEXT NOT NULL,
    namespace_id TEXT NOT NULL,
    table_name TEXT NOT NULL,
    query TEXT NOT NULL,                 -- SQL query (SELECT statement)
    options JSONB,                       -- NEW: Subscription configuration
    changes BIGINT DEFAULT 0,            -- NEW: Total notifications delivered
    node TEXT,                           -- NEW: Cluster node identifier
    created_at TIMESTAMP DEFAULT now(),
    last_update TIMESTAMP DEFAULT now()
);

CREATE INDEX idx_live_queries_user ON system.live_queries(user_id);
CREATE INDEX idx_live_queries_table ON system.live_queries(namespace_id, table_name);
CREATE INDEX idx_live_queries_node ON system.live_queries(node);
```

### Column Definitions

| Column | Type | Nullable | Description |
|--------|------|----------|-------------|
| `live_id` | UUID | No | Unique subscription identifier |
| `user_id` | TEXT | No | User who created the subscription |
| `namespace_id` | TEXT | No | Table namespace |
| `table_name` | TEXT | No | Table name being monitored |
| `query` | TEXT | No | SQL SELECT query defining subscription filter |
| `options` | JSONB | Yes | **NEW**: Subscription configuration (last_rows, buffer_size, etc.) |
| `changes` | BIGINT | No | **NEW**: Total change notifications delivered (default: 0) |
| `node` | TEXT | Yes | **NEW**: Cluster node hosting WebSocket connection |
| `created_at` | TIMESTAMP | No | Subscription creation time |
| `last_update` | TIMESTAMP | No | Last change notification time |

### New Fields Details

#### options (JSONB)
JSON object containing subscription configuration:

**Structure**:
```json
{
  "last_rows": 100,
  "buffer_size": 1000
}
```

**Fields**:
- `last_rows` (integer, optional): Number of recent rows to fetch immediately (0 = none)
- `buffer_size` (integer): WebSocket event buffer size (default: 1000)

**Examples**:
```json
// Basic subscription (no initial data)
{"buffer_size": 1000}

// With initial data fetch
{"last_rows": 100, "buffer_size": 1000}

// Large buffer for high-frequency changes
{"last_rows": 50, "buffer_size": 5000}
```

#### changes (BIGINT)
Counter incremented with each change notification delivered to the client. Useful for:
- Monitoring subscription activity
- Detecting stuck/idle subscriptions
- Capacity planning (high-change tables)

**Examples**:
```
0        -- New subscription, no changes yet
1523     -- Delivered 1,523 notifications
1000000  -- Million notifications (active table)
```

#### node (TEXT)
Identifier of the cluster node hosting the WebSocket connection. Format is implementation-defined.

**Examples**:
```
"node-1"
"server-abc"
"localhost"
"us-west-2-instance-3"
```

**Use Cases**:
- Cluster failover (reconnect subscriptions from failed node)
- Load balancing (distribute subscriptions across nodes)
- Debugging (which node has high subscription load)

### Query Examples

```sql
-- Get all subscriptions for a user
SELECT * FROM system.live_queries WHERE user_id = 'jamal';

-- Get subscriptions with initial data fetch
SELECT 
    live_id,
    table_name,
    options->>'last_rows' AS initial_rows,
    changes
FROM system.live_queries
WHERE options->>'last_rows' IS NOT NULL;

-- Find most active subscriptions (by change count)
SELECT 
    user_id,
    namespace_id || '.' || table_name AS table,
    changes,
    changes / EXTRACT(EPOCH FROM (now() - created_at)) AS changes_per_second
FROM system.live_queries
ORDER BY changes DESC
LIMIT 10;

-- Get subscriptions per node (cluster load distribution)
SELECT 
    node,
    COUNT(*) AS subscription_count,
    SUM(changes) AS total_changes
FROM system.live_queries
GROUP BY node
ORDER BY subscription_count DESC;

-- Find idle subscriptions (no changes in last hour)
SELECT * FROM system.live_queries
WHERE last_update < now() - INTERVAL '1 hour';

-- Get subscriptions to specific table
SELECT 
    live_id,
    user_id,
    query,
    options,
    changes
FROM system.live_queries
WHERE namespace_id = 'chat' AND table_name = 'messages';

-- Find subscriptions with large buffers (potential memory pressure)
SELECT 
    live_id,
    table_name,
    options->>'buffer_size' AS buffer_size,
    node
FROM system.live_queries
WHERE (options->>'buffer_size')::int > 2000;
```

---

## system.storages (Renamed)

### Purpose
Track storage locations for flushed data. Renamed from `system.storage_locations` for consistency.

### Schema

```sql
CREATE TABLE system.storages (
    storage_id UUID PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,           -- Storage location name
    base_path TEXT NOT NULL,             -- Base directory path
    path_template TEXT NOT NULL,         -- Path template with variables
    created_at TIMESTAMP DEFAULT now()
);
```

### Migration

**Table renamed**: `system.storage_locations` → `system.storages`

**Migration SQL**:
```sql
ALTER TABLE system.storage_locations RENAME TO system.storages;
```

All references in code, documentation, and configuration updated to use `system.storages`.

### Column Definitions

| Column | Type | Nullable | Description |
|--------|------|----------|-------------|
| `storage_id` | UUID | No | Unique storage location identifier |
| `name` | TEXT | No | Storage name (e.g., "primary", "backup", "archive") |
| `base_path` | TEXT | No | Base directory path (e.g., "./data/storage") |
| `path_template` | TEXT | No | Path template with variables (e.g., "{storageLocation}/{namespace}/users/{userId}/{tableName}/") |
| `created_at` | TIMESTAMP | No | Storage location creation time |

### Query Examples

```sql
-- List all storage locations
SELECT * FROM system.storages;

-- Get storage by name
SELECT * FROM system.storages WHERE name = 'primary';

-- Find storages with specific base path
SELECT * FROM system.storages WHERE base_path LIKE '/mnt/storage%';
```

---

## ~~system.table_schemas (DEPRECATED)~~

> **⚠️ DEPRECATED**: This table has been replaced by `information_schema.tables` which stores schema history inline within the `TableDefinition` struct. This section is kept for historical reference only.

### Purpose
~~Store schema history for all tables to enable schema versioning and DESCRIBE TABLE history.~~

**Replacement**: Schema versions are now stored in `information_schema.tables` as a `schema_history` array within each `TableDefinition` document.

### Schema

```sql
-- DEPRECATED - Do not use
CREATE TABLE system.table_schemas (
    schema_id UUID PRIMARY KEY,
    namespace_id TEXT NOT NULL,
    table_name TEXT NOT NULL,
    schema_version BIGINT NOT NULL,      -- Monotonically increasing version
    schema_json JSONB NOT NULL,          -- Arrow schema as JSON
    created_at TIMESTAMP DEFAULT now(),
    INDEX (namespace_id, table_name, schema_version)
);
```

### Column Definitions

| Column | Type | Nullable | Description |
|--------|------|----------|-------------|
| `schema_id` | UUID | No | Unique schema version identifier |
| `namespace_id` | TEXT | No | Table namespace |
| `table_name` | TEXT | No | Table name |
| `schema_version` | BIGINT | No | Schema version number (starts at 1, increments on ALTER TABLE) |
| `schema_json` | JSONB | No | Arrow schema serialized as JSON (columns, types, nullability) |
| `created_at` | TIMESTAMP | No | Schema version creation time |

### schema_json Structure

**Format**: Arrow schema JSON representation

**Example**:
```json
{
  "fields": [
    {
      "name": "id",
      "data_type": "Int64",
      "nullable": false,
      "metadata": {}
    },
    {
      "name": "user_id",
      "data_type": "Utf8",
      "nullable": false,
      "metadata": {}
    },
    {
      "name": "content",
      "data_type": "Utf8",
      "nullable": false,
      "metadata": {}
    },
    {
      "name": "created_at",
      "data_type": "Timestamp(Microsecond, None)",
      "nullable": false,
      "metadata": {}
    },
    {
      "name": "metadata",
      "data_type": "Struct([...])",
      "nullable": true,
      "metadata": {}
    }
  ]
}
```

### Query Examples

```sql
-- Get all schema versions for a table
SELECT 
    schema_version,
    created_at,
    jsonb_array_length(schema_json->'fields') AS column_count
FROM system.table_schemas
WHERE namespace_id = 'chat' AND table_name = 'messages'
ORDER BY schema_version DESC;

-- Get current schema version
SELECT * FROM system.table_schemas
WHERE namespace_id = 'chat' AND table_name = 'messages'
ORDER BY schema_version DESC
LIMIT 1;

-- Find when column was added
SELECT 
    schema_version,
    created_at
FROM system.table_schemas
WHERE namespace_id = 'chat' 
  AND table_name = 'messages'
  AND schema_json->'fields' @> '[{"name": "metadata"}]'
ORDER BY schema_version ASC
LIMIT 1;

-- Get schema differences between versions
SELECT 
    v1.schema_version AS old_version,
    v2.schema_version AS new_version,
    v1.schema_json AS old_schema,
    v2.schema_json AS new_schema
FROM system.table_schemas v1
JOIN system.table_schemas v2 
  ON v1.namespace_id = v2.namespace_id 
  AND v1.table_name = v2.table_name
WHERE v1.namespace_id = 'chat'
  AND v1.table_name = 'messages'
  AND v2.schema_version = v1.schema_version + 1;
```

---

## System Table Query Patterns

### Monitoring Active Operations

```sql
-- Get all active background jobs
SELECT 
    j.job_type,
    j.status,
    j.parameters,
    j.started_at,
    EXTRACT(EPOCH FROM (now() - j.started_at)) AS running_seconds
FROM system.jobs j
WHERE j.status = 'running'
ORDER BY j.started_at;

-- Get all active subscriptions with activity metrics
SELECT 
    lq.user_id,
    lq.namespace_id || '.' || lq.table_name AS table,
    lq.changes,
    lq.node,
    EXTRACT(EPOCH FROM (now() - lq.last_update)) AS seconds_since_last_change
FROM system.live_queries lq
ORDER BY lq.changes DESC;
```

### Performance Analysis

```sql
-- Average flush job duration and resource usage
SELECT 
    AVG(EXTRACT(EPOCH FROM (completed_at - started_at))) AS avg_duration_seconds,
    AVG(memory_used) / 1024 / 1024 AS avg_memory_mb,
    AVG(cpu_used) / 1000000 AS avg_cpu_seconds,
    COUNT(*) AS job_count
FROM system.jobs
WHERE job_type = 'flush' AND status = 'completed'
  AND created_at > now() - INTERVAL '24 hours';

-- Subscription changes per table
SELECT 
    namespace_id || '.' || table_name AS table,
    COUNT(*) AS subscription_count,
    SUM(changes) AS total_changes,
    AVG(changes) AS avg_changes_per_subscription
FROM system.live_queries
GROUP BY namespace_id, table_name
ORDER BY total_changes DESC;
```

### Operational Troubleshooting

```sql
-- Find failed jobs in last hour
SELECT 
    job_id,
    job_type,
    parameters,
    error,
    trace,
    created_at
FROM system.jobs
WHERE status = 'failed'
  AND created_at > now() - INTERVAL '1 hour'
ORDER BY created_at DESC;

-- Find stuck subscriptions (no updates in 10 minutes)
SELECT 
    live_id,
    user_id,
    table_name,
    changes,
    last_update,
    node
FROM system.live_queries
WHERE last_update < now() - INTERVAL '10 minutes'
ORDER BY last_update;
```

---

## Migration Guide

### From Older Versions

**Step 1**: Add new columns to existing tables

```sql
-- system.jobs
ALTER TABLE system.jobs ADD COLUMN parameters JSONB;
ALTER TABLE system.jobs ADD COLUMN result TEXT;
ALTER TABLE system.jobs ADD COLUMN trace TEXT;
ALTER TABLE system.jobs ADD COLUMN memory_used BIGINT;
ALTER TABLE system.jobs ADD COLUMN cpu_used BIGINT;

-- system.live_queries
ALTER TABLE system.live_queries ADD COLUMN options JSONB;
ALTER TABLE system.live_queries ADD COLUMN changes BIGINT DEFAULT 0;
ALTER TABLE system.live_queries ADD COLUMN node TEXT;

-- Rename storage locations
ALTER TABLE system.storage_locations RENAME TO system.storages;
```

**Step 2**: Create new table

```sql
CREATE TABLE system.table_schemas (
    schema_id UUID PRIMARY KEY,
    namespace_id TEXT NOT NULL,
    table_name TEXT NOT NULL,
    schema_version BIGINT NOT NULL,
    schema_json JSONB NOT NULL,
    created_at TIMESTAMP DEFAULT now()
);

CREATE INDEX idx_table_schemas_lookup 
ON system.table_schemas(namespace_id, table_name, schema_version);
```

**Step 3**: Backfill current schemas

```sql
-- Insert current schema for each existing table as version 1
INSERT INTO system.table_schemas (schema_id, namespace_id, table_name, schema_version, schema_json)
SELECT 
    gen_random_uuid(),
    namespace_id,
    table_name,
    1,
    '<current_schema_as_json>'
FROM system.tables;
```

---

## Backward Compatibility

**Queries without new columns**: Existing queries continue to work (new columns are nullable or have defaults).

**Applications**: Old applications can query tables without using new columns.

**Indexes**: New indexes do not affect existing query performance.

**Table rename**: `system.storage_locations` queries fail after migration; must update to `system.storages`.

---

## Version History

- **1.0.0** (2025-10-21): Initial specification
  - Enhanced system.jobs with parameters, result, trace, memory_used, cpu_used
  - Enhanced system.live_queries with options, changes, node
  - Renamed system.storage_locations → system.storages
  - Added system.table_schemas for schema versioning
