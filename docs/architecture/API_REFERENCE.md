# KalamDB REST API Reference

**Version**: 0.1.0  
**Base URL**: `http://localhost:8080`  
**API Version**: v1

## Overview

KalamDB provides a versioned REST API with a single SQL endpoint that accepts SQL commands for all operations. This unified interface simplifies client integration and enables full SQL expressiveness.

### API Versioning

All API endpoints are versioned with a `/v1` prefix to ensure backward compatibility as the API evolves.

**Current Version**: v1  
**Endpoint Prefix**: `/v1`

**Versioned Endpoints**:
- `/v1/api/sql` - Execute SQL commands
- `/v1/ws` - WebSocket for live query subscriptions
- `/v1/api/healthcheck` - Server health status

**Version Strategy**:
- **Breaking changes** require a new version (e.g., `/v2/api/sql`)
- **Non-breaking changes** (new optional fields, new endpoints) can be added to existing versions
- **Deprecation policy**: Old versions supported for at least 6 months after new version release

---

## Authentication

All requests must include HTTP Basic Authentication or JWT token:

### HTTP Basic Auth (Recommended)
```http
Authorization: Basic <base64(username:password)>
```

### JWT Token Authentication
```http
Authorization: Bearer <JWT_TOKEN>
```

---

## Endpoints

### POST /v1/api/sql

Execute a SQL command and receive results.

**Version**: v1  
**Path**: `/v1/api/sql`

#### Request

**Headers**:
- `Content-Type: application/json`
- `Authorization: Basic <credentials>` or `Authorization: Bearer <token>` (required)

**Body**:
```json
{
  "sql": "SELECT * FROM app.messages LIMIT 10"
}
```

#### Response

**Success (200 OK)**:
```json
{
  "status": "success",
  "results": [
    {
      "id": 1,
      "content": "Hello World",
      "timestamp": "2025-10-20T10:30:00Z",
      "_updated": "2025-10-20T10:30:00Z",
      "_deleted": false
    }
  ],
  "took_ms": 42,
  "rows_affected": 1
}
```

**DDL Operations (200 OK)**:
```json
{
  "status": "success",
  "message": "Table created successfully",
  "took_ms": 15
}
```

**Error (400 Bad Request)**:
```json
{
  "status": "error",
  "error": "Table not found: app.nonexistent",
  "error_type": "TableNotFound",
  "took_ms": 5
}
```

**Error (500 Internal Server Error)**:
```json
{
  "status": "error",
  "error": "Internal server error: RocksDB write failed",
  "error_type": "Storage",
  "took_ms": 10
}
```

---

## SQL Command Reference

### Namespace Operations

#### CREATE NAMESPACE

Create a new namespace (database).

```sql
CREATE NAMESPACE app;
CREATE NAMESPACE IF NOT EXISTS app;
```

**Response**:
```json
{
  "status": "success",
  "message": "Namespace 'app' created successfully"
}
```

#### DROP NAMESPACE

Drop an existing namespace and all its tables.

```sql
DROP NAMESPACE app;
DROP NAMESPACE IF EXISTS app;
```

**Response**:
```json
{
  "status": "success",
  "message": "Namespace 'app' dropped successfully"
}
```

---

### User Table Operations

User tables create one table instance per user with isolated storage.

#### CREATE USER TABLE

```sql
CREATE USER TABLE app.messages (
  id BIGINT,
  conversation_id BIGINT,
  content TEXT,
  author TEXT,
  timestamp TIMESTAMP
) FLUSH POLICY ROW_LIMIT 1000;

CREATE USER TABLE app.events (
  event_id TEXT,
  data TEXT
) FLUSH POLICY TIME_INTERVAL 300;

CREATE USER TABLE app.analytics (
  metric_name TEXT,
  value DOUBLE
) FLUSH POLICY COMBINED ROW_LIMIT 5000 TIME_INTERVAL 600;
```

**Flush Policies**:
- `ROW_LIMIT <n>`: Flush to Parquet after `n` rows
- `TIME_INTERVAL <seconds>`: Flush to Parquet every `<seconds>` seconds
- `COMBINED ROW_LIMIT <n> TIME_INTERVAL <s>`: Flush when either condition is met

**System Columns** (auto-added):
- `_updated TIMESTAMP`: Last update timestamp
- `_deleted BOOLEAN`: Soft delete flag

**Response**:
```json
{
  "status": "success",
  "message": "User table 'app.messages' created successfully"
}
```

---

### Shared Table Operations

Shared tables are accessible to all users with centralized storage.

#### CREATE SHARED TABLE

```sql
CREATE SHARED TABLE app.global_config (
  config_key TEXT,
  config_value TEXT,
  updated_at TIMESTAMP
) FLUSH POLICY ROW_LIMIT 100;
```

**System Columns** (auto-added):
- `_updated TIMESTAMP`: Last update timestamp
- `_deleted BOOLEAN`: Soft delete flag

**Response**:
```json
{
  "status": "success",
  "message": "Shared table 'app.global_config' created successfully"
}
```

---

### Stream Table Operations

Stream tables are ephemeral with TTL-based eviction. Data is memory-only (no Parquet files).

#### CREATE STREAM TABLE

```sql
CREATE STREAM TABLE app.live_events (
  event_id TEXT,
  event_type TEXT,
  payload TEXT,
  timestamp TIMESTAMP
) RETENTION 10 EPHEMERAL MAX_BUFFER 10000;

CREATE STREAM TABLE app.sensor_data (
  sensor_id TEXT,
  temperature DOUBLE,
  humidity DOUBLE
) RETENTION 60 MAX_BUFFER 50000;
```

**Options**:
- `RETENTION <seconds>`: TTL in seconds (rows expire after this duration)
- `EPHEMERAL`: Only keep rows in memory if subscribers exist (no buffering without subscribers)
- `MAX_BUFFER <n>`: Maximum buffer size in rows (oldest rows evicted when exceeded)

**Note**: Stream tables do NOT have `_updated` or `_deleted` columns (ephemeral data only).

**Response**:
```json
{
  "status": "success",
  "message": "Stream table 'app.live_events' created successfully"
}
```

---

### Data Manipulation

#### INSERT

```sql
INSERT INTO app.messages (id, content, author, timestamp)
VALUES (1, 'Hello World', 'alice', NOW());

-- Batch insert
INSERT INTO app.messages (id, content) VALUES
  (2, 'Message 1'),
  (3, 'Message 2'),
  (4, 'Message 3');
```

**Response**:
```json
{
  "status": "success",
  "rows_affected": 1,
  "took_ms": 3
}
```

#### UPDATE

```sql
UPDATE app.messages
SET content = 'Updated message'
WHERE id = 1;
```

**Response**:
```json
{
  "status": "success",
  "rows_affected": 1,
  "took_ms": 5
}
```

**Note**: Updates set `_updated = NOW()` automatically.

#### DELETE

```sql
-- Soft delete (sets _deleted = true)
DELETE FROM app.messages WHERE id = 1;
```

**Response**:
```json
{
  "status": "success",
  "rows_affected": 1,
  "took_ms": 4
}
```

**Note**: Soft-deleted rows remain in storage for the configured retention period (default: 7 days).

#### SELECT

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
```

**Response**:
```json
{
  "status": "success",
  "results": [
    {"id": 1, "content": "Hello", "author": "alice", "timestamp": "2025-10-20T10:00:00Z"}
  ],
  "took_ms": 12,
  "rows_returned": 1
}
```

---

### Schema Evolution

#### ALTER TABLE

```sql
-- Add column
ALTER TABLE app.messages ADD COLUMN reaction TEXT;

-- Drop column
ALTER TABLE app.messages DROP COLUMN reaction;

-- Rename column
ALTER TABLE app.messages RENAME COLUMN content TO message_text;
```

**Response**:
```json
{
  "status": "success",
  "message": "Schema updated successfully. Version incremented to 2.",
  "took_ms": 8
}
```

**Restrictions**:
- Cannot alter system columns (`_updated`, `_deleted`)
- Cannot alter stream tables (immutable schema)
- Cannot drop required columns (would break existing queries)

---

### Table Management

#### DROP TABLE

```sql
DROP TABLE app.messages;
DROP TABLE IF EXISTS app.messages;
```

**Response**:
```json
{
  "status": "success",
  "message": "Table 'app.messages' dropped successfully"
}
```

**Note**: Drops RocksDB column family and deletes all Parquet files.

---

### Storage Management

KalamDB supports multiple storage backends for flushing table data to Parquet files. Storage backends can be managed dynamically via SQL commands.

#### CREATE STORAGE

Define a new storage backend for table data.

**Filesystem Storage**:
```sql
CREATE STORAGE archive
TYPE filesystem
NAME 'Archive Storage'
DESCRIPTION 'Cold storage for archived data'
PATH '/data/archive'
SHARED_TABLES_TEMPLATE '{namespace}/{tableName}'
USER_TABLES_TEMPLATE '{namespace}/{tableName}/{userId}';
```

**S3 Storage**:
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

**Parameters**:
- `storage_id` (required): Unique identifier for the storage backend
- `TYPE` (required): Storage type - `filesystem` or `s3`
- `NAME` (required): Human-readable name
- `DESCRIPTION` (optional): Description of the storage backend
- **For filesystem**:
  - `PATH`: Base directory path
- **For S3**:
  - `BUCKET`: S3 bucket name
  - `REGION`: AWS region
- `SHARED_TABLES_TEMPLATE` (required): Path template for shared tables
- `USER_TABLES_TEMPLATE` (required): Path template for user tables

**Template Variables**:
- `{namespace}`: Namespace identifier
- `{tableName}`: Table name
- `{userId}`: User identifier (required for user tables)
- `{shard}`: Shard identifier (optional)

**Template Ordering Rules**:
- **Shared tables**: `{namespace}` must appear before `{tableName}`
- **User tables**: Must include `{userId}` and follow ordering: `{namespace}` → `{tableName}` → `{userId}` → `{shard}`

**Response**:
```json
{
  "status": "success",
  "message": "Storage 'archive' created successfully"
}
```

**Errors**:
- `400` - Invalid storage type
- `400` - Invalid template (wrong variable ordering)
- `400` - Duplicate storage_id
- `400` - Missing required parameters

#### ALTER STORAGE

Update an existing storage backend configuration.

```sql
ALTER STORAGE archive
SET NAME = 'Updated Archive Storage'
SET DESCRIPTION = 'Long-term cold storage'
SET SHARED_TABLES_TEMPLATE = '{namespace}/v2/{tableName}'
SET USER_TABLES_TEMPLATE = '{namespace}/v2/{tableName}/{userId}';
```

**Note**: You can update any field except `storage_id` and `storage_type`. Template changes are validated before application.

**Response**:
```json
{
  "status": "success",
  "message": "Storage 'archive' updated successfully"
}
```

#### DROP STORAGE

Delete a storage backend. Protected by referential integrity - cannot delete if tables reference it.

```sql
DROP STORAGE old_storage;
```

**Response**:
```json
{
  "status": "success",
  "message": "Storage 'old_storage' deleted successfully"
}
```

**Errors**:
- `400` - Storage not found
- `400` - Cannot delete 'local' storage (protected)
- `400` - Cannot delete storage: N table(s) still reference it

#### SHOW STORAGES

List all registered storage backends.

```sql
SHOW STORAGES;
```

**Response**:
```json
{
  "status": "success",
  "results": [
    {
      "storage_id": "local",
      "storage_name": "Local Filesystem",
      "description": "Default local filesystem storage",
      "storage_type": "filesystem",
      "base_directory": "",
      "shared_tables_template": "{namespace}/{tableName}",
      "user_tables_template": "{namespace}/{tableName}/{userId}",
      "created_at": 1729612800000,
      "updated_at": 1729612800000
    },
    {
      "storage_id": "s3_prod",
      "storage_name": "S3 Production Storage",
      "description": "Primary production S3 bucket",
      "storage_type": "s3",
      "base_directory": "s3://kalamdb-prod",
      "shared_tables_template": "s3://kalamdb-prod/shared/{namespace}/{tableName}",
      "user_tables_template": "s3://kalamdb-prod/users/{userId}/{namespace}/{tableName}",
      "created_at": 1729612900000,
      "updated_at": 1729612900000
    }
  ]
}
```

**Note**: Results are ordered with 'local' first, then alphabetically by storage_id.

#### Using Storage with Tables

Specify storage backend when creating tables:

```sql
-- Use specific storage for a table
CREATE USER TABLE app.messages (
    id BIGINT,
    content TEXT
) TABLE_TYPE user
OWNER_ID 'alice'
USE_USER_STORAGE 's3_prod';

-- Use default 'local' storage (implicit)
CREATE SHARED TABLE app.shared_data (
    id BIGINT,
    data TEXT
) TABLE_TYPE shared;
```

**Storage Lookup Chain** (for user tables):
1. If `USE_USER_STORAGE` specified in CREATE TABLE → use that storage
2. If user has `storage_id` preference → use user's preferred storage
3. Otherwise → use 'local' storage (default)

---

### Backup and Restore

#### BACKUP DATABASE

```sql
BACKUP DATABASE app TO '/backups/app-20251020';
```

**Response**:
```json
{
  "status": "success",
  "message": "Backup completed successfully",
  "backup_path": "/backups/app-20251020",
  "files_backed_up": 127,
  "total_bytes": 104857600,
  "took_ms": 5432
}
```

**What's Backed Up**:
- Namespace metadata
- All table metadata and schemas
- All Parquet files (user tables + shared tables)
- Soft-deleted rows preserved
- Stream tables excluded (ephemeral data)

#### RESTORE DATABASE

```sql
RESTORE DATABASE app FROM '/backups/app-20251020';
```

**Response**:
```json
{
  "status": "success",
  "message": "Restore completed successfully",
  "files_restored": 127,
  "total_bytes": 104857600,
  "took_ms": 6234
}
```

**Note**: Restores all metadata and Parquet files. RocksDB buffers start empty (data in cold storage).

#### SHOW BACKUP

```sql
SHOW BACKUP '/backups/app-20251020';
```

**Response**:
```json
{
  "status": "success",
  "backup_info": {
    "namespace": "app",
    "created_at": "2025-10-20T15:30:00Z",
    "table_count": 5,
    "total_rows": 1000000,
    "total_bytes": 104857600,
    "tables": [
      {"name": "messages", "type": "User", "rows": 500000, "bytes": 52428800},
      {"name": "analytics", "type": "Shared", "rows": 500000, "bytes": 52428800}
    ]
  }
}
```

---

### Catalog Browsing

#### SHOW TABLES

```sql
SHOW TABLES;
SHOW TABLES IN app;
```

**Response**:
```json
{
  "status": "success",
  "results": [
    {"table_name": "messages", "table_type": "User"},
    {"table_name": "global_config", "table_type": "Shared"},
    {"table_name": "live_events", "table_type": "Stream"}
  ]
}
```

#### DESCRIBE TABLE

```sql
DESCRIBE TABLE app.messages;
```

**Response**:
```json
{
  "status": "success",
  "table_info": {
    "namespace": "app",
    "table_name": "messages",
    "table_type": "User",
    "current_schema_version": 2,
    "storage_location": "user/{user_id}/messages/",
    "flush_policy": {
      "type": "ROW_LIMIT",
      "row_limit": 1000
    },
    "schema": [
      {"column": "id", "type": "BIGINT", "nullable": false},
      {"column": "content", "type": "TEXT", "nullable": true},
      {"column": "_updated", "type": "TIMESTAMP", "nullable": false, "system": true},
      {"column": "_deleted", "type": "BOOLEAN", "nullable": false, "system": true}
    ]
  }
}
```

#### SHOW STATS FOR TABLE

```sql
SHOW STATS FOR TABLE app.messages;
```

**Response**:
```json
{
  "status": "success",
  "stats": {
    "table_name": "messages",
    "hot_rows": 500,
    "cold_rows": 10000,
    "total_rows": 10500,
    "storage_bytes": 1048576,
    "last_flushed_at": "2025-10-20T15:00:00Z"
  }
}
```

---

## Error Types

| Error Type | HTTP Code | Description |
|-----------|-----------|-------------|
| `TableNotFound` | 400 | Table does not exist |
| `NamespaceNotFound` | 400 | Namespace does not exist |
| `SchemaVersionNotFound` | 400 | Requested schema version not found |
| `AlreadyExists` | 409 | Resource already exists |
| `InvalidSql` | 400 | SQL syntax error |
| `InvalidOperation` | 400 | Operation not allowed (e.g., alter stream table) |
| `InvalidSchemaEvolution` | 400 | Schema change violates constraints |
| `PermissionDenied` | 403 | User lacks permission |
| `ColumnFamily` | 500 | RocksDB column family error |
| `Flush` | 500 | Flush operation failed |
| `Backup` | 500 | Backup/restore operation failed |
| `Storage` | 500 | RocksDB storage error |
| `Internal` | 500 | Unexpected internal error |

---

## Rate Limiting

*Coming in Phase 17*

Rate limits will be enforced per user:
- Max requests per second: 100
- Max concurrent connections: 10

**Response (429 Too Many Requests)**:
```json
{
  "status": "error",
  "error": "Rate limit exceeded. Try again in 5 seconds.",
  "error_type": "RateLimitExceeded",
  "retry_after": 5
}
```

---

## Examples

### Complete Workflow

```bash
# 1. Create namespace
curl -X POST http://localhost:8080/v1/api/sql \
  -u alice:password123 \
  -H "Content-Type: application/json" \
  -d '{"sql": "CREATE NAMESPACE app"}'

# 2. Create user table
curl -X POST http://localhost:8080/v1/api/sql \
  -u alice:password123 \
  -H "Content-Type: application/json" \
  -d '{"sql": "CREATE USER TABLE app.messages (id BIGINT, content TEXT) FLUSH POLICY ROW_LIMIT 1000"}'

# 3. Insert data
curl -X POST http://localhost:8080/v1/api/sql \
  -u alice:password123 \
  -H "Content-Type: application/json" \
  -d '{"sql": "INSERT INTO app.messages (id, content) VALUES (1, '\''Hello World'\'')"}'

# 4. Query data
curl -X POST http://localhost:8080/v1/api/sql \
  -u alice:password123 \
  -H "Content-Type: application/json" \
  -d '{"sql": "SELECT * FROM app.messages LIMIT 10"}'

# 5. Backup database
curl -X POST http://localhost:8080/v1/api/sql \
  -u alice:password123 \
  -H "Content-Type: application/json" \
  -d '{"sql": "BACKUP DATABASE app TO '\''/backups/app-backup'\''"}'
```


---

### GET /v1/api/healthcheck

Check server health status and readiness.

**Version**: v1  
**Path**: `/v1/api/healthcheck`

#### Request

**Method**: GET  
**Headers**: None required

```bash
curl http://localhost:8080/v1/api/healthcheck
```

#### Response

**Success (200 OK)**:
```json
{
  "status": "healthy",
  "version": "0.1.0",
  "uptime_seconds": 3600,
  "timestamp": "2025-10-23T15:30:00Z"
}
```

**Unhealthy (503 Service Unavailable)**:
```json
{
  "status": "unhealthy",
  "error": "Database connection failed",
  "timestamp": "2025-10-23T15:30:00Z"
}
```

---

### WebSocket /v1/ws

Real-time live query subscriptions via WebSocket.

**Version**: v1  
**Path**: `/v1/ws`

#### Connection

**Protocol**: WebSocket (ws:// or wss://)  
**URL**: `ws://localhost:8080/v1/ws`

**Authentication**:
- Use HTTP Basic Auth encoded in the WebSocket URL query parameter, or
- Pass JWT token as a query parameter: `?token=<JWT_TOKEN>`

**Example**:
```javascript
// Using Basic Auth (encoded as base64)
const auth = btoa('alice:password123');
const ws = new WebSocket(`ws://localhost:8080/v1/ws?auth=${auth}`);

// Or using JWT token
const ws = new WebSocket(`ws://localhost:8080/v1/ws?token=${jwtToken}`);
```

#### Protocol

For detailed WebSocket protocol documentation including subscription messages, change notifications, and error handling, see:

**[WebSocket Protocol Documentation](WEBSOCKET_PROTOCOL.md)**

**Quick Example**:
```javascript
// Subscribe to live query
ws.send(JSON.stringify({
  action: 'subscribe',
  query_id: 'query-123',
  sql: 'SELECT * FROM app.messages WHERE user_id = $1',
  params: ['alice']
}));

// Receive change notifications
ws.onmessage = (event) => {
  const notification = JSON.parse(event.data);
  console.log('Change:', notification);
};

// Unsubscribe
ws.send(JSON.stringify({
  action: 'unsubscribe',
  query_id: 'query-123'
}));
```

---

## See Also

- [WebSocket Protocol](WEBSOCKET_PROTOCOL.md) - Real-time subscriptions
- [SQL Syntax Reference](SQL_SYNTAX.md) - Complete SQL command documentation
- [Quick Start Guide](../QUICK_START.md) - Getting started tutorial
