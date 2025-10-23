# KalamDB SQL Syntax Reference

**Version**: 0.1.0  
**SQL Engine**: Apache DataFusion 35.0

## Overview

KalamDB supports standard SQL via Apache DataFusion with custom DDL extensions for namespace, table, and backup operations.

---

## Table of Contents

1. [Namespace Operations](#namespace-operations)
2. [User Table Operations](#user-table-operations)
3. [Shared Table Operations](#shared-table-operations)
4. [Stream Table Operations](#stream-table-operations)
5. [Schema Evolution](#schema-evolution)
6. [Data Manipulation](#data-manipulation)
7. [Backup and Restore](#backup-and-restore)
8. [Catalog Browsing](#catalog-browsing)
9. [Data Types](#data-types)
10. [System Columns](#system-columns)

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

## User Table Operations

User tables create one table instance per user with isolated storage.

### CREATE USER TABLE

```sql
CREATE USER TABLE [<namespace>.]<table_name> (
  <column_name> <data_type> [NOT NULL],
  ...
) FLUSH POLICY <policy>;
```

**Flush Policies**:
- `ROW_LIMIT <n>`: Flush after `n` rows inserted
- `TIME_INTERVAL <seconds>`: Flush every `<seconds>` seconds
- `COMBINED ROW_LIMIT <n> TIME_INTERVAL <s>`: Flush when either condition is met

**Examples**:
```sql
-- Simple user table with row-based flush
CREATE USER TABLE app.messages (
  id BIGINT NOT NULL,
  content TEXT,
  author TEXT,
  timestamp TIMESTAMP
) FLUSH POLICY ROW_LIMIT 1000;

-- Time-based flush (every 5 minutes)
CREATE USER TABLE app.events (
  event_id TEXT NOT NULL,
  event_type TEXT,
  data TEXT
) FLUSH POLICY TIME_INTERVAL 300;

-- Combined flush (flush when either 5000 rows OR 10 minutes)
CREATE USER TABLE app.analytics (
  metric_name TEXT NOT NULL,
  value DOUBLE,
  tags TEXT
) FLUSH POLICY COMBINED ROW_LIMIT 5000 TIME_INTERVAL 600;
```

**System Columns** (automatically added):
- `_updated TIMESTAMP`: Last update timestamp (indexed for time-range queries)
- `_deleted BOOLEAN`: Soft delete flag (default: false)

**Storage**:
- Hot tier: `RocksDB column family: user_table:{table_name}`
- Cold tier: `{storage_path}/user/{user_id}/{table_name}/batch-*.parquet`

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
  <column_name> <data_type> [NOT NULL],
  ...
) FLUSH POLICY <policy>;
```

**Examples**:
```sql
-- Global configuration table
CREATE SHARED TABLE app.config (
  config_key TEXT NOT NULL,
  config_value TEXT,
  updated_at TIMESTAMP
) FLUSH POLICY ROW_LIMIT 100;

-- Shared analytics
CREATE SHARED TABLE app.global_metrics (
  metric_name TEXT NOT NULL,
  value DOUBLE,
  timestamp TIMESTAMP
) FLUSH POLICY TIME_INTERVAL 60;
```

**System Columns** (automatically added):
- `_updated TIMESTAMP`: Last update timestamp
- `_deleted BOOLEAN`: Soft delete flag

**Storage**:
- Hot tier: `RocksDB column family: shared_table:{table_name}`
- Cold tier: `{storage_path}/shared/{table_name}/batch-*.parquet`

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

-- 2. Create user table
CREATE USER TABLE app.messages (
  id BIGINT NOT NULL,
  content TEXT,
  author TEXT,
  timestamp TIMESTAMP
) FLUSH POLICY ROW_LIMIT 1000;

-- 3. Create shared table
CREATE SHARED TABLE app.config (
  config_key TEXT NOT NULL,
  config_value TEXT
) FLUSH POLICY ROW_LIMIT 100;

-- 4. Create stream table
CREATE STREAM TABLE app.events (
  event_id TEXT NOT NULL,
  event_type TEXT,
  payload TEXT
) RETENTION 10 EPHEMERAL MAX_BUFFER 5000;

-- 5. Insert data
INSERT INTO app.messages (id, content, author, timestamp)
VALUES (1, 'Hello World', 'alice', NOW());

INSERT INTO app.config (config_key, config_value)
VALUES ('app_name', 'KalamDB');

INSERT INTO app.events (event_id, event_type, payload)
VALUES ('evt_123', 'user_action', '{"action":"click"}');

-- 6. Query data
SELECT * FROM app.messages
WHERE timestamp > NOW() - INTERVAL '1 hour'
ORDER BY timestamp DESC;

SELECT * FROM app.config WHERE config_key = 'app_name';

SELECT * FROM app.events LIMIT 10;

-- 7. Update data
UPDATE app.messages SET content = 'Updated' WHERE id = 1;

-- 8. Schema evolution
ALTER TABLE app.messages ADD COLUMN reaction TEXT;

-- 9. Backup
BACKUP DATABASE app TO '/backups/app-snapshot';

-- 10. Catalog browsing
SHOW TABLES IN app;
DESCRIBE TABLE app.messages;
SHOW STATS FOR TABLE app.messages;

-- 11. Cleanup
DROP TABLE app.events;
DROP TABLE app.messages;
DROP TABLE app.config;
DROP NAMESPACE app;
```

---

## See Also

- [REST API Reference](API_REFERENCE.md) - HTTP endpoint documentation
- [WebSocket Protocol](WEBSOCKET_PROTOCOL.md) - Real-time subscriptions
- [Quick Start Guide](../QUICK_START.md) - Getting started tutorial
- [DataFusion SQL Reference](https://arrow.apache.org/datafusion/user-guide/sql/index.html) - Complete SQL function list
