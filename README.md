# KalamDB (IN DEVELOPMENT)

KalamDB is designed for speed, efficiency, and minimal resource use.
We aim to store and process data with the smallest possible footprint, reducing CPU, memory, storage, and token costs while improving performance.

## Our goal:
Faster operations. Lower infrastructure expenses. Zero waste.

[![Rust](https://img.shields.io/badge/rust-1.92%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](https://www.apache.org/licenses/LICENSE-2.0)
[![CI](https://github.com/jamals86/KalamDB/actions/workflows/ci.yml/badge.svg)](https://github.com/jamals86/KalamDB/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/jamals86/KalamDB?display_name=tag)](https://github.com/jamals86/KalamDB/releases)
[![Docker](https://img.shields.io/docker/v/jamals86/kalamdb?sort=semver)](https://hub.docker.com/r/jamals86/kalamdb)
[![Docker Pulls](https://img.shields.io/docker/pulls/jamals86/kalamdb)](https://hub.docker.com/r/jamals86/kalamdb)

**Latest release:** v0.2.0-alpha1

---

## 🎯 At a Glance

KalamDB is a **SQL-first, real-time database** that scales to millions of concurrent users through a revolutionary **table-per-user architecture**. Built in Rust with Apache Arrow and DataFusion, it combines the familiarity of SQL with the performance needed for modern chat applications and AI assistants.

### 🐳 Quick Start with Docker

Get KalamDB running in seconds:

Single node:

```bash
KALAMDB_JWT_SECRET="$(openssl rand -base64 32)" \
curl -sSL https://raw.githubusercontent.com/jamals86/KalamDB/main/docker/run/single/docker-compose.yml | docker-compose -f - up -d
```

3-node cluster:

```bash
KALAMDB_JWT_SECRET="$(openssl rand -base64 32)" \
curl -sSL https://raw.githubusercontent.com/jamals86/KalamDB/main/docker/run/cluster/docker-compose.yml | docker-compose -f - up -d
```

---

## 🚀 What Makes KalamDB Different?

* ⚡ **Sub‑millisecond writes** using RocksDB hot tier
* 📡 **Live SQL subscriptions** over WebSockets
* 🧍‍♂️➡️🧍‍♀️ **Per‑user isolation** — each user gets their own table & storage
* 💾 **Cold tier (Parquet)** optimized for analytics and long‑term storage
* 🌍 **Multiple storage backends**: Local, S3, Azure, GCS

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
✅ Scales to millions of concurrent users
✅ Storage isolation for privacy, compliance, security & cost
```
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

`kalamdb-core` orchestrates everything and never talks to RocksDB or the filesystem directly; it goes through `kalamdb-store` (key/value hot path) and `kalamdb-filestore` (Parquet + `manifest.json` and batch indexes).

## 🧩 Clustering & High Availability

KalamDB includes built-in clustering with **high availability** using **Raft consensus**. The cluster coordinates metadata and data placement while ensuring durability and failover when nodes go down.

- **Raft-based consensus** for leader election and log replication
- **Multi‑Raft groups** for sharding and scaling out
- **Automatic failover** to keep the cluster available
- **Snapshots and backups** for recovery and fast restarts

## 🌟 **KalamDB Core Features & Roadmap**

### ✅ **Implemented**

- SQL engine with full DDL/DML support
- Three table types: USER, SHARED, STREAM
- Per-user tables with hot (RocksDB) + cold (Parquet) storage
- Real-time subscriptions over WebSocket
- Unified schema system with 16 data types (incl. EMBEDDING)
- Role-based access control and authentication
- `kalam` CLI tool
- High-Availability with Raft consensus and multi-raft groups with sharding
- Cluster management (snapshotting, backups)
- Admin UI with SQL Studio
- SDK for TypeScript using WASM
- Support for more storage backends (Azure, GCS, S3-compatible) using ObjectStore 

---

### 🚧 **In Progress**
- Indexes for both cold/hot storages
- Backup/restore and system catalog tables
- Stronger WebSocket auth and rate limiting
- Cleanup and simplification of docs and examples
- Query caching and more indexes

---

### 📋 **Planned / Future**
- Run workflows on data changes (triggers)
- File storage and BLOB support
- Richer search (full-text, vector embeddings as DataType)
- Connectors to external data sources (Flink, Kafka, etc)
- Transactions and constraints
---

## 🚀 Quick Start

### Installation

```bash
# Clone the repository
git clone https://github.com/jamals86/KalamDB.git
cd KalamDB/backend

# Run the server (uses config.toml or defaults)
cargo run --release --bin kalamdb-server
```

See [Quick Start Guide](docs/getting-started/quick-start.md) for detailed setup instructions.

### Basic Usage – Real-World Chat + AI Example (with `kalam` CLI)

Below is a minimal but realistic end-to-end example for a **chat app with AI**. It uses:

- One **user table** for conversations
- One **user table** for messages
- One **stream table** for ephemeral typing/thinking/cancel events

We assume:

- The server is running at `http://localhost:8080`
- You're running on localhost (automatically connects as `root` user)
- You have the CLI built and available as `kalam` (see `docs/getting-started/cli.md`)

#### 1. Start Interactive CLI and Create Schema

```bash
# Start the interactive CLI (connects as root on localhost by default)
kalam
```

Now inside the `kalam>` prompt:

```sql
-- Create namespace and tables
CREATE NAMESPACE IF NOT EXISTS chat;

CREATE TABLE chat.conversations (
  id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
  title TEXT NOT NULL,
  created_at TIMESTAMP DEFAULT NOW()
) WITH (TYPE = 'USER', FLUSH_POLICY = 'rows:1000');

CREATE TABLE chat.messages (
  id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
  conversation_id BIGINT NOT NULL,
  role_id TEXT NOT NULL,
  content TEXT NOT NULL,
  created_at TIMESTAMP DEFAULT NOW()
) WITH (TYPE = 'USER', FLUSH_POLICY = 'rows:1000');

CREATE TABLE chat.typing_events (
  id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
  conversation_id BIGINT NOT NULL,
  user_id TEXT NOT NULL,
  event_type TEXT NOT NULL,
  created_at TIMESTAMP DEFAULT NOW()
) WITH (TYPE = 'STREAM', TTL_SECONDS = 30);
```

#### 2. Start a Conversation and Add Messages

```sql
-- Create a new conversation and get its id
INSERT INTO chat.conversations (id, title) VALUES (1, 'Chat with AI About KalamDB');

-- Suppose the returned id is 1 – insert user + AI messages
INSERT INTO chat.messages (conversation_id, role_id, content) VALUES
  (1, 'user', 'Hello, AI!'),
  (1, 'assistant', 'Hi! How can I help you today?');

-- Query the conversation history
SELECT id, role_id, content, created_at
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
\subscribe SELECT * FROM chat.typing_events WHERE conversation_id = 1 OPTIONS (last_rows=50);

-- You can also subscribe to live messages
\subscribe SELECT * FROM chat.messages WHERE conversation_id = 1 OPTIONS (last_rows=20);
```

**Note**: Press `Ctrl+C` to stop the subscription and return to the prompt.

#### 4. Subscribe to Live Message + Typing Updates (TypeScript SDK)

The recommended way to subscribe to real-time updates is using the official TypeScript SDK:

```typescript
import { createClient, Auth } from 'kalam-link';

// Connect to KalamDB
const client = createClient({
  url: 'http://localhost:8080',
  auth: Auth.basic('admin', 'admin')
});
await client.connect();

// Subscribe to messages for a specific conversation with options
const unsubMessages = await client.subscribeWithSql(
  'SELECT * FROM chat.messages WHERE conversation_id = 1 ORDER BY created_at DESC',
  (event) => {
    if (event.type === 'change') {
      console.log('New message:', event.rows);
    }
  },
  { batch_size: 50 }  // Load initial data in batches of 50
);

// Subscribe to typing events (simple table subscription)
const unsubTyping = await client.subscribe('chat.typing_events', (event) => {
  if (event.type === 'change') {
    console.log('Typing event:', event.change_type, event.rows);
  }
});

// Check active subscriptions
console.log(`Active subscriptions: ${client.getSubscriptionCount()}`);

// Later: cleanup
await unsubMessages();
await unsubTyping();
await client.disconnect();
```

> **Note**: You can also connect directly via WebSocket at `ws://localhost:8080/v1/ws` for custom implementations. See [SDK Documentation](docs/sdk/sdk.md) for the full API reference and [API Documentation](docs/api/api.md) for raw WebSocket protocol details.

**📖 Complete SQL Reference**: See [SQL Syntax Documentation](docs/reference/sql.md) for the full command reference with all options.

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

### 2. **Collaborative Editing Tools**
**Challenge**: Multiple users editing shared documents with real-time synchronization and conflict resolution.

**KalamDB Solution**:
- Shared tables for document content
- Stream tables for ephemeral cursor positions and presence
- Live query subscriptions for real-time collaboration
- User tables for per-user edit history

**Example**:
```sql
-- Shared document storage
CREATE TABLE docs.content (
  doc_id TEXT PRIMARY KEY,
  version INT,
  content TEXT,
  author TEXT DEFAULT CURRENT_USER(),
  updated_at TIMESTAMP DEFAULT NOW()
) WITH (TYPE = 'SHARED', FLUSH_POLICY = 'interval:60');

-- Ephemeral presence tracking
CREATE TABLE docs.presence (
  doc_id TEXT PRIMARY KEY,
  user_id TEXT,
  cursor_position INT,
  last_seen TIMESTAMP DEFAULT NOW()
) WITH (TYPE = 'STREAM', TTL_SECONDS = 5);  -- Auto-evict after 5 seconds

-- Subscribe to document changes
SUBSCRIBE TO docs.content 
WHERE doc_id = 'project-proposal' 
OPTIONS (last_rows=1);
```

**Result**: Google Docs-style real-time collaboration with sub-second latency.

---

### 3. **IoT Sensor Data & Monitoring**
**Challenge**: Ingest millions of sensor readings per second with time-series analytics and real-time alerts.

**KalamDB Solution**:
- Stream tables for ephemeral sensor data with TTL eviction
- Shared tables for aggregated metrics and alerts
- Live subscriptions for anomaly detection
- Automatic cold tier archival for historical analysis

**Example**:
```sql
-- Ephemeral sensor readings (10-second retention)
CREATE TABLE iot.sensor_data (
  sensor_id TEXT PRIMARY KEY,
  temperature DOUBLE,
  humidity DOUBLE,
  timestamp TIMESTAMP DEFAULT NOW()
) WITH (TYPE = 'STREAM', TTL_SECONDS = 10);

-- Aggregated metrics (persisted)
CREATE TABLE iot.metrics (
  sensor_id TEXT PRIMARY KEY,
  avg_temp DOUBLE,
  max_temp DOUBLE,
  min_temp DOUBLE,
  hour TIMESTAMP
) WITH (TYPE = 'SHARED', FLUSH_POLICY = 'interval:3600');  -- Flush every hour

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
CREATE TABLE app.user_data (
  id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
  data_type TEXT,
  content TEXT,
  created_by TEXT DEFAULT CURRENT_USER(),
  created_at TIMESTAMP DEFAULT NOW()
) WITH (TYPE = 'USER', FLUSH_POLICY = 'interval:300');

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
CREATE TABLE saas.customer_data (
  id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
  customer_id TEXT DEFAULT CURRENT_USER(),
  entity_type TEXT,
  entity_data TEXT,
  created_at TIMESTAMP DEFAULT NOW()
) WITH (TYPE = 'USER', FLUSH_POLICY = 'rows:10000,interval:600');

-- Cross-tenant analytics (aggregated)
CREATE TABLE saas.analytics (
  id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
  metric_name TEXT,
  metric_value DOUBLE,
  tenant_id TEXT,
  timestamp TIMESTAMP DEFAULT NOW()
) WITH (TYPE = 'SHARED', FLUSH_POLICY = 'interval:3600');

-- Create service account per tenant
CREATE USER 'tenant_acme' WITH PASSWORD 'SecureKey123!' ROLE 'service';
```

**Result**: Salesforce-style multi-tenancy with SQL simplicity.

---

## 🛠️ Technology Stack

| Component | Technology | Version | Purpose |
|-----------|-----------|---------|---------|
| **Language** | Rust | 1.92+ | Performance, safety, concurrency |
| **Storage (Hot)** | RocksDB | 0.24 | Fast buffered writes (<1ms latency) |
| **Storage (Cold)** | Apache Parquet | 57.0 | Compressed columnar format for analytics |
| **Query Engine** | Apache DataFusion | 51.0 | SQL execution across hot+cold storage |
| **In-Memory** | Apache Arrow | 57.0 | Zero-copy data structures |
| **API Server** | Actix-web | 4.12 | REST endpoints + WebSocket subscriptions |
| **Authentication** | bcrypt + JWT | - | Password hashing + token-based auth |
| **Real-time** | WebSocket | - | Live message notifications |
| **Deployment** | Docker | - | Production-ready containerization |
| **Client SDK** | Rust → WASM | coming soon | TypeScript/JavaScript bindings |

---

## 📚 Documentation

### 🚀 Getting Started

- **[Quick Start Guide](docs/getting-started/quick-start.md)** - Get up and running in 10 minutes

### 📖 SQL, API & CLI

- **[SQL Reference](docs/reference/sql.md)** – SQL syntax and examples
- **[API Reference](docs/api/api.md)** – HTTP & WebSocket API overview
- **[CLI Guide](docs/getting-started/cli.md)** – using the `kalam` command-line client
 - **[SDK (TypeScript/WASM)](docs/sdk/sdk.md)** – browser/Node.js client (under development)

---

### TypeScript/JavaScript SDK

The official TypeScript SDK provides a type-safe wrapper around KalamDB with real-time subscriptions:

```typescript
import { createClient, Auth } from 'kalam-link';

const client = createClient({
  url: 'http://localhost:8080',
  auth: Auth.basic('admin', 'admin')
});
await client.connect();

// Query data
const result = await client.query('SELECT * FROM chat.messages LIMIT 10');

// Subscribe to changes (Firebase/Supabase style)
const unsubscribe = await client.subscribe('chat.messages', (event) => {
  console.log('Change:', event);
});

// Subscription management
console.log(`Active: ${client.getSubscriptionCount()}`);
await unsubscribe();
await client.disconnect();
```

Features:
- ✅ Built on Rust → WASM for performance
- ✅ Real-time subscriptions with Firebase/Supabase-style API
- ✅ Subscription tracking (`getSubscriptionCount()`, `unsubscribeAll()`)
- ✅ Works in browsers and Node.js
- ✅ Full TypeScript type definitions

See [SDK Documentation](docs/sdk/sdk.md) for the complete API reference.

---

## 🤲 Why I Started KalamDB

KalamDB began while I was building several AI applications and realized there was **no single database** that could provide:

* **Per-user storage isolation** (true physical separation, not just WHERE filters)
* **GDPR-ready design** where storing user data is isolated per user, and deleting a user simply removes their entire storage directory
* **Real-time subscriptions** without needing Redis, Kafka, or WebSocket proxies
* **Fast message/event storage** with <1ms latency
* **Event listening** for AI typing/thinking/cancel states
* **Both hot (RocksDB) and cold (Parquet) storage** built-in
* **All inside one database**, without chaining multiple services together

Traditionally, you need a *stack* to achieve this:

* a relational database for storage
* Redis/Kafka for real-time events
* a backend API to glue things together
* a WebSocket reverse proxy/server
* constant fine-tuning, caching, scaling, sharding, backpressure handling…

That setup becomes complex, expensive, and fragile.

I wanted something **simple**, **fast**, and **built for AI-era workloads** — without needing to maintain a cluster of different technologies.

So KalamDB was created as a lightweight, unified database that:

* isolates data per user for scale & privacy,
* pushes live changes instantly,
* is GDPR-friendly by design,
* avoids unnecessary complexity,
* and scales to millions with minimal tuning.

---

## 📄 License

Apache 2.0 License

---

## 🌟 Why its called "Kalam"?

**Kalam** (كلام) means "speech" or "conversation" in Arabic — fitting for a database designed specifically for storing and streaming human conversations and AI interactions.

---

**Built with ❤️ in Rust for real-time conversations at scale.**
