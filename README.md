# KalamDB (IN DEVELOPMENT)

KalamDB is designed for speed, efficiency, and minimal resource use.
We aim to store and process data with the smallest possible footprint, reducing CPU, memory, storage, and token costs while improving performance.

## Our goal:
Faster operations. Lower infrastructure expenses. Zero waste.

[![Rust](https://img.shields.io/badge/rust-1.90%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](https://www.apache.org/licenses/LICENSE-2.0)
[![Tests](https://img.shields.io/badge/tests-32%20passing-brightgreen.svg)](backend/tests/)
[![Docker](https://img.shields.io/badge/docker-ready-blue.svg)](docker/README.md)

---

## ⚡ Quick Stats

| Metric | Value | Description |
|--------|-------|-------------|
| **Write Latency** | <1ms | RocksDB hot tier with sub-millisecond writes |
| **Concurrent Users** | Millions | Per-user table isolation enables horizontal scaling |
| **Storage Tiers** | 2 | Hot (RocksDB) + Cold (Parquet) with automatic flushing |
| **Table Types** | 3 | USER (isolated), SHARED (global), STREAM (ephemeral) |
| **Data Types** | 16 | BOOLEAN, INT, BIGINT, DOUBLE, FLOAT, TEXT, TIMESTAMP, DATE, DATETIME, TIME, JSON, BYTES, EMBEDDING, UUID, DECIMAL, SMALLINT |
| **Schema Cache** | >99% | DashMap-based concurrent schema caching with sub-100μs lookups |
| **SQL Engine** | DataFusion 40.0 | Full SQL support with 100% standard compatibility |
| **Storage Backends** | 1 | Local Filesystem (S3/Azure/GCS planned) |
| **Real-Time** | WebSocket | Live query subscriptions with change tracking |

---

## 🎯 At a Glance

KalamDB is a **SQL-first, real-time database** that scales to millions of concurrent users through a revolutionary **table-per-user architecture**. Built in Rust with Apache Arrow and DataFusion, it combines the familiarity of SQL with the performance needed for modern chat applications and AI assistants.

```sql
-- Create a user table with real-time subscriptions
CREATE USER TABLE app.messages (
  id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
  content TEXT NOT NULL,
  timestamp TIMESTAMP DEFAULT NOW()
) FLUSH ROW_THRESHOLD 1000;

-- Subscribe to live updates (WebSocket)
SUBSCRIBE TO app.messages 
WHERE timestamp > NOW() - INTERVAL '5 minutes' 
OPTIONS (last_rows=10);
```

**Result**: Each user gets their own isolated table instance, enabling millions of concurrent real-time subscriptions with sub-millisecond write latency.

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

## ⚡ Main Features

### **SQL-First Interface with Full DDL/DML Support**
- **Complete SQL Engine**: Built on Apache DataFusion with 100% standard SQL compatibility
- **Custom Extensions**: Namespace management, multi-table types, live query subscriptions
- **Advanced Functions**: Auto-generated IDs (SNOWFLAKE_ID, UUID_V7, ULID), session context (CURRENT_USER)
- **Full Documentation**: [Complete SQL Syntax Reference](docs/architecture/SQL_SYNTAX.md)

```sql
-- Standard SQL with KalamDB extensions
CREATE NAMESPACE app;
CREATE USER TABLE app.conversations (
  id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
  user_id TEXT DEFAULT CURRENT_USER(),
  message TEXT NOT NULL,
  created_at TIMESTAMP DEFAULT NOW()
) FLUSH ROW_THRESHOLD 1000;

SELECT * FROM app.conversations 
WHERE created_at > NOW() - INTERVAL '1 hour' 
ORDER BY id DESC;
```

### **Three Table Types for Different Use Cases**

#### 1. **USER Tables** - Per-User Isolation
- One table instance per user with separate storage
- Perfect for personal data, chat history, user preferences
- Scales horizontally to millions of users

```sql
CREATE USER TABLE app.messages (
  id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
  content TEXT NOT NULL
) FLUSH ROW_THRESHOLD 1000;
-- alice's data → user/alice123/messages/batch-<index>.parquet
-- bob's data   → user/bob456/messages/batch-<index>.parquet
```

#### 2. **SHARED Tables** - Global Data
- Single table accessible to all users
- Ideal for configuration, analytics, reference data
- Centralized storage with shared access

```sql
CREATE SHARED TABLE app.config (
  key TEXT PRIMARY KEY,
  value JSON
) FLUSH ROW_THRESHOLD 100;
-- All users read/write → shared/app/config/batch-<index>.parquet
```

#### 3. **STREAM Tables** - Ephemeral Real-Time
- Memory-only tables with TTL-based eviction
- Perfect for live events, notifications, sensor data
- No persistence, maximum performance

```sql
CREATE STREAM TABLE app.live_events (
  id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
  payload JSON,
  created_at TIMESTAMP DEFAULT NOW()
) TTL 10;
-- Data expires after 10 seconds, memory-only
```

### **Sub-Millisecond Writes with Two-Tier Storage**
- **Hot Tier (RocksDB)**: <1ms write latency for recent data
- **Cold Tier (Parquet)**: Compressed columnar storage for historical data
- **Automatic Flushing**: Configurable row-based, time-based, or combined policies
- **Seamless Queries**: DataFusion transparently queries both tiers

```sql
-- Configure flush policy
CREATE USER TABLE app.logs (...)
FLUSH INTERVAL 300s ROW_THRESHOLD 5000;  -- Flush every 5min OR 5000 rows
```

### **Real-Time Live Query Subscriptions**
- **WebSocket-Based**: Instant change notifications (INSERT/UPDATE/DELETE)
- **SQL Filtering**: Subscribe to specific data with WHERE clauses
- **Initial Data**: Load last N rows when subscribing
- **Automatic Cleanup**: Connection-based lifecycle management

```sql
-- Subscribe to filtered real-time updates
SUBSCRIBE TO chat.messages 
WHERE room_id = 'general'
OPTIONS (last_rows=10);
```

**WebSocket Response**:
```json
{
  "type": "INSERT",
  "query_id": "chat-subscription",
  "data": {"id": 123, "content": "New message", "timestamp": "2025-10-28T10:00:00Z"}
}
```

### **User-Centric Data Ownership & Privacy**
- **Physical Isolation**: Each user's data in separate RocksDB column families and Parquet files
- **Easy Export**: Per-user data backup as simple file copy
- **GDPR Compliance**: Delete user = delete their storage directory
- **Soft Deletes**: 7-day retention for recovery (configurable)

### **Multi-Storage Backend Support**
- **Local Filesystem**: Default, zero-config
- **Amazon S3**: S3-compatible object storage (AWS, MinIO, etc.)
- **Azure Blob**: Microsoft Azure cloud storage
- **Google Cloud Storage**: GCS object storage
- **Template Paths**: Dynamic storage paths with variables

```sql
CREATE STORAGE s3_prod
TYPE s3
PATH 's3://my-bucket/kalamdb/${namespace}/${user_id}'
CREDENTIALS '{"access_key_id": "...", "secret_access_key": "..."}';

CREATE USER TABLE app.messages (...) STORAGE s3_prod;
```

### **Role-Based Access Control (RBAC)**
- **Four Roles**: `user` (default), `service` (automation), `dba` (admin), `system` (internal)
- **Password Authentication**: bcrypt hashing with configurable cost
- **OAuth Integration**: Google, GitHub, Azure providers
- **HTTP Basic Auth & JWT**: Secure authentication for all API requests
- **Soft Delete Users**: 30-day grace period for recovery

```sql
-- Create users with different roles
CREATE USER 'alice' WITH PASSWORD 'Secret123!' ROLE 'user';
CREATE USER 'backup_service' WITH PASSWORD 'Key456!' ROLE 'service';
CREATE USER 'db_admin' WITH PASSWORD 'Admin789!' ROLE 'dba' EMAIL 'admin@company.com';

-- OAuth-based user
CREATE USER 'google_sync' WITH OAUTH PROVIDER 'google' SUBJECT 'sync@example.com' ROLE 'service';
```

### **Unified Schema System with Performance Optimization**
- **Single Source of Truth**: All schema definitions consolidated in `kalamdb-commons` for consistency
- **Type-Safe Models**: 16 data types with wire format encoding (BOOLEAN, INT, BIGINT, DOUBLE, FLOAT, TEXT, TIMESTAMP, DATE, DATETIME, TIME, JSON, BYTES, EMBEDDING, UUID, DECIMAL, SMALLINT)
- **Schema Caching**: >99% cache hit rate with DashMap-based concurrent access
- **Arrow Conversion**: Lossless bidirectional conversion between KalamDB and Apache Arrow types
- **Column Ordering**: Deterministic SELECT * results via ordinal_position tracking
- **Schema Versioning**: All versions stored, queryable via `DESCRIBE TABLE HISTORY`
- **ADD/DROP/RENAME Columns**: Live schema changes with backward compatibility
- **Automatic Projection**: Old Parquet files adapt to new schema
- **Breaking Change Prevention**: Cannot drop required columns

```sql
-- Create table with modern data types
CREATE USER TABLE app.messages (
  id BIGINT DEFAULT SNOWFLAKE_ID(),
  user_id UUID DEFAULT UUID_V7(),
  content TEXT,
  price DECIMAL(10, 2),
  priority SMALLINT,
  embedding EMBEDDING(384),  -- Vector embeddings for AI/ML
  created_at TIMESTAMP DEFAULT NOW()
);

-- Evolve schema safely
ALTER TABLE app.messages ADD COLUMN reaction TEXT;
ALTER TABLE app.messages RENAME COLUMN content TO message_text;

-- View schema history
DESCRIBE TABLE app.messages HISTORY;
```

### **Comprehensive Catalog & Introspection**
- **System Tables**: Query metadata via SQL
- **SHOW Commands**: List namespaces, tables, storage, users
- **DESCRIBE**: View table schemas and configuration
- **STATS**: Hot/cold row counts, storage bytes

```sql
SHOW TABLES IN app;
DESCRIBE TABLE app.messages;
SHOW STATS FOR TABLE app.messages;
SELECT * FROM system.users WHERE role = 'dba';
```

### **Production-Ready Deployment**
- **Docker Support**: Multi-stage builds, non-root user, health checks
- **Environment Variables**: Override all config settings
- **CLI Tool**: Interactive `kalam` client with auto-completion
- **Integration Tests**: Comprehensive test suite

---

## 📊 Architecture Overview

KalamDB stores data in a simple, inspectable layout. Each folder now contains a small `manifest.json` alongside the data files.

```text
data/
├── rocksdb/                         # Hot storage (RocksDB column families)
│   ├── system_*                     # System tables
│   └── user_* / shared_*            # Hot buffers per table
└── storage/                         # Cold storage (Parquet segments)
    ├── user/{user_id}/{table}/
    │   ├── manifest.json            # Schema + segment index
    │   └── batch-<index>.parquet    # Flushed segments
    └── shared/{table}/
  ├── manifest.json
  └── batch-<index>.parquet
```

High level crate graph today:

```text
        +----------------+
        |  kalamdb-api   |   HTTP + WebSocket server
        +--------+-------+
           |
           v
        +----------------+
        |  kalamdb-core  |   SQL handlers, jobs, tables
        +--------+-------+
           |
    +------------+-------------+
    v                          v
  +---------------+          +-----------------+
  | kalamdb-store |          | kalamdb-filestore|  (Parquet + manifests)
  +-------+-------+          +---------+-------+
    |                             |
    v                             v
   RocksDB column families         Filesystem / object storage
```

`kalamdb-core` orchestrates everything and never talks to RocksDB or the filesystem directly; it goes through `kalamdb-store` (key/value hot path) and `kalamdb-filestore` (Parquet + `manifest.json` and batch indexes).

## 🌟 **KalamDB Core Features & Roadmap**

### ✅ **Implemented**

- SQL engine with full DDL/DML support
- Three table types: USER, SHARED, STREAM
- Per-user tables with hot (RocksDB) + cold (Parquet) storage
- Real-time subscriptions over WebSocket
- Unified schema system with 16 data types (incl. EMBEDDING)
- Role-based access control and authentication
- Backup/restore and system catalog tables
- Dockerized server and `kalam` CLI

---

### 🚧 **In Progress**

- SDK for TypeScript using WASM
- Performance tuning and metrics
- Stronger WebSocket auth and rate limiting
- Cleanup and simplification of docs and examples
- Support for more storage backends (Azure, GCS, S3-compatible)

---

### 📋 **Planned / Future**
- Run workflows on data changes (triggers)
- High-availability and replication
- Richer search (full-text, vector)
- Query caching and more indexes
- Transactions and constraints
- Admin UI and better cloud/Kubernetes story
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

### Basic Usage – Real-World Chat + AI Example (with `kalam` CLI)

Below is a minimal but realistic end-to-end example for a **chat app with AI**. It uses:

- One **user table** for conversations
- One **user table** for messages
- One **stream table** for ephemeral typing/thinking/cancel events

We assume:

- The server is running at `http://localhost:8080`
- You're running on localhost (automatically connects as `root` user)
- You have the CLI built and available as `kalam` (see `docs/CLI.md`)

#### 1. Start Interactive CLI and Create Schema

```bash
# Start the interactive CLI (connects as root on localhost by default)
kalam
```

Now inside the `kalam>` prompt:

```sql
-- Create namespace and tables
CREATE NAMESPACE IF NOT EXISTS chat;

CREATE USER TABLE chat.conversations (
  id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
  title TEXT NOT NULL,
  created_at TIMESTAMP DEFAULT NOW()
) FLUSH ROW_THRESHOLD 1000;

CREATE USER TABLE chat.messages (
  id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
  conversation_id BIGINT NOT NULL,
  role TEXT NOT NULL,
  content TEXT NOT NULL,
  created_at TIMESTAMP DEFAULT NOW()
) FLUSH ROW_THRESHOLD 1000;

CREATE STREAM TABLE chat.typing_events (
  id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
  conversation_id BIGINT NOT NULL,
  user_id TEXT NOT NULL,
  event_type TEXT NOT NULL,
  created_at TIMESTAMP DEFAULT NOW()
) TTL 30;
```

#### 2. Start a Conversation and Add Messages

```sql
-- Create a new conversation and get its id
INSERT INTO chat.conversations (title)
VALUES ('Chat with AI')
RETURNING id;

-- Suppose the returned id is 1 – insert user + AI messages
INSERT INTO chat.messages (conversation_id, role, content) VALUES
  (1, 'user', 'Hello, AI!'),
  (1, 'assistant', 'Hi! How can I help you today?');

-- Query the conversation history
SELECT role, content, created_at
FROM chat.messages
WHERE conversation_id = 1
ORDER BY created_at ASC;
```

#### 3. Track Typing/Thinking/Cancel Events (Stream Table)

```sql
-- User starts typing
INSERT INTO chat.typing_events (conversation_id, user_id, event_type)
VALUES (1, 'user_123', 'typing');

-- AI starts thinking
INSERT INTO chat.typing_events (conversation_id, user_id, event_type)
VALUES (1, 'ai_model', 'thinking');

-- AI cancels / stops
INSERT INTO chat.typing_events (conversation_id, user_id, event_type)
VALUES (1, 'ai_model', 'cancelled');

-- Subscribe to live typing events
SUBSCRIBE TO chat.typing_events WHERE conversation_id = 1 OPTIONS (last_rows=50);
```

**Note**: Press `Ctrl+C` to stop the subscription and return to the prompt.

#### 4. Subscribe to Live Message + Typing Updates (WebSocket API)

```javascript
const ws = new WebSocket('ws://localhost:8080/ws');

ws.onopen = () => {
  ws.send(JSON.stringify({
    subscriptions: [
      {
        query_id: "chat-messages-1",
        sql: "SELECT * FROM chat.messages WHERE conversation_id = 1 ORDER BY created_at DESC",
        options: { last_rows: 20 }
      },
      {
        query_id: "chat-typing-1",
        sql: "SELECT * FROM chat.typing_events WHERE conversation_id = 1",
        options: { last_rows: 10 }
      }
    ]
  }));
};

ws.onmessage = (event) => {
  const notification = JSON.parse(event.data);
  console.log('Change detected:', notification.query_id, notification.type, notification.data);
};
```

**📖 Complete SQL Reference**: See [SQL Syntax Documentation](docs/SQL.md) for the full command reference with all options.

---

## � Use Cases

### 1. **Chat Applications at Scale**
**Challenge**: Traditional databases struggle with millions of concurrent users each needing real-time message updates.

**KalamDB Solution**:
- Per-user table isolation means 1 million users = 1 million independent WebSocket subscriptions
- No global table locks or complex WHERE filtering
- Sub-millisecond writes to RocksDB hot tier
- Automatic message history archival to Parquet cold tier

---

### 2. **AI Assistant Platforms**
**Challenge**: Store millions of user ↔ AI conversations with complete context for RAG (Retrieval-Augmented Generation).

**KalamDB Solution**:
- User tables store conversation history with perfect isolation
- Full-text search via DataFusion SQL
- Time-range queries for recent context
- Schema evolution as AI models change

---

### 3. **Collaborative Editing Tools**
**Challenge**: Multiple users editing shared documents with real-time synchronization and conflict resolution.

**KalamDB Solution**:
- Shared tables for document content
- Stream tables for ephemeral cursor positions and presence
- Live query subscriptions for real-time collaboration
- User tables for per-user edit history

**Example**:
```sql
-- Shared document storage
CREATE SHARED TABLE docs.content (
  doc_id TEXT,
  version INT,
  content TEXT,
  author TEXT DEFAULT CURRENT_USER(),
  updated_at TIMESTAMP DEFAULT NOW()
) FLUSH INTERVAL 60s;

-- Ephemeral presence tracking
CREATE STREAM TABLE docs.presence (
  doc_id TEXT,
  user_id TEXT,
  cursor_position INT,
  last_seen TIMESTAMP DEFAULT NOW()
) TTL 5;  -- Auto-evict after 5 seconds

-- Subscribe to document changes
SUBSCRIBE TO docs.content 
WHERE doc_id = 'project-proposal' 
OPTIONS (last_rows=1);
```

**Result**: Google Docs-style real-time collaboration with sub-second latency.

---

### 4. **IoT Sensor Data & Monitoring**
**Challenge**: Ingest millions of sensor readings per second with time-series analytics and real-time alerts.

**KalamDB Solution**:
- Stream tables for ephemeral sensor data with TTL eviction
- Shared tables for aggregated metrics and alerts
- Live subscriptions for anomaly detection
- Automatic cold tier archival for historical analysis

**Example**:
```sql
-- Ephemeral sensor readings (10-second retention)
CREATE STREAM TABLE iot.sensor_data (
  sensor_id TEXT,
  temperature DOUBLE,
  humidity DOUBLE,
  timestamp TIMESTAMP DEFAULT NOW()
) TTL 10;

-- Aggregated metrics (persisted)
CREATE SHARED TABLE iot.metrics (
  sensor_id TEXT,
  avg_temp DOUBLE,
  max_temp DOUBLE,
  min_temp DOUBLE,
  hour TIMESTAMP
) FLUSH INTERVAL 3600s;  -- Flush every hour

-- Real-time alert subscription
SUBSCRIBE TO iot.sensor_data 
WHERE temperature > 80.0 OR humidity > 95.0;
```

**Result**: Prometheus-style monitoring with SQL queries and real-time alerting.

---

### 5. **Compliance & Privacy (GDPR, CCPA)**
**Challenge**: Provide complete data export and deletion for user privacy regulations.

**KalamDB Solution**:
- Per-user storage enables trivial data export (copy directory)
- Soft delete with configurable grace period
- Physical data isolation prevents cross-user leakage
- Audit trails with `CURRENT_USER()` tracking

**Example**:
```sql
-- Create user with audit trail
CREATE USER TABLE app.user_data (
  id BIGINT DEFAULT SNOWFLAKE_ID(),
  data_type TEXT,
  content TEXT,
  created_by TEXT DEFAULT CURRENT_USER(),
  created_at TIMESTAMP DEFAULT NOW()
) FLUSH INTERVAL 300s;

-- Export user data (simple file copy)
-- cp -r /var/lib/kalamdb/user/alice123/ /exports/alice-gdpr-export/

-- Delete user (soft delete with 30-day grace period)
DROP USER 'alice';

-- Hard delete after grace period (automatic cleanup)
-- Scheduled job removes /var/lib/kalamdb/user/alice123/
```

**Result**: GDPR-compliant data management with minimal engineering effort.

---

### 6. **Multi-Tenant SaaS Applications**
**Challenge**: Isolate customer data while sharing infrastructure efficiently.

**KalamDB Solution**:
- User tables provide tenant-level isolation
- Shared tables for cross-tenant analytics
- Per-tenant backup and restore
- Role-based access control (RBAC)

**Example**:
```sql
-- Tenant-isolated data
CREATE USER TABLE saas.customer_data (
  id BIGINT DEFAULT SNOWFLAKE_ID(),
  customer_id TEXT DEFAULT CURRENT_USER(),
  entity_type TEXT,
  entity_data TEXT,
  created_at TIMESTAMP DEFAULT NOW()
) FLUSH INTERVAL 600s ROW_THRESHOLD 10000;

-- Cross-tenant analytics (aggregated)
CREATE SHARED TABLE saas.analytics (
  metric_name TEXT,
  metric_value DOUBLE,
  tenant_id TEXT,
  timestamp TIMESTAMP DEFAULT NOW()
) FLUSH INTERVAL 3600s;

-- Create service account per tenant
CREATE USER 'tenant_acme' WITH PASSWORD 'SecureKey123!' ROLE 'service';
```

**Result**: Salesforce-style multi-tenancy with SQL simplicity.

---

## 🛠️ Technology Stack

| Component | Technology | Version | Purpose |
|-----------|-----------|---------|---------|
| **Language** | Rust | 1.90+ | Performance, safety, concurrency |
| **Storage (Hot)** | RocksDB | 0.24 | Fast buffered writes (<1ms latency) |
| **Storage (Cold)** | Apache Parquet | 52.0 | Compressed columnar format for analytics |
| **Query Engine** | Apache DataFusion | 40.0 | SQL execution across hot+cold storage |
| **In-Memory** | Apache Arrow | 52.0 | Zero-copy data structures |
| **API Server** | Actix-web | 4.4 | REST endpoints + WebSocket subscriptions |
| **Authentication** | bcrypt + JWT | - | Password hashing + token-based auth |
| **Real-time** | WebSocket | - | Live message notifications |
| **Deployment** | Docker | - | Production-ready containerization |
| **Client SDK** | Rust → WASM | coming soon | TypeScript/JavaScript bindings |

**Why These Choices?**

- **Rust**: Memory safety without garbage collection, fearless concurrency
- **RocksDB**: Proven LSM-tree storage (powers Facebook, LinkedIn, Netflix)
- **Parquet**: Industry-standard columnar format (used by AWS Athena, Google BigQuery)
- **DataFusion**: High-performance SQL engine (10x faster than traditional databases for analytics)
- **Arrow**: Zero-copy data exchange between components
- **Actix-web**: One of the fastest web frameworks in any language

---

## 📐 Design Principles

1. **Simplicity First** - Direct code paths, minimal abstractions
2. **Performance by Design** - Sub-millisecond writes, SQL queries
3. **Data Ownership** - User-centric partitions, isolated storage, per-user tables
4. **Zero-Copy Efficiency** - Arrow IPC, Parquet, minimal allocations
5. **Open & Extensible** - Embeddable as library or standalone server
6. **Transparency** - Observable operations via structured logs
7. **Secure by Default** - JWT auth, tenant isolation, AEAD encryption

---

## 📚 Documentation

### 🚀 Getting Started

- **[Quick Start Guide](docs/quickstart/QUICK_START.md)** - Get up and running in 10 minutes

### 📖 SQL, API & CLI

- **[SQL Reference](docs/SQL.md)** – SQL syntax and examples
- **[API Reference](docs/API.md)** – HTTP & WebSocket API overview
- **[CLI Guide](docs/cli.md)** – using the `kalam` command-line client
 - **[SDK (TypeScript/WASM)](docs/SDK.md)** – browser/Node.js client (under development)

---

## 🐳 Deployment Options

### Docker Deployment (Production-Ready)

Run KalamDB in a Docker container with persistent storage and environment variable configuration:

```bash
# Build the Docker image
cd docker/backend
./build-backend.sh

# Start with docker-compose
docker-compose up -d

# Create a user with password
docker exec -it kalamdb kalam user create --name "myuser" --password "SecurePass123!" --role "user"
```

Features:
- ✅ Multi-stage build for minimal image size
- ✅ Non-root user for security
- ✅ Persistent data volumes
- ✅ Environment variable override for all config
- ✅ Health check included
- ✅ Both `kalamdb-server` and `kalam` CLI in container

**Documentation**: See [docker/README.md](docker/README.md) for complete deployment guide.

### WASM Client / SDK (Coming Soon)

TypeScript/JavaScript SDK built on Rust → WASM is under development.
See `docs/SDK.md` for the current design and status.

### Example Application: React TODO App

See a complete real-world example of KalamDB in action:

```bash
cd examples/simple-typescript
./setup.sh           # Create database tables
npm install          # Install dependencies
npm run dev          # Start development server
```

Features:
- ✅ Real-time sync across browser tabs
- ✅ Offline-first with localStorage caching
- ✅ Professional UI with dark mode support
- ✅ < 500 lines of code
- ✅ Complete with tests and documentation

**Live Demo**: See [examples/simple-typescript/README.md](examples/simple-typescript/README.md) for complete walkthrough.

---

## 📄 License

Apache 2.0 License

---

## 🌟 Why "KalamDB"?

**Kalam** (كلام) means "speech" or "conversation" in Arabic — fitting for a database designed specifically for storing and streaming human conversations and AI interactions.

---

**Built with ❤️ in Rust for real-time conversations at scale.**
