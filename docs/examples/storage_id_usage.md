# Storage ID Usage Examples

This guide demonstrates how to use `storage_id` when creating tables in KalamDB.

## Prerequisites

First, create storage locations using `CREATE STORAGE`:

```sql
-- Create a local filesystem storage
CREATE STORAGE local
    TYPE filesystem
    NAME 'Local Storage'
    DESCRIPTION 'Default local storage'
    BASE_DIRECTORY './data/storage'
    SHARED_TABLES_TEMPLATE '{namespace}/shared/{tableName}/'
    USER_TABLES_TEMPLATE '{namespace}/users/{tableName}/{shard}/{userId}/';

-- Create an S3 storage
CREATE STORAGE s3_prod
    TYPE s3
    NAME 'Production S3 Storage'
    DESCRIPTION 'Production S3 bucket'
    BASE_DIRECTORY 's3://my-bucket/kalamdb/'
    SHARED_TABLES_TEMPLATE '{namespace}/shared/{tableName}/'
    USER_TABLES_TEMPLATE '{namespace}/users/{tableName}/{shard}/{userId}/';
```

## Creating Shared Tables with Storage ID

### Using Default Storage (local)
```sql
-- Implicitly uses 'local' storage
CREATE TABLE config (
    key TEXT NOT NULL,
    value TEXT
);
```

### Specifying Storage Explicitly
```sql
-- Use specific storage
CREATE TABLE config (
    key TEXT NOT NULL,
    value TEXT
) STORAGE s3_prod;
```

### With Other Options
```sql
-- Storage with flush policy
CREATE TABLE events (
    event_id BIGINT AUTO_INCREMENT,
    event_type TEXT,
    timestamp TIMESTAMP
) STORAGE s3_prod
  FLUSH ROWS 10000;
```

## Creating User Tables with Storage ID

### Using Default Storage (local)
```sql
-- Implicitly uses 'local' storage
CREATE USER TABLE messages (
    id BIGINT AUTO_INCREMENT,
    content TEXT,
    created_at TIMESTAMP
);
```

### Specifying Storage Explicitly
```sql
-- Use specific storage
CREATE USER TABLE messages (
    id BIGINT AUTO_INCREMENT,
    content TEXT,
    created_at TIMESTAMP
) STORAGE s3_prod;
```

### With Flush Policy
```sql
-- Storage with flush policy
CREATE USER TABLE activity_log (
    id BIGINT AUTO_INCREMENT,
    action TEXT,
    timestamp TIMESTAMP
) STORAGE s3_prod
  FLUSH ROWS 5000;
```

## Verifying Storage Configuration

Check which storage a table is using:

```sql
SELECT table_name, storage_id, table_type
FROM system.tables
WHERE namespace = 'your_namespace';
```

Expected output:
```
+-------------+------------+------------+
| table_name  | storage_id | table_type |
+-------------+------------+------------+
| config      | s3_prod    | shared     |
| messages    | s3_prod    | user       |
| activity_log| local      | user       |
+-------------+------------+------------+
```

## Error Handling

### Storage Not Found
```sql
CREATE TABLE config (key TEXT) STORAGE nonexistent;
```

Error:
```
Not found: Storage 'nonexistent' does not exist. Create it first with CREATE STORAGE.
```

### Resolution
First create the storage, then create the table:

```sql
-- 1. Create the storage
CREATE STORAGE my_storage
    TYPE filesystem
    NAME 'My Storage'
    BASE_DIRECTORY '/path/to/storage'
    SHARED_TABLES_TEMPLATE '{namespace}/shared/{tableName}/'
    USER_TABLES_TEMPLATE '{namespace}/users/{tableName}/{userId}/';

-- 2. Create the table
CREATE TABLE config (key TEXT) STORAGE my_storage;
```

## Best Practices

1. **Always create default 'local' storage on system initialization**
   - This ensures tables without explicit storage work correctly

2. **Use meaningful storage names**
   - `s3_prod`, `s3_dev`, `local_ssd`, etc.

3. **Document storage usage in your application**
   - Keep track of which tables use which storage

4. **Validate storage exists before table creation**
   - The system will error if storage doesn't exist

5. **Consider storage costs and performance**
   - S3: Cost-effective, slightly higher latency
   - Local filesystem: Fast, limited by disk space

## Migration Example

Moving from default to explicit storage:

```sql
-- Old way (implicit local)
CREATE TABLE metrics (id BIGINT, value DOUBLE);

-- New way (explicit)
CREATE TABLE metrics (id BIGINT, value DOUBLE) STORAGE local;

-- Or use a different storage
CREATE TABLE metrics (id BIGINT, value DOUBLE) STORAGE s3_prod;
```

Both approaches work, but explicit is clearer and allows for easier future changes.
