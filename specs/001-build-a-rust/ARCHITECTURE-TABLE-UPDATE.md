# Table Architecture Update

**Date**: 2025-10-13  
**Feature**: Permission-Based Virtual Tables

## Summary

Updated KalamDB table architecture to use:
1. **Physical tables** with lowercase underscore naming (`conversations`, `conversation_users`)
2. **Virtual views** filtered by user permissions (`userId.conversations`, `userId.conversation_users`)
3. **Permission table** (`userId.perms`) controlling access

## Key Changes

### Naming Convention

All table and field names now use **lowercase with underscores**:

**Tables**:
- ❌ OLD: `ConversationUser`, `conversationUsers`
- ✅ NEW: `conversation_users`

**Fields**:
- ❌ OLD: `conversationId`, `userId`, `msgId`, `contentRef`
- ✅ NEW: `conversation_id`, `user_id`, `msg_id`, `content_ref`

### Table Architecture

#### Physical Tables (Actual Storage)

1. **`conversations`** - All conversations in the system
   ```sql
   conversation_id   VARCHAR PRIMARY KEY
   conversation_type VARCHAR  -- 'ai' or 'group'
   user_id          VARCHAR  -- For AI conversations, NULL for group
   first_msg_id     BIGINT
   last_msg_id      BIGINT   -- Hybrid: RocksDB (real-time) → S3 Parquet (fallback)
   created          BIGINT
   updated          BIGINT   -- Hybrid: RocksDB (real-time) → S3 Parquet (fallback)
   storage_path     VARCHAR
   total_messages   BIGINT   -- Virtual: counter from RocksDB + S3 counter file
   ```
   
   **Hybrid Fields Explained**:
   - `last_msg_id`: Check RocksDB first for real-time value, fall back to S3 Parquet if not cached
   - `updated`: Check RocksDB first for real-time value, fall back to S3 Parquet if not cached
   - `total_messages`: Virtual field combining RocksDB counter + S3 counter file (not stored in Parquet directly)

2. **`conversation_users`** - Junction table for user-conversation relationships
   ```sql
   user_id          VARCHAR PRIMARY KEY (composite)
   conversation_id  VARCHAR PRIMARY KEY (composite)
   role             VARCHAR  -- 'member', 'admin', 'owner'
   created          BIGINT
   updated          BIGINT
   metadata         VARCHAR  -- JSON
   ```

#### Virtual Tables (User-Scoped Views)

These are **NOT** physical tables but virtual views filtered by `userId.perms`:

1. **`userId.messages`** - User's messages in accessible conversations
   - Filtered by conversations in `userId.perms`
   - Returns messages user has permission to read

2. **`userId.conversations`** - Conversations user can access
   ```sql
   -- This is a virtual view equivalent to:
   SELECT * FROM conversations 
   WHERE conversation_id IN (
     SELECT conversation_id FROM perms WHERE user_id = 'userId'
   )
   ```

3. **`userId.conversation_users`** - Conversation memberships user can see
   ```sql
   -- This is a virtual view equivalent to:
   SELECT * FROM conversation_users 
   WHERE conversation_id IN (
     SELECT conversation_id FROM perms WHERE user_id = 'userId'
   )
   ```

4. **`userId.perms`** - User's permission/access control table
   ```sql
   user_id          VARCHAR PRIMARY KEY (composite)
   conversation_id  VARCHAR PRIMARY KEY (composite)
   permissions      VARCHAR  -- JSON array: ["read", "write", "delete", "admin"]
   granted_at       BIGINT
   granted_by       VARCHAR
   ```

### Permission Model

**Access Control Flow**:

```
User Query: SELECT * FROM userId.conversations
              ↓
SQL Engine checks: userId.perms table
              ↓
Filters: conversations WHERE conversation_id IN (permitted conversations)
              ↓
Returns: Only conversations user has access to
```

**Permission Types**:
- `"read"` - Can query messages and metadata
- `"write"` - Can insert messages
- `"delete"` - Can delete messages/conversations
- `"admin"` - Can modify permissions and add/remove users

**When Permissions Are Created**:
1. **AI Conversation**: Auto-created when conversation is created
   - User becomes owner with all permissions
   - Entry in `userId.perms`: `["read", "write", "delete", "admin"]`

2. **Group Conversation**: Created when user is added
   - User added to `conversation_users` table
   - Corresponding entry created in `userId.perms`
   - Permissions determined by role

## SQL Query Examples

### Create AI Conversation

```sql
-- Insert into physical table
INSERT INTO conversations 
(conversation_id, conversation_type, user_id, first_msg_id, last_msg_id, created, updated, storage_path) 
VALUES 
('conv_123', 'ai', 'user_john', 1, 1, 1699000000000000, 1699000000000000, 'user_john/');

-- Create permission (happens automatically in implementation)
INSERT INTO perms 
(user_id, conversation_id, permissions, granted_at, granted_by) 
VALUES 
('user_john', 'conv_123', '["read", "write", "delete", "admin"]', 1699000000000000, 'system');
```

### Create Group Conversation

```sql
-- Insert into physical table
INSERT INTO conversations 
(conversation_id, conversation_type, user_id, first_msg_id, last_msg_id, created, updated, storage_path) 
VALUES 
('conv_456', 'group', NULL, 1, 1, 1699000000000000, 1699000000000000, 'shared/conversations/conv_456/');

-- Add members
INSERT INTO conversation_users 
(user_id, conversation_id, role, created, updated) 
VALUES 
('user_alice', 'conv_456', 'owner', 1699000000000000, 1699000000000000),
('user_bob', 'conv_456', 'member', 1699000000000000, 1699000000000000);

-- Create permissions (happens automatically in implementation)
INSERT INTO perms 
(user_id, conversation_id, permissions, granted_at, granted_by) 
VALUES 
('user_alice', 'conv_456', '["read", "write", "delete", "admin"]', 1699000000000000, 'system'),
('user_bob', 'conv_456', '["read", "write"]', 1699000000000000, 'user_alice');
```

### Query User's Conversations

```sql
-- User queries their virtual view
SELECT * FROM userId.conversations 
ORDER BY updated DESC;

-- Engine internally executes:
SELECT * FROM conversations 
WHERE conversation_id IN (
  SELECT conversation_id FROM perms WHERE user_id = 'userId'
) 
ORDER BY updated DESC;
```

### Query Conversation Members

```sql
-- User queries their virtual view
SELECT * FROM userId.conversation_users 
WHERE conversation_id = 'conv_456';

-- Engine internally executes:
SELECT * FROM conversation_users 
WHERE conversation_id = 'conv_456'
  AND conversation_id IN (
    SELECT conversation_id FROM perms WHERE user_id = 'userId'
  );
```

### Remove User from Group

```sql
-- Remove from conversation_users
DELETE FROM conversation_users 
WHERE conversation_id = 'conv_456' 
  AND user_id = 'user_bob';

-- Revoke permissions (happens automatically in implementation)
DELETE FROM perms 
WHERE user_id = 'user_bob' 
  AND conversation_id = 'conv_456';
```

## Implementation Notes

### SQL Engine Responsibilities

1. **Parse SQL queries**: Identify table names (physical vs virtual)
2. **Enforce scope**: Rewrite `userId.*` queries to filter by `perms` table
3. **Validate permissions**: Check `permissions` array for operation type
4. **Optimize queries**: Cache permission lookups, use indexes
5. **Resolve hybrid fields**: For `last_msg_id`, `updated`, `total_messages`, check RocksDB first, fall back to S3

### RocksDB Key Design

```rust
// Physical tables
conversations:{conversation_id}
conversation_users:{conversation_id}:{user_id}
perms:{user_id}:{conversation_id}

// Indexes for fast lookups
user_convs:{user_id}:{conversation_id}      // User's conversations
user_perms:{user_id}:{conversation_id}      // User's permissions (fast filter)

// Real-time conversation metadata (hybrid fields)
conv_meta:{conversation_id}:last_msg_id     // Real-time last message ID
conv_meta:{conversation_id}:updated         // Real-time last updated timestamp
conv_meta:{conversation_id}:msg_count       // Real-time message counter (partial)
```

### S3/Filesystem Storage

```
# Conversation metadata files (persistent storage)
{storage_path}/conversations/{conversation_id}.parquet  # Base conversation metadata
{storage_path}/conversations/{conversation_id}_counter.txt  # Total message count

# Example for AI conversation
user_john/conversations/conv_123.parquet
user_john/conversations/conv_123_counter.txt

# Example for Group conversation  
shared/conversations/conv_456/metadata.parquet
shared/conversations/conv_456/counter.txt
```

### Security Benefits

1. ✅ **No direct table access**: Users can only query via virtual views
2. ✅ **Automatic filtering**: SQL engine enforces permissions
3. ✅ **Audit trail**: `granted_by` tracks who granted access
4. ✅ **Granular control**: Per-conversation, per-user permissions
5. ✅ **Scalable**: Permissions cached in RocksDB for fast lookups

## Migration from Previous Design

### What Changed

1. **Table names**: `conversationUsers` → `conversation_users`
2. **Field names**: `conversationId` → `conversation_id`, etc.
3. **Architecture**: Added `perms` table and virtual view concept
4. **Queries**: `userId.conversations` is now a filtered view, not a partition

### What Stayed the Same

1. **Storage layout**: Messages still duplicated to each user's storage
2. **Conversation types**: AI vs Group distinction unchanged
3. **Large content optimization**: Still stored in shared folders
4. **SQL-first API**: Single `/api/v1/query` endpoint

### Backward Compatibility

**Breaking Changes**:
- ❌ Field names changed (camelCase → snake_case)
- ❌ `userId.conversations` semantics changed (partition → filtered view)

**Migration Required**:
- Update all SQL queries to use lowercase underscore field names
- Update client code to use new field names in results
- No data migration needed (rename fields during Parquet read)

## Hybrid Field Resolution

### How `last_msg_id` Works

**Query Flow**:
```
SELECT last_msg_id FROM userId.conversations WHERE conversation_id = 'conv_123'
    ↓
1. Check RocksDB: conv_meta:conv_123:last_msg_id
    ↓
2a. If found → return real-time value (fast path, <1ms)
    ↓
2b. If NOT found → read from S3 Parquet: conversations/conv_123.parquet
    ↓
3. Return value from Parquet (slower, 50-200ms)
```

**Write Flow** (when new message arrives):
```
INSERT INTO userId.messages (msg_id, conversation_id, ...) VALUES (999, 'conv_123', ...)
    ↓
1. Write message to RocksDB + buffer
    ↓
2. Update RocksDB: conv_meta:conv_123:last_msg_id = 999
    ↓
3. Update RocksDB: conv_meta:conv_123:updated = NOW()
    ↓
4. Increment RocksDB: conv_meta:conv_123:msg_count++
    ↓
5. Background: Consolidate to Parquet (every 5 min or 10K messages)
    ↓
6. Update S3: conversations/conv_123.parquet (last_msg_id, updated)
    ↓
7. Update S3: conversations/conv_123_counter.txt (total from RocksDB + existing S3)
```

### How `updated` Works

**Same pattern as `last_msg_id`**:
- **Real-time**: `conv_meta:{conversation_id}:updated` in RocksDB
- **Persistent**: `updated` field in S3 Parquet file
- **Resolution**: Check RocksDB first, fall back to S3 if not cached

**Use Cases**:
- Sorting conversations by most recent activity
- Filtering conversations updated in last 24 hours
- Showing "last active" timestamp in UI

### How `total_messages` Works (Virtual Field)

**Composition**: `total_messages = S3_counter + RocksDB_counter`

**Storage**:
```
# S3/Filesystem (persistent baseline)
{storage_path}/conversations/{conversation_id}_counter.txt
Content: 1000000  # Total messages up to last consolidation

# RocksDB (incremental since last consolidation)
conv_meta:{conversation_id}:msg_count
Value: 42  # Messages since last Parquet write
```

**Query Flow**:
```sql
SELECT conversation_id, total_messages FROM userId.conversations
    ↓
1. Read S3 counter file: conversations/conv_123_counter.txt → 1000000
    ↓
2. Read RocksDB counter: conv_meta:conv_123:msg_count → 42
    ↓
3. Return: total_messages = 1000000 + 42 = 1000042
```

**Write Flow**:
```
New message arrives
    ↓
1. Increment RocksDB: conv_meta:conv_123:msg_count++
    ↓
2. Background consolidation (every 5 min):
   a. Read current S3 counter: 1000000
   b. Read RocksDB counter: 42
   c. Write new S3 counter: 1000042
   d. Reset RocksDB counter: 0
```

**Benefits**:
- ✅ Always accurate (combines persistent + real-time)
- ✅ Fast queries (RocksDB counter is small integer)
- ✅ No Parquet scan needed (separate counter file)
- ✅ Efficient consolidation (single file update)

### Performance Characteristics

| Field | RocksDB Lookup | S3 Fallback | Typical Case |
|-------|---------------|-------------|--------------|
| `last_msg_id` | <1ms | 50-200ms | RocksDB hit (99%+) |
| `updated` | <1ms | 50-200ms | RocksDB hit (99%+) |
| `total_messages` | <1ms (counter) | 5-10ms (read txt file) | Combined (always) |

**Cache Strategy**:
- RocksDB keeps hot conversation metadata in memory
- S3 counters cached in RocksDB after first read
- Stale RocksDB data cleaned up after 24 hours of inactivity

## Benefits

1. **Clearer Semantics**: Virtual views make permission model explicit
2. **Standard SQL**: Lowercase underscore naming matches SQL conventions
3. **Flexible Access Control**: Easy to grant/revoke access per conversation
4. **Auditable**: Track who granted access and when
5. **Performant**: Permissions cached in RocksDB, minimal overhead
6. **Real-time Accuracy**: Hybrid fields always reflect latest state from RocksDB
7. **Cost-Efficient**: Counter files avoid expensive Parquet scans for message counts

## Next Steps

1. Implement `perms` table in RocksDB
2. Build SQL query rewriter for virtual views
3. Update Parquet schema with lowercase field names
4. Add permission checks to write operations
5. Create admin endpoints for permission management
