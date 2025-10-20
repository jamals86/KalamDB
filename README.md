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

### ✅ **Implemented Features**

#### 📦 **Three Table Types**
- **User Tables**: One table instance per user, isolated storage, perfect for personal data
- **Shared Tables**: Single table accessible to all users, ideal for global data
- **Stream Tables**: Ephemeral tables with TTL eviction, memory-only for real-time events

#### ⚡ **Sub-millisecond Writes**
RocksDB hot storage with <1ms write latency. Configurable flush policies (row-based, time-based, or combined).

#### 🔍 **SQL-First Interface**
Full SQL support via DataFusion engine:
- DDL: CREATE/DROP NAMESPACE, CREATE TABLE (USER/SHARED/STREAM), ALTER TABLE, BACKUP/RESTORE DATABASE
- DML: INSERT, UPDATE, DELETE, SELECT
- System tables: Query active subscriptions, jobs, storage locations, table metadata

#### 🔒 **User-Centric Data Ownership**
- Each user's data physically isolated in separate RocksDB column families and Parquet files
- User isolation enforced at storage layer (key prefix filtering)
- Easy per-user data export, backup, and migration
- Privacy and GDPR compliance by design

#### 🌐 **Live Query Subscriptions**
WebSocket-based real-time updates with:
- INSERT/UPDATE/DELETE change notifications with old and new values
- Filtered subscriptions with SQL WHERE clauses
- Initial data fetch with `last_rows` option
- User isolation (users only see their own data changes)
- Flush notifications when data moves to cold storage

#### 📊 **Schema Evolution**
- Add/drop/rename columns with ALTER TABLE
- Schema versioning (all versions stored in system_table_schemas)
- Backward compatibility (old Parquet files projected to current schema)
- Prevent breaking changes (dropping required columns, altering system columns)

#### 🛡️ **Backup and Restore**
- Namespace-level backup (all tables, schemas, metadata)
- Parquet file backup with checksum verification
- BACKUP DATABASE / RESTORE DATABASE SQL commands
- Job tracking in system.jobs table
- Soft-deleted rows preserved in backups

#### 📋 **Catalog Browsing**
- SHOW TABLES [IN namespace]
- DESCRIBE TABLE [namespace.]table_name (shows schema, flush policy, retention, storage location)
- SHOW STATS FOR TABLE (hot/cold row counts, storage bytes)
- information_schema.tables virtual table

### 🚧 **Planned Features**

#### 🔐 **Authentication & Authorization** (Phase 17)
- WebSocket JWT authentication
- User-based permissions
- Rate limiting (per user, per connection)

#### 📈 **Performance Optimization** (Phase 17)
- Connection pooling for RocksDB
- Schema caching in DataFusion
- Query result caching for system tables
- Parquet bloom filters on _updated column

#### 📝 **Observability** (Phase 17)
- Structured logging for all operations
- Metrics collection (query latency, flush duration, WebSocket throughput)
- Request/response logging with execution times

---

## 🏗️ Architecture Overview

### Three-Layer Architecture

KalamDB follows a clean **three-layer architecture** that ensures maintainability and testability:

```
┌──────────────────────────────────────────┐
│         kalamdb-core (Layer 1)          │  ← Business logic, services, SQL execution
│  - Table operations                      │
│  - Live query management                 │
│  - Flush/backup services                 │
│  - DataFusion integration                │
└───────────┬──────────────────────────────┘
            │ uses (no direct RocksDB access)
            ▼
┌──────────────────────────────────────────┐
│    kalamdb-sql + kalamdb-store (Layer 2)│  ← Data access layer
│                                          │
│  kalamdb-sql:                           │
│  - System tables (namespaces, tables,   │
│    schemas, storage_locations, jobs)    │
│  - Metadata operations                   │
│                                          │
│  kalamdb-store:                         │
│  - UserTableStore                        │
│  - SharedTableStore                      │
│  - StreamTableStore                      │
│  - Parquet file operations               │
└───────────┬──────────────────────────────┘
            │ uses
            ▼
┌──────────────────────────────────────────┐
│         RocksDB (Layer 3)               │  ← Persistence layer
│  - Column families                       │
│  - System tables storage                 │
│  - Hot data buffering                    │
└──────────────────────────────────────────┘
```

**Key Principle**: `kalamdb-core` **never** directly accesses RocksDB. All data operations flow through `kalamdb-sql` (for metadata) and `kalamdb-store` (for user/shared/stream data).

### RocksDB Column Family Architecture

All configuration and metadata is stored in **RocksDB system tables** (no JSON config files):

```
RocksDB Column Families:
├── system_namespaces           # Namespace definitions
├── system_tables               # Table metadata (name, type, flush policy, location)
├── system_table_schemas        # Schema versions per table
├── system_storage_locations    # Centralized storage location registry
├── system_jobs                 # Background job tracking (flush, backup, etc.)
├── system_live_queries         # Active WebSocket subscriptions
├── system_users                # User authentication and permissions
│
├── user_table:{table_name}     # Hot data buffer (per user table)
├── shared_table:{table_name}   # Hot data buffer (per shared table)
├── stream_table:{table_name}   # Ephemeral data buffer (per stream table)
└── user_table_counters         # Per-user row counts for flush triggers
```

### Storage Layout

```
/var/lib/kalamdb/
├── rocksdb/                             # RocksDB data directory
│   ├── system_namespaces/              # System table CFs
│   ├── system_tables/
│   ├── user_table:messages/            # User table hot buffers
│   └── shared_table:analytics/         # Shared table hot buffers
│
├── user/{user_id}/                      # Per-user Parquet storage
│   ├── messages/
│   │   ├── batch-20251020-001.parquet  # Flushed user data
│   │   └── batch-20251020-002.parquet
│   └── conversations/
│       └── batch-*.parquet
│
└── shared/{table_name}/                 # Shared table Parquet storage
    ├── batch-20251020-001.parquet      # Flushed shared data
    └── batch-20251020-002.parquet
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

### Installation

```bash
# Clone the repository
git clone https://github.com/jamals86/KalamDB.git
cd KalamDB/backend

# Build the server
cargo build --release --bin kalamdb-server

# Run the server (uses config.toml or defaults)
cargo run --release --bin kalamdb-server
```

See [Quick Start Guide](docs/QUICK_START.md) for detailed setup instructions.

### Basic Usage

#### 1. Create a Namespace and Table

```bash
curl -X POST http://localhost:8080/api/sql \
  -H "X-User-Id: user1" \
  -H "Content-Type: application/json" \
  -d '{
    "sql": "CREATE NAMESPACE IF NOT EXISTS app"
  }'

curl -X POST http://localhost:8080/api/sql \
  -H "X-User-Id: user1" \
  -H "Content-Type: application/json" \
  -d '{
    "sql": "CREATE USER TABLE app.messages (id BIGINT, content TEXT, timestamp TIMESTAMP) FLUSH POLICY ROW_LIMIT 1000"
  }'
```

#### 2. Insert and Query Data

```bash
# Insert data (goes to RocksDB hot buffer)
curl -X POST http://localhost:8080/api/sql \
  -H "X-User-Id: user1" \
  -H "Content-Type: application/json" \
  -d '{
    "sql": "INSERT INTO app.messages (id, content, timestamp) VALUES (1, '\''Hello World'\'', NOW())"
  }'

# Query data (reads from hot + cold storage)
curl -X POST http://localhost:8080/api/sql \
  -H "X-User-Id: user1" \
  -H "Content-Type: application/json" \
  -d '{
    "sql": "SELECT * FROM app.messages ORDER BY timestamp DESC LIMIT 10"
  }'
```

#### 3. Subscribe to Live Updates (WebSocket)

```javascript
const ws = new WebSocket('ws://localhost:8080/ws');

// Subscribe to live queries
ws.send(JSON.stringify({
  subscriptions: [{
    query_id: "sub1",
    sql: "SELECT * FROM app.messages WHERE timestamp > NOW() - INTERVAL '\''5 minutes'\''",
    options: { last_rows: 10 }
  }]
}));

// Receive real-time notifications
ws.onmessage = (event) => {
  const notification = JSON.parse(event.data);
  console.log('Change detected:', notification.type, notification.data);
  // notification.type: "INSERT" | "UPDATE" | "DELETE" | "FLUSH"
};
```

### Common SQL Commands

```sql
-- Namespace management
CREATE NAMESPACE app;
DROP NAMESPACE app;

-- User tables (one table per user)
CREATE USER TABLE app.messages (
  id BIGINT,
  content TEXT,
  created_at TIMESTAMP
) FLUSH POLICY ROW_LIMIT 1000;

-- Shared tables (global data)
CREATE SHARED TABLE app.analytics (
  event_name TEXT,
  count BIGINT,
  timestamp TIMESTAMP
) FLUSH POLICY TIME_INTERVAL 300;

-- Stream tables (ephemeral, memory-only)
CREATE STREAM TABLE app.events (
  event_id TEXT,
  data TEXT
) RETENTION 10 EPHEMERAL MAX_BUFFER 5000;

-- Schema evolution
ALTER TABLE app.messages ADD COLUMN author TEXT;
ALTER TABLE app.messages DROP COLUMN author;

-- Backup and restore
BACKUP DATABASE app TO '/backups/app-20251020';
RESTORE DATABASE app FROM '/backups/app-20251020';

-- Catalog browsing
SHOW TABLES IN app;
DESCRIBE TABLE app.messages;
SHOW STATS FOR TABLE app.messages;
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

### Getting Started

- **[🚀 Quick Start Guide](docs/QUICK_START.md)** - Get up and running in 10 minutes
- **[📘 Development Setup](docs/DEVELOPMENT_SETUP.md)** - Complete installation guide for Windows/macOS/Linux
- **[Backend README](backend/README.md)** - Project structure and development workflow

### Architecture & Design

- **[Complete Specification](specs/001-build-a-rust/SPECIFICATION-COMPLETE.md)** - Full design overview
- **[Data Model](specs/001-build-a-rust/data-model.md)** - Entities, schemas, lifecycle
- **[API Architecture](specs/001-build-a-rust/API-ARCHITECTURE.md)** - SQL-first approach
- **[SQL Examples](specs/001-build-a-rust/sql-query-examples.md)** - Query cookbook
- **[WebSocket Protocol](specs/001-build-a-rust/contracts/websocket-protocol.md)** - Real-time streaming
- **[REST API (OpenAPI)](specs/001-build-a-rust/contracts/rest-api.yaml)** - HTTP endpoints

### Development Guidelines

- **[Constitution](.specify/memory/constitution.md)** - Project principles and standards
- **[Implementation Plan](specs/001-build-a-rust/plan.md)** - Development roadmap

---

## 🎯 Roadmap

### ✅ Completed (Phase 1-16)
- [x] Complete specification design (002-simple-kalamdb)
- [x] Three-layer architecture (kalamdb-core → kalamdb-sql + kalamdb-store → RocksDB)
- [x] RocksDB storage implementation with column family architecture
- [x] DataFusion SQL engine integration
- [x] Three table types (User, Shared, Stream)
- [x] Schema evolution (ALTER TABLE with versioning)
- [x] Configurable flush policies (row-based, time-based, combined)
- [x] Parquet consolidation (hot → cold storage)
- [x] WebSocket live query subscriptions with change tracking
- [x] System tables (namespaces, tables, schemas, storage_locations, live_queries, jobs, users)
- [x] Backup and restore (namespace-level with Parquet file copy)
- [x] Catalog browsing (SHOW TABLES, DESCRIBE TABLE, SHOW STATS)
- [x] Integration tests and quickstart script (32 automated tests)

### 🚧 In Progress (Phase 17 - Polish)
- [ ] Enhanced error handling (✅ error types added, integration pending)
- [ ] Configuration improvements (✅ RocksDB settings, ✅ environment variables)
- [ ] Structured logging for all operations
- [ ] Request/response logging (REST API + WebSocket)
- [ ] Performance optimizations (connection pooling, schema cache, query cache)
- [ ] Parquet bloom filters on _updated column
- [ ] Metrics collection (query latency, flush duration, WebSocket throughput)
- [ ] WebSocket authentication and authorization
- [ ] Rate limiting (per user, per connection)
- [ ] Comprehensive documentation (✅ README updated, API docs, ADRs pending)

### 📅 Future (Post Phase 17)
- [ ] Distributed replication (system.nodes table, tag-based routing)
- [ ] Incremental backups
- [ ] Admin web UI
- [ ] Kubernetes deployment
- [ ] Cloud storage backends (S3, Azure Blob, GCS)

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
