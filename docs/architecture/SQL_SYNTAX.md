# KalamDB SQL Syntax Reference

**Version**: 0.1.0  
**SQL Engine**: Apache DataFusion 35.0  
**Last Updated**: October 28, 2025

---

## Overview

KalamDB is a **SQL-first database** built for real-time chat and AI message history storage with a unique **table-per-user architecture**. This document provides comprehensive SQL syntax reference for all operations.

**Key Features**:
- ✅ Full SQL support via Apache DataFusion
- ✅ Three table types: USER (per-user isolation), SHARED (global), STREAM (ephemeral)
- ✅ Real-time WebSocket subscriptions with live queries
- ✅ Multi-storage backends (local filesystem, S3, Azure Blob, GCS)
- ✅ Role-based access control (user, service, dba, system)
- ✅ Schema evolution with backward compatibility
- ✅ Automatic ID generation (SNOWFLAKE_ID, UUID_V7, ULID)

---

## Table of Contents

### I. Core Concepts
1. [Namespace Operations](#namespace-operations) - Logical containers for tables
2. [Storage Management](#storage-management) - Multi-backend storage configuration
3. [User Management](#user-management) - Authentication and role-based access control
4. [Data Types](#data-types) - Supported SQL data types
5. [System Columns](#system-columns) - Auto-managed metadata columns

### II. Table Operations
6. [User Table Operations](#user-table-operations) - Per-user isolated tables
7. [Shared Table Operations](#shared-table-operations) - Global shared tables
8. [Stream Table Operations](#stream-table-operations) - Ephemeral real-time tables
9. [Schema Evolution](#schema-evolution) - ALTER TABLE operations

### III. Data Manipulation
10. [Data Manipulation (DML)](#data-manipulation) - INSERT, UPDATE, DELETE, SELECT
11. [Manual Flushing](#manual-flushing) - Explicit hot-to-cold tier migration
12. [Live Query Subscriptions](#live-query-subscriptions) - Real-time WebSocket subscriptions

### IV. Administration
13. [Backup and Restore](#backup-and-restore) - Database backup and recovery
14. [Catalog Browsing](#catalog-browsing) - SHOW, DESCRIBE commands
15. [Custom Functions](#kalamdb-custom-functions) - SNOWFLAKE_ID, UUID_V7, ULID, CURRENT_USER
16. [PostgreSQL/MySQL Compatibility](#postgresqlmysql-compatibility) - Migration support

---

## Quick Reference

### Most Common Commands

```sql
-- Create namespace and tables
CREATE NAMESPACE app;
CREATE USER TABLE app.messages (id BIGINT DEFAULT SNOWFLAKE_ID(), content TEXT) FLUSH ROW_THRESHOLD 1000;

-- Insert and query data
INSERT INTO app.messages (content) VALUES ('Hello World');
SELECT * FROM app.messages ORDER BY id DESC LIMIT 10;

-- Real-time subscriptions
SUBSCRIBE TO app.messages WHERE timestamp > NOW() - INTERVAL '1 hour' OPTIONS (last_rows=10);

-- User management
CREATE USER 'alice' WITH PASSWORD 'Secret123!' ROLE 'user';
ALTER USER 'alice' SET PASSWORD 'NewPassword123!';

-- Backup and restore
BACKUP DATABASE app TO '/backups/app-20251028';
RESTORE DATABASE app FROM '/backups/app-20251028';
```

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

## User Management

User management commands allow creation, modification, and deletion of user accounts with role-based access control.

### CREATE USER

Create a new user account with authentication credentials and role.

**Basic Password Authentication**:
```sql
CREATE USER '<username>' 
  WITH PASSWORD '<password>' 
  ROLE '<user|service|dba|system>';
```

**With Email and Metadata**:
```sql
CREATE USER 'alice' 
  WITH PASSWORD 'secure_password_123!' 
  ROLE 'user'
  EMAIL 'alice@company.com'
  METADATA '{"department": "engineering", "team": "backend"}';
```

**OAuth Authentication**:
```sql
CREATE USER 'backup_service'
  WITH OAUTH PROVIDER 'google'
  SUBJECT 'backup@service.account'
  ROLE 'service';
```

**Internal System User (Localhost-Only)**:
```sql
CREATE USER 'replicator'
  WITH INTERNAL
  ROLE 'system';
-- No password required, localhost-only access
```

**System User with Remote Access**:
```sql
CREATE USER 'emergency_admin'
  WITH PASSWORD 'strong_password_123!'
  ROLE 'system'
  ALLOW_REMOTE true;
-- Password REQUIRED when allow_remote is enabled
```

**With IF NOT EXISTS**:
```sql
CREATE USER IF NOT EXISTS 'alice' WITH PASSWORD 'secret' ROLE 'user';
```

**Parameters**:
- `user_id`: Auto-generated from username (prefixed with role: usr_, svc_, dba_, sys_)
- `PASSWORD`: Hashed with bcrypt cost 12 (min 8 chars, max 1024 chars)
  - **REQUIRED** for `role='system'` when `ALLOW_REMOTE true`
  - **REQUIRED** for `role='user'`, `'service'`, `'dba'`
  - **OPTIONAL** for `role='system'` when localhost-only
- `ROLE`: One of `'user'`, `'service'`, `'dba'`, `'system'` (default: `'user'`)
- `EMAIL`: Optional email address
- `METADATA`: Optional JSON object
- `OAUTH PROVIDER`: OAuth provider name (e.g., 'google', 'github', 'azure')
- `SUBJECT`: OAuth subject identifier
- `INTERNAL`: Mark as internal system user (localhost-only, no password needed)
- `ALLOW_REMOTE`: Allow remote connections (only for `role='system'`, requires password)

**Authorization**: Only `dba` and `system` roles can execute CREATE USER

**Validation**:
- If `ROLE='system'` AND `ALLOW_REMOTE=true` → `PASSWORD` is **REQUIRED**
- If `ROLE='system'` AND `ALLOW_REMOTE=true` AND no password → **ERROR**: "System users with remote access must have a password"

**Examples**:
```sql
-- Standard user account
CREATE USER 'alice' WITH PASSWORD 'Secret123!' ROLE 'user';

-- Service account for automation
CREATE USER 'etl_service' WITH PASSWORD 'ServiceKey456!' ROLE 'service';

-- Database administrator
CREATE USER 'db_admin' WITH PASSWORD 'AdminPass789!' ROLE 'dba' EMAIL 'admin@company.com';

-- OAuth-based user
CREATE USER 'google_sync' WITH OAUTH PROVIDER 'google' SUBJECT 'sync@example.com' ROLE 'service';

-- Internal system user (localhost only, no password)
CREATE USER 'compaction_job' WITH INTERNAL ROLE 'system';

-- Emergency admin with remote access
CREATE USER 'emergency_dba' 
  WITH PASSWORD 'EmergencyAccess123!' 
  ROLE 'system' 
  ALLOW_REMOTE true
  METADATA '{"purpose": "emergency_access", "created_by": "db_admin"}';
```

---

### ALTER USER

Modify existing user credentials, role, or metadata.

**Change Password**:
```sql
ALTER USER '<username>' SET PASSWORD '<new_password>';
```

**Change Role** (DBA only):
```sql
ALTER USER '<username>' SET ROLE '<new_role>';
```

**Update Email and Metadata**:
```sql
ALTER USER '<username>' SET 
  EMAIL '<new_email>',
  METADATA '<json_metadata>';
```

**Authorization**: 
- Users can change their own password
- Only `dba` and `system` can change roles and other user attributes

**Examples**:
```sql
-- User changes their own password
ALTER USER 'alice' SET PASSWORD 'NewPassword123!';

-- DBA promotes user to service role
ALTER USER 'integration_bot' SET ROLE 'service';

-- Update user metadata
ALTER USER 'alice' SET 
  EMAIL 'alice.new@company.com',
  METADATA '{"department": "frontend", "level": "senior"}';

-- Update multiple fields
ALTER USER 'bob' SET 
  PASSWORD 'UpdatedPass456!',
  EMAIL 'bob@newcompany.com',
  METADATA '{"status": "active"}';
```

---

### DROP USER

Soft delete a user account (sets deleted_at timestamp, allows recovery within grace period).

**Basic Syntax**:
```sql
DROP USER '<username>';
```

**With IF EXISTS**:
```sql
DROP USER IF EXISTS '<username>';
```

**Behavior**:
- Sets `deleted_at` to current timestamp
- User becomes hidden from default queries
- User's tables remain accessible during grace period
- Scheduled cleanup job deletes user permanently after grace period (default: 30 days)
- User can be restored by DBA within grace period

**Authorization**: Only `dba` and `system` roles

**Examples**:
```sql
-- Soft delete user
DROP USER 'alice';

-- Soft delete with error suppression
DROP USER IF EXISTS 'old_user';
```

---

### Restore Deleted User

Restore a soft-deleted user within the grace period (DBA only).

```sql
UPDATE system.users 
SET deleted_at = NULL 
WHERE user_id = '<user_id>';
```

**Requirements**:
- Must be within grace period (default: 30 days from deletion)
- Only `dba` and `system` roles can restore users

**Example**:
```sql
-- Restore user by user_id
UPDATE system.users 
SET deleted_at = NULL 
WHERE user_id = 'usr_alice123';

-- Restore user by username
UPDATE system.users 
SET deleted_at = NULL 
WHERE username = 'alice';
```

---

### Query Users

Standard SQL SELECT on `system.users` table.

**List all active users** (excludes soft-deleted):
```sql
SELECT user_id, username, email, role, created_at 
FROM system.users 
WHERE deleted_at IS NULL;
```

**List deleted users** (within grace period):
```sql
SELECT user_id, username, deleted_at, role 
FROM system.users 
WHERE deleted_at IS NOT NULL;
```

**Filter by role**:
```sql
SELECT username, email, created_at 
FROM system.users 
WHERE role = 'service' AND deleted_at IS NULL;
```

**Find specific user**:
```sql
SELECT * FROM system.users WHERE username = 'alice';
```

---

## User Table Operations

User tables create one table instance per user with isolated storage.

### CREATE USER TABLE

```sql
CREATE USER TABLE [<namespace>.]<table_name> (
  <column_name> <data_type> [NOT NULL] [DEFAULT <value|function>],
  ...
) [STORAGE <storage_id>] [USE_USER_STORAGE] [FLUSH <flush_policy>];
```

**Storage Options**:
- `STORAGE <storage_id>`: Storage location reference from `system.storages` (defaults to `local`).
- `USE_USER_STORAGE`: Opt-in flag telling KalamDB to prefer each caller's storage preference (when defined) and fall back to the table storage otherwise. The flag is already persisted; the flush path will start honoring it once the per-user resolver ships.
- Storage IDs must be pre-configured via `CREATE STORAGE` command.

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

-- Table that opts into per-user storage preferences
CREATE USER TABLE app.user_events (
  event_id BIGINT NOT NULL DEFAULT SNOWFLAKE_ID(),
  payload JSON,
  created_at TIMESTAMP DEFAULT NOW()
) STORAGE s3_us USE_USER_STORAGE FLUSH ROW_THRESHOLD 5000;

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
- Pair `USE_USER_STORAGE` tables with per-user preferences (next section) to keep each tenant's data in their chosen region

### Per-user storage preferences

User rows in `system.users` expose two columns that control routing when `USE_USER_STORAGE` is enabled:

- `storage_mode`: `table` (default) or `region`. `region` tells KalamDB to try the user's own storage first.
- `storage_id`: optional reference to `system.storages`.

Example assignments:

```sql
UPDATE system.users
SET storage_mode = 'region', storage_id = 's3_eu'
WHERE username = 'alice';

UPDATE system.users
SET storage_mode = 'table', storage_id = NULL
WHERE username = 'bob';
```

Only `dba`/`system` roles should modify these columns. A dedicated `ALTER USER` syntax is planned. See `docs/how-to/user-table-storage.md` for a deeper walkthrough and current limitations.

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
) TTL <seconds>;
```

**Options**:
- `TTL <seconds>`: **Required** - Time-to-live in seconds (rows expire after this duration)

**Note**: `EPHEMERAL` and `MAX_BUFFER` options shown in examples are **not yet implemented**. Currently only `TTL` is supported.

**Examples**:
```sql
-- Live events with 10-second TTL
CREATE STREAM TABLE app.live_events (
  event_id TEXT NOT NULL,
  event_type TEXT,
  payload TEXT,
  timestamp TIMESTAMP
) TTL 10;

-- Sensor data with 60-second TTL
CREATE STREAM TABLE app.sensor_data (
  sensor_id TEXT NOT NULL,
  temperature DOUBLE,
  humidity DOUBLE,
  timestamp TIMESTAMP
) TTL 60;

-- Notifications with 5-second TTL
CREATE STREAM TABLE app.notifications (
  user_id TEXT NOT NULL,
  message TEXT,
  timestamp TIMESTAMP
) TTL 5;
```

**Important**:
- **No system columns**: Stream tables do NOT have `_updated` or `_deleted` columns
- **Memory-only**: Data stored ONLY in memory (no RocksDB, no Parquet files)
- **Data lost on restart**: All stream table data is cleared when server restarts (ephemeral by design)
- **TTL-based eviction**: Old rows automatically deleted when TTL expires
- **Future features**: `EPHEMERAL` mode and `MAX_BUFFER` limits are planned but not yet implemented

**Storage**:
- **In-memory only**: Stored in `DashMap<table_key, BTreeMap<event_key, event_data>>`
- **No persistence**: No RocksDB column families, no Parquet files
- **No cold tier**: All data is ephemeral and memory-resident
- **Server restart**: All stream table data is lost (expected behavior)

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
FLUSH ALL TABLES [IN <namespace>];
```

**Behavior**:
- Triggers flush for all USER and SHARED tables in namespace
- Each table gets its own async flush job
- Returns array of job_ids (one per table)
- When the `IN` clause is omitted, the command uses the current session namespace (defaults to `default`)

**Examples**:
```sql
-- Flush all tables in app namespace
FLUSH ALL TABLES IN app;

-- Flush all tables in production namespace
FLUSH ALL TABLES IN production;

-- Use current session namespace (e.g., after `USE analytics;`)
FLUSH ALL TABLES;
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
| `SMALLINT` | 16-bit signed integer | `-32768` to `32767` |
| `FLOAT` | 32-bit floating point | `3.14` |
| `DOUBLE` | 64-bit floating point | `2.718281828` |
| `DECIMAL(p, s)` | Fixed-precision decimal | `DECIMAL(10, 2)` for money |
| `TEXT` / `VARCHAR` | Variable-length string | `'Hello World'` |
| `UUID` | 128-bit universally unique identifier | `'550e8400-e29b-41d4-a716-446655440000'` |
| `TIMESTAMP` | Date and time | `'2025-10-20T15:30:00Z'`, `NOW()` |
| `DATE` | Date only | `'2025-10-20'` |
| `TIME` | Time only | `'15:30:00'` |
| `INTERVAL` | Time duration | `INTERVAL '1 hour'`, `INTERVAL '7 days'` |
| `BINARY` / `BYTES` | Binary data | `X'DEADBEEF'` |
| `JSON` | JSON data (stored as TEXT) | `'{"key": "value"}'` |
| `EMBEDDING(dimension)` | Fixed-size vector for AI/ML | `EMBEDDING(384)`, `EMBEDDING(768)` |

**Notes**:
- `TEXT` and `VARCHAR` are equivalent
- `TIMESTAMP` supports time zones (stored in UTC)
- `JSON` is stored as TEXT (parse/validate in application)
- `DECIMAL(precision, scale)` - precision: 1-38 digits, scale ≤ precision
- `UUID` stored as 16-byte FixedSizeBinary in Arrow/Parquet
- `EMBEDDING(dimension)` - dimension: 1-8192 (common: 384, 768, 1536, 3072)

### Modern Data Types (Added in v0.2.0)

#### UUID - Universally Unique Identifiers

**Description**: 128-bit identifier following RFC 4122 standard.

**Storage**: 16 bytes (FixedSizeBinary(16) in Arrow/Parquet)

**Usage**:
```sql
CREATE USER TABLE app.users (
  user_id UUID PRIMARY KEY DEFAULT UUID_V7(),
  email TEXT NOT NULL,
  created_at TIMESTAMP DEFAULT NOW()
);

-- Insert with explicit UUID
INSERT INTO app.users (user_id, email) VALUES 
  ('550e8400-e29b-41d4-a716-446655440000', 'alice@example.com');

-- UUID functions
SELECT user_id FROM app.users WHERE user_id = UUID_V7();
```

#### DECIMAL - Fixed-Precision Numbers

**Description**: Exact numeric type for financial calculations.

**Format**: `DECIMAL(precision, scale)`
- **precision**: Total digits (1-38)
- **scale**: Decimal places (≤ precision)

**Storage**: 16 bytes (Decimal128 in Arrow/Parquet)

**Usage**:
```sql
CREATE SHARED TABLE app.products (
  product_id BIGINT PRIMARY KEY,
  price DECIMAL(10, 2),  -- Up to $99,999,999.99
  tax_rate DECIMAL(5, 4), -- Up to 9.9999 (e.g., 0.0825 = 8.25%)
  quantity INT
);

INSERT INTO app.products (product_id, price, tax_rate, quantity) VALUES
  (1, 1234.56, 0.0825, 100);

-- Exact arithmetic (no floating-point errors)
SELECT product_id, price * (1 + tax_rate) AS total_price
FROM app.products;
```

**Common Precision/Scale Combinations**:
- `DECIMAL(10, 2)` - Money (up to $99,999,999.99)
- `DECIMAL(19, 4)` - High-precision money
- `DECIMAL(5, 4)` - Percentages/rates (0.0000 to 9.9999)

#### SMALLINT - Space-Efficient Small Integers

**Description**: 16-bit signed integer for small numeric values.

**Range**: -32,768 to 32,767

**Storage**: 2 bytes (Int16 in Arrow/Parquet)

**Usage**:
```sql
CREATE USER TABLE app.tasks (
  task_id BIGINT PRIMARY KEY,
  priority SMALLINT,  -- -32768 to 32767
  status_code SMALLINT,  -- e.g., 200, 404, 500
  retry_count SMALLINT,
  description TEXT
);

INSERT INTO app.tasks (task_id, priority, status_code, retry_count, description) VALUES
  (1, 5, 200, 0, 'High priority task');

-- Efficient storage for enums/status codes
SELECT * FROM app.tasks WHERE status_code = 200 AND priority > 3;
```

#### EMBEDDING - Vector Storage for AI/ML

**Description**: Fixed-size float32 vector for semantic search, embeddings, and ML applications.

**Format**: `EMBEDDING(dimension)`
- **dimension**: Vector size (1-8192)
- Common dimensions: 384 (MiniLM), 768 (BERT), 1536 (OpenAI text-embedding-3-small), 3072 (OpenAI text-embedding-3-large)

**Storage**: `dimension * 4` bytes (FixedSizeList<Float32> in Arrow/Parquet)

**Usage**:
```sql
-- Create table with embeddings
CREATE USER TABLE app.documents (
  doc_id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
  content TEXT NOT NULL,
  embedding EMBEDDING(384),  -- MiniLM sentence embeddings
  created_at TIMESTAMP DEFAULT NOW()
);

-- Insert document with embedding (from application)
INSERT INTO app.documents (content, embedding) VALUES
  ('KalamDB is a real-time database', ARRAY[0.123, -0.456, 0.789, ...]); -- 384 floats

-- Query embeddings (typically for vector search in application layer)
SELECT doc_id, content, embedding FROM app.documents WHERE doc_id = 123;
```

**Common Embedding Dimensions**:

| Model | Dimension | Use Case |
|-------|-----------|----------|
| `all-MiniLM-L6-v2` | 384 | Fast semantic search |
| `BERT-base` | 768 | General NLP tasks |
| `OpenAI text-embedding-3-small` | 1536 | Production embeddings |
| `OpenAI text-embedding-3-large` | 3072 | High-quality embeddings |
| `Llama-2-7B` | 4096 | Large language models |

**Performance Characteristics**:
- **Storage**: EMBEDDING(384) = 1,536 bytes per row
- **Parquet Compression**: ~30-50% reduction with SNAPPY
- **Insert Performance**: Same as other column types (included in batch writes)
- **Query Performance**: Full scan (vector search requires external index)

**Integration with Vector Search**:

KalamDB stores embeddings efficiently but does NOT provide built-in vector similarity search (cosine distance, dot product, etc.). For semantic search:

1. **Store embeddings** in KalamDB EMBEDDING columns
2. **Retrieve embeddings** via standard SQL queries
3. **Perform similarity search** in application layer or external vector index (FAISS, Pinecone, Weaviate)

**Example Workflow**:

```rust
// Application layer
use kalamdb_client::KalamClient;

// 1. Store document embeddings in KalamDB
let embedding = model.encode("KalamDB is a real-time database"); // Vec<f32> with 384 dims
client.execute("INSERT INTO app.documents (content, embedding) VALUES (?, ?)", 
               &[content, embedding]).await?;

// 2. Retrieve all embeddings for search
let rows = client.query("SELECT doc_id, embedding FROM app.documents").await?;

// 3. Compute cosine similarity in application
let query_embedding = model.encode(user_query);
let results = rows.iter()
    .map(|row| (row.doc_id, cosine_similarity(&query_embedding, &row.embedding)))
    .sorted_by(|(_, score1), (_, score2)| score2.partial_cmp(score1))
    .take(10)
    .collect();
```

**Future Enhancements** (Roadmap):
- Native vector similarity functions: `COSINE_DISTANCE(emb1, emb2)`
- HNSW index support for approximate nearest neighbor (ANN) search
- Quantization (int8, binary) for memory efficiency
- GPU-accelerated similarity computations

**Best Practices**:
1. **Normalize embeddings** to unit length before storage (enables dot product = cosine similarity)
2. **Use appropriate dimension** for your model (don't over-provision)
3. **Batch inserts** for multiple embeddings (use INSERT with multiple VALUES)
4. **Separate table** for embeddings if not querying with document content
5. **Compress Parquet files** with SNAPPY or ZSTD (30-50% size reduction)

**Validation**:
- **Dimension range**: 1 ≤ dimension ≤ 8192
- **Array length**: Must match declared dimension exactly
- **Value type**: Float32 (-3.4e38 to 3.4e38)

**Errors**:
```sql
-- ❌ Invalid dimension (too large)
CREATE USER TABLE app.docs (embedding EMBEDDING(10000));
-- Error: EMBEDDING dimension must be between 1 and 8192

-- ❌ Array length mismatch
INSERT INTO app.docs (embedding) VALUES (ARRAY[0.1, 0.2]);  -- Expected 384 elements
-- Error: Expected 384 floats, got 2
```

### DATETIME and TIMESTAMP Timezone Behavior

**Important**: KalamDB normalizes all `DATETIME` and `TIMESTAMP` values to UTC for storage. **Original timezone offsets are NOT preserved.**

**Behavior**:
- Input with timezone offset → **Converted to UTC** → Stored without offset
- Input without timezone → Treated as **local time** → Stored as-is
- On retrieval → Returned as UTC timestamp (no timezone info)

**Examples**:

```sql
-- Input: '2025-01-01T12:00:00+02:00' (Berlin time, noon)
-- Stored: '2025-01-01T10:00:00Z' (UTC, 10:00 AM)
-- Retrieved: '2025-01-01T10:00:00' (no +00:00 suffix)

-- Input: '2025-01-01T12:00:00' (no timezone specified)
-- Stored: '2025-01-01T12:00:00' (assumed UTC or local time)
-- Retrieved: '2025-01-01T12:00:00'
```

**Best Practices**:
1. **Always specify timezone offsets** in DATETIME literals: `'2025-01-01T12:00:00+02:00'`
2. **Store all times in UTC** at the application layer if timezone preservation is needed
3. **Use a separate column** to store original timezone if required: `user_timezone TEXT`

**Example with Timezone Handling**:

```sql
CREATE USER TABLE app.events (
  id BIGINT DEFAULT SNOWFLAKE_ID(),
  event_time DATETIME NOT NULL,        -- Stored in UTC
  user_timezone TEXT,                   -- Store original timezone separately
  description TEXT
);

-- Insert with explicit timezone (Berlin)
INSERT INTO app.events (event_time, user_timezone, description) VALUES
  ('2025-01-01T14:00:00+01:00', 'Europe/Berlin', 'Meeting scheduled');

-- Query: event_time is now '2025-01-01T13:00:00' (UTC)
-- Use user_timezone column to convert back to local time in application
```

**Testing Timezone Behavior**:

See `backend/tests/test_datetime_timezone_storage.rs` for comprehensive timezone handling tests demonstrating UTC normalization.

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

## KalamDB Custom Functions

KalamDB provides custom SQL functions for ID generation and session context. These functions can be used in DEFAULT clauses, SELECT statements, and WHERE conditions.

### ID Generation Functions

#### SNOWFLAKE_ID()

Generates a 64-bit distributed unique identifier with time-ordering properties.

**Signature**: `SNOWFLAKE_ID() -> BIGINT`

**Structure** (64 bits total):
- **41 bits**: Timestamp in milliseconds since Unix epoch (supports ~69 years)
- **10 bits**: Node ID (0-1023 for distributed deployments)
- **12 bits**: Sequence number (4096 IDs per millisecond per node)

**Properties**:
- **Time-ordered**: IDs increase monotonically with time
- **Distributed**: No coordination needed across nodes (configure `node_id` in config.toml)
- **High throughput**: 4 million IDs per second per node
- **Sortable**: Numeric sorting equals chronological sorting
- **Compact**: 8 bytes storage

**Configuration**:
```toml
# config.toml
[flush]
node_id = 0  # Range: 0-1023
```

**Examples**:

```sql
-- CREATE: Table with auto-generated primary key
CREATE USER TABLE app.orders (
  order_id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
  customer_id TEXT NOT NULL,
  total_amount DOUBLE,
  created_at TIMESTAMP DEFAULT NOW()
) FLUSH ROW_THRESHOLD 1000;

-- INSERT: Omit order_id, it will be auto-generated
INSERT INTO app.orders (customer_id, total_amount)
VALUES ('customer_123', 99.99);

-- INSERT: Multiple rows with auto-generated IDs
INSERT INTO app.orders (customer_id, total_amount) VALUES
  ('customer_456', 149.50),
  ('customer_789', 75.25),
  ('customer_101', 250.00);

-- SELECT: Query with SNOWFLAKE_ID() in WHERE clause
SELECT * FROM app.orders 
WHERE order_id > SNOWFLAKE_ID()  -- Orders created after this moment
ORDER BY order_id DESC;

-- SELECT: Generate IDs in query result
SELECT 
  SNOWFLAKE_ID() as new_id,
  customer_id,
  total_amount
FROM app.orders;

-- UPDATE: Use SNOWFLAKE_ID() for versioning
UPDATE app.orders 
SET version_id = SNOWFLAKE_ID()
WHERE order_id = 12345;
```

**Use Cases**:
- Primary keys for high-volume tables
- Event IDs in distributed systems
- Correlation IDs for tracing
- Timestamp-based sharding keys

---

#### UUID_V7()

Generates RFC 9562 compliant UUIDv7 identifiers with time-ordering.

**Signature**: `UUID_V7() -> TEXT`

**Structure** (128 bits, 36 characters with hyphens):
- **48 bits**: Unix timestamp in milliseconds
- **12 bits**: Version (7) and variant bits
- **62 bits**: Random data

**Format**: `018b6e8a-07d1-7000-8000-0123456789ab` (8-4-4-4-12 pattern)

**Properties**:
- **RFC 9562 compliant**: Standard UUID format
- **Time-ordered**: Lexicographically sortable by creation time
- **Globally unique**: 128-bit randomness ensures no collisions
- **Interoperable**: Works with UUID libraries in all languages
- **URL-safe**: Can be used in URLs without encoding

**Examples**:

```sql
-- CREATE: Table with UUID primary key
CREATE USER TABLE app.sessions (
  session_id TEXT PRIMARY KEY DEFAULT UUID_V7(),
  user_id TEXT NOT NULL,
  ip_address TEXT,
  created_at TIMESTAMP DEFAULT NOW(),
  expires_at TIMESTAMP
) FLUSH ROW_THRESHOLD 500;

-- INSERT: Auto-generate session IDs
INSERT INTO app.sessions (user_id, ip_address, expires_at)
VALUES ('user_alice', '192.168.1.100', NOW() + INTERVAL '1 hour');

-- INSERT: Multiple sessions with UUIDs
INSERT INTO app.sessions (user_id, ip_address, expires_at) VALUES
  ('user_bob', '192.168.1.101', NOW() + INTERVAL '2 hours'),
  ('user_charlie', '192.168.1.102', NOW() + INTERVAL '30 minutes');

-- SELECT: Query sessions created in last hour
SELECT session_id, user_id, created_at
FROM app.sessions
WHERE created_at > NOW() - INTERVAL '1 hour'
ORDER BY session_id;  -- Time-ordered due to UUID_V7

-- SELECT: Generate new UUIDs in query
SELECT 
  UUID_V7() as correlation_id,
  session_id,
  user_id
FROM app.sessions
WHERE expires_at > NOW();

-- UPDATE: Add correlation ID
UPDATE app.sessions
SET correlation_id = UUID_V7()
WHERE session_id = '018b6e8a-07d1-7000-8000-0123456789ab';
```

**Use Cases**:
- Session identifiers
- API request/response tracking
- Distributed transaction IDs
- External-facing identifiers (customer-visible)
- Multi-system integration keys

---

#### ULID()

Generates Universally Unique Lexicographically Sortable Identifiers.

**Signature**: `ULID() -> TEXT`

**Structure** (128 bits, 26 characters):
- **48 bits**: Unix timestamp in milliseconds
- **80 bits**: Random data
- **Encoding**: Crockford Base32 (0-9, A-Z excluding I, L, O, U)

**Format**: `01HGW4VJKQZ7Y8X9P6T5R4N3M2` (26 characters, no hyphens)

**Properties**:
- **Time-ordered**: Lexicographically sortable
- **URL-safe**: No special characters or hyphens
- **Case-insensitive**: Can be stored uppercase or lowercase
- **Compact**: 26 characters vs 36 for UUID
- **Copy-paste friendly**: No hyphens to break selection
- **Collision-resistant**: 1.21e+24 possible values per millisecond

**Examples**:

```sql
-- CREATE: Table with ULID primary key
CREATE USER TABLE app.events (
  event_id TEXT PRIMARY KEY DEFAULT ULID(),
  event_type TEXT NOT NULL,
  user_id TEXT,
  payload TEXT,
  created_at TIMESTAMP DEFAULT NOW()
) FLUSH ROW_THRESHOLD 2000;

-- INSERT: Auto-generate event IDs
INSERT INTO app.events (event_type, user_id, payload)
VALUES ('user_login', 'user_alice', '{"ip": "192.168.1.100"}');

-- INSERT: Bulk events with ULIDs
INSERT INTO app.events (event_type, user_id, payload) VALUES
  ('page_view', 'user_bob', '{"page": "/dashboard"}'),
  ('button_click', 'user_charlie', '{"button": "submit"}'),
  ('api_call', 'user_alice', '{"endpoint": "/api/users"}');

-- SELECT: Query recent events (time-ordered by ULID)
SELECT event_id, event_type, user_id, created_at
FROM app.events
ORDER BY event_id DESC  -- Most recent first
LIMIT 100;

-- SELECT: Generate correlation ULIDs
SELECT 
  ULID() as trace_id,
  event_id,
  event_type,
  user_id
FROM app.events
WHERE event_type = 'api_call';

-- UPDATE: Add trace ID for distributed tracing
UPDATE app.events
SET trace_id = ULID()
WHERE event_type IN ('api_call', 'database_query');
```

**Use Cases**:
- Event tracking and logging
- Distributed tracing IDs
- Short URLs and slugs
- Mobile app offline ID generation
- QR code identifiers

---

#### CURRENT_USER()

Returns the user ID from the current session context.

**Signature**: `CURRENT_USER() -> TEXT`

**Properties**:
- **Session-scoped**: Returns authenticated user's ID
- **Stable**: Constant within a transaction/request
- **Automatic**: No configuration needed

**Examples**:

```sql
-- CREATE: Table tracking who created records
CREATE USER TABLE app.documents (
  doc_id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
  title TEXT NOT NULL,
  content TEXT,
  created_by TEXT DEFAULT CURRENT_USER(),
  created_at TIMESTAMP DEFAULT NOW(),
  modified_by TEXT,
  modified_at TIMESTAMP
) FLUSH ROW_THRESHOLD 1000;

-- INSERT: created_by automatically set to current user
INSERT INTO app.documents (title, content)
VALUES ('Quarterly Report', 'Sales increased by 25%...');

-- INSERT: Multiple documents
INSERT INTO app.documents (title, content) VALUES
  ('Meeting Notes', 'Attendees: Alice, Bob, Charlie...'),
  ('Project Plan', 'Phase 1: Requirements gathering...');

-- SELECT: Query documents created by current user
SELECT doc_id, title, created_at
FROM app.documents
WHERE created_by = CURRENT_USER()
ORDER BY created_at DESC;

-- UPDATE: Track who modified the document
UPDATE app.documents
SET 
  content = 'Updated content...',
  modified_by = CURRENT_USER(),
  modified_at = NOW()
WHERE doc_id = 12345;

-- SELECT: Audit trail
SELECT 
  doc_id,
  title,
  created_by,
  created_at,
  modified_by,
  modified_at,
  CURRENT_USER() as current_session_user
FROM app.documents
WHERE modified_by IS NOT NULL;
```

**Use Cases**:
- Audit trails and tracking
- Row-level security
- User-specific data filtering
- Change history logging

---

### ID Generation Function Comparison

| Feature | SNOWFLAKE_ID() | UUID_V7() | ULID() |
|---------|----------------|-----------|--------|
| **Type** | BIGINT (64-bit) | TEXT (36 chars) | TEXT (26 chars) |
| **Size** | 8 bytes | 36 bytes | 26 bytes |
| **Format** | Numeric | 8-4-4-4-12 hyphenated | Crockford Base32 |
| **Time-ordered** | ✅ Yes | ✅ Yes | ✅ Yes |
| **Distributed** | ✅ Yes (node_id) | ✅ Yes | ✅ Yes |
| **Throughput** | 4M/sec/node | Unlimited | Unlimited |
| **Collision Risk** | None (with node_id) | Negligible | Negligible |
| **URL-safe** | ✅ Yes | ⚠️ Requires encoding | ✅ Yes |
| **Human-readable** | ❌ No | ⚠️ Partially | ⚠️ Partially |
| **Database Index** | ✅ Excellent | ✅ Good | ✅ Good |
| **RFC Standard** | ❌ No | ✅ RFC 9562 | ❌ No (spec exists) |
| **Best For** | High-volume tables | External APIs | Logs/events |

**Choosing the Right Function**:

- **Use SNOWFLAKE_ID()** when:
  - You need maximum performance (numeric indexing)
  - Working with high-volume tables (millions of rows)
  - Building distributed systems with multiple nodes
  - Storage efficiency is critical

- **Use UUID_V7()** when:
  - You need RFC-standard UUIDs
  - Integrating with external systems
  - Customer-facing identifiers
  - Cross-platform compatibility required

- **Use ULID()** when:
  - You need URL-friendly IDs
  - Building APIs with short, readable IDs
  - Copy-paste friendliness matters
  - You want compact text IDs

---

### Complete Example: E-commerce System

```sql
-- 1. Create namespace
CREATE NAMESPACE shop;

-- 2. Users table with SNOWFLAKE_ID
CREATE USER TABLE shop.users (
  user_id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
  username TEXT NOT NULL,
  email TEXT NOT NULL,
  created_at TIMESTAMP DEFAULT NOW(),
  created_by TEXT DEFAULT CURRENT_USER()
) FLUSH ROW_THRESHOLD 1000;

-- 3. Orders table with UUID_V7 (external-facing)
CREATE USER TABLE shop.orders (
  order_id TEXT PRIMARY KEY DEFAULT UUID_V7(),
  user_id BIGINT NOT NULL,
  total_amount DOUBLE NOT NULL,
  status TEXT DEFAULT 'pending',
  created_at TIMESTAMP DEFAULT NOW(),
  updated_at TIMESTAMP DEFAULT NOW()
) FLUSH ROW_THRESHOLD 500;

-- 4. Events table with ULID (logging)
CREATE SHARED TABLE shop.events (
  event_id TEXT PRIMARY KEY DEFAULT ULID(),
  event_type TEXT NOT NULL,
  user_id BIGINT,
  order_id TEXT,
  payload TEXT,
  created_at TIMESTAMP DEFAULT NOW()
) FLUSH INTERVAL 60s ROW_THRESHOLD 5000;

-- 5. Insert users (IDs auto-generated)
INSERT INTO shop.users (username, email) VALUES
  ('alice', 'alice@example.com'),
  ('bob', 'bob@example.com'),
  ('charlie', 'charlie@example.com');

-- 6. Insert orders (IDs auto-generated)
INSERT INTO shop.orders (user_id, total_amount) VALUES
  (1, 99.99),
  (2, 149.50),
  (1, 75.25);

-- 7. Log events (IDs auto-generated)
INSERT INTO shop.events (event_type, user_id, order_id, payload) VALUES
  ('order_created', 1, '018b6e8a-07d1-7000-8000-0123456789ab', '{"items": 3}'),
  ('payment_processed', 1, '018b6e8a-07d1-7000-8000-0123456789ab', '{"method": "card"}'),
  ('order_shipped', 1, '018b6e8a-07d1-7000-8000-0123456789ab', '{"carrier": "ups"}');

-- 8. Query recent orders for current user
SELECT 
  order_id,
  total_amount,
  status,
  created_at
FROM shop.orders
WHERE user_id IN (
  SELECT user_id FROM shop.users WHERE created_by = CURRENT_USER()
)
ORDER BY created_at DESC
LIMIT 10;

-- 9. Query order timeline (events sorted by ULID)
SELECT 
  event_id,
  event_type,
  payload,
  created_at
FROM shop.events
WHERE order_id = '018b6e8a-07d1-7000-8000-0123456789ab'
ORDER BY event_id;  -- Time-ordered by ULID

-- 10. Update order status with audit trail
UPDATE shop.orders
SET 
  status = 'shipped',
  updated_at = NOW()
WHERE order_id = '018b6e8a-07d1-7000-8000-0123456789ab';

-- Log the status change
INSERT INTO shop.events (event_type, user_id, order_id, payload)
VALUES (
  'status_changed',
  (SELECT user_id FROM shop.orders WHERE order_id = '018b6e8a-07d1-7000-8000-0123456789ab'),
  '018b6e8a-07d1-7000-8000-0123456789ab',
  '{"old": "pending", "new": "shipped", "by": "' || CURRENT_USER() || '"}'
);
```

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
) STORAGE local FLUSH INTERVAL 300s ROW_THRESHOLD 10;

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
) TTL 10;

-- 6. Insert data (using DEFAULT functions)
INSERT INTO app.messages (content, author) VALUES ('Hello World', 'alice');

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
