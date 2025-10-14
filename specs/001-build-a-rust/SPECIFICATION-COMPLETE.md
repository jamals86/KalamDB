# KalamDB Specification - Complete Summary

**Versi### 2. **Table-Per-User Multi-Tenancy** ⭐

**Revolutionary Scalability**: Each user has their own logical table (`userId.messages`) backed by isolated storage partitions (`{userId}/batch-*.parquet`).

**Why This Matters**:
- **Traditional databases**: All users share one massive `messages` table → complex triggers, expensive filtering, performance degrades with scale
- **KalamDB**: Each user has isolated storage → simple file notifications, O(1) subscription complexity, millions of concurrent users

**Real-World Impact**:
```
Listening to User Updates:

❌ Traditional Shared Table:
   CREATE TRIGGER ON messages WHERE userId = 'user_12345'
   → Must monitor entire table with billions of rows
   → Performance degrades as user count grows
   
✅ KalamDB Per-User Table:
   watch_directory("user_12345/messages/")
   → Only monitors one user's partition
   → O(1) complexity regardless of total users
   → Scales to millions of concurrent subscriptions
```

**Benefits**:
- 🚀 **Massive Scalability**: Support millions of concurrent real-time subscriptions
- ⚡ **Simple Implementation**: File-level monitoring replaces complex database triggers
- 🎯 **Efficient Queries**: Direct partition access, no userId filtering overhead
- 📦 **Horizontal Scaling**: Adding users doesn't impact existing performance
- 🔒 **Physical Isolation**: User data separated at storage layer for privacy and compliance

### 3. **Two Conversation Types**n**: 2.0.0 (SQL-First)  
**Date**: 2025-10-13  
**Status**: Design Complete, Ready for Implementation

---

## 📋 What We Built

A complete specification for **KalamDB** - a SQL-first chat and AI message history storage system with intelligent deletion, conversation types, and optimized storage.

---

## 🎯 Core Design Principles

### 1. **SQL-First Interface**
- **Single endpoint**: `POST /api/v1/query`
- No specialized CRUD endpoints needed
- Universal interface for all operations (SELECT, INSERT, UPDATE, DELETE)
- DataFusion-based query engine

### 2. **Table-Per-User Multi-Tenancy** ⭐

**Revolutionary Scalability**: Each user has their own logical table (`userId.messages`) backed by isolated storage partitions (`{userId}/batch-*.parquet`).

**Why This Matters**:
- **Traditional databases**: All users share one massive `messages` table → complex triggers, expensive filtering, performance degrades with scale
- **KalamDB**: Each user has isolated storage → simple file notifications, O(1) subscription complexity, millions of concurrent users

**Real-World Impact**:
```
Listening to User Updates:

❌ Traditional Shared Table:
   CREATE TRIGGER ON messages WHERE userId = 'user_12345'
   → Must monitor entire table with billions of rows
   → Performance degrades as user count grows
   
✅ KalamDB Per-User Table:
   watch_directory("user_12345/messages/")
   → Only monitors one user's partition
   → O(1) complexity regardless of total users
   → Scales to millions of concurrent subscriptions
```

**Benefits**:
- 🚀 **Massive Scalability**: Support millions of concurrent real-time subscriptions
- ⚡ **Simple Implementation**: File-level monitoring replaces complex database triggers
- 🎯 **Efficient Queries**: Direct partition access, no userId filtering overhead
- 📦 **Horizontal Scaling**: Adding users doesn't impact existing performance
- 🔒 **Physical Isolation**: User data separated at storage layer for privacy and compliance

### 3. **Two Conversation Types**

**AI Conversations** (User ↔ AI):
- Messages: `{userId}/batch-*.parquet`
- Content: All in user folder
- No duplication (single participant)

**Group Conversations** (User ↔ User):
- Messages: **Duplicated** to each participant's storage
- Small content (<100KB): Inline (duplicated)
- Large content (≥100KB): Stored **once** in `shared/conversations/{convId}/`
- Media: Stored **once** in shared folder

### 4. **Intelligent Deletion**
- Single message deletion with cascade to files
- Conversation deletion with cascade to all messages
- Reference counting for shared content
- Soft delete option with retention policy

### 5. **Hot/Cold Storage Architecture**
- **Hot**: RocksDB (buffered writes, <1ms)
- **Cold**: Parquet files (consolidated, S3/local)
- Query engine automatically merges both sources

---

## 📁 Specification Files

### Core Documentation

| File | Purpose | Status |
|------|---------|--------|
| `data-model.md` | Complete data model with entities, schemas, lifecycle | ✅ Complete |
| `API-ARCHITECTURE.md` | SQL-first API approach and design rationale | ✅ Complete |
| `sql-query-examples.md` | Comprehensive SQL examples for all operations | ✅ Complete |
| `ARCHITECTURE-UPDATE.md` | Migration guide and architecture diagrams | ✅ Complete |
| `plan.md` | Implementation plan with tech stack | ✅ Updated |
| `research.md` | Technical decisions and research | ✅ Complete |
| `quickstart.md` | Developer quickstart guide | ✅ Complete |

### API Contracts

| File | Purpose | Status |
|------|---------|--------|
| `contracts/rest-api.yaml` | OpenAPI spec with SQL query endpoint | ✅ Complete |
| `contracts/websocket-protocol.md` | WebSocket protocol for real-time streaming | ✅ Complete |

### Architecture Documents

| File | Purpose | Status |
|------|---------|--------|
| `architecture/message-duplication.md` | Message duplication strategy | ✅ Complete |
| `architecture/media-files-support.md` | Media file support guide | ✅ Complete |

---

## 🗂️ Storage Architecture

```
/var/lib/kalamdb/
├── user_john/                          # User storage
│   ├── batch-20251013-001.parquet      # AI + group messages (duplicated)
│   ├── msg-123.bin                     # Large AI message content
│   └── media-456.jpg                   # AI conversation media
├── user_alice/
│   ├── batch-*.parquet                 # Alice's message copies
│   └── ...
└── shared/                             # Shared storage
    └── conversations/
        └── conv_abc123/                # Group conversation
            ├── msg-789.bin             # Large group message (once)
            └── media-101.jpg           # Group media (once)
```

---

## 🔧 API Endpoints

### Primary Endpoint

```
POST /api/v1/query
```

**Request**:
```json
{
  "sql": "SELECT * FROM userId.messages WHERE conversationId = 'conv_123' LIMIT 100"
}
```

**Response**:
```json
{
  "columns": ["msgId", "conversationId", "from", "timestamp", "content"],
  "rows": [[123, "conv_123", "user_john", 1699000000, "Hello"]],
  "rowCount": 1,
  "executionTimeMs": 15
}
```

### Optional Endpoints

- `POST /api/v1/messages` - Convenience for message submission
- `POST /api/v1/media/upload` - Media file upload
- `GET /api/v1/health` - Health check
- `WebSocket /ws` - Real-time streaming

---

## 💾 Storage Optimization

### Without Optimization
- 100k messages × 3 participants = 300k copies
- Large content duplicated × 3
- **Total**: ~52GB/year

### With Optimization (Current Design)
- 100k messages × 3 participants = 300k copies (messages)
- Large content stored **once** (not duplicated)
- **Total**: ~26GB/year
- **Savings**: 50% storage reduction

---

## 🗄️ SQL Operations

### Query Messages
```sql
SELECT * FROM userId.messages WHERE conversationId = 'conv_123' LIMIT 100;
```

### Delete Message
```sql
DELETE FROM userId.messages WHERE msgId = 123;
-- Cascades to files (AI: user folder, Group: checks reference count)
```

### Delete Conversation
```sql
DELETE FROM userId.conversations WHERE conversationId = 'conv_123';
-- Cascades to all messages + files for this user
```

### Count Messages
```sql
SELECT conversationId, COUNT(*) FROM userId.messages GROUP BY conversationId;
```

### Search Content
```sql
SELECT * FROM userId.messages WHERE content LIKE '%search%';
```

---

## 🔍 Query Engine Architecture

```
SQL Query
    ↓
Parser & Validator
    ↓
Query Planner
    ↓
┌─────────────┴────────────┐
│                          │
RocksDB Scan          Parquet Scan
(Hot Data)            (Cold Data)
│                          │
└─────────────┬────────────┘
    ↓
Merge & Deduplicate
    ↓
Apply Filters
    ↓
JSON Response
```

**Performance**:
- Hot data: <10ms
- Cold data: 50-200ms
- Automatic caching

---

## 🗑️ Deletion Logic

### Single Message

**AI Conversation**:
1. Remove from RocksDB and Parquet
2. User owns content → **delete** `{userId}/msg-*.bin`

**Group Conversation**:
1. Remove from RocksDB and Parquet
2. Check reference count for shared content
3. If last user → **delete** `shared/conversations/{convId}/msg-*.bin`
4. If other users exist → **keep** shared content

### Entire Conversation

**AI Conversation**:
1. Delete conversation metadata
2. Delete all messages
3. Delete all files in user folder

**Group Conversation (Single User)**:
1. Remove user from ConversationUser table
2. Delete user's message copies
3. Check if last participant
4. If YES → delete `shared/conversations/{convId}/`
5. If NO → keep shared folder

---

## 🏗️ Implementation Roadmap

### Phase 1: Core Storage (4-6 weeks)
- [ ] RocksDB write buffer implementation
- [ ] Parquet file writer with consolidation
- [ ] Snowflake ID generator
- [ ] Storage layer (local filesystem + S3)

### Phase 2: Query Engine (4-6 weeks)
- [ ] DataFusion SQL integration
- [ ] Hot/cold merge logic
- [ ] Query parser and validator
- [ ] Authentication middleware (JWT)

### Phase 3: Deletion & Content (2-3 weeks)
- [ ] Deletion cascade logic
- [ ] Reference counting for shared content
- [ ] Media file upload and storage
- [ ] Content retrieval with signed URLs

### Phase 4: WebSocket & Real-time (2-3 weeks)
- [ ] WebSocket connection management
- [ ] Subscription system
- [ ] Message broadcasting
- [ ] Replay functionality

### Phase 5: Admin UI (3-4 weeks)
- [ ] React + Vite setup
- [ ] SQL query editor
- [ ] Results visualization
- [ ] System metrics dashboard

---

## 📊 Key Metrics

### Storage
- **AI conversations**: 1x storage (no duplication)
- **Group messages**: Nx storage (N = participants)
- **Group large content**: 1x storage (shared)
- **Overall savings**: ~50% vs full duplication

### Performance
- **Write latency**: <1ms (RocksDB)
- **Query latency**: <10ms (hot), 50-200ms (cold)
- **Consolidation**: Every 5 min or 10k messages

### Scalability
- **Users**: 10,000+ supported
- **Messages/sec**: 1,000+ writes
- **Group size**: 100+ participants

---

## 🎓 Example Workflows

### Send AI Message
```bash
curl -X POST http://localhost:8080/api/v1/query \
  -H "Authorization: Bearer $JWT" \
  -d '{"sql": "INSERT INTO userId.messages (msgId, conversationId, conversationType, from, timestamp, content) VALUES (123, '\''conv_ai'\'', '\''ai'\'', '\''user_john'\'', 1699000000, '\''Hello AI'\'')"}'
```

### Send Group Message with Media
```bash
# 1. Upload media
curl -X POST http://localhost:8080/api/v1/media/upload \
  -H "Authorization: Bearer $JWT" \
  -F "file=@image.jpg" \
  -F "conversationId=conv_group" \
  -F "conversationType=group"
# Returns: {"contentRef": "shared/conversations/conv_group/media-789.jpg"}

# 2. Insert message with contentRef
curl -X POST http://localhost:8080/api/v1/messages \
  -H "Authorization: Bearer $JWT" \
  -d '{
    "conversationId": "conv_group",
    "conversationType": "group",
    "from": "user_alice",
    "content": "Check this!",
    "contentRef": "shared/conversations/conv_group/media-789.jpg"
  }'
```

### Query Messages
```bash
curl -X POST http://localhost:8080/api/v1/query \
  -H "Authorization: Bearer $JWT" \
  -d '{"sql": "SELECT * FROM userId.messages WHERE conversationId = '\''conv_123'\'' LIMIT 100"}'
```

### Delete Conversation
```bash
curl -X POST http://localhost:8080/api/v1/query \
  -H "Authorization: Bearer $JWT" \
  -d '{"sql": "DELETE FROM userId.conversations WHERE conversationId = '\''conv_old'\''"}'
```

---

## ✅ What's Complete

1. ✅ Data model with 5 core entities
2. ✅ SQL-first API architecture
3. ✅ REST API specification (OpenAPI)
4. ✅ WebSocket protocol specification
5. ✅ Deletion operations with cascade logic
6. ✅ Message duplication strategy
7. ✅ Media file support
8. ✅ Storage layout and optimization
9. ✅ SQL query examples (50+ queries)
10. ✅ Architecture diagrams and migration guide

---

## ⏳ Next Steps

1. **Begin Phase 1 Implementation**:
   - Set up Rust project structure
   - Implement RocksDB write buffer
   - Build Parquet consolidation
   
2. **Create `/speckit.tasks` Breakdown**:
   - Break down each phase into actionable tasks
   - Assign effort estimates
   - Define acceptance criteria

3. **Set Up Development Environment**:
   - Initialize Rust workspace
   - Configure development tools
   - Set up testing framework

---

## 🎯 Success Criteria

**MVP Complete When**:
- ✅ Users can send messages (AI and group conversations)
- ✅ Messages stored in RocksDB → consolidated to Parquet
- ✅ SQL queries work across hot/cold storage
- ✅ Deletion operations cascade correctly
- ✅ WebSocket streaming functional
- ✅ Admin UI can execute SQL queries
- ✅ Media files upload and retrieve successfully

---

## 📚 Reference Documents

**Start Here**: `API-ARCHITECTURE.md` - Understanding the SQL-first approach

**Implementation**: `data-model.md` - Complete technical specification

**Examples**: `sql-query-examples.md` - SQL query cookbook

**API Contract**: `contracts/rest-api.yaml` - OpenAPI specification

**Migration**: `ARCHITECTURE-UPDATE.md` - From v1 to v2 guide

---

## 🚀 Ready to Build!

All design decisions are documented. All specifications are complete. The architecture is optimized for:
- ✅ **Simplicity**: Single SQL endpoint
- ✅ **Performance**: Hot/cold storage merge
- ✅ **Efficiency**: 50% storage savings
- ✅ **Flexibility**: Unlimited query expressiveness
- ✅ **Scalability**: User-centric partitioning

**Next command**: `/speckit.tasks` to break down into implementation tasks! 🎉
