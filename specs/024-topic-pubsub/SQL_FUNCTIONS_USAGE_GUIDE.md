# SQL Functions Usage Guide for INSERT, UPDATE, DELETE

This document demonstrates how to use KalamDB SQL functions in INSERT, UPDATE, and DELETE statements.

## Functions Available

### Context Functions
- `CURRENT_USER()` - Returns the current username (String)
- `CURRENT_USER_ID()` - Returns the current user ID (String, restricted to admin roles)
- `CURRENT_ROLE()` - Returns the current user's role (String)

### ID Generation Functions
- `SNOWFLAKE_ID()` - Generates 64-bit time-ordered distributed IDs (BIGINT)
- `UUID_V7()` - Generates RFC 9562 compliant UUIDs (String)
- `ULID()` - Generates 26-character Crockford Base32 time-sortable IDs (String)

---

## INSERT Statements

### Using Context Functions

```sql
-- Insert record with current user information
INSERT INTO audit_log (user, timestamp, action)
VALUES (CURRENT_USER(), NOW(), 'login');

-- Insert with user role
INSERT INTO user_sessions (username, role, session_start)
VALUES (CURRENT_USER(), CURRENT_ROLE(), NOW());

-- Insert with restricted CURRENT_USER_ID (DBA-only)
INSERT INTO admin_actions (performed_by_user_id, action)
VALUES (CURRENT_USER_ID(), 'created_database');
```

### Using ID Generation Functions

```sql
-- Insert with auto-generated Snowflake ID (recommended for distributed systems)
INSERT INTO products (id, name, created_by)
VALUES (SNOWFLAKE_ID(), 'Widget', CURRENT_USER());

-- Insert with UUID V7 (sortable UUID with timestamp)
INSERT INTO transactions (transaction_id, description, creator)
VALUES (UUID_V7(), 'Payment processed', CURRENT_USER());

-- Insert with ULID (human-readable, sortable)
INSERT INTO events (event_id, event_type, triggered_by)
VALUES (ULID(), 'user_login', CURRENT_USER());
```

### Combining Context and ID Functions

```sql
-- Full example: Create audit record with all function types
INSERT INTO audit_records (
  audit_id,
  user_id,
  username,
  user_role,
  action_timestamp,
  reference_uuid
) VALUES (
  SNOWFLAKE_ID(),           -- Unique audit ID
  CURRENT_USER_ID(),        -- User who performed action (admin-only)
  CURRENT_USER(),           -- Username
  CURRENT_ROLE(),           -- User's role
  NOW(),                    -- Current timestamp
  UUID_V7()                 -- Correlation ID for tracing
);

-- Create event record with ULID
INSERT INTO activity_log (
  event_id,
  performer,
  role,
  activity_type,
  event_time
) VALUES (
  ULID(),                   -- Sortable event ID
  CURRENT_USER(),           -- Who did it
  CURRENT_ROLE(),           -- What role they had
  'document_creation',      -- What they did
  NOW()                     -- When
);
```

---

## UPDATE Statements

### Using Context Functions in SET Clause

```sql
-- Update modified_by and modification timestamp
UPDATE documents
SET modified_by = CURRENT_USER(),
    modified_at = NOW(),
    last_editor_role = CURRENT_ROLE()
WHERE document_id = 123;

-- Update with CURRENT_USER_ID (restricted to admin roles)
UPDATE system_config
SET last_modified_by_admin_id = CURRENT_USER_ID(),
    last_modification_time = NOW()
WHERE config_key = 'max_connections';
```

### Using ID Generation Functions in SET Clause

```sql
-- Update with version ID using Snowflake ID
UPDATE document_versions
SET new_snapshot_id = SNOWFLAKE_ID(),
    updated_by = CURRENT_USER()
WHERE document_id = 456;

-- Update with new UUID for document sync
UPDATE cloud_sync
SET sync_token = UUID_V7()
WHERE document_id = 789;
```

### Complex UPDATE with Multiple Functions

```sql
-- Update activity record when user views document
UPDATE document_views
SET last_viewed_by = CURRENT_USER(),
    viewer_role = CURRENT_ROLE(),
    view_count = view_count + 1,
    last_viewed_at = NOW(),
    view_session_id = ULID()
WHERE document_id = 999 AND user_id = (SELECT id FROM users WHERE name = CURRENT_USER());

-- Archive old data with tracking
UPDATE archived_records
SET archive_id = SNOWFLAKE_ID(),
    archived_by = CURRENT_USER(),
    archived_at = NOW(),
    archive_transaction_id = UUID_V7()
WHERE created_at < DATE_SUB(NOW(), INTERVAL 90 DAY);
```

---

## DELETE Statements

### Using Context Functions in WHERE Clause

```sql
-- Delete only your own drafts (user isolation)
DELETE FROM drafts
WHERE owner = CURRENT_USER()
  AND status = 'draft';

-- Delete admin-only records (role-based)
DELETE FROM temporary_admin_data
WHERE created_by = CURRENT_USER()
  AND created_by_role = CURRENT_ROLE()
  AND CURRENT_ROLE() = 'dba';
```

### Using ID Lookups in WHERE Clause

```sql
-- Delete specific records by generated ID
DELETE FROM temporary_sessions
WHERE session_id IN (
  SELECT session_id FROM session_log 
  WHERE username = CURRENT_USER() 
    AND created_at < DATE_SUB(NOW(), INTERVAL 1 HOUR)
);

-- Delete with CURRENT_USER_ID constraint (DBA-only)
DELETE FROM system_logs
WHERE created_by_user_id = CURRENT_USER_ID()
  AND log_level = 'DEBUG'
  AND created_at < DATE_SUB(NOW(), INTERVAL 7 DAY);
```

### Complex DELETE with Multiple Conditions

```sql
-- Delete user's own temporary data
DELETE FROM user_temp_data
WHERE owner = CURRENT_USER()
  AND (
    status = 'expired'
    OR created_at < DATE_SUB(NOW(), INTERVAL 24 HOUR)
  );

-- Delete admin-created test records
DELETE FROM test_data
WHERE created_by = CURRENT_USER()
  AND CURRENT_ROLE() IN ('dba', 'system')
  AND created_at < DATE_SUB(NOW(), INTERVAL 30 DAY);

-- Cascade delete with audit
DELETE FROM user_data
WHERE user_id IN (
  SELECT user_id FROM users 
  WHERE created_by = CURRENT_USER()
    AND CURRENT_ROLE() = 'dba'
    AND status = 'inactive'
)
AND created_at < DATE_SUB(NOW(), INTERVAL 1 YEAR);
```

---

## Best Practices

### 1. Security with Context Functions

```sql
-- ✅ GOOD: Use CURRENT_USER() for data isolation
INSERT INTO notes (owner, content) 
VALUES (CURRENT_USER(), 'My note');

-- ✅ GOOD: Use CURRENT_ROLE() for role-based access
UPDATE user_permissions 
SET modified_by = CURRENT_USER()
WHERE CURRENT_ROLE() = 'dba';

-- ⚠️ CAUTION: CURRENT_USER_ID() restricted to admin roles
-- This will FAIL for regular users:
DELETE FROM admin_logs WHERE admin_id = CURRENT_USER_ID();
```

### 2. Distributed System with ID Functions

```sql
-- ✅ GOOD: Use SNOWFLAKE_ID() for distributed unique IDs
INSERT INTO global_transactions (id, data)
VALUES (SNOWFLAKE_ID(), '...');

-- ✅ GOOD: Use UUID_V7() for sortable globally unique IDs
INSERT INTO event_stream (event_id, event_data)
VALUES (UUID_V7(), '...');

-- ✅ GOOD: Use ULID() for human-readable sortable IDs
INSERT INTO audit_trail (event_id, audit_data)
VALUES (ULID(), '...');
```

### 3. Audit Trail with Combined Functions

```sql
-- ✅ GOOD: Comprehensive audit log
INSERT INTO audit_log (
  event_id,           -- ULID for readability
  performer,          -- CURRENT_USER() for accountability
  performer_role,     -- CURRENT_ROLE() for context
  reference_id,       -- UUID_V7() for tracing
  event_timestamp
) VALUES (
  ULID(),
  CURRENT_USER(),
  CURRENT_ROLE(),
  UUID_V7(),
  NOW()
);
```

### 4. Soft Deletes with Audit

```sql
-- ✅ GOOD: Track who deleted and add deletion ID
UPDATE records
SET is_deleted = true,
    deleted_by = CURRENT_USER(),
    deleted_at = NOW(),
    deletion_event_id = ULID()
WHERE id = 123;
```

### 5. Multi-Tenant Isolation

```sql
-- ✅ GOOD: Use CURRENT_USER() for row-level security
DELETE FROM tenant_data
WHERE tenant_owner = CURRENT_USER()
  AND record_id = 456;
```

---

## Performance Considerations

### Context Functions
- **CURRENT_USER()**: Zero-cost lookup (already in session context)
- **CURRENT_USER_ID()**: Zero-cost lookup (already in session context)
- **CURRENT_ROLE()**: Zero-cost lookup (already in session context)

### ID Generation Functions
- **SNOWFLAKE_ID()**: Fast generation (~microseconds)
- **UUID_V7()**: Fast generation (~microseconds)
- **ULID()**: Fast generation (~microseconds)

All functions are safe to use in bulk INSERT/UPDATE operations without performance degradation.

---

## Error Handling

### CURRENT_USER() Errors
```sql
-- Error: CURRENT_USER() without username in session
SELECT CURRENT_USER();
-- Result: DataFusionError: "Username must not be null or empty"
```

### CURRENT_USER_ID() Errors
```sql
-- Error: Regular user trying to access CURRENT_USER_ID()
SELECT CURRENT_USER_ID();  -- User role
-- Result: DataFusionError: "can only be called by system, dba, or service roles"
```

---

## Test Coverage

Comprehensive tests are provided in:
- `backend/crates/kalamdb-core/tests/test_all_sql_functions.rs`
- `backend/crates/kalamdb-core/tests/test_context_functions.rs`

Tests cover:
- All three context functions (CURRENT_USER, CURRENT_USER_ID, CURRENT_ROLE)
- All three ID generation functions (SNOWFLAKE_ID, UUID_V7, ULID)
- Usage in SELECT, WHERE, CASE, subqueries
- Authorization checks for role-restricted functions
- Format validation for generated IDs
