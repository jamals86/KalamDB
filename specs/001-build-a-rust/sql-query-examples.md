# SQL Query Examples

**Feature**: KalamDB SQL-First Query Interface  
**Date**: 2025-10-13  
**Version**: 2.0.0

## Overview

KalamDB uses SQL as the universal interface for all data operations. This document provides practical examples of common queries.

## Table Schemas

### userId.messages
```sql
msgId            BIGINT       -- Snowflake ID (primary key)
conversationId   VARCHAR      -- Conversation identifier
conversationType VARCHAR      -- 'ai' or 'group'
from             VARCHAR      -- Sender userId or 'ai'
timestamp        BIGINT       -- Unix timestamp (microseconds)
content          VARCHAR      -- Message text or preview
contentRef       VARCHAR      -- Optional reference to large content/media
metadata         VARCHAR      -- JSON metadata
```

### userId.conversations
```sql
conversationId   VARCHAR      -- Conversation identifier (primary key)
conversationType VARCHAR      -- 'ai' or 'group'
userId           VARCHAR      -- User ID (for AI conversations, NULL for group)
firstMsgId       BIGINT       -- First message ID
lastMsgId        BIGINT       -- Last message ID
created          BIGINT       -- Created timestamp (microseconds)
updated          BIGINT       -- Last updated timestamp
storagePath      VARCHAR      -- Storage location path
```

### userId.conversationUsers
```sql
userId           VARCHAR      -- User ID
conversationId   VARCHAR      -- Conversation identifier
role             VARCHAR      -- 'member', 'admin', 'owner'
created          BIGINT       -- Joined timestamp (microseconds)
updated          BIGINT       -- Last updated timestamp
metadata         VARCHAR      -- JSON metadata
```

---

## SELECT Queries (Read)

### Basic Message Queries

```sql
-- Get all messages in a conversation (most recent first)
SELECT * FROM userId.messages 
WHERE conversationId = 'conv_123' 
ORDER BY timestamp DESC 
LIMIT 100;

-- Get messages after a specific timestamp
SELECT * FROM userId.messages 
WHERE conversationId = 'conv_123' 
  AND timestamp > 1699000000 
ORDER BY timestamp ASC;

-- Get a specific message by ID
SELECT * FROM userId.messages 
WHERE msgId = 123456789;

-- Get messages from a specific sender
SELECT * FROM userId.messages 
WHERE conversationId = 'conv_123' 
  AND from = 'user_john';
```

### Full-Text Search

```sql
-- Search message content
SELECT * FROM userId.messages 
WHERE content LIKE '%search term%' 
LIMIT 50;

-- Search with time range
SELECT * FROM userId.messages 
WHERE content LIKE '%important%' 
  AND timestamp > 1699000000 
ORDER BY timestamp DESC;

-- Case-insensitive search (if supported by engine)
SELECT * FROM userId.messages 
WHERE LOWER(content) LIKE LOWER('%Search Term%');
```

### Aggregations

```sql
-- Count messages per conversation
SELECT conversationId, COUNT(*) as msgCount 
FROM userId.messages 
GROUP BY conversationId 
ORDER BY msgCount DESC;

-- Messages per day
SELECT DATE(timestamp / 1000000) as day, COUNT(*) as msgCount 
FROM userId.messages 
GROUP BY day 
ORDER BY day DESC;

-- Average messages per conversation
SELECT AVG(msgCount) as avgMessages 
FROM (
  SELECT conversationId, COUNT(*) as msgCount 
  FROM userId.messages 
  GROUP BY conversationId
);

-- Most active conversations (last 7 days)
SELECT conversationId, COUNT(*) as recentMsgCount 
FROM userId.messages 
WHERE timestamp > (UNIX_TIMESTAMP() * 1000000 - 7 * 24 * 60 * 60 * 1000000) 
GROUP BY conversationId 
ORDER BY recentMsgCount DESC 
LIMIT 10;
```

### Conversation Queries

```sql
-- List all conversations (most recent first)
SELECT * FROM userId.conversations 
ORDER BY updated DESC;

-- Get AI conversations only
SELECT * FROM userId.conversations 
WHERE conversationType = 'ai' 
ORDER BY updated DESC;

-- Get group conversations only
SELECT * FROM userId.conversations 
WHERE conversationType = 'group' 
ORDER BY updated DESC;

-- Get conversations with recent activity (last 24 hours)
SELECT * FROM userId.conversations 
WHERE updated > (UNIX_TIMESTAMP() * 1000000 - 24 * 60 * 60 * 1000000) 
ORDER BY updated DESC;

-- Find conversation by ID
SELECT * FROM userId.conversations 
WHERE conversationId = 'conv_123';
```

### Conversation User Queries

```sql
-- List all group conversations for user
SELECT * FROM userId.conversationUsers 
ORDER BY created DESC;

-- Get members of a specific conversation
SELECT * FROM userId.conversationUsers 
WHERE conversationId = 'conv_123';

-- Check if user is admin of conversation
SELECT * FROM userId.conversationUsers 
WHERE conversationId = 'conv_123' 
  AND userId = 'user_john' 
  AND role = 'admin';
```

### Complex Joins (if supported)

```sql
-- Get conversations with message counts
SELECT c.conversationId, c.conversationType, c.updated, COUNT(m.msgId) as msgCount 
FROM userId.conversations c 
LEFT JOIN userId.messages m ON c.conversationId = m.conversationId 
GROUP BY c.conversationId, c.conversationType, c.updated 
ORDER BY c.updated DESC;

-- Get group conversations with member info
SELECT c.conversationId, c.updated, COUNT(cu.userId) as memberCount 
FROM userId.conversations c 
LEFT JOIN userId.conversationUsers cu ON c.conversationId = cu.conversationId 
WHERE c.conversationType = 'group' 
GROUP BY c.conversationId, c.updated 
ORDER BY memberCount DESC;
```

---

## INSERT Queries (Create)

### Create Conversation

```sql
-- Create AI conversation
INSERT INTO userId.conversations 
(conversationId, conversationType, userId, firstMsgId, lastMsgId, created, updated, storagePath) 
VALUES 
('conv_new_ai', 'ai', 'user_john', 1, 1, 1699000000000000, 1699000000000000, 'user_john/');

-- Create group conversation
INSERT INTO userId.conversations 
(conversationId, conversationType, userId, firstMsgId, lastMsgId, created, updated, storagePath) 
VALUES 
('conv_new_group', 'group', NULL, 1, 1, 1699000000000000, 1699000000000000, 'shared/conversations/conv_new_group/');
```

### Insert Message

```sql
-- Insert text message
INSERT INTO userId.messages 
(msgId, conversationId, conversationType, from, timestamp, content, metadata) 
VALUES 
(123456789, 'conv_123', 'ai', 'user_john', 1699000000000000, 'Hello AI!', '{"role": "user"}');

-- Insert message with media reference
INSERT INTO userId.messages 
(msgId, conversationId, conversationType, from, timestamp, content, contentRef, metadata) 
VALUES 
(123456790, 'conv_456', 'group', 'user_alice', 1699000001000000, 'Check this image', 'shared/conversations/conv_456/media-789.jpg', '{"mediaType": "image/jpeg", "width": 1920, "height": 1080}');
```

### Add User to Group Conversation

```sql
-- Add member to group
INSERT INTO userId.conversationUsers 
(userId, conversationId, role, created) 
VALUES 
('user_bob', 'conv_group_123', 'member', 1699000000000000);

-- Add admin to group
INSERT INTO userId.conversationUsers 
(userId, conversationId, role, created) 
VALUES 
('user_alice', 'conv_group_123', 'admin', 1699000000000000);
```

---

## UPDATE Queries (Modify)

### Update Conversation

```sql
-- Update conversation metadata (after new message)
UPDATE userId.conversations 
SET lastMsgId = 999, updated = 1699999999000000 
WHERE conversationId = 'conv_123';

-- Update storage path (if moved)
UPDATE userId.conversations 
SET storagePath = 'new/path/' 
WHERE conversationId = 'conv_123';
```

### Update Conversation User Role

```sql
-- Promote user to admin
UPDATE userId.conversationUsers 
SET role = 'admin', updated = 1699999999000000 
WHERE conversationId = 'conv_123' 
  AND userId = 'user_bob';

-- Demote admin to member
UPDATE userId.conversationUsers 
SET role = 'member' 
WHERE conversationId = 'conv_123' 
  AND userId = 'user_alice';
```

### Update Message (Edit)

```sql
-- Edit message content
UPDATE userId.messages 
SET content = 'Updated message content', 
    metadata = '{"edited": true, "editedAt": 1699999999}' 
WHERE msgId = 123456789;
```

---

## DELETE Queries (Remove)

### Delete Single Message

```sql
-- Delete specific message (cascades to files if owned)
DELETE FROM userId.messages 
WHERE msgId = 123456789;

-- Delete messages older than 30 days
DELETE FROM userId.messages 
WHERE timestamp < (UNIX_TIMESTAMP() * 1000000 - 30 * 24 * 60 * 60 * 1000000);

-- Delete all messages from a sender
DELETE FROM userId.messages 
WHERE conversationId = 'conv_123' 
  AND from = 'user_spam';
```

### Delete Entire Conversation

```sql
-- Delete AI conversation (cascades to all messages + files)
DELETE FROM userId.conversations 
WHERE conversationId = 'conv_ai_123';

-- Delete group conversation (removes user's copy, checks if last participant)
DELETE FROM userId.conversations 
WHERE conversationId = 'conv_group_456';
```

### Remove User from Group

```sql
-- Remove user from conversation
DELETE FROM userId.conversationUsers 
WHERE conversationId = 'conv_123' 
  AND userId = 'user_bob';

-- Leave all conversations
DELETE FROM userId.conversationUsers 
WHERE userId = 'user_john';
```

---

## Advanced Queries

### Pagination

```sql
-- Page 1 (first 100 messages)
SELECT * FROM userId.messages 
WHERE conversationId = 'conv_123' 
ORDER BY timestamp DESC 
LIMIT 100 OFFSET 0;

-- Page 2 (next 100 messages)
SELECT * FROM userId.messages 
WHERE conversationId = 'conv_123' 
ORDER BY timestamp DESC 
LIMIT 100 OFFSET 100;

-- Cursor-based pagination (more efficient)
SELECT * FROM userId.messages 
WHERE conversationId = 'conv_123' 
  AND timestamp < 1699000000000000 
ORDER BY timestamp DESC 
LIMIT 100;
```

### Analytics

```sql
-- Most active hours
SELECT HOUR(timestamp / 1000000) as hour, COUNT(*) as msgCount 
FROM userId.messages 
GROUP BY hour 
ORDER BY msgCount DESC;

-- Message length distribution
SELECT 
  CASE 
    WHEN LENGTH(content) < 50 THEN 'short'
    WHEN LENGTH(content) < 200 THEN 'medium'
    ELSE 'long'
  END as msgLength,
  COUNT(*) as count
FROM userId.messages 
GROUP BY msgLength;

-- Conversations by type
SELECT conversationType, COUNT(*) as count 
FROM userId.conversations 
GROUP BY conversationType;
```

### Conditional Queries

```sql
-- Get conversations with no recent activity
SELECT * FROM userId.conversations 
WHERE updated < (UNIX_TIMESTAMP() * 1000000 - 7 * 24 * 60 * 60 * 1000000) 
ORDER BY updated ASC 
LIMIT 20;

-- Find conversations with large messages (contentRef present)
SELECT DISTINCT conversationId 
FROM userId.messages 
WHERE contentRef IS NOT NULL;

-- Get conversations with media files
SELECT DISTINCT conversationId 
FROM userId.messages 
WHERE contentRef LIKE '%.jpg' 
   OR contentRef LIKE '%.png' 
   OR contentRef LIKE '%.pdf';
```

---

## Admin Queries (Cross-User)

**Note**: Admin users with special permissions can query across all users.

```sql
-- Count total messages across all users
SELECT COUNT(*) as totalMessages 
FROM *.messages;

-- Most active users
SELECT from as userId, COUNT(*) as msgCount 
FROM *.messages 
GROUP BY from 
ORDER BY msgCount DESC 
LIMIT 10;

-- Storage usage by user
SELECT 
  SPLIT_PART(storagePath, '/', 1) as userId,
  COUNT(*) as conversationCount
FROM *.conversations 
GROUP BY userId 
ORDER BY conversationCount DESC;

-- System-wide conversation types
SELECT conversationType, COUNT(*) as count 
FROM *.conversations 
GROUP BY conversationType;
```

---

## REST API Usage

### Execute Query via REST

```bash
# Query messages
curl -X POST http://localhost:8080/api/v1/query \
  -H "Authorization: Bearer <jwt-token>" \
  -H "Content-Type: application/json" \
  -d '{
    "sql": "SELECT * FROM userId.messages WHERE conversationId = '\''conv_123'\'' LIMIT 10"
  }'

# Delete conversation
curl -X POST http://localhost:8080/api/v1/query \
  -H "Authorization: Bearer <jwt-token>" \
  -H "Content-Type: application/json" \
  -d '{
    "sql": "DELETE FROM userId.conversations WHERE conversationId = '\''conv_old'\''"
  }'

# Insert conversation
curl -X POST http://localhost:8080/api/v1/query \
  -H "Authorization: Bearer <jwt-token>" \
  -H "Content-Type: application/json" \
  -d '{
    "sql": "INSERT INTO userId.conversations (conversationId, conversationType, userId, firstMsgId, lastMsgId, created, updated, storagePath) VALUES ('\''conv_new'\'', '\''ai'\'', '\''user_john'\'', 1, 1, 1699000000000000, 1699000000000000, '\''user_john/'\'')"
  }'
```

---

## Best Practices

### Performance Optimization

1. **Use indexes**: Filter on `conversationId`, `timestamp`, `msgId`
2. **Limit result sets**: Always use `LIMIT` to avoid large result sets
3. **Avoid `SELECT *`**: Select only needed columns
4. **Use pagination**: Cursor-based pagination for large datasets
5. **Leverage caching**: Repeated queries are automatically cached

### Security

1. **Validate input**: All SQL queries are parsed and validated
2. **Scope enforcement**: Users can only query their own data (`userId.*`)
3. **Rate limiting**: Queries are rate-limited per user
4. **Prepared statements**: Use parameters to prevent SQL injection

### Query Patterns

```sql
-- Good: Specific conversation, limited results
SELECT * FROM userId.messages 
WHERE conversationId = 'conv_123' 
ORDER BY timestamp DESC 
LIMIT 100;

-- Bad: No filters, no limit (slow, large result set)
SELECT * FROM userId.messages;

-- Good: Efficient aggregation
SELECT conversationId, COUNT(*) 
FROM userId.messages 
GROUP BY conversationId;

-- Bad: Inefficient subquery
SELECT * FROM userId.messages 
WHERE msgId IN (SELECT msgId FROM userId.messages WHERE content LIKE '%term%');
```

---

## Summary

**Key Benefits of SQL-First Approach**:
- ✅ **Flexibility**: Express complex queries without specialized endpoints
- ✅ **Consistency**: Same interface for all operations (read, write, delete)
- ✅ **Performance**: Query optimizer handles RocksDB + Parquet merge
- ✅ **Developer Experience**: Use familiar SQL syntax
- ✅ **Admin Tools**: Standard SQL clients work with KalamDB

**Next Steps**:
1. Test queries in development environment
2. Monitor query performance metrics
3. Optimize indexes based on query patterns
4. Build UI query builder for common operations
