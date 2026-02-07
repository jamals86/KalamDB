# Practical SQL Functions Examples

## Real-World Usage Patterns

This document provides practical examples of using SQL functions in real database operations.

## 1. Audit Logging Pattern

### Create Audit Table
```sql
CREATE TABLE audit_log (
  id BIGINT PRIMARY KEY,              -- SNOWFLAKE_ID()
  trace_id STRING,                    -- UUID_V7() for distributed tracing
  event_id STRING,                    -- ULID() for human-readability
  username STRING,                    -- CURRENT_USER()
  user_role STRING,                   -- CURRENT_ROLE()
  action STRING,
  resource_type STRING,
  resource_id STRING,
  old_values STRING,                  -- JSON
  new_values STRING,                  -- JSON
  ip_address STRING,
  timestamp BIGINT,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP()
);
```

### Insert Audit Record
```sql
-- Full audit trail with context
INSERT INTO audit_log (
  id,
  trace_id,
  event_id,
  username,
  user_role,
  action,
  resource_type,
  resource_id,
  old_values,
  new_values,
  ip_address,
  timestamp
)
VALUES (
  SNOWFLAKE_ID(),                     -- Distributed ID, auto-ordered by time
  UUID_V7(),                          -- Trace ID for request tracking
  ULID(),                             -- Human-readable event ID
  CURRENT_USER(),                     -- Username of who made change
  CURRENT_ROLE(),                     -- Role context (user/dba/system)
  'UPDATE',
  'users',
  'user_123',
  '{"status": "active"}',
  '{"status": "inactive"}',
  '192.168.1.100',
  UNIX_TIMESTAMP(NOW())
);
```

### Query Audit Trail
```sql
-- Find all actions by current user
SELECT event_id, action, resource_type, created_at
FROM audit_log
WHERE username = CURRENT_USER()
ORDER BY created_at DESC
LIMIT 100;

-- Find all admin actions (role-based filtering)
SELECT event_id, username, action, created_at
FROM audit_log
WHERE user_role = CURRENT_ROLE()
  AND user_role IN ('dba', 'system')
ORDER BY created_at DESC;
```

## 2. Session Tracking Pattern

### Create Session Table
```sql
CREATE TABLE user_sessions (
  session_id STRING PRIMARY KEY,      -- ULID() for session ID
  server_request_id STRING,           -- SNOWFLAKE_ID() on server side
  trace_id STRING,                    -- UUID_V7() for tracing
  username STRING NOT NULL,
  user_role STRING NOT NULL,
  ip_address STRING,
  user_agent STRING,
  started_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP(),
  last_activity TIMESTAMP DEFAULT CURRENT_TIMESTAMP(),
  expires_at TIMESTAMP,
  is_active BOOLEAN DEFAULT true
);
```

### Track Session Start
```sql
-- When user logs in, create session record
INSERT INTO user_sessions (
  session_id,
  server_request_id,
  trace_id,
  username,
  user_role,
  ip_address,
  user_agent,
  expires_at
)
VALUES (
  ULID(),                             -- Session token (user-facing ID)
  SNOWFLAKE_ID(),                     -- Server-side request ID
  UUID_V7(),                          -- Trace ID for support/debugging
  CURRENT_USER(),
  CURRENT_ROLE(),
  '192.168.1.50',
  'Mozilla/5.0...',
  DATE_ADD(NOW(), INTERVAL 24 HOUR)
);
```

### Update Last Activity
```sql
-- Update session on each request
UPDATE user_sessions
SET 
  last_activity = CURRENT_TIMESTAMP(),
  server_request_id = SNOWFLAKE_ID()  -- Track latest request
WHERE 
  username = CURRENT_USER()
  AND session_id = ? 
  AND is_active = true
  AND NOW() <= expires_at;
```

### Clean Up Expired Sessions
```sql
-- Delete expired sessions
DELETE FROM user_sessions
WHERE expires_at < NOW()
  OR (is_active = true AND last_activity < DATE_SUB(NOW(), INTERVAL 1 DAY));
```

## 3. Activity Stream Pattern

### Create Activity Table
```sql
CREATE TABLE user_activity (
  activity_id STRING PRIMARY KEY,        -- ULID() for readability
  distributed_id BIGINT,                 -- SNOWFLAKE_ID()
  request_trace_id STRING UNIQUE,        -- UUID_V7() for request tracing
  user_id STRING NOT NULL,
  username STRING NOT NULL,
  action_type STRING,                    -- 'CREATE', 'UPDATE', 'DELETE', 'READ'
  entity_type STRING,                    -- 'document', 'table', 'user'
  entity_id STRING,
  description STRING,
  status STRING DEFAULT 'completed',     -- 'pending', 'completed', 'failed'
  error_message STRING,
  metadata STRING,                       -- JSON
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP()
);
```

### Log Activity with User Context
```sql
-- Log user action with full context
INSERT INTO user_activity (
  activity_id,
  distributed_id,
  request_trace_id,
  user_id,
  username,
  action_type,
  entity_type,
  entity_id,
  description,
  metadata
)
VALUES (
  ULID(),                                -- Activity ID (24h friendly lookup)
  SNOWFLAKE_ID(),                        -- Distributed ID (ordering)
  UUID_V7(),                             -- Request trace ID (debugging)
  CURRENT_USER_ID(),                     -- User ID (admin-only info)
  CURRENT_USER(),                        -- Username (visible to all)
  'CREATE',
  'document',
  doc_id,
  CONCAT(CURRENT_USER(), ' created document'),
  CONCAT(
    '{"role": "', CURRENT_ROLE(), 
    '", "timestamp": "', NOW(), 
    '"}'
  )
);
```

### Query Activity by User
```sql
-- User sees their own activity
SELECT activity_id, action_type, entity_type, created_at
FROM user_activity
WHERE username = CURRENT_USER()
ORDER BY created_at DESC;

-- Admins see activity filtered by role
SELECT activity_id, username, action_type, entity_type, created_at
FROM user_activity
WHERE created_at > DATE_SUB(NOW(), INTERVAL 7 DAY)
  AND (
    CURRENT_ROLE() = 'system'  -- System sees everything
    OR CURRENT_ROLE() = 'dba'   -- DBA sees non-system activity
  )
ORDER BY created_at DESC;
```

## 4. Document Management Pattern

### Create Documents Table
```sql
CREATE TABLE documents (
  id BIGINT PRIMARY KEY,                 -- SNOWFLAKE_ID()
  public_id STRING UNIQUE,               -- ULID() for sharing
  trace_id STRING,                       -- UUID_V7() for request tracking
  owner_username STRING NOT NULL,
  owner_user_id STRING,                  -- CURRENT_USER_ID() from audit
  creator_username STRING,
  title STRING NOT NULL,
  content STRING,
  version INT DEFAULT 1,
  status STRING DEFAULT 'active',
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP(),
  updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP(),
  deleted_at TIMESTAMP,
  is_public BOOLEAN DEFAULT false
);
```

### Create Document
```sql
-- Create new document with tracking IDs
INSERT INTO documents (
  id,
  public_id,
  trace_id,
  owner_username,
  owner_user_id,
  creator_username,
  title,
  content
)
VALUES (
  SNOWFLAKE_ID(),                        -- Auto-ordered by timestamp
  ULID(),                                -- User-friendly ID for sharing
  UUID_V7(),                             -- Request tracking ID
  CURRENT_USER(),
  CURRENT_USER_ID(),                     -- Will fail for non-admin, but captured
  CURRENT_USER(),
  ?,
  ?
);
```

### Soft Delete Pattern
```sql
-- Soft delete (user's own documents only)
UPDATE documents
SET 
  deleted_at = CURRENT_TIMESTAMP(),
  status = 'deleted'
WHERE 
  id = ?
  AND owner_username = CURRENT_USER();

-- Physical delete (admin only - role checked in app layer)
DELETE FROM documents
WHERE deleted_at IS NOT NULL
  AND deleted_at < DATE_SUB(NOW(), INTERVAL 30 DAY);
```

### Find User's Documents
```sql
-- User sees only their documents
SELECT id, public_id, title, status, updated_at
FROM documents
WHERE owner_username = CURRENT_USER()
  AND deleted_at IS NULL
ORDER BY updated_at DESC;

-- Admin sees all documents
SELECT id, public_id, owner_username, title, status, updated_at
FROM documents
WHERE deleted_at IS NULL
  AND (
    CURRENT_ROLE() = 'dba'
    OR CURRENT_ROLE() = 'system'
  )
ORDER BY updated_at DESC;
```

## 5. Event Stream Pattern

### Create Events Table
```sql
CREATE TABLE events (
  id BIGINT PRIMARY KEY,                 -- SNOWFLAKE_ID() for ordering
  event_id STRING UNIQUE,                -- ULID() for external reference
  correlation_id STRING,                 -- UUID_V7() for request correlation
  event_type STRING NOT NULL,
  source_system STRING,
  initiator_username STRING,             -- CURRENT_USER()
  initiator_role STRING,                 -- CURRENT_ROLE()
  payload STRING,                        -- JSON
  version STRING DEFAULT '1.0',
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP(),
  processed_at TIMESTAMP
);
```

### Emit Event
```sql
-- Emit event with full tracking context
INSERT INTO events (
  id,
  event_id,
  correlation_id,
  event_type,
  source_system,
  initiator_username,
  initiator_role,
  payload
)
VALUES (
  SNOWFLAKE_ID(),                        -- Globally ordered ID
  ULID(),                                -- External event reference
  UUID_V7(),                             -- Correlation for distributed tracing
  'user.updated',
  'core-api',
  CURRENT_USER(),
  CURRENT_ROLE(),
  CONCAT(
    '{"timestamp": "', NOW(), 
    '", "changes": {"status": "active"}, ',
    '"user": "', CURRENT_USER(), 
    '"}'
  )
);
```

## 6. Error Handling Examples

### Handling CURRENT_USER_ID Errors
```sql
-- This will fail if user has 'user' role
-- Error: "Unauthorized: CURRENT_USER_ID() requires system, dba, or service role"
INSERT INTO admin_log (user_id)
SELECT CURRENT_USER_ID() AS user_id;

-- Solution: Check role first in application code
-- Or use CASE statement to handle
SELECT 
  CASE 
    WHEN CURRENT_ROLE() IN ('system', 'dba', 'service') 
    THEN CURRENT_USER_ID()
    ELSE 'unauthorized'
  END AS user_id,
  CURRENT_USER() AS username;
```

### Handling Missing Username
```sql
-- This will fail if username is not set in session
-- Error: "CURRENT_USER() requires username to be set in session"
SELECT CURRENT_USER();

-- Check with NULL coalesce
SELECT COALESCE(CURRENT_USER(), 'anonymous') AS username;
```

## 7. Analytics Pattern

### Create Analytics Events Table
```sql
CREATE TABLE analytics_events (
  id BIGINT PRIMARY KEY,                 -- SNOWFLAKE_ID()
  event_id STRING UNIQUE,                -- ULID() for storage partitioning
  session_trace_id STRING,               -- UUID_V7() from session
  username STRING,                       -- CURRENT_USER() context
  user_role STRING,                      -- CURRENT_ROLE() context
  event_name STRING,
  event_properties STRING,               -- JSON
  timestamp BIGINT
);
```

### Track Analytics Event
```sql
-- Track user action with all context
INSERT INTO analytics_events (
  id,
  event_id,
  session_trace_id,
  username,
  user_role,
  event_name,
  event_properties,
  timestamp
)
VALUES (
  SNOWFLAKE_ID(),                        -- For time-ordered queries
  ULID(),                                -- For partition key (first 6 chars = time)
  UUID_V7(),                             -- Trace back to session request
  CURRENT_USER(),
  CURRENT_ROLE(),
  'page_view',
  '{"page": "/dashboard", "referrer": "/home"}',
  UNIX_TIMESTAMP(NOW())
);
```

## 8. Multi-Tenant Isolation Pattern

### Create Tenant Data Table
```sql
CREATE TABLE tenant_data (
  id BIGINT PRIMARY KEY,                 -- SNOWFLAKE_ID()
  tenant_id STRING,
  data_key STRING UNIQUE,                -- ULID() per row for sharing
  owner_username STRING NOT NULL,        -- CURRENT_USER()
  owner_role STRING,                     -- CURRENT_ROLE()
  trace_id STRING,                       -- UUID_V7() for debugging
  content STRING,
  created_at TIMESTAMP,
  updated_at TIMESTAMP
);
```

### Create Tenant Record
```sql
-- Create data scoped to tenant and user
INSERT INTO tenant_data (
  id,
  tenant_id,
  data_key,
  owner_username,
  owner_role,
  trace_id,
  content
)
VALUES (
  SNOWFLAKE_ID(),
  'tenant_' || ? || '_' || ULID(),       -- Multi-part key
  ULID(),
  CURRENT_USER(),
  CURRENT_ROLE(),
  UUID_V7(),
  ?
);
```

### Query User's Tenant Data
```sql
-- User sees only their tenant data
SELECT id, data_key, content, updated_at
FROM tenant_data
WHERE 
  owner_username = CURRENT_USER()
  AND tenant_id LIKE 'tenant_%'
ORDER BY updated_at DESC;
```

## 9. Bulk Operations with Functions

### Bulk Insert with ID Generation
```sql
-- Insert multiple records, each with unique ID
INSERT INTO events (id, event_id, username, action)
SELECT 
  SNOWFLAKE_ID(),
  ULID(),
  CURRENT_USER(),
  action_type
FROM staging_events
WHERE processed = false;
```

### Bulk Update with Timestamp IDs
```sql
-- Update multiple records, tracking with IDs
UPDATE documents
SET 
  trace_id = UUID_V7(),
  updated_at = NOW(),
  updater_username = CURRENT_USER()
WHERE 
  owner_username = CURRENT_USER()
  AND status = 'draft'
  AND updated_at < DATE_SUB(NOW(), INTERVAL 1 DAY);
```

## Performance Considerations

| Function | Cost | Notes |
|----------|------|-------|
| `CURRENT_USER()` | ~0μs | Simple string lookup from context |
| `CURRENT_USER_ID()` | ~0μs | Simple lookup + role check |
| `CURRENT_ROLE()` | ~0μs | Simple enum-to-string conversion |
| `SNOWFLAKE_ID()` | ~100ns | Atomic counter increment |
| `UUID_V7()` | ~500ns | Cryptographic timestamp |
| `ULID()` | ~200ns | Random component generation |

All functions are suitable for use in:
- High-frequency INSERT operations
- Bulk operations (UNION ALL, SELECT INSERT)
- WHERE clause filtering
- UPDATE SET clauses
- CASE expressions

## Best Practices

1. **Use SNOWFLAKE_ID() for:**
   - Primary keys in high-volume tables
   - Request IDs for tracing
   - Ordering-critical operations

2. **Use ULID() for:**
   - User-facing IDs (sharing, URLs)
   - Partition keys (first 6 chars = timestamp)
   - Log entries

3. **Use UUID_V7() for:**
   - Distributed tracing (correlation IDs)
   - Request tracking across services
   - External integrations

4. **Use CURRENT_USER() for:**
   - Audit trails (who made the change)
   - Data ownership (owner_username)
   - User-facing logs

5. **Use CURRENT_USER_ID() for:**
   - Admin logs (when role must be tracked)
   - Authorization enforcement
   - DBA/system operations only

6. **Use CURRENT_ROLE() for:**
   - Role-based data filtering
   - Audit logging (who with what role)
   - Feature access control
