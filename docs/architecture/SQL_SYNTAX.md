# KalamDB SQL Syntax Reference

**Version**: 0.1.0  
**SQL Engine**: Apache DataFusion 35.0

## Overview

KalamDB supports standard SQL via Apache DataFusion with custom DDL extensions for namespace, table, and backup operations.

---

## Table of Contents

1. [Namespace Operations](#namespace-operations)
2. [Storage Management](#storage-management)
3. [User Table Operations](#user-table-operations)
4. [Shared Table Operations](#shared-table-operations)
5. [Stream Table Operations](#stream-table-operations)
6. [Schema Evolution](#schema-evolution)
7. [Data Manipulation](#data-manipulation)
8. [Manual Flushing](#manual-flushing)
9. [Live Query Subscriptions](#live-query-subscriptions)
10. [Backup and Restore](#backup-and-restore)
11. [Catalog Browsing](#catalog-browsing)
12. [Data Types](#data-types)
13. [System Columns](#system-columns)

---

## Namespace Operations

Namespaces are logical containers for tables (similar to databases/schemas in traditional RDBMS).

### CREATE NAMESPACE

```sql
CREATE NAMESPACE <namespace_name>;
CREATE NAMESPACE IF NOT EXISTS <namespace_name>;
```

**Examples**:
```sql
CREATE NAMESPACE app;
CREATE NAMESPACE IF NOT EXISTS production;
CREATE NAMESPACE dev_environment;
```

**Notes**:
- Namespace names must be unique
- Use alphanumeric characters and underscores only
- Case-sensitive

---

### DROP NAMESPACE

```sql
DROP NAMESPACE <namespace_name>;
DROP NAMESPACE IF EXISTS <namespace_name>;
```

**Examples**:
```sql
DROP NAMESPACE app;
DROP NAMESPACE IF EXISTS old_namespace;
```

**Warning**: Drops all tables in the namespace and deletes all data (including Parquet files).

---

## Storage Management

Storage locations define where table data (Parquet files) are stored. Each table references a storage_id from the `system.storages` table.

### CREATE STORAGE

```sql
CREATE STORAGE <storage_id>
TYPE <filesystem|s3|azure_blob|gcs>
PATH '<storage_path>'
[CREDENTIALS <credentials_json>];
```

**Storage Types**:
- `filesystem`: Local or network filesystem storage
- `s3`: Amazon S3 or S3-compatible storage (MinIO, etc.)
- `azure_blob`: Azure Blob Storage
- `gcs`: Google Cloud Storage

**Examples**:
```sql
-- Local filesystem storage (default)
CREATE STORAGE local
TYPE filesystem
PATH './data';

-- S3 storage with credentials
CREATE STORAGE s3_prod
TYPE s3
PATH 's3://my-bucket/kalamdb-data'
CREDENTIALS '{
  "access_key_id": "AKIAIOSFODNN7EXAMPLE",
  "secret_access_key": "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
  "region": "us-west-2"
}';

-- S3-compatible storage (MinIO)
CREATE STORAGE minio_local
TYPE s3
PATH 's3://kalamdb/data'
CREDENTIALS '{
  "access_key_id": "minioadmin",
  "secret_access_key": "minioadmin",
  "endpoint": "http://localhost:9000",
  "region": "us-east-1"
}';

-- Azure Blob Storage
CREATE STORAGE azure_prod
TYPE azure_blob
PATH 'azure://container-name/kalamdb'
CREDENTIALS '{
  "account_name": "mystorageaccount",
  "account_key": "base64-encoded-key"
}';

-- Google Cloud Storage
CREATE STORAGE gcs_backup
TYPE gcs
PATH 'gs://my-bucket/kalamdb-backups'
CREDENTIALS '{
  "service_account_key": "path/to/service-account.json"
}';
```

**Notes**:
- Storage ID 'local' is pre-configured and always available
- Credentials are stored encrypted in `system.storages`
- Path supports template variables: `${namespace}`, `${table_name}`, `${user_id}`

---

### ALTER STORAGE

```sql
ALTER STORAGE <storage_id>
[SET PATH '<new_path>']
[SET CREDENTIALS <credentials_json>];
```

**Examples**:
```sql
-- Update S3 path
ALTER STORAGE s3_prod
SET PATH 's3://new-bucket/kalamdb';

-- Rotate credentials
ALTER STORAGE s3_prod
SET CREDENTIALS '{
  "access_key_id": "NEW_ACCESS_KEY",
  "secret_access_key": "NEW_SECRET_KEY",
  "region": "us-west-2"
}';

-- Update both
ALTER STORAGE azure_prod
SET PATH 'azure://new-container/kalamdb'
SET CREDENTIALS '{
  "account_name": "newaccount",
  "account_key": "new-key"
}';
```

---

### DROP STORAGE

```sql
DROP STORAGE <storage_id>;
DROP STORAGE IF EXISTS <storage_id>;
```

**Examples**:
```sql
DROP STORAGE old_s3_storage;
DROP STORAGE IF EXISTS temp_storage;
```

**Restrictions**:
- Cannot drop 'local' storage (system reserved)
- Cannot drop storage if tables are using it (check `system.tables` first)

**Error Handling**:
```sql
DROP STORAGE s3_prod;
-- ERROR: Cannot drop storage 's3_prod': 3 table(s) still reference it
-- Hint: DROP tables first or migrate them to different storage
```

---

### SHOW STORAGES

```sql
SHOW STORAGES;
```

**Example Output**:
```
storage_id   | type       | path                          | table_count
-------------|------------|-------------------------------|------------
local        | filesystem | ./data                        | 5
s3_prod      | s3         | s3://my-bucket/kalamdb-data  | 12
minio_local  | s3         | s3://kalamdb/data            | 3
```

**Notes**:
- Credentials are not displayed (security)
- `table_count` shows how many tables reference each storage
- Query `system.storages` table directly for full details

---

## User Table Operations

User tables create one table instance per user with isolated storage.

### CREATE USER TABLE

```sql
CREATE USER TABLE [<namespace>.]<table_name> (
  <column_name> <data_type> [NOT NULL] [DEFAULT <value|function>],
  ...
) [STORAGE <storage_id>] [FLUSH <flush_policy>];
```

**Storage Options**:
- `STORAGE <storage_id>`: Storage location reference from `system.storages` (defaults to `local`)
- Storage IDs must be pre-configured via `CREATE STORAGE` command

**Flush Policies** (new syntax):
- `FLUSH ROW_THRESHOLD <n>`: Flush after `n` rows inserted
- `FLUSH INTERVAL <seconds>s`: Flush every `<seconds>` seconds
- `FLUSH INTERVAL <s>s ROW_THRESHOLD <n>`: Flush when either condition is met (combined)

**Legacy Flush Syntax** (still supported):
- `FLUSH ROWS <n>`: Same as `ROW_THRESHOLD`
- `FLUSH SECONDS <s>`: Same as `INTERVAL`

**Examples**:
```sql
-- Simple user table with row-based flush (new syntax)
CREATE USER TABLE app.messages2 (
  id BIGINT NOT NULL DEFAULT SNOWFLAKE_ID(),
  content TEXT,
  author TEXT,
  timestamp TIMESTAMP
) STORAGE local FLUSH ROW_THRESHOLD 1000;

-- Time-based flush (every 5 minutes)
CREATE USER TABLE app.events (
  event_id TEXT NOT NULL,
  event_type TEXT,
  data TEXT
) STORAGE local FLUSH INTERVAL 300s;

-- Combined flush (flush when either 5000 rows OR 10 minutes)
CREATE USER TABLE app.analytics (
  metric_name TEXT NOT NULL,
  value DOUBLE,
  tags TEXT
) STORAGE s3_prod FLUSH INTERVAL 600s ROW_THRESHOLD 5000;

-- With DEFAULT values (auto-generated IDs and timestamps)
CREATE USER TABLE app.users (
  id BIGINT DEFAULT SNOWFLAKE_ID(),
  username TEXT NOT NULL,
  email TEXT,
  created_at TIMESTAMP DEFAULT NOW()
) STORAGE local FLUSH ROW_THRESHOLD 1000;

-- Legacy syntax (still works)
CREATE USER TABLE app.logs (
  id BIGINT,
  message TEXT
) STORAGE local FLUSH ROWS 500;
```

**System Columns** (automatically added):
- `_updated TIMESTAMP`: Last update timestamp (indexed for time-range queries)
- `_deleted BOOLEAN`: Soft delete flag (default: false)

**Storage**:
- Hot tier: `RocksDB column family: user_table:{table_name}`
- Cold tier (after flush): `{storage_path}/users/{user_id}/{namespace}/{table_name}/batch-{timestamp}-{uuid}.parquet`
- Storage path determined by storage_id from system.storages

**DEFAULT Functions**:
- `NOW()`: Current timestamp (for TIMESTAMP columns)
- `SNOWFLAKE_ID()`: Distributed unique ID (for BIGINT columns)
- `UUID_V7()`: Time-ordered UUID v7 (for TEXT/VARCHAR columns)
- `ULID()`: Universally unique lexicographically sortable ID (for TEXT/VARCHAR columns)

**Notes**:
- Storage defaults to 'local' if not specified
- Flush policy is optional (no auto-flush if not specified)
- User tables create isolated storage per user_id
- System columns cannot be specified in INSERT/UPDATE

---

### DROP USER TABLE

```sql
DROP TABLE [<namespace>.]<table_name>;
DROP TABLE IF EXISTS [<namespace>.]<table_name>;
```

**Examples**:
```sql
DROP TABLE app.messages;
DROP TABLE IF EXISTS app.old_table;
```

**Warning**: Deletes RocksDB column family and all Parquet files for all users.

---

## Shared Table Operations

Shared tables are accessible to all users with centralized storage.

### CREATE SHARED TABLE

```sql
CREATE SHARED TABLE [<namespace>.]<table_name> (
  <column_name> <data_type> [NOT NULL] [DEFAULT <value|function>],
  ...
) [STORAGE <storage_id>] [FLUSH <flush_policy>];
```

**Examples**:
```sql
-- Global configuration table
CREATE SHARED TABLE app.config (
  config_key TEXT NOT NULL,
  config_value TEXT,
  updated_at TIMESTAMP DEFAULT NOW()
) STORAGE local FLUSH ROW_THRESHOLD 100;

-- Shared analytics with combined flush
CREATE SHARED TABLE app.global_metrics (
  metric_name TEXT NOT NULL,
  value DOUBLE,
  timestamp TIMESTAMP DEFAULT NOW()
) STORAGE s3_shared FLUSH INTERVAL 60s ROW_THRESHOLD 1000;

-- Simple shared table (no auto-flush)
CREATE SHARED TABLE app.settings (
  setting_key TEXT NOT NULL,
  setting_value TEXT
) STORAGE local;
```

**System Columns** (automatically added):
- `_updated TIMESTAMP`: Last update timestamp
- `_deleted BOOLEAN`: Soft delete flag

**Storage**:
- Hot tier: `RocksDB column family: shared_table:{table_name}`
- Cold tier (after flush): `{storage_path}/shared/{namespace}/{table_name}/batch-{timestamp}-{uuid}.parquet`
- Accessible to all users (no per-user isolation)

---

### DROP SHARED TABLE

```sql
DROP TABLE [<namespace>.]<table_name>;
```

**Warning**: Deletes RocksDB column family and all Parquet files (global data).

---

## Stream Table Operations

Stream tables are ephemeral with TTL-based eviction. Data is memory-only (no Parquet files).

### CREATE STREAM TABLE

```sql
CREATE STREAM TABLE [<namespace>.]<table_name> (
  <column_name> <data_type> [NOT NULL],
  ...
) RETENTION <seconds> [EPHEMERAL] [MAX_BUFFER <n>];
```

**Options**:
- `RETENTION <seconds>`: TTL in seconds (rows expire after this duration)
- `EPHEMERAL`: Only buffer rows when subscribers exist (no buffering without subscribers)
- `MAX_BUFFER <n>`: Maximum buffer size in rows (oldest rows evicted when exceeded)

**Examples**:
```sql
-- Live events with 10-second retention
CREATE STREAM TABLE app.live_events (
  event_id TEXT NOT NULL,
  event_type TEXT,
  payload TEXT,
  timestamp TIMESTAMP
) RETENTION 10 EPHEMERAL MAX_BUFFER 10000;

-- Sensor data with 60-second retention
CREATE STREAM TABLE app.sensor_data (
  sensor_id TEXT NOT NULL,
  temperature DOUBLE,
  humidity DOUBLE,
  timestamp TIMESTAMP
) RETENTION 60 MAX_BUFFER 50000;

-- Ephemeral-only (no buffering without subscribers)
CREATE STREAM TABLE app.notifications (
  user_id TEXT NOT NULL,
  message TEXT,
  timestamp TIMESTAMP
) RETENTION 5 EPHEMERAL MAX_BUFFER 1000;
```

**Important**:
- **No system columns**: Stream tables do NOT have `_updated` or `_deleted` columns
- **Memory-only**: Data never written to Parquet files
- **Auto-eviction**: Old rows deleted when TTL expires or MAX_BUFFER exceeded

**Storage**:
- Hot tier only: `RocksDB column family: stream_table:{table_name}`
- No cold tier (ephemeral data)

---

### DROP STREAM TABLE

```sql
DROP TABLE [<namespace>.]<table_name>;
```

---

## Schema Evolution

### ALTER TABLE

Modify table schema (user tables and shared tables only - stream tables are immutable).

#### ADD COLUMN

```sql
ALTER TABLE [<namespace>.]<table_name> ADD COLUMN <column_name> <data_type>;
```

**Examples**:
```sql
ALTER TABLE app.messages ADD COLUMN reaction TEXT;
ALTER TABLE app.events ADD COLUMN priority INT;
```

**Notes**:
- New column is nullable by default
- Increments schema version
- Old Parquet files projected to new schema (new column filled with NULL)

---

#### DROP COLUMN

```sql
ALTER TABLE [<namespace>.]<table_name> DROP COLUMN <column_name>;
```

**Examples**:
```sql
ALTER TABLE app.messages DROP COLUMN reaction;
```

**Restrictions**:
- Cannot drop system columns (`_updated`, `_deleted`)
- Cannot drop required columns (would break existing queries)

---

#### RENAME COLUMN

```sql
ALTER TABLE [<namespace>.]<table_name> RENAME COLUMN <old_name> TO <new_name>;
```

**Examples**:
```sql
ALTER TABLE app.messages RENAME COLUMN content TO message_text;
```

**Restrictions**:
- Cannot rename system columns (`_updated`, `_deleted`)

---

### Restrictions

- **Stream tables**: Schema is immutable (cannot ALTER STREAM TABLE)
- **System columns**: Cannot alter `_updated` or `_deleted` columns
- **Active live queries**: ALTER TABLE fails if live queries are subscribed (prevents breaking changes)

---

## Data Manipulation

### INSERT

```sql
INSERT INTO [<namespace>.]<table_name> (<column1>, <column2>, ...)
VALUES (<value1>, <value2>, ...);

-- Batch insert
INSERT INTO [<namespace>.]<table_name> (<column1>, <column2>, ...)
VALUES
  (<value1a>, <value2a>, ...),
  (<value1b>, <value2b>, ...),
  (<value1c>, <value2c>, ...);
```

**Examples**:
```sql
-- Single insert
INSERT INTO app.messages (id, content, author, timestamp)
VALUES (1, 'Hello World', 'alice', NOW());

-- Batch insert
INSERT INTO app.messages (id, content) VALUES
  (2, 'Message 1'),
  (3, 'Message 2'),
  (4, 'Message 3');

-- Stream table insert
INSERT INTO app.live_events (event_id, event_type, payload, timestamp)
VALUES ('evt_123', 'user_action', '{"action":"click"}', NOW());
```

**Notes**:
- System columns (`_updated`, `_deleted`) are set automatically
- For user tables: Data written to user's isolated partition
- For shared tables: Data written to shared partition
- For stream tables: Data buffered in memory (ephemeral mode = no buffering without subscribers)

---

### UPDATE

```sql
UPDATE [<namespace>.]<table_name>
SET <column1> = <value1>, <column2> = <value2>, ...
WHERE <condition>;
```

**Examples**:
```sql
-- Simple update
UPDATE app.messages
SET content = 'Updated message'
WHERE id = 1;

-- Update with multiple columns
UPDATE app.messages
SET content = 'New content', author = 'bob'
WHERE id = 2;

-- Conditional update
UPDATE app.messages
SET content = 'Read'
WHERE timestamp < NOW() - INTERVAL '1 day';
```

**Notes**:
- `_updated` column set to NOW() automatically
- Not supported for stream tables (immutable)

---

### DELETE

```sql
DELETE FROM [<namespace>.]<table_name>
WHERE <condition>;
```

**Examples**:
```sql
-- Delete single row
DELETE FROM app.messages WHERE id = 1;

-- Delete with condition
DELETE FROM app.messages
WHERE timestamp < NOW() - INTERVAL '7 days';
```

**Behavior**:
- **User/Shared tables**: Soft delete (sets `_deleted = true`)
- **Stream tables**: Hard delete (row removed immediately)

**Soft Delete Retention**:
- Default: 7 days (configurable via `default_deleted_retention_hours`)
- Soft-deleted rows remain in Parquet files for auditing/recovery
- Excluded from SELECT queries by default (use `WHERE _deleted = true` to query)

---

### SELECT

```sql
SELECT <columns>
FROM [<namespace>.]<table_name>
WHERE <condition>
ORDER BY <column>
LIMIT <n>;
```

**Examples**:
```sql
-- Basic query
SELECT * FROM app.messages LIMIT 10;

-- Filtered query
SELECT id, content, timestamp
FROM app.messages
WHERE timestamp > NOW() - INTERVAL '1 hour'
ORDER BY timestamp DESC
LIMIT 50;

-- Aggregation
SELECT author, COUNT(*) as message_count
FROM app.messages
GROUP BY author;

-- Join (across tables in same namespace)
SELECT m.id, m.content, u.username
FROM app.messages m
JOIN app.users u ON m.author = u.user_id;

-- Time-range query (uses _updated index)
SELECT * FROM app.messages
WHERE _updated >= NOW() - INTERVAL '1 day';

-- Include soft-deleted rows
SELECT * FROM app.messages WHERE _deleted = true;
```

**Notes**:
- Queries read from both hot (RocksDB) and cold (Parquet) storage
- DataFusion optimizes query execution (projection pushdown, filter pushdown, etc.)
- For user tables: Automatically filtered by current user's data

---

## Manual Flushing

Manual flushing allows you to explicitly trigger the flush of data from RocksDB (hot tier) to Parquet files (cold tier) without waiting for automatic flush policies.

### FLUSH TABLE

```sql
FLUSH TABLE [<namespace>.]<table_name>;
```

**Behavior**:
- Triggers asynchronous flush job (returns immediately with job_id)
- Flush executes in background via JobManager
- Only works for USER and SHARED tables (not STREAM tables)
- Prevents concurrent flushes on the same table

**Examples**:
```sql
-- Flush a user table
FLUSH TABLE app.messages;

-- Flush a shared table
FLUSH TABLE app.config;
```

**Response**:
```json
{
  "status": "success",
  "message": "Flush started for table 'app.messages'. Job ID: flush-messages-1761332099970-f14aa44d"
}
```

**Job Tracking**:
```sql
-- Check job status
SELECT job_id, status, result, created_at, completed_at
FROM system.jobs
WHERE job_id = 'flush-messages-1761332099970-f14aa44d';

-- View all flush jobs
SELECT * FROM system.jobs
WHERE job_id LIKE 'flush-%'
ORDER BY created_at DESC;
```

---

### FLUSH ALL TABLES

```sql
FLUSH ALL TABLES IN <namespace>;
```

**Behavior**:
- Triggers flush for all USER and SHARED tables in namespace
- Each table gets its own async flush job
- Returns array of job_ids (one per table)

**Examples**:
```sql
-- Flush all tables in app namespace
FLUSH ALL TABLES IN app;

-- Flush all tables in production namespace
FLUSH ALL TABLES IN production;
```

**Response**:
```json
{
  "status": "success",
  "message": "Flush started for 3 table(s) in namespace 'app'. Job IDs: [flush-messages-..., flush-config-..., flush-logs-...]"
}
```

---

### FLUSH Restrictions

**Stream Tables**:
```sql
FLUSH TABLE app.live_events;
-- ERROR: Cannot flush stream table 'app.live_events'. Only user and shared tables support flushing.
```

**Concurrent Flush Detection**:
```sql
-- First flush
FLUSH TABLE app.messages;
-- Returns: job_id_1

-- Immediate second flush (before first completes)
FLUSH TABLE app.messages;
-- ERROR: Flush job already running for table 'app.messages'
-- Or returns different job_id if first flush completed
```

**Non-Existent Table**:
```sql
FLUSH TABLE app.nonexistent;
-- ERROR: Table 'app.nonexistent' does not exist
```

---

### KILL JOB

Cancel a running flush job:

```sql
KILL JOB '<job_id>';
```

**Examples**:
```sql
-- Cancel a specific flush job
KILL JOB 'flush-messages-1761332099970-f14aa44d';

-- Response
-- "Job 'flush-messages-...' has been cancelled"
```

**Notes**:
- Only works for jobs in 'pending' or 'running' state
- Completed/failed jobs cannot be killed
- Job status updated to 'cancelled' in system.jobs

---

## Live Query Subscriptions

KalamDB supports real-time live queries through WebSocket subscriptions using the `SUBSCRIBE TO` SQL command. When data changes in a subscribed table, clients receive instant notifications over the WebSocket connection.

### SUBSCRIBE TO

The `SUBSCRIBE TO` command initiates a live query subscription, establishing a WebSocket connection for real-time change notifications.

```sql
SUBSCRIBE TO <namespace>.<table_name>
[WHERE <condition>]
[OPTIONS (last_rows=<n>)];
```

**Parameters**:
- `<namespace>.<table_name>`: **Required** - Fully qualified table name (namespace must be specified)
- `WHERE <condition>`: **Optional** - Filter condition for the subscription (only matching changes are sent)
- `OPTIONS (last_rows=<n>)`: **Optional** - Number of most recent rows to send as initial data

**Returns**: Subscription metadata with WebSocket connection details:
```json
{
  "status": "subscription_required",
  "ws_url": "ws://localhost:8080/v1/ws",
  "subscription": {
    "id": "sub-7f8e9d2a",
    "sql": "SELECT * FROM chat.messages WHERE user_id = 'user123'",
    "options": {"last_rows": 10}
  },
  "message": "WebSocket connection required for live query subscription"
}
```

**Examples**:

```sql
-- Basic subscription (all rows, all changes)
SUBSCRIBE TO chat.messages;

-- Subscription with filter (only messages from specific user)
SUBSCRIBE TO chat.messages WHERE user_id = 'user123';

-- Subscription with initial data (last 10 rows)
SUBSCRIBE TO chat.messages WHERE room_id = 'general' OPTIONS (last_rows=10);

-- Subscription with complex filter
SUBSCRIBE TO app.events 
WHERE event_type IN ('error', 'warning') 
  AND timestamp > NOW() - INTERVAL '1 hour';

-- Stream table subscription (ephemeral data)
SUBSCRIBE TO live.sensor_data WHERE sensor_id = 'temp-01';
```

### WebSocket Protocol

After receiving the subscription metadata from `SUBSCRIBE TO`, clients must:

1. **Connect to WebSocket** using the provided `ws_url`
2. **Authenticate** with JWT token in Authorization header
3. **Send subscription message** with the subscription details
4. **Receive notifications** for matching data changes

**WebSocket Connection Flow**:

```
1. Execute SUBSCRIBE TO via HTTP POST /v1/api/sql
   ↓
2. Receive subscription metadata (subscription_id, ws_url, sql)
   ↓
3. Connect WebSocket to ws_url with Authorization: Bearer <token>
   ↓
4. Send subscription message with subscription_id and sql
   ↓
5. Receive initial data (if last_rows specified)
   ↓
6. Receive real-time notifications for INSERT/UPDATE/DELETE
```

**Subscription Message** (Client → Server):
```json
{
  "subscriptions": [
    {
      "query_id": "messages-subscription",
      "sql": "SELECT * FROM chat.messages WHERE user_id = 'user123'",
      "options": {"last_rows": 10}
    }
  ]
}
```

**Initial Data Message** (Server → Client):
```json
{
  "type": "initial_data",
  "query_id": "messages-subscription",
  "rows": [
    {"id": 1, "content": "Hello", "user_id": "user123", "timestamp": "2025-10-20T10:00:00Z"},
    {"id": 2, "content": "World", "user_id": "user123", "timestamp": "2025-10-20T10:01:00Z"}
  ],
  "count": 2
}
```

**Change Notification** (Server → Client):
```json
{
  "query_id": "messages-subscription",
  "type": "INSERT",
  "data": {
    "id": 3,
    "content": "New message",
    "user_id": "user123",
    "timestamp": "2025-10-20T10:05:00Z"
  },
  "timestamp": "2025-10-20T10:05:00.123Z"
}
```

**Change Types**:
- `INSERT`: New row added matching the subscription filter
- `UPDATE`: Existing row modified (includes `old_values` field with previous data)
- `DELETE`: Row deleted matching the subscription filter

**UPDATE Notification** (includes old values):
```json
{
  "query_id": "messages-subscription",
  "type": "UPDATE",
  "data": {
    "id": 2,
    "content": "Updated message",
    "user_id": "user123",
    "timestamp": "2025-10-20T10:06:00Z"
  },
  "old_values": {
    "id": 2,
    "content": "World",
    "user_id": "user123",
    "timestamp": "2025-10-20T10:01:00Z"
  },
  "timestamp": "2025-10-20T10:06:00.456Z"
}
```

### Supported Table Types

- ✅ **User Tables**: Subscriptions are automatically filtered by authenticated user's data
- ✅ **Shared Tables**: Subscriptions receive changes from all users
- ✅ **Stream Tables**: Subscriptions receive ephemeral real-time events

### Subscription Lifecycle

**Active Subscription**:
- Registered in `system.live_queries` table
- Monitored by live query manager
- Sends notifications for all matching changes

**Automatic Cleanup**:
- When WebSocket connection closes
- When client explicitly unsubscribes
- After connection timeout (default: 10 seconds without heartbeat)

**Subscription Limits**:
- Max subscriptions per connection: Configurable (default: 10)
- Max connections per user: Configurable (default: 5)
- Rate limiting: Configurable via `RateLimiter`

### Query System.live_queries

View active subscriptions in the system table:

```sql
SELECT subscription_id, sql, user_id, created_at
FROM system.live_queries
WHERE user_id = 'user123';
```

### Client Libraries

**JavaScript/TypeScript Example**:
```javascript
// 1. Execute SUBSCRIBE TO via REST API
const response = await fetch('http://localhost:8080/v1/api/sql', {
  method: 'POST',
  headers: {
    'Authorization': `Bearer ${token}`,
    'Content-Type': 'application/json'
  },
  body: JSON.stringify({
    sql: 'SUBSCRIBE TO chat.messages WHERE user_id = \'user123\' OPTIONS (last_rows=10)'
  })
});

const {ws_url, subscription} = await response.json();

// 2. Connect to WebSocket
const ws = new WebSocket(ws_url, {
  headers: {'Authorization': `Bearer ${token}`}
});

// 3. Send subscription message
ws.onopen = () => {
  ws.send(JSON.stringify({
    subscriptions: [{
      query_id: 'messages-sub',
      sql: subscription.sql,
      options: subscription.options
    }]
  }));
};

// 4. Receive notifications
ws.onmessage = (event) => {
  const notification = JSON.parse(event.data);
  console.log('Change:', notification.type, notification.data);
};
```

**Rust Example** (using `tokio-tungstenite`):
```rust
use tokio_tungstenite::{connect_async, tungstenite::Message};
use serde_json::json;

// Execute SUBSCRIBE TO via HTTP client first...
let (subscription_id, ws_url, sql) = /* ... */;

// Connect to WebSocket
let (mut ws_stream, _) = connect_async(ws_url).await?;

// Send subscription message
let msg = json!({
    "subscriptions": [{
        "query_id": "messages-sub",
        "sql": sql,
        "options": {"last_rows": 10}
    }]
});
ws_stream.send(Message::Text(msg.to_string())).await?;

// Receive notifications
while let Some(msg) = ws_stream.next().await {
    let notification = serde_json::from_str(&msg?)?;
    println!("Change: {:?}", notification);
}
```

### Error Handling

**Missing Namespace**:
```sql
SUBSCRIBE TO messages;  -- ERROR: Namespace must be specified
```

**Invalid Syntax**:
```sql
SUBSCRIBE messages;  -- ERROR: Expected 'TO' after 'SUBSCRIBE'
```

**Invalid Options**:
```sql
SUBSCRIBE TO chat.messages OPTIONS (last_rows=abc);  
-- ERROR: Invalid option value: last_rows must be a positive integer
```

**Table Not Found**:
```sql
SUBSCRIBE TO chat.nonexistent;
-- ERROR: Table 'chat.nonexistent' does not exist
```

### Performance Considerations

**Filter Efficiency**:
- Use indexed columns in WHERE clauses when possible (`_updated` has bloom filter)
- Complex filters are evaluated in-memory for each change
- Consider table design for optimal subscription performance

**Initial Data**:
- `last_rows` option queries Parquet files and RocksDB
- Large `last_rows` values may impact initial connection time
- Default: No initial data sent (only real-time changes)

**Notification Delivery**:
- Notifications are batched for high-frequency changes
- WebSocket backpressure handling prevents client overwhelm
- Failed deliveries trigger automatic subscription cleanup

### Best Practices

1. **Specify Filters**: Use WHERE clauses to reduce notification volume
```sql
-- Good: Filtered subscription
SUBSCRIBE TO chat.messages WHERE room_id = 'general';

-- Bad: Unfiltered (all changes)
SUBSCRIBE TO chat.messages;
```

2. **Use Initial Data Sparingly**: Only request last_rows when needed
```sql
-- Good: Load initial context
SUBSCRIBE TO chat.messages WHERE room_id = 'general' OPTIONS (last_rows=10);

-- Avoid: Large initial data loads
SUBSCRIBE TO chat.messages OPTIONS (last_rows=100000);  -- Too much!
```

3. **Handle Connection Failures**: Implement reconnection logic with exponential backoff
```javascript
let reconnectAttempts = 0;
ws.onclose = () => {
  const delay = Math.min(1000 * Math.pow(2, reconnectAttempts), 30000);
  setTimeout(reconnect, delay);
  reconnectAttempts++;
};
```

4. **Clean Up Subscriptions**: Always close WebSocket connections when done
```javascript
// Clean up on component unmount / page unload
window.addEventListener('beforeunload', () => ws.close());
```

5. **Monitor Subscription Count**: Track active subscriptions to avoid hitting limits
```sql
SELECT COUNT(*) FROM system.live_queries WHERE user_id = 'user123';
```

### See Also

- [WebSocket Protocol Documentation](WEBSOCKET_PROTOCOL.md) - Detailed protocol specification
- [REST API Reference](API_REFERENCE.md) - HTTP endpoint for SUBSCRIBE TO
- [Live Query Architecture](adrs/ADR-015-live-query-architecture.md) - Implementation details

---

## Backup and Restore

### BACKUP DATABASE

```sql
BACKUP DATABASE <namespace> TO '<backup_path>';
```

**Examples**:
```sql
BACKUP DATABASE app TO '/backups/app-20251020';
BACKUP DATABASE production TO '/mnt/backups/prod-snapshot';
```

**What's Backed Up**:
- Namespace metadata
- All table metadata (schemas, flush policies, storage locations)
- All schema versions (system_table_schemas)
- All Parquet files (user tables + shared tables)
- Soft-deleted rows (preserved in Parquet files)

**What's NOT Backed Up**:
- Stream tables (ephemeral data, no Parquet files)
- RocksDB hot buffers (only cold storage)

**Output**:
- Manifest file: `{backup_path}/manifest.json`
- User table data: `{backup_path}/user_tables/{table_name}/{user_id}/batch-*.parquet`
- Shared table data: `{backup_path}/shared_tables/{table_name}/batch-*.parquet`

---

### RESTORE DATABASE

```sql
RESTORE DATABASE <namespace> FROM '<backup_path>';
```

**Examples**:
```sql
RESTORE DATABASE app FROM '/backups/app-20251020';
RESTORE DATABASE production FROM '/mnt/backups/prod-snapshot';
```

**Behavior**:
1. Validates backup manifest and Parquet file integrity
2. Creates namespace
3. Recreates all tables with metadata
4. Restores all schema versions
5. Copies Parquet files to active storage
6. Verifies checksums

**Notes**:
- RocksDB buffers start empty (data in cold storage only)
- Overwrites existing namespace (use DROP NAMESPACE first if needed)

---

### SHOW BACKUP

```sql
SHOW BACKUP '<backup_path>';
```

**Examples**:
```sql
SHOW BACKUP '/backups/app-20251020';
```

**Output**:
- Namespace name
- Created timestamp
- Table count
- Total rows
- Total bytes
- List of tables with metadata

---

## Catalog Browsing

### SHOW TABLES

```sql
SHOW TABLES;
SHOW TABLES IN <namespace>;
```

**Examples**:
```sql
SHOW TABLES;
SHOW TABLES IN app;
```

**Output**: List of tables with `table_name` and `table_type` (User/Shared/Stream).

---

### DESCRIBE TABLE

```sql
DESCRIBE TABLE [<namespace>.]<table_name>;
```

**Examples**:
```sql
DESCRIBE TABLE app.messages;
DESCRIBE TABLE production.analytics;
```

**Output**:
- Namespace
- Table name
- Table type (User/Shared/Stream)
- Current schema version
- Storage location
- Flush policy (type, row_limit, time_interval)
- Schema (columns, data types, nullable, system columns)
- Retention hours (for stream tables)

---

### SHOW STATS FOR TABLE

```sql
SHOW STATS FOR TABLE [<namespace>.]<table_name>;
```

**Examples**:
```sql
SHOW STATS FOR TABLE app.messages;
```

**Output**:
- Table name
- Hot rows (RocksDB buffer)
- Cold rows (Parquet files)
- Total rows
- Storage bytes
- Last flushed timestamp

---

## Data Types

KalamDB supports all DataFusion data types:

| Type | Description | Example |
|------|-------------|---------|
| `BOOLEAN` | True/false | `true`, `false` |
| `INT` / `INTEGER` | 32-bit signed integer | `42`, `-100` |
| `BIGINT` | 64-bit signed integer | `9223372036854775807` |
| `FLOAT` | 32-bit floating point | `3.14` |
| `DOUBLE` | 64-bit floating point | `2.718281828` |
| `TEXT` / `VARCHAR` | Variable-length string | `'Hello World'` |
| `TIMESTAMP` | Date and time | `'2025-10-20T15:30:00Z'`, `NOW()` |
| `DATE` | Date only | `'2025-10-20'` |
| `TIME` | Time only | `'15:30:00'` |
| `INTERVAL` | Time duration | `INTERVAL '1 hour'`, `INTERVAL '7 days'` |
| `BINARY` | Binary data | `X'DEADBEEF'` |
| `JSON` | JSON data (stored as TEXT) | `'{"key": "value"}'` |

**Notes**:
- `TEXT` and `VARCHAR` are equivalent
- `TIMESTAMP` supports time zones (stored in UTC)
- `JSON` is stored as TEXT (parse/validate in application)

---

## System Columns

### User Tables and Shared Tables

| Column | Type | Description | Indexed | Mutable |
|--------|------|-------------|---------|---------|
| `_updated` | `TIMESTAMP` | Last update timestamp | Yes (bloom filter) | No (auto-managed) |
| `_deleted` | `BOOLEAN` | Soft delete flag | No | No (auto-managed) |

**Usage**:
```sql
-- Query recently updated rows
SELECT * FROM app.messages
WHERE _updated >= NOW() - INTERVAL '1 hour';

-- Query soft-deleted rows
SELECT * FROM app.messages WHERE _deleted = true;

-- Exclude soft-deleted rows (default behavior)
SELECT * FROM app.messages WHERE _deleted = false;
```

**Notes**:
- `_updated` is set to NOW() on INSERT and UPDATE
- `_deleted` is set to true on DELETE (soft delete)
- Cannot be altered via ALTER TABLE
- Cannot be specified in INSERT/UPDATE statements

---

### Stream Tables

**No system columns**: Stream tables do NOT have `_updated` or `_deleted` columns (ephemeral data only).

---

## SQL Functions

KalamDB supports all DataFusion SQL functions. Common ones:

### Date/Time Functions
- `NOW()`: Current timestamp
- `DATE_TRUNC('day', timestamp)`: Truncate to day
- `EXTRACT(YEAR FROM timestamp)`: Extract year

### String Functions
- `UPPER(text)`, `LOWER(text)`: Case conversion
- `LENGTH(text)`: String length
- `SUBSTRING(text, start, length)`: Extract substring
- `CONCAT(text1, text2)`: Concatenate strings

### Aggregation Functions
- `COUNT(*)`, `COUNT(column)`: Row count
- `SUM(column)`, `AVG(column)`: Numeric aggregation
- `MIN(column)`, `MAX(column)`: Min/max values
- `ARRAY_AGG(column)`: Aggregate into array

### Window Functions
- `ROW_NUMBER() OVER (ORDER BY ...)`: Row numbering
- `RANK() OVER (PARTITION BY ... ORDER BY ...)`: Ranking
- `LAG(column) OVER (ORDER BY ...)`: Previous value

See [DataFusion SQL Reference](https://arrow.apache.org/datafusion/user-guide/sql/index.html) for complete list.

---

## PostgreSQL/MySQL Compatibility

KalamDB aims for maximum compatibility with PostgreSQL and MySQL syntax to ease migration and provide familiar interfaces for developers.

### Supported PostgreSQL Features

#### Data Type Aliases

PostgreSQL-specific type names are automatically mapped to Arrow/DataFusion types:

| PostgreSQL Type | KalamDB Type | Notes |
|-----------------|--------------|-------|
| `SERIAL` | `INT` | Auto-increment (sequence-based) |
| `BIGSERIAL` | `BIGINT` | Auto-increment (sequence-based) |
| `SMALLSERIAL` | `SMALLINT` | Auto-increment (sequence-based) |
| `SERIAL2` | `SMALLINT` | Alias for SMALLSERIAL |
| `SERIAL4` | `INT` | Alias for SERIAL |
| `SERIAL8` | `BIGINT` | Alias for BIGSERIAL |
| `INT2` | `SMALLINT` | 16-bit integer |
| `INT4` | `INT` | 32-bit integer |
| `INT8` | `BIGINT` | 64-bit integer |
| `FLOAT4` | `FLOAT` | 32-bit float |
| `FLOAT8` | `DOUBLE` | 64-bit float |
| `VARCHAR(n)` | `TEXT` | Variable-length text |
| `CHAR(n)` | `TEXT` | Fixed-length text (stored as TEXT) |
| `CHARACTER VARYING` | `TEXT` | Variable-length text |
| `BOOL` | `BOOLEAN` | True/false |
| `JSONB` | `TEXT` | JSON data (stored as TEXT) |

**Example**:
```sql
-- PostgreSQL-style CREATE TABLE
CREATE USER TABLE app.users (
  id SERIAL PRIMARY KEY,
  username VARCHAR(255) NOT NULL,
  age INT4,
  balance FLOAT8,
  is_active BOOL,
  metadata JSONB
) FLUSH POLICY ROW_LIMIT 1000;
```

#### Error Message Format

KalamDB uses PostgreSQL-style error messages by default:

```
ERROR: relation "users" does not exist
ERROR: column "age" does not exist
ERROR: syntax error at or near "FROM"
```

**Programmatic Access**:
```rust
use kalamdb_sql::compatibility::{
    format_postgres_error,
    format_postgres_table_not_found,
    format_postgres_column_not_found,
    format_postgres_syntax_error,
};

// Generate PostgreSQL-style errors
let err = format_postgres_table_not_found("users");
// Output: "ERROR: relation \"users\" does not exist"
```

### Supported MySQL Features

#### Data Type Aliases

MySQL-specific type names are automatically mapped:

| MySQL Type | KalamDB Type | Notes |
|------------|--------------|-------|
| `TINYINT` | `SMALLINT` | 8-bit integer |
| `MEDIUMINT` | `INT` | 24-bit integer (stored as INT) |
| `INT` | `INT` | 32-bit integer |
| `BIGINT` | `BIGINT` | 64-bit integer |
| `UNSIGNED INT` | `UINT` | Unsigned 32-bit integer |
| `UNSIGNED BIGINT` | `UBIGINT` | Unsigned 64-bit integer |
| `REAL` | `FLOAT` | 32-bit float |
| `DOUBLE PRECISION` | `DOUBLE` | 64-bit float |
| `VARCHAR(n)` | `TEXT` | Variable-length text |
| `CHAR(n)` | `TEXT` | Fixed-length text |
| `TEXT` | `TEXT` | Long text |
| `JSON` | `TEXT` | JSON data |

**Example**:
```sql
-- MySQL-style CREATE TABLE
CREATE USER TABLE app.products (
  id INT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
  name VARCHAR(255) NOT NULL,
  price DOUBLE PRECISION,
  stock MEDIUMINT,
  description TEXT
) FLUSH POLICY ROW_LIMIT 1000;
```

#### Error Message Format

MySQL-style error messages can be generated using compatibility functions:

```
ERROR 1146 (42S02): Table 'db.users' doesn't exist
ERROR 1054 (42S22): Unknown column 'age' in 'field list'
ERROR 1064 (42000): You have an error in your SQL syntax; check the manual that corresponds to your MySQL server version for the right syntax to use near 'FROM' at line 1
```

**Programmatic Access**:
```rust
use kalamdb_sql::compatibility::{
    format_mysql_error,
    format_mysql_table_not_found,
    format_mysql_column_not_found,
    format_mysql_syntax_error,
};

// Generate MySQL-style errors
let err = format_mysql_table_not_found("mydb", "users");
// Output: "ERROR 1146 (42S02): Table 'mydb.users' doesn't exist"
```

### Syntax Compatibility

#### CREATE TABLE Variants

Both PostgreSQL and MySQL CREATE TABLE syntax variants are supported:

```sql
-- PostgreSQL style with SERIAL
CREATE USER TABLE app.users (
  id SERIAL PRIMARY KEY,
  username VARCHAR(255) NOT NULL
) FLUSH POLICY ROW_LIMIT 1000;

-- MySQL style with AUTO_INCREMENT
CREATE USER TABLE app.users (
  id INT AUTO_INCREMENT PRIMARY KEY,
  username VARCHAR(255) NOT NULL
) FLUSH POLICY ROW_LIMIT 1000;

-- Standard SQL with explicit NOT NULL
CREATE USER TABLE app.users (
  id BIGINT NOT NULL,
  username TEXT NOT NULL
) FLUSH POLICY ROW_LIMIT 1000;
```

#### INSERT Statement Compatibility

Standard SQL INSERT syntax is fully supported:

```sql
-- Single row insert
INSERT INTO app.users (id, username) VALUES (1, 'alice');

-- Multi-row insert (PostgreSQL/MySQL compatible)
INSERT INTO app.users (id, username) VALUES 
  (1, 'alice'),
  (2, 'bob'),
  (3, 'charlie');

-- Column list inference (if all columns provided)
INSERT INTO app.users VALUES (1, 'alice');
```

### CLI Output Format

The KalamDB CLI (`kalam-cli`) uses **psql-style output formatting** for familiarity:

**Table Borders** (psql-compatible):
```
┌─────┬──────────┐
│ id  │ username │
├─────┼──────────┤
│ 1   │ alice    │
│ 2   │ bob      │
└─────┴──────────┘
(2 rows)

Time: 45.123 ms
```

**Row Count Display**:
- Shows `(N rows)` after query results
- Uses `(1 row)` for single-row results (singular)

**Timing Display**:
- Shows `Time: X.XXX ms` in milliseconds
- 3 decimal places for precision
- Blank line separator between results and timing

**DDL/DML Results**:
```
Query OK, 1 rows affected

Time: 12.456 ms
```

### Differences from PostgreSQL/MySQL

While KalamDB aims for compatibility, some differences exist:

#### Architecture-Specific Features

1. **Table Types**: KalamDB has USER, SHARED, and STREAM tables (PostgreSQL/MySQL have standard tables)
2. **FLUSH POLICY**: Required for KalamDB tables (no equivalent in PostgreSQL/MySQL)
3. **System Columns**: `_updated` and `_deleted` are auto-managed (not user-specified)
4. **Namespace Operations**: KalamDB uses `CREATE NAMESPACE` (PostgreSQL uses `CREATE SCHEMA`, MySQL uses `CREATE DATABASE`)

#### Not Yet Supported

1. **Transactions**: No BEGIN/COMMIT/ROLLBACK (planned for future)
2. **Foreign Keys**: No FK constraints (planned for future)
3. **Triggers**: No CREATE TRIGGER support
4. **Stored Procedures**: No procedural SQL (PL/pgSQL, MySQL procedures)
5. **Views**: No CREATE VIEW support (planned for future)
6. **Indexes**: No explicit index creation (automatic bloom filters only)

#### SQL Parser Implementation

KalamDB uses **sqlparser-rs** (https://github.com/sqlparser-rs/sqlparser-rs) for standard SQL parsing with custom extensions for KalamDB-specific commands:

- **Standard SQL**: Parsed by sqlparser-rs (SELECT, INSERT, UPDATE, DELETE)
- **Custom Extensions**: Custom parsers for CREATE NAMESPACE, FLUSH POLICY, CREATE STORAGE, etc.
- **Dialect**: Extends PostgreSQL dialect with KalamDB-specific keywords

See [ADR-012: sqlparser-rs Integration](adrs/ADR-012-sqlparser-integration.md) for implementation details.

---

## Examples

### Complete Workflow

```sql
-- 1. Create namespace
CREATE NAMESPACE app;

-- 2. Create storage (optional, 'local' is default)
CREATE STORAGE s3_prod
TYPE s3
PATH 's3://my-bucket/kalamdb-data'
CREDENTIALS '{
  "access_key_id": "AKIAIOSFODNN7EXAMPLE",
  "secret_access_key": "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
  "region": "us-west-2"
}';

-- 3. Create user table with DEFAULT functions
CREATE USER TABLE app.messages (
  id BIGINT DEFAULT SNOWFLAKE_ID(),
  content TEXT,
  author TEXT,
  created_at TIMESTAMP DEFAULT NOW()
) STORAGE local FLUSH INTERVAL 300s ROW_THRESHOLD 1000;

-- 4. Create shared table
CREATE SHARED TABLE app.config (
  config_key TEXT NOT NULL,
  config_value TEXT,
  updated_at TIMESTAMP DEFAULT NOW()
) STORAGE local FLUSH ROW_THRESHOLD 100;

-- 5. Create stream table
CREATE STREAM TABLE app.events (
  event_id TEXT NOT NULL,
  event_type TEXT,
  payload TEXT,
  timestamp TIMESTAMP DEFAULT NOW()
) RETENTION 10 EPHEMERAL MAX_BUFFER 5000;

-- 6. Insert data (using DEFAULT functions)
INSERT INTO app.messages2 (content, author) VALUES ('Hello World', 'alice');

INSERT INTO app.config (config_key, config_value)
VALUES ('app_name', 'KalamDB');

INSERT INTO app.events (event_id, event_type, payload)
VALUES ('evt_123', 'user_action', '{"action":"click"}');

-- 7. Query data
SELECT * FROM app.messages
WHERE created_at > NOW() - INTERVAL '1 hour'
ORDER BY created_at DESC;

SELECT * FROM app.config WHERE config_key = 'app_name';

SELECT * FROM app.events LIMIT 10;

-- 8. Update data
UPDATE app.messages SET content = 'Updated' WHERE id = 1;

-- 9. Schema evolution
ALTER TABLE app.messages ADD COLUMN reaction TEXT;

-- 10. Manual flush
FLUSH TABLE app.messages;

-- Check flush job status
SELECT job_id, status, result 
FROM system.jobs 
WHERE job_id LIKE 'flush-messages-%' 
ORDER BY created_at DESC 
LIMIT 1;

-- 11. Subscribe to live changes (WebSocket)
SUBSCRIBE TO app.messages 
WHERE author = 'alice' 
OPTIONS (last_rows=10);

-- 12. Backup
BACKUP DATABASE app TO '/backups/app-snapshot';

-- 13. Catalog browsing
SHOW STORAGES;
SHOW TABLES IN app;
DESCRIBE TABLE app.messages;
SHOW STATS FOR TABLE app.messages;

-- 14. Cleanup
DROP TABLE app.events;
DROP TABLE app.messages;
DROP TABLE app.config;
DROP STORAGE s3_prod;
DROP NAMESPACE app;
```

---

## See Also

- [REST API Reference](API_REFERENCE.md) - HTTP endpoint documentation
- [WebSocket Protocol](WEBSOCKET_PROTOCOL.md) - Real-time subscriptions
- [Quick Start Guide](../QUICK_START.md) - Getting started tutorial
- [DataFusion SQL Reference](https://arrow.apache.org/datafusion/user-guide/sql/index.html) - Complete SQL function list
