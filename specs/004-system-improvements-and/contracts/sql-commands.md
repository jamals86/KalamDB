# New SQL Commands Contract

**Version**: 1.0.0  
**Feature**: System Improvements and Performance Optimization  
**Language**: SQL (KalamDB dialect)

## Purpose

This document defines new SQL commands added in this feature for manual flushing, live query management, user administration, and system inspection.

---

## FLUSH Commands

### FLUSH TABLE

**Purpose**: Manually trigger immediate table flush to storage.

**Syntax**:
```sql
FLUSH TABLE [<namespace>.]<table_name>
```

**Description**: Flushes all buffered data for the specified table to Parquet files in storage. The operation is synchronous and blocks until flush completes.

**Examples**:
```sql
-- Flush specific table
FLUSH TABLE chat.messages;

-- Response:
Flushed 1,523 records to ./data/storage/chat/users/jamal/messages/j/batch-2025-10-21-143022.parquet
```

**Response Fields**:
- `records_flushed`: Number of rows written
- `storage_location`: Path to Parquet file(s)
- `execution_time_ms`: Time taken to complete flush

**Errors**:
- Table does not exist
- Table has no buffered data (warning, not error)
- Storage location unavailable or disk full
- Concurrent flush already in progress (queued)

---

### FLUSH ALL TABLES

**Purpose**: Flush all tables with buffered data.

**Syntax**:
```sql
FLUSH ALL TABLES
```

**Description**: Flushes all tables that have buffered data. Tables are flushed sequentially in alphabetical order.

**Examples**:
```sql
FLUSH ALL TABLES;

-- Response:
Flushed 3 tables:
  - chat.messages: 1,523 records → ./data/storage/chat/users/jamal/messages/j/
  - chat.channels: 45 records → ./data/storage/chat/channels/
  - analytics.events: 10,234 records → ./data/storage/analytics/events/
Total time: 4.582s
```

**Response Fields**:
- List of flushed tables with record counts and paths
- Total execution time

**Errors**:
- Storage location unavailable

---

## Live Query Management Commands

### SUBSCRIBE TO

**Purpose**: Establish WebSocket subscription for real-time table changes.

**Syntax**:
```sql
SUBSCRIBE TO [<namespace>.]<table_name>
    [WHERE <condition>]
    [LAST <n>]
```

**Description**: Creates a WebSocket connection that delivers real-time change notifications (INSERT, UPDATE, DELETE) as they occur. Optional `LAST n` fetches the most recent N rows immediately before streaming changes.

**Examples**:
```sql
-- Basic subscription
SUBSCRIBE TO chat.messages;

-- With filter
SUBSCRIBE TO chat.messages WHERE user_id = 'jamal';

-- With initial data fetch
SUBSCRIBE TO chat.messages LAST 100;

-- Combined
SUBSCRIBE TO chat.messages WHERE created_at > '2025-10-20' LAST 50;

-- Response:
Subscription established (live_id: 660e8400-e29b-41d4-a716-446655440001)
Initial data: 50 rows
Listening for changes... (Press Ctrl+C to stop)
```

**Limitations**:
- Only supported on user tables and stream tables
- **Not supported on shared tables** (returns error)
- Maximum 100 concurrent subscriptions per user
- `LAST n` limited to 10,000 rows

**Errors**:
- Table does not exist
- Subscription not supported on shared tables
- Subscription limit exceeded
- Invalid WHERE clause

---

### KILL LIVE QUERY

**Purpose**: Terminate an active live query subscription.

**Syntax**:
```sql
KILL LIVE QUERY '<live_id>'
```

**Description**: Gracefully disconnects the specified WebSocket subscription and removes it from `system.live_queries` table.

**Examples**:
```sql
-- Get live query ID from system table
SELECT live_id, table_name, query FROM system.live_queries WHERE user_id = 'jamal';

-- Kill specific subscription
KILL LIVE QUERY '660e8400-e29b-41d4-a716-446655440001';

-- Response:
Live query 660e8400-e29b-41d4-a716-446655440001 terminated.
```

**Errors**:
- Live query ID does not exist
- Live query already disconnected
- Permission denied (can only kill own subscriptions, unless admin)

---

## User Management Commands

### INSERT INTO system.users

**Purpose**: Add a new user to the system.

**Syntax**:
```sql
INSERT INTO system.users (user_id, username, metadata)
VALUES ('<user_id>', '<username>', '<json_metadata>')
```

**Description**: Creates a new user record. `user_id` must be unique. Timestamps (`created_at`, `updated_at`) are set automatically.

**Examples**:
```sql
-- Basic user
INSERT INTO system.users (user_id, username)
VALUES ('user123', 'john_doe');

-- User with metadata
INSERT INTO system.users (user_id, username, metadata)
VALUES ('user123', 'john_doe', '{"role": "admin", "team": "engineering"}');

-- Response:
1 row inserted.
```

**Validation**:
- `user_id`: Required, unique, max 64 chars, alphanumeric + underscore + hyphen
- `username`: Required, max 128 chars
- `metadata`: Valid JSON or NULL

**Errors**:
- User with user_id already exists
- Invalid JSON in metadata field
- Missing required fields (user_id, username)

---

### UPDATE system.users

**Purpose**: Update existing user information.

**Syntax**:
```sql
UPDATE system.users
SET username = '<new_username>', metadata = '<new_metadata>'
WHERE user_id = '<user_id>'
```

**Description**: Updates user record. Only specified fields are modified. `updated_at` timestamp is set automatically.

**Examples**:
```sql
-- Update username only
UPDATE system.users
SET username = 'jane_doe'
WHERE user_id = 'user123';

-- Update metadata only
UPDATE system.users
SET metadata = '{"role": "user", "team": "support"}'
WHERE user_id = 'user123';

-- Update both
UPDATE system.users
SET username = 'jane_smith', metadata = '{"role": "manager"}'
WHERE user_id = 'user123';

-- Response:
1 row updated.
```

**Validation**:
- `metadata`: Must be valid JSON if provided

**Errors**:
- User with user_id not found
- Invalid JSON in metadata field

---

### DELETE FROM system.users

**Purpose**: Remove a user from the system.

**Syntax**:
```sql
DELETE FROM system.users WHERE user_id = '<user_id>'
```

**Description**: Deletes user record. **Note**: This does not delete user's data in other tables (user tables remain).

**Examples**:
```sql
DELETE FROM system.users WHERE user_id = 'user123';

-- Response:
1 row deleted.
```

**Errors**:
- User with user_id not found

---

## System Inspection Commands

### SHOW TABLE STATS

**Purpose**: Display statistics about a table's storage and buffer status.

**Syntax**:
```sql
SHOW TABLE STATS [<namespace>.]<table_name>
```

**Description**: Returns metrics about table including row counts, storage size, and flush status.

**Examples**:
```sql
SHOW TABLE STATS chat.messages;

-- Response (tabular):
┌───────────────────┬────────────┐
│ Metric            │ Value      │
├───────────────────┼────────────┤
│ Buffered rows     │ 1,523      │
│ Flushed rows      │ 10,000     │
│ Total rows        │ 11,523     │
│ Storage size      │ 45.2 MB    │
│ Last flush        │ 5 min ago  │
│ Parquet files     │ 12         │
│ Flush config      │ Auto (5m)  │
└───────────────────┴────────────┘
```

**Response Fields**:
- `buffered_rows`: Rows in RocksDB buffer (not yet flushed)
- `flushed_rows`: Rows in Parquet storage
- `total_rows`: Sum of buffered + flushed
- `storage_size`: Total size of Parquet files on disk
- `last_flush`: Time since last flush operation
- `parquet_files`: Number of Parquet files for this table
- `flush_config`: Flush configuration (Auto/Manual, interval)

**Errors**:
- Table does not exist

---

### DESCRIBE TABLE (Enhanced)

**Purpose**: Show table schema with version information.

**Syntax**:
```sql
DESCRIBE [<namespace>.]<table_name>
```

**Description**: Returns table schema with column definitions, current schema version, and reference to schema history.

**Examples**:
```sql
DESCRIBE chat.messages;

-- Response:
┌──────────────┬───────────┬──────────┬─────────────────┐
│ Column       │ Type      │ Nullable │ Default         │
├──────────────┼───────────┼──────────┼─────────────────┤
│ id           │ INT       │ No       │ AUTO_INCREMENT  │
│ user_id      │ TEXT      │ No       │                 │
│ content      │ TEXT      │ No       │                 │
│ created_at   │ TIMESTAMP │ No       │ now()           │
│ _updated     │ TIMESTAMP │ Yes      │                 │
│ _deleted     │ BOOLEAN   │ Yes      │ false           │
└──────────────┴───────────┴──────────┴─────────────────┘

Schema version: 3
History: SELECT * FROM system.table_schemas WHERE namespace_id = 'chat' AND table_name = 'messages'
```

**New Fields** (compared to original DESCRIBE):
- `Schema version`: Current schema version number
- `History`: SQL query to view all schema versions

**Errors**:
- Table does not exist

---

## Batch SQL Execution

**Purpose**: Execute multiple SQL statements in a single request.

**Syntax**:
```sql
<statement1>; <statement2>; <statement3>;
```

**Description**: Multiple statements separated by semicolons are executed sequentially. If any statement fails, execution stops at that point.

**Examples**:
```sql
-- Create and populate table
CREATE NAMESPACE demo;
CREATE USER TABLE demo.messages (id INT, content TEXT);
INSERT INTO demo.messages VALUES (1, 'First');
INSERT INTO demo.messages VALUES (2, 'Second');

-- Response:
Statement 1: Namespace created
Statement 2: Table created
Statement 3: 1 row inserted
Statement 4: 1 row inserted

Completed 4 statements in 0.145s
```

**Error Handling**:
```sql
-- First statement succeeds, second fails, third never executes
INSERT INTO demo.messages VALUES (1, 'First');
INSERT INTO nonexistent.table VALUES (2, 'Second');
INSERT INTO demo.messages VALUES (3, 'Third');

-- Response:
Statement 1: 1 row inserted
Statement 2: ERROR: Table 'nonexistent.table' does not exist
Stopped at statement 2 due to error.
```

---

## Parametrized Queries (API)

**Purpose**: Execute queries with parameter substitution for security and performance.

**Syntax** (via `/api/sql` endpoint):
```json
{
  "sql": "SELECT * FROM messages WHERE user_id = $1 AND created_at > $2",
  "params": ["jamal", "2025-10-20T00:00:00Z"]
}
```

**Description**: Parameter placeholders ($1, $2, ...) are substituted with values from `params` array. Query execution plan is cached for reuse.

**Parameter Types**:
- String: `"text"`
- Integer: `123`
- Float: `45.67`
- Boolean: `true` or `false`
- Timestamp: `"2025-10-21T14:30:00Z"` (ISO 8601)
- Null: `null`

**Examples**:
```json
// INSERT with parameters
{
  "sql": "INSERT INTO messages (user_id, content, created_at) VALUES ($1, $2, $3)",
  "params": ["jamal", "Hello, world!", "2025-10-21T14:30:00Z"]
}

// SELECT with multiple parameters
{
  "sql": "SELECT * FROM messages WHERE user_id = $1 AND id > $2 AND created_at BETWEEN $3 AND $4",
  "params": ["jamal", 100, "2025-10-20T00:00:00Z", "2025-10-21T23:59:59Z"]
}

// UPDATE with parameters
{
  "sql": "UPDATE messages SET content = $1, updated = $2 WHERE id = $3",
  "params": ["Updated content", "2025-10-21T14:35:00Z", 123]
}
```

**Performance**: First execution compiles query plan (~50ms). Subsequent executions with different parameters reuse cached plan (~10ms) - **40% faster**.

**Errors**:
- Parameter count mismatch (e.g., 2 parameters provided, 3 placeholders in SQL)
- Parameter type incompatible with column type
- Invalid parameter placeholder (e.g., $0, $-1, gaps like $1, $3 without $2)

---

## Permission Model

### Command Permissions

| Command | Required Permission | Notes |
|---------|-------------------|-------|
| FLUSH TABLE | Table owner or admin | Can only flush own tables |
| FLUSH ALL TABLES | Admin | Flushes all tables across all users |
| SUBSCRIBE TO | Table read access | User tables: own data only |
| KILL LIVE QUERY | Subscription owner or admin | Can only kill own subscriptions |
| INSERT INTO system.users | Admin | User management restricted |
| UPDATE system.users | Admin | User management restricted |
| DELETE FROM system.users | Admin | User management restricted |
| SHOW TABLE STATS | Table read access | Anyone with table access |
| DESCRIBE | Table read access | Anyone with table access |

### Localhost Bypass

When localhost bypass is enabled in `config.toml`:
```toml
[auth]
localhost_bypass = true
localhost_default_user = "system"
```

Connections from 127.0.0.1 bypass JWT authentication and use `localhost_default_user` for permissions.

---

## SQL Dialect Compatibility

### Standard SQL Support
- Basic SELECT, INSERT, UPDATE, DELETE
- WHERE, ORDER BY, GROUP BY, LIMIT, OFFSET
- JOINs (INNER, LEFT, RIGHT, OUTER)
- Aggregate functions (COUNT, SUM, AVG, MIN, MAX)
- Subqueries

### KalamDB Extensions
- SUBSCRIBE TO (live queries)
- FLUSH TABLE / FLUSH ALL TABLES
- KILL LIVE QUERY
- CREATE NAMESPACE
- CREATE USER TABLE / CREATE SHARED TABLE / CREATE STREAM TABLE
- User table qualifier: `.user.` (e.g., `FROM namespace.user.table_name`)

### Not Supported (Yet)
- Stored procedures
- Triggers
- Views (planned)
- Indexes (BLOOM, SORTED planned)
- Transactions (BEGIN/COMMIT/ROLLBACK)

---

## Examples by Use Case

### Development Workflow

```sql
-- Setup
CREATE NAMESPACE dev;
CREATE USER TABLE dev.test (id INT, data TEXT);
INSERT INTO dev.test VALUES (1, 'test');

-- Query and verify
SELECT * FROM dev.test;

-- Clean up
DROP TABLE dev.test;
DROP NAMESPACE dev;
```

### Monitoring and Administration

```sql
-- Check active subscriptions
SELECT * FROM system.live_queries;

-- View recent jobs
SELECT * FROM system.jobs ORDER BY created_at DESC LIMIT 10;

-- Flush specific table
FLUSH TABLE chat.messages;

-- Get table statistics
SHOW TABLE STATS chat.messages;
```

### Real-Time Data Streaming

```sql
-- Terminal 1: Subscribe to table
SUBSCRIBE TO chat.messages WHERE user_id = 'jamal' LAST 10;

-- Terminal 2: Insert data
INSERT INTO chat.messages VALUES (100, 'jamal', 'Real-time message', now());

-- Terminal 1 receives immediately:
[14:40:00] INSERT → id=100, content=Real-time message
```

---

## Testing Strategy

### Unit Tests
- SQL parsing for new commands
- Parameter substitution validation
- Error message formatting

### Integration Tests
- Execute each command via `/api/sql` endpoint
- Verify response format and content
- Test error conditions
- Validate permissions

### Contract Tests
- API clients use parametrized queries correctly
- WebSocket subscriptions work with all options
- Batch execution handles errors as specified

---

## Version History

- **1.0.0** (2025-10-21): Initial specification
  - FLUSH TABLE, FLUSH ALL TABLES
  - SUBSCRIBE TO (with LAST n)
  - KILL LIVE QUERY
  - User management (INSERT/UPDATE/DELETE on system.users)
  - SHOW TABLE STATS
  - Enhanced DESCRIBE TABLE
  - Parametrized queries
  - Batch SQL execution
