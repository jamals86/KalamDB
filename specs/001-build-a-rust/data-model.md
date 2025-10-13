# Data Model

**Feature**: Chat and AI Message History Storage System  
**Date**: 2025-10-13  
**Status**: Phase 1 Design

## Overview

KalamDb's data model is designed for efficient storage, querying, and streaming of chat and AI message history. The model enforces user-centric data ownership with isolation at the storage layer, supports flexible metadata, and optimizes for both real-time writes and historical queries.

### Conversation Types & Storage Strategy

KalamDB supports two distinct conversation types with different storage strategies:

#### 1. AI Conversations (User ↔ AI)
**Definition**: Single user conversing with AI assistant(s)

**Storage Strategy**: User-owned storage only
- Messages stored ONLY in user's partition: `{userId}/batch-*.parquet`
- No duplication (only one participant: the user)
- AI responses stored alongside user messages
- Complete conversation history in user's storage
- Easy to export, backup, or delete with user data

**Example**:
```
userA talks to AI assistant
→ Messages stored in: userA/batch-*.parquet only
→ No shared storage needed
```

**Benefits**:
- ✅ Complete user ownership of AI conversation history
- ✅ No cross-user dependencies
- ✅ Minimal storage overhead (single copy)
- ✅ Fast queries (single partition)

#### 2. Group Conversations (User ↔ User)
**Definition**: Two or more human users conversing together

**Storage Strategy**: Shared conversation storage
- Messages stored ONCE in shared location: `shared/conversations/{conversationId}/`
- Each participant maintains lightweight index/reference in their partition
- All messages for the conversation co-located
- Efficient for large groups (no duplication)

**Example**:
```
userA, userB, userC in group conversation
→ Messages stored in: shared/conversations/convABC/batch-*.parquet
→ Each user has reference: userA/conversation-refs/{convABC}.json
                           userB/conversation-refs/{convABC}.json
                           userC/conversation-refs/{convABC}.json
```

**Benefits**:
- ✅ No duplication overhead (single copy for all participants)
- ✅ Efficient for large groups (100+ participants)
- ✅ Shared media files naturally co-located
- ✅ Conversation-level operations (archive, export) are simple

**Large Content & Media File Optimization**:
- When `content.length > large_message_threshold` (configurable in `config.toml`, e.g., 100KB)
- Store actual content in conversation-specific location: `shared/conversations/{conversationId}/msg-{msgId}.bin`
- Support for media files (images, documents, audio, video, etc.) alongside text messages
- Store media in conversation location: `shared/conversations/{conversationId}/media-{msgId}.{ext}`
- Replace `content` field with reference: `{"type": "ref", "uri": "s3://bucket/shared/conversations/convA/msg-123.bin"}`
- `contentRef` can point to text content OR media files (image.jpg, document.pdf, audio.mp3, etc.)

---

## Core Entities

### 1. Message

Represents a single message in a conversation (chat message, AI response, system notification).

**Storage** (varies by conversation type):

**AI Conversations**:
- **RocksDB**: Key = `{userId}:{msgId}`, Value = MessageProto
- **Parquet**: `{userId}/batch-<timestamp>-<index>.parquet`
- **No duplication**: Single user owns all messages

**Group Conversations**:
- **RocksDB**: Key = `conv:{conversationId}:{msgId}`, Value = MessageProto
- **Parquet**: `shared/conversations/{conversationId}/batch-<timestamp>-<index>.parquet`
- **Shared storage**: All participants read from same location
- **User references**: Lightweight index in `{userId}/conversation-refs/{conversationId}.json`

**Schema**:

| Field | Type | Description | Constraints | Index |
|-------|------|-------------|-------------|-------|
| `msgId` | i64 | Snowflake ID (time-ordered unique identifier) | NOT NULL, PRIMARY KEY | Sorted (implicit in Parquet) |
| `conversationId` | String | Conversation this message belongs to | NOT NULL | Indexed (row group) |
| `conversationType` | String | Type of conversation: "ai" or "group" | NOT NULL | - |
| `from` | String | Sender userId (can be AI, user, or another user) | NOT NULL | - |
| `timestamp` | i64 | Unix timestamp in microseconds (UTC) | NOT NULL | Sorted (via msgId) |
| `content` | String | Message body/text OR content preview (for large/media messages) | NOT NULL, MAX 1MB (configurable) | - |
| `metadata` | String (JSON) | Flexible key-value pairs (role, model, tokens, contentType, fileName, fileSize, etc.) | NULLABLE | - |
| `contentRef` | String (optional) | Reference URI for large messages or media files (relative to conversation storage location) | NULLABLE | - |

**Parquet Schema (Arrow)**:
```rust
Schema::new(vec![
    Field::new("msgId", DataType::Int64, false),
    Field::new("conversationId", DataType::Utf8, false),
    Field::new("conversationType", DataType::Utf8, false), // "ai" or "group"
    Field::new("from", DataType::Utf8, false),
    Field::new("timestamp", DataType::Int64, false),
    Field::new("content", DataType::Utf8, false),
    Field::new("contentRef", DataType::Utf8, true), // Optional reference
    Field::new("metadata", DataType::Utf8, true), // JSON string
])
```

**Content Storage Strategies**:

1. **Small/Medium Text Messages** (< `large_message_threshold`):
   - **AI conversations**: Stored inline in user's Parquet: `{userId}/batch-*.parquet`
   - **Group conversations**: Stored inline in shared Parquet: `shared/conversations/{convId}/batch-*.parquet`
   - Fast access, no external lookup
   - Example: `content: "Hello, how are you?"`

2. **Large Text Messages** (≥ `large_message_threshold`):
   - **AI conversations**: Stored in user's folder: `{userId}/msg-{msgId}.bin`
   - **Group conversations**: Stored in conversation folder: `shared/conversations/{conversationId}/msg-{msgId}.bin`
   - `content` field contains truncated preview (first 1KB)
   - `contentRef` field contains relative path: `msg-{msgId}.bin`
   - `metadata` includes: `{"contentType": "text/plain", "fullSize": 500000}`

3. **Media Files** (images, documents, audio, video, etc.):
   - **AI conversations**: Stored in user's folder: `{userId}/media-{msgId}.{ext}`
   - **Group conversations**: Stored in conversation folder: `shared/conversations/{conversationId}/media-{msgId}.{ext}`
   - `content` field contains caption/description text or placeholder
   - `contentRef` field contains relative path: `media-{msgId}.jpg`
   - `metadata` includes rich information:
     ```json
     {
       "contentType": "image/jpeg",
       "fileName": "vacation-photo.jpg",
       "fileSize": 2457600,
       "width": 1920,
       "height": 1080,
       "thumbnail": "data:image/jpeg;base64,/9j/4AAQ...",
       "duration": null
     }
     ```
   - Supported media types: `image/*`, `audio/*`, `video/*`, `application/pdf`, `application/*`, etc.

**Validation Rules**:
- `msgId`: Must be valid snowflake ID (generated by server)
- `conversationId`: Non-empty string, max 255 chars
- `from`: Non-empty string, max 255 chars
- `timestamp`: Must be <= current time (server validates, rejects future timestamps)
- `content`: Non-empty, max size enforced by config (default 1MB, or truncated if `contentRef` present)
- `contentRef`: Valid URI if present (must point to accessible storage)
- `metadata`: Valid JSON if present

**Lifecycle**:

**AI Conversations**:
1. **Write**: 
   - Determine content type (text or media)
   - If large text or media: upload to user's storage folder `{userId}/`
   - Write message to RocksDB: `{userId}:{msgId}`
   - Acknowledge immediately
   
2. **Consolidation**: 
   - Background task reads RocksDB for user
   - Writes to user's Parquet: `{userId}/batch-*.parquet`
   - Large content/media remain in user's folder
   
3. **Query**: 
   - DataFusion reads user's own Parquet files
   - If `contentRef` present: fetch from user's storage folder

**Group Conversations**:
1. **Write**:
   - Lookup conversation participants via ConversationUser table
   - Determine content type (text or media)
   - If large text or media: upload to conversation storage `shared/conversations/{convId}/`
   - Write message to RocksDB: `conv:{conversationId}:{msgId}`
   - Update each participant's conversation reference
   - Acknowledge after write complete
   
2. **Consolidation**:
   - Background task reads RocksDB for conversation
   - Writes to shared Parquet: `shared/conversations/{convId}/batch-*.parquet`
   - Large content/media remain in conversation folder
   
3. **Query**:
   - Verify user is participant (check ConversationUser table)
   - DataFusion reads shared Parquet: `shared/conversations/{convId}/batch-*.parquet`
   - If `contentRef` present: fetch from conversation storage folder

---

### 2. Conversation

Represents a conversation thread containing multiple messages. Each user has their own set of conversations.

**Storage**:
- **RocksDB** (metadata cache): Key = `{userId}:conversations:{conversationId}`, Value = ConversationMetadata
- **Separate metadata table** (future: SQLite or Parquet index file for fast lookups)

**Schema**:

| Field | Type | Description | Constraints |
|-------|------|-------------|-------------|
| `userId` | String | Owner of this conversation | NOT NULL, PARTITION KEY |
| `conversationId` | String | Unique conversation identifier | NOT NULL, PRIMARY KEY |
| `firstMsgId` | i64 | Snowflake ID of first message | NOT NULL |
| `lastMsgId` | i64 | Snowflake ID of most recent message | NOT NULL, updated periodically |
| `created` | i64 | Unix timestamp when conversation created (microseconds) | NOT NULL |
| `updated` | i64 | Unix timestamp when conversation last modified (microseconds) | NOT NULL |

**Validation Rules**:
- `conversationId`: Non-empty, max 255 chars, unique per user
- `firstMsgId` <= `lastMsgId`
- `created` <= `updated`

**Lifecycle**:
1. **Create**: When first message in conversation arrives, create metadata entry
2. **Update**: Periodically during consolidation, update `lastMsgId` and `updated` timestamp
3. **Query**: List conversations via RocksDB scan of `{userId}:conversations:*` keys

**State Transitions**:
```
[Initial] --first message--> [Active]
[Active] --new message--> [Active] (update lastMsgId)
[Active] --query--> [Active] (read-only)
```

---

### 3. ConversationUser

Represents the relationship between users and conversations, tracking which users have access to which conversations. This enables multi-user conversations and access control.

**Storage**:
- **RocksDB** (metadata): Key = `{conversationId}:users:{userId}`, Value = ConversationUserMetadata
- **Secondary index**: Key = `{userId}:convusers:{conversationId}` for user-centric queries
- **Separate metadata table** (future: SQLite for relational queries)

**Schema**:

| Field | Type | Description | Constraints |
|-------|------|-------------|-------------|
| `userId` | String | User ID with access to this conversation | NOT NULL, PRIMARY KEY (composite) |
| `conversationId` | String | Conversation identifier | NOT NULL, PRIMARY KEY (composite) |
| `created` | i64 | Unix timestamp when user joined conversation (microseconds) | NOT NULL |
| `updated` | i64 | Unix timestamp when user's participation last changed (microseconds) | NOT NULL |
| `metadata` | String (JSON) | Flexible key-value pairs (role, permissions, lastRead, etc.) | NULLABLE |

**Validation Rules**:
- `userId`: Non-empty, max 255 chars
- `conversationId`: Non-empty, max 255 chars
- `created` <= `updated`
- `metadata`: Valid JSON if present

**Example Metadata**:
```json
{
  "role": "participant",
  "permissions": ["read", "write"],
  "lastReadMsgId": 1234567890123456,
  "notificationsEnabled": true,
  "displayName": "John Doe"
}
```

**Lifecycle**:
1. **Create**: When user is added to a conversation (manually or via first message)
2. **Update**: When user's permissions change or they read messages
3. **Query**: List users in conversation via `{conversationId}:users:*` scan, or list user's conversations via `{userId}:convusers:*` scan
4. **Delete**: When user leaves conversation or is removed

**State Transitions**:
```
[Initial] --user joins--> [Active]
[Active] --permissions change--> [Active] (update metadata)
[Active] --user leaves--> [Removed]
```

---

### 4. Subscription

Represents an active WebSocket connection subscribing to real-time message updates.

**Storage**:
- **In-memory** (not persisted): HashMap<userId, Vec<SubscriptionHandle>>

**Schema**:

| Field | Type | Description |
|-------|------|-------------|
| `connectionId` | String (UUID) | Unique WebSocket connection identifier |
| `userId` | String | User whose messages this subscription receives |
| `conversationId` | Option<String> | Optional: filter to specific conversation |
| `lastMsgId` | Option<i64> | Optional: only receive messages > this ID |
| `connectedAt` | i64 | Unix timestamp when subscription established |
| `lastHeartbeat` | i64 | Unix timestamp of last ping/pong |
| `websocket` | WebSocket (Actix) | Active WebSocket connection handle |

**Validation Rules**:
- `userId`: Must match authenticated JWT token subject
- `conversationId`: If present, must exist for this user
- `lastMsgId`: If present, replay messages > this ID before subscribing to live stream

**Lifecycle**:
1. **Subscribe**: Client sends WebSocket subscribe message, server creates Subscription entry
2. **Replay**: If `lastMsgId` provided, query Parquet/RocksDB for messages > lastMsgId, send to client
3. **Live Stream**: New messages matching userId/conversationId forwarded to WebSocket
4. **Heartbeat**: Client sends ping every 30s, server updates `lastHeartbeat`
5. **Unsubscribe**: Client disconnects, server removes Subscription entry

---

### 5. User

Logical entity representing a user whose messages are isolated in separate storage partitions.

**Storage**:
- **Implicit**: User is partition key, not stored as separate entity
- **Directory structure**: `<storage-root>/<userId>/batch-*.parquet`

**Schema** (logical, not persisted):

| Field | Type | Description |
|-------|------|-------------|
| `userId` | String | Unique user identifier (from JWT token) |
| `totalMessages` | i64 | Total messages for this user (computed from Parquet + RocksDB) |
| `storageBytes` | i64 | Total storage size for this user (computed from file sizes) |
| `conversations` | Vec<Conversation> | List of conversations for this user |

**Validation Rules**:
- `userId`: Non-empty, max 255 chars, alphanumeric + hyphens/underscores

**Lifecycle**:
- **Create**: Implicitly created when first message for userId arrives
- **Delete**: (Future) Delete all Parquet files and RocksDB entries for userId

---

## Relationships

```
User (1) ----< (N) ConversationUser (N) >---- (1) Conversation
Conversation (1) ----< (N) Message
User (1) ----< (N) Subscription
```

**Cardinality**:
- User to ConversationUser: One User has many ConversationUser entries (1:N)
- ConversationUser to Conversation: Many Users can participate in many Conversations (N:M via ConversationUser join table)
- One Conversation has many Messages (1:N) - but messages are **duplicated** across all participant users
- One User has many active Subscriptions (1:N)

**Message Duplication Model**:
- **Physical Storage**: Each message exists in N copies (where N = number of conversation participants)
- **Logical Model**: Each message has one canonical msgId, but stored in multiple user partitions
- **Storage Path**: `{userId}/batch-*.parquet` contains ALL messages from conversations where userId is participant
- **Example**: Message in conversation with 3 users → 3 copies stored in 3 separate user partitions

**Referential Integrity**:
- Messages reference Conversation via `conversationId` (soft reference, no FK constraint)
- ConversationUser references both User and Conversation via `userId` and `conversationId` (validated on write)
- Subscriptions reference User via `userId` (validated against JWT token)
- Subscriptions can optionally filter by `conversationId` (validated against ConversationUser for access control)
- Large message content stored in shared location: `shared/conversations/{conversationId}/msg-{msgId}.bin`

---

## Data Flow

### Write Path
```
Client --> REST API (POST /messages)
       --> Validate JWT + message
       --> Check ConversationUser access (userId has write permission for conversationId)
       --> Generate snowflake msgId
       --> Query ConversationUser table for all participants
       --> Determine if message is large (content.length >= large_message_threshold)
       --> If large:
           --> Upload content to shared storage: shared/conversations/{convId}/msg-{msgId}.bin
           --> Truncate content field (keep first 1KB as preview)
           --> Set contentRef field with S3 URI
       --> For EACH participant user (parallel writes):
           --> Write to RocksDB ({participantUserId}:{msgId} = Message)
       --> Wait for all writes to complete
       --> Acknowledge (return msgId)
       --> Broadcast to active Subscriptions (WebSocket, filtered by ConversationUser access)
```

**Write Path Performance**:
- Parallel writes to RocksDB (one per participant)
- Target latency: <100ms for conversations with ≤10 participants
- Large message upload happens async if needed
- Acknowledgment sent after RocksDB writes (before consolidation)

### Consolidation Path
```
Background Task (every 5 min or 10k messages per user)
       --> For each user exceeding threshold:
           --> Read messages from RocksDB ({userId}:*)
           --> Group by conversationId for row group optimization
           --> Write Parquet file ({userId}/batch-{ts}-{idx}.parquet)
           --> Parquet contains user's own messages + duplicated messages from shared conversations
           --> Delete from RocksDB to free memory
           --> Update conversation metadata (lastMsgId)
```

**Consolidation Notes**:
- Each user's consolidation runs independently
- Messages with `contentRef` stored with reference (not full content)
- Large message content remains in shared storage
- Deduplication NOT performed (duplication is intentional)

### Query Path
```
Client --> REST API (GET /messages?userId=X&conversationId=Y)
       --> Validate JWT (extract userId from token)
       --> Query ONLY user's own storage: {userId}/batch-*.parquet
       --> DataFusion: SELECT * FROM user_parquet_files WHERE conversationId = Y
       --> Filter row groups via conversationId metadata
       --> Merge with RocksDB (recent messages from {userId}:* keys)
       --> If message has contentRef and full content requested:
           --> Fetch from shared storage: shared/conversations/{convId}/msg-{msgId}.bin
       --> Return paginated results
```

**Query Path Benefits**:
- No cross-user joins needed (all data in user's partition)
- Fast parallel queries (each user's data independent)
- Storage can be sharded/moved per user
- Privacy: users can't access other users' storage

### Subscription Path
```
Client --> WebSocket (subscribe message)
       --> Validate JWT
       --> Create Subscription in memory
       --> If lastMsgId provided:
           --> Query messages > lastMsgId (replay)
           --> Send replay messages to WebSocket
       --> Subscribe to live stream
       --> Forward new messages matching userId/conversationId
```

---

## Indexes & Optimization

### Parquet Row Groups
- **Primary sort**: `msgId` (implicit, snowflake IDs are time-ordered)
- **Row group organization**: Messages grouped by `conversationId`
  - Enables DataFusion to skip row groups when filtering by `conversationId`
- **Row group size**: Target 1MB per row group (~1000 messages @ 1KB/message)

### RocksDB Key Design
- **Message keys**: `{userId}:{msgId}` (8-byte prefix + 8-byte msgId)
  - Enables prefix scan for all messages by userId
- **Conversation keys**: `{userId}:conversations:{conversationId}`
  - Separate namespace for conversation metadata
- **Column families**: Single default CF (keep it simple)

### Bloom Filters
- RocksDB bloom filters enabled (10 bits per key)
- Reduces read amplification during subscription queries

---

## Schema Evolution

### Future Extensions

1. **Custom Columns** (Phase 2+):
   - Allow users to define custom Parquet columns beyond base schema
   - Store custom column definitions in conversation metadata
   - Use Arrow schema merging to combine base + custom columns

2. **Encryption** (Phase 2+):
   - Add `encryptedContent` column (BLOB)
   - Store encryption metadata in `metadata` field (key ID, algorithm)
   - Decrypt on query using user-provided key

3. **Attachments** (Phase 2+):
   - Store attachment URLs/references in `metadata` field
   - Separate storage for large files (S3 with signed URLs)

4. **Message Edits/Deletes** (Phase 2+):
   - Add `editHistory` array column (list of previous versions)
   - Add `deletedAt` timestamp (soft delete)
   - Parquet files immutable; edits stored in separate delta files

---

## Data Retention & Archival

### Retention Policy (configurable)
- **Default**: Messages stored indefinitely
- **Optional**: Delete messages older than N days
  - Implementation: Background task deletes old Parquet files
  - Metadata updated to reflect retention

### Archival Strategy
- **Cold storage**: Move old Parquet files to S3 Glacier (future)
- **Hot storage**: Recent messages (last 30 days) in local/S3 standard
- **Query optimization**: DataFusion skips archived files unless explicitly requested

---

## Data Consistency

### Eventual Consistency Model
- **Writes**: Acknowledged immediately after RocksDB write
- **Consolidation**: Async background process (5 min lag)
- **Queries**: May return stale `lastMsgId` in conversation metadata (updated periodically)

### Consistency Guarantees
- **Write durability**: Messages persist in RocksDB WAL (survives crash)
- **Read-after-write**: Recent messages read from RocksDB + Parquet (merge view)
- **Subscription replay**: Guaranteed delivery of all messages > lastMsgId (no gaps)

### Conflict Resolution
- **Concurrent writes**: RocksDB handles atomicity (single-threaded write path)
- **Duplicate msgIds**: Impossible (snowflake IDs are unique)
- **Consolidation conflicts**: Per-user locking prevents concurrent consolidation

---

## Storage Implications of Message Duplication

### Storage Multiplication Factor
- **Duplication Factor**: Messages stored N times (where N = number of conversation participants)
- **Example**: 3-person conversation → each message stored 3x
- **Trade-off**: Higher storage cost for better query performance and user data ownership

### Storage Cost Analysis

**Scenario: 1000 users, average 3 participants per conversation**

| Metric | Without Duplication | With Duplication (3x avg) | Overhead |
|--------|---------------------|---------------------------|----------|
| 100k messages/day | ~40MB Parquet | ~120MB Parquet | 3x |
| Annual storage | ~14.6GB | ~43.8GB | 3x |
| Per-user average | 14.6MB/year | 43.8MB/year | 3x |

**Large Message Optimization**:
- Messages > `large_message_threshold` (e.g., 100KB) stored once in shared storage
- Only reference duplicated (negligible size)
- Reduces overhead for large attachments/content

### Single Message Size
- **RocksDB** (small message): ~1.2KB per copy
- **RocksDB** (large message): ~0.5KB (reference only) + shared storage
- **Parquet** (small message): ~0.4KB per copy (3x compression)
- **Parquet** (large message): ~0.2KB (reference only) + shared storage

### Configuration Parameters (config.toml)

```toml
[message]
# Maximum inline message size before using shared storage
large_message_threshold_bytes = 102400  # 100 KB

[storage]
# Shared storage location for large message content
shared_storage_path = "shared/conversations"
# or for S3:
# shared_storage_uri = "s3://bucket/kalamdb/shared/conversations"

[duplication]
# Enable/disable message duplication (future: allow single-storage mode)
enabled = true
# Maximum participants before warning (high duplication cost)
max_participants_warning = 20
```

### Storage Growth Estimates

**Typical Workload (1000 users, 100 messages/day each, 3-participant conversations)**:
- **Daily writes**: 100k logical messages → 300k physical messages (3x duplication)
- **Daily Parquet growth**: ~120MB (with compression)
- **Annual storage**: ~43.8GB
- **With 10% large messages (>100KB)**: ~30GB (savings from shared storage)

**High Duplication Scenario (10-participant group chats)**:
- **Duplication factor**: 10x
- **Annual storage**: ~146GB for same workload
- **Mitigation**: Large message optimization critical for group chats

---

## Summary

KalamDb's data model prioritizes:
1. **User-centric isolation**: Data partitioned by userId at storage layer
2. **Fast writes**: RocksDB with WAL, <1ms latency
3. **Efficient queries**: Parquet with conversationId row groups, DataFusion SQL
4. **Real-time streaming**: In-memory subscriptions with WebSocket
5. **Flexible metadata**: JSON metadata field for extensibility

**Next Phase**: Define API contracts (REST + WebSocket endpoints)
