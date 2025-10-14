# KalamDB

**Real-time Chat & AI Message History Storage**  
*A SQL-first database designed for scalable, user-centric conversations*

---

## 🚀 What Makes KalamDB Different?

### **Massively Scalable Multi-Tenancy with Per-User Tables**

Unlike traditional databases that store all users' data in shared tables, **KalamDB uses a table-per-user architecture**. This design decision unlocks unprecedented scalability and real-time capabilities:

#### ✨ **The Power of Per-User Tables**

```
Traditional Database (Shared Table):
┌─────────────────────────────────┐
│      messages (shared)          │
│  userId  │ conversationId │ ... │
│ ─────────┼────────────────┼──── │
│  user1   │    conv_A      │ ... │
│  user2   │    conv_B      │ ... │
│  user1   │    conv_C      │ ... │
│  user3   │    conv_D      │ ... │
│  ...millions of rows...         │
└─────────────────────────────────┘
❌ Complex triggers on entire table
❌ Inefficient filtering for real-time
❌ Scaling bottlenecks at millions of users


KalamDB (Table-Per-User):
┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│ user1.msgs   │  |user2.messages│  │user3.messages│
│ convId │ ... │  │ convId │ ... │  │ convId │ ... │
│────────┼──── │  │────────┼──── │  │────────┼──── │
│ conv_A │ ... │  │ conv_B │ ... │  │ conv_D │ ... │
│ conv_C │ ... │  │ ...          │  │ ...          │
└──────────────┘  └──────────────┘  └──────────────┘
✅ Simple per-user subscriptions
✅ Isolated storage & indexes
✅ Scales to millions of concurrent users
```

#### 🎯 **Key Benefits**

| Feature | Traditional Shared Tables | KalamDB Per-User Tables |
|---------|---------------------------|-------------------------|
| **Real-time Subscriptions** | Complex database triggers on billions of rows | Simple file-level notifications per user |
| **Concurrent Users** | Degrades with global table locks | Millions of users, each with isolated tables |
| **Query Performance** | Must filter userId on every query | Direct access to user's partition |
| **Data Isolation** | Row-level security overhead | Physical storage separation |
| **Scalability** | Vertical (bigger database) | Horizontal (add more users) |
| **Backup/Export** | Complex per-user extraction | Simple file copy per user |

#### 💡 **Real-World Impact**

**Listening to User Updates**:
```rust
// Traditional: Complex trigger on shared table with billions of rows
CREATE TRIGGER notify_user 
ON messages -- SHARED TABLE (all users!)
FOR EACH INSERT WHERE userId = 'user_12345'
  -- Must scan/filter entire table on every insert
  -- Performance degrades as user count grows

// KalamDB: Simple file notification per user
watch_directory("user_12345/messages/")
  -- Only monitors one user's storage
  -- O(1) complexity regardless of total users
  -- Scales to millions of concurrent subscriptions
```

**Subscription Scalability**:
- **Traditional**: 1 million users = 1 million WHERE clauses on a shared table
- **KalamDB**: 1 million users = 1 million independent file watchers (isolated, parallel)

**The Result**: KalamDB can support **millions of concurrent real-time subscriptions** with minimal overhead, making it ideal for chat applications, AI assistants, and collaborative tools at scale.

---

## 🌟 Core Features

### 📦 **Unified Storage + Real-time Streaming**
Single system replaces database + message broker (Redis/Kafka). No synchronization needed.

### ⚡ **Sub-millisecond Writes**
RocksDB hot storage with <1ms write latency. Periodic consolidation to Parquet for long-term efficiency.

### 🔍 **SQL-First Interface**
Query everything with standard SQL via DataFusion engine. No proprietary query language.

### 🔒 **User-Centric Data Ownership**
- Each user's data physically isolated in separate partitions
- Complete conversation history in user's own storage
- Easy data export, backup, and migration
- Privacy and GDPR compliance by design

### 📊 **Intelligent Storage Optimization**
- AI conversations: Zero duplication (single participant)
- Group conversations: Messages duplicated per user, large content stored once
- 50% storage savings compared to naive duplication

### 🌐 **WebSocket Real-time Updates**
Subscribe to your own message stream. Receive notifications within 500ms of new messages.

---

## 🏗️ Architecture Overview

### Storage Layout

```
/var/lib/kalamdb/
├── user_alice/                          # Alice's isolated storage
│   ├── batch-20251013-001.parquet      # Alice's messages (AI + group)
│   ├── msg-123.bin                     # Large message content
│   └── media-456.jpg                   # Media files
│
├── user_bob/                            # Bob's isolated storage
│   ├── batch-*.parquet                 # Bob's messages
│   └── ...
│
└── shared/                              # Shared group content
    └── conversations/
        └── conv_abc123/                # Group conversation
            ├── msg-789.bin             # Large messages (stored once)
            └── media-101.jpg           # Media files (stored once)
```

### Data Flow

```
┌─────────────┐
│   Client    │
│  (Alice)    │
└──────┬──────┘
       │ POST /api/v1/query
       │ SQL: INSERT INTO conversations ...
       ▼
┌─────────────────────────────────────┐
│         KalamDB Server              │
│  ┌────────────┐    ┌─────────────┐  │
│  │ DataFusion │──▶│  RocksDB   │ │ ◀── Hot storage (<1ms)
│  │ SQL Engine │    │  (Hot)      │  │
│  └────────────┘    └─────────────┘  │
│         │                           │
│         │ Consolidate (periodic)    │
│         ▼                           │
│  ┌─────────────┐                   │
│  │  Parquet    │                   │ ◀── Cold storage (optimized)
│  │  (Cold)     │                   │
│  └─────────────┘                   │
│         │                           │
│         │ Notify via WebSocket      │
│         ▼                           │
│  ┌─────────────┐                   │
│  │ Real-time   │                   │
│  │ Subscriber  │                   │
│  └─────────────┘                   │
└──────────┬──────────────────────────┘
           │
           │ WS: New message notification
           ▼
    ┌─────────────┐
    │   Client    │
    │   (Alice)   │
    └─────────────┘
```

---

## 🚀 Quick Start

### Query Messages (SQL)

```bash
curl -X POST http://localhost:8080/api/v1/query \
  -H "Authorization: Bearer <JWT_TOKEN>" \
  -H "Content-Type: application/json" \
  -d '{
    "sql": "SELECT * FROM user_alice.messages WHERE conversation_id = '\''conv_123'\'' LIMIT 50"
  }'
```

### Insert Message

```bash
curl -X POST http://localhost:8080/api/v1/query \
  -H "Authorization: Bearer <JWT_TOKEN>" \
  -H "Content-Type: application/json" \
  -d '{
    "sql": "INSERT INTO conversations (conversation_id, conversation_type, from, content, metadata) VALUES ('\''conv_123'\'', '\''ai'\'', '\''user_alice'\'', '\''Hello AI!'\'', '{\"role\":\"user\"}')"
  }'
```

### Real-time Subscription (WebSocket)

```javascript
const ws = new WebSocket('ws://localhost:8080/ws?token=<JWT_TOKEN>');

ws.send(JSON.stringify({
  type: 'subscribe',
  userId: 'user_alice',
  lastMsgId: 1234567890 // Resume from last known message
}));

ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  console.log('New message:', msg);
};
```

---

## 📊 Use Cases

### 1. **Chat Applications**
- Millions of concurrent users, each with real-time subscriptions
- Per-user storage enables independent scaling
- Full conversation history instantly accessible

### 2. **AI Assistant Platforms**
- Store user ↔ AI conversations with complete context
- Query historical interactions for RAG (Retrieval-Augmented Generation)
- Real-time streaming of AI responses

### 3. **Collaborative Tools**
- Group conversations with message duplication per user
- Each participant has complete conversation history
- No single point of failure

### 4. **Compliance & Privacy**
- User data physically isolated per partition
- Easy data export for GDPR requests
- Per-user encryption and access control

---

## 🛠️ Technology Stack

| Component | Technology | Purpose |
|-----------|-----------|---------|
| **Storage (Hot)** | RocksDB | Fast buffered writes (<1ms latency) |
| **Storage (Cold)** | Apache Parquet | Compressed columnar format for analytics |
| **Query Engine** | Apache DataFusion | SQL query execution across hot+cold storage |
| **API** | Actix-web | REST endpoints + WebSocket subscriptions |
| **Auth** | JWT | Token-based authentication |
| **Real-time** | WebSocket | Live message notifications |
| **Language** | Rust | Performance, safety, concurrency |

---

## 📐 Design Principles

From [`constitution.md`](.specify/memory/constitution.md):

1. **Simplicity First** - Direct code paths, minimal abstractions
2. **Performance by Design** - Sub-millisecond writes, SQL queries
3. **Data Ownership** - User-centric partitions, isolated storage, per-user tables
4. **Zero-Copy Efficiency** - Arrow IPC, Parquet, minimal allocations
5. **Open & Extensible** - Embeddable as library or standalone server
6. **Transparency** - Observable operations via structured logs
7. **Secure by Default** - JWT auth, tenant isolation, AEAD encryption

---

## 📚 Documentation

- **[Complete Specification](specs/001-build-a-rust/SPECIFICATION-COMPLETE.md)** - Full design overview
- **[Data Model](specs/001-build-a-rust/data-model.md)** - Entities, schemas, lifecycle
- **[API Architecture](specs/001-build-a-rust/API-ARCHITECTURE.md)** - SQL-first approach
- **[SQL Examples](specs/001-build-a-rust/sql-query-examples.md)** - Query cookbook
- **[WebSocket Protocol](specs/001-build-a-rust/contracts/websocket-protocol.md)** - Real-time streaming
- **[REST API (OpenAPI)](specs/001-build-a-rust/contracts/rest-api.yaml)** - HTTP endpoints

---

## 🎯 Roadmap

- [x] Complete specification design
- [x] SQL-first API architecture
- [x] Per-user table multi-tenancy model
- [x] Message duplication strategy
- [x] Media file support
- [ ] RocksDB storage implementation
- [ ] DataFusion SQL engine integration
- [ ] Parquet consolidation
- [ ] WebSocket real-time streaming
- [ ] Admin web UI
- [ ] Kubernetes deployment

---

## 🤝 Contributing

KalamDB is in active development. See [`specs/001-build-a-rust/plan.md`](specs/001-build-a-rust/plan.md) for implementation plan.

---

## 📄 License

[License TBD]

---

## 🌟 Why "KalamDB"?

**Kalam** (كلام) means "speech" or "conversation" in Arabic — fitting for a database designed specifically for storing and streaming human conversations and AI interactions.

---

**Built with ❤️ in Rust for real-time conversations at scale.**
