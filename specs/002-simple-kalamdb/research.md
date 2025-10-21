# Research: Simple KalamDB Technical Decisions

**Feature**: Simple KalamDB - User-Based Database with Live Queries  
**Branch**: `002-simple-kalamdb`  
**Date**: 2025-10-15

## Overview

This document consolidates all technical research and decisions made for the KalamDB implementation. All unknowns from the Technical Context have been resolved.

## Decision 1: DataFusion for SQL Execution

**Context**: Need a SQL query engine that can work with custom storage backends (RocksDB + Parquet).

**Decision**: Use Apache DataFusion 35.0

**Rationale**:
- **Production-ready**: Battle-tested SQL parser, planner, and optimizer
- **Zero-copy integration**: Built on Apache Arrow for efficient data handling
- **Custom TableProvider**: Supports implementing custom storage backends
- **Query optimization**: Automatic projection pushdown, filter pushdown, predicate evaluation
- **No reinvention**: Avoid building custom SQL parser/executor

**Alternatives Considered**:
1. **Custom SQL Parser** - Rejected: High complexity, months of development, error-prone
2. **SQLite FFI** - Rejected: Cannot integrate with RocksDB/Parquet storage
3. **SQL string parsing** - Rejected: Lacks optimization, type safety, error handling

**Implementation Details**:
- Create `SessionContext` per request with custom `TableProvider` implementations
- Register system tables, user tables, shared tables dynamically
- Inject `_deleted = false` filter for all queries (soft delete support)
- Use `SchemaRef::to_json()` for schema serialization

**References**:
- DataFusion Docs: https://arrow.apache.org/datafusion/
- Custom TableProvider: https://arrow.apache.org/datafusion/user-guide/dataframe.html#custom-table-providers

---

## Decision 2: RocksDB Column Families for Table Isolation

**Context**: Need isolated storage for different table types with independent flush policies.

**Decision**: Use RocksDB column families with naming convention

**Rationale**:
- **Isolation**: Each column family has separate memtable, write buffer, compaction settings
- **Performance**: Independent flush policies per table without cross-table interference
- **User isolation**: Different users' data in same user table stored separately by key prefix
- **Proven pattern**: Used by Cassandra, TiKV, and other distributed databases

**Naming Convention**:
- User tables: `user_table:{namespace}:{table_name}` (e.g., `user_table:app:messages`)
- Shared tables: `shared_table:{namespace}:{table_name}` (e.g., `shared_table:app:conversations`)
- System tables: `system_table:{table_name}` (e.g., `system_table:users`)
- Stream tables: `stream_table:{namespace}:{table_name}` (e.g., `stream_table:app:events`)

**Key Format**:
- User tables: `{user_id}:{row_id}` (isolates data by user)
- Shared tables: `{row_id}` (global access)
- System tables: `{entity_id}` (e.g., user_id, live_id, job_id)
- Stream tables: `{timestamp}:{row_id}` (ordered by time)

**Alternatives Considered**:
1. **Key prefixes only** - Rejected: Shared memtable limits flush policy independence
2. **Separate RocksDB instances** - Rejected: Too many file descriptors, complex management
3. **Single column family** - Rejected: Cannot enforce per-table flush policies

**Configuration**:
```rust
let mut opts = Options::default();
opts.set_write_buffer_size(64 * 1024 * 1024); // 64MB per table
opts.set_max_write_buffer_number(3);
opts.set_compression_type(DBCompressionType::Lz4);
```

**References**:
- RocksDB Column Families: https://github.com/facebook/rocksdb/wiki/Column-Families
- Tuning Guide: https://github.com/facebook/rocksdb/wiki/RocksDB-Tuning-Guide

---

## Decision 3: Actix-Web Actor Model for WebSocket Sessions

**Context**: Need to manage thousands of concurrent WebSocket connections with live query subscriptions.

**Decision**: Use Actix-Web 4.4 with actor-based WebSocket handling

**Rationale**:
- **Actor per connection**: Each WebSocket connection is an independent actor
- **Message passing**: Type-safe messages for subscription registration, change notifications
- **Lifecycle management**: Automatic cleanup on disconnect
- **Battle-tested**: Used in production by many Rust web services
- **Actix integration**: Seamless integration with Actix-Web HTTP server

**Architecture**:
```rust
// WebSocket session actor
struct WebSocketSession {
    connection_id: String,
    user_id: String,
    live_ids: Vec<String>,
}

// Message types
struct Subscribe { query_id: String, sql: String }
struct ChangeNotification { live_id: String, change_type: String, rows: Vec<Row> }
```

**Alternatives Considered**:
1. **tokio-tungstenite** - Rejected: Manual state management, complex lifecycle
2. **axum WebSocket** - Rejected: Less mature than Actix for long-lived connections
3. **warp WebSocket** - Rejected: Warp project less actively maintained

**References**:
- Actix-Web WebSocket: https://actix.rs/docs/websockets
- Actix Actors: https://actix.rs/book/actix/

---

## Decision 4: In-Memory Registry for Live Query Routing

**Context**: Need to route change notifications to correct WebSocket actors across distributed nodes.

**Decision**: Hybrid model - RocksDB for metadata, in-memory for actor addresses

**Rationale**:
- **Cannot persist actors**: Actix actor addresses (`Addr<WebSocketSession>`) cannot be serialized
- **Fast lookup**: HashMap lookup for connection_id → actor is O(1)
- **Node-aware**: system.live_queries stores which node owns each subscription
- **Fault tolerance**: On node restart, in-memory registry rebuilds as clients reconnect

**Data Structures**:

**In-Memory (per node)**:
```rust
struct LiveQueryRegistry {
    // connection_id → WebSocket actor + live_ids
    connections: HashMap<String, ConnectedWebSocket>,
    
    // live_id → connection_id (for reverse lookup)
    live_to_connection: HashMap<String, String>,
    
    node_id: String,
}

struct ConnectedWebSocket {
    actor: Addr<WebSocketSession>,
    live_ids: Vec<String>,
}
```

**RocksDB (system.live_queries table)**:
```
live_id (PK): "conn123-messages"
connection_id: "conn123"
table_id: "messages"
query_id: "messages"
user_id: "user456"
query: "SELECT * FROM messages WHERE..."
options: {"last_rows": 100}
created_at: timestamp
updated_at: timestamp
changes: 42
node: "node1"
```

**Routing Logic**:
1. RocksDB write triggers change detection
2. Query system.live_queries for matching subscriptions
3. Filter by `node = current_node_id`
4. For each matching live_id:
   - Lookup connection_id from `live_to_connection`
   - Lookup actor from `connections[connection_id]`
   - Send message to actor

**Alternatives Considered**:
1. **Pure RocksDB** - Rejected: Cannot serialize actor addresses
2. **Redis registry** - Rejected: Adds external dependency, network latency
3. **Distributed actor system (Akka-like)** - Rejected: Too complex for MVP

**References**:
- Actix Actors cannot be serialized: https://actix.rs/docs/actix/arbiter/

---

## Decision 5: Arrow Schema JSON Serialization

**Context**: Need to version table schemas and support schema evolution (ALTER TABLE).

**Decision**: Use DataFusion's built-in schema serialization

**Rationale**:
- **Built-in support**: `SchemaRef::to_json()` and `SchemaRef::from_json()`
- **Standard format**: JSON is human-readable, Git-friendly
- **Metadata support**: Can embed table options in schema metadata field
- **Version-safe**: DataFusion handles deserialization across versions

**Schema File Structure**:
```
conf/{namespace}/schemas/{table_name}/
├── manifest.json              # Schema version history
├── schema_v1.json            # Initial schema
├── schema_v2.json            # After ALTER TABLE
└── current.json              # Symlink to latest version
```

**manifest.json Format**:
```json
{
  "table_name": "messages",
  "namespace": "app",
  "current_version": 2,
  "versions": [
    {
      "version": 1,
      "created_at": "2025-10-14T10:00:00Z",
      "changes": "Initial schema"
    },
    {
      "version": 2,
      "created_at": "2025-10-15T12:00:00Z",
      "changes": "ADD COLUMN user_id STRING"
    }
  ]
}
```

**schema_v1.json Format** (DataFusion output):
```json
{
  "fields": [
    {"name": "id", "data_type": "Int64", "nullable": false},
    {"name": "text", "data_type": "Utf8", "nullable": true},
    {"name": "_updated", "data_type": "Timestamp(Microsecond, None)", "nullable": false},
    {"name": "_deleted", "data_type": "Boolean", "nullable": false}
  ],
  "metadata": {
    "table_options": "{\"flush_policy\":{\"row_limit\":10000}}"
  }
}
```

**Alternatives Considered**:
1. **Custom schema format** - Rejected: Reinventing DataFusion's serialization
2. **Protocol Buffers** - Rejected: Not human-readable, requires compilation
3. **SQL CREATE TABLE** - Rejected: Harder to parse, version, and diff

**References**:
- DataFusion Schema: https://docs.rs/datafusion/latest/datafusion/arrow/datatypes/struct.Schema.html

---

## Decision 6: Parquet Bloom Filters on _updated Column

**Context**: Need efficient time-range queries (e.g., "messages since yesterday").

**Decision**: Configure Parquet bloom filter on `_updated` timestamp column

**Rationale**:
- **Query skipping**: Parquet reader can skip files that don't overlap query time range
- **Automatic injection**: `_updated` added to all user/shared tables automatically
- **Standard feature**: Parquet spec supports bloom filters natively
- **Performance**: Reduces disk I/O for time-range queries significantly

**Implementation**:
```rust
use parquet::file::properties::WriterProperties;

let props = WriterProperties::builder()
    .set_column_bloom_filter_enabled("_updated".into(), true)
    .set_bloom_filter_fpp("_updated".into(), 0.05) // 5% false positive rate
    .build();
```

**Query Optimization**:
- User query: `SELECT * FROM messages WHERE created_at > '2025-10-14'`
- DataFusion adds: `AND _deleted = false`
- Parquet reader: Uses `_updated` bloom filter to skip old files
- Only scans files where `_updated` overlaps query range

**Alternatives Considered**:
1. **Bloom filter on primary key** - Rejected: Doesn't help time-range queries
2. **No bloom filters** - Rejected: Degrades query performance on large datasets
3. **Parquet row groups only** - Rejected: Less granular than bloom filters

**References**:
- Parquet Bloom Filters: https://github.com/apache/parquet-format/blob/master/BloomFilter.md
- DataFusion Parquet: https://arrow.apache.org/datafusion/user-guide/sql/datafusion-query-execution.html

---

## Decision 7: JWT for User Authentication

**Context**: Need to identify users for CURRENT_USER() function and data isolation.

**Decision**: JWT tokens with user_id claim

**Rationale**:
- **Stateless**: No server-side session storage required
- **Standard**: Industry-standard authentication mechanism
- **User identification**: Extract `user_id` from JWT claims
- **Context propagation**: Store user_id in DataFusion `SessionContext`

**Implementation**:
```rust
// Extract user_id from JWT
let claims = decode_jwt(token)?;
let user_id = claims.get("user_id")?;

// Create DataFusion session with user context
let mut ctx = SessionContext::new();
ctx.state().set_user_id(user_id);

// CURRENT_USER() function implementation
fn current_user(ctx: &SessionContext) -> Result<String> {
    ctx.state().get_user_id()
}
```

**Security**:
- Verify JWT signature on every request
- Reject expired tokens
- Rotate signing keys periodically

**Alternatives Considered**:
1. **Session-based auth** - Rejected: Requires shared session store, complex in distributed setup
2. **API keys** - Rejected: Lacks user identity, harder to rotate
3. **OAuth2** - Rejected: Overkill for MVP, adds external dependencies

**References**:
- JWT: https://jwt.io/introduction
- jsonwebtoken crate: https://docs.rs/jsonwebtoken/latest/jsonwebtoken/

---

## Decision 8: JSON Configuration Files (Not RocksDB) ~~DEPRECATED~~

> **⚠️ DEPRECATED**: This decision has been superseded. See "Decision 8: RocksDB-Only for Metadata Storage (Updated Architecture)" below for the current architecture.

**Original Context**: Where to store namespace and storage location metadata.

**Original Decision**: JSON files in `conf/` directory, loaded at startup

**Why Deprecated**: Dual persistence (JSON + RocksDB) created complexity and sync issues. Architecture simplified to use RocksDB-only for all metadata.

**Original Rationale** (for historical reference):
- **Infrequent updates**: Namespaces and storage locations rarely change
- **Human-readable**: Easy to inspect, edit, backup
- **Git-friendly**: Version control for configuration changes
- **Startup load**: Load into in-memory cache, no runtime RocksDB reads

**What Changed**: Clarification Q2 identified that dual persistence was unnecessary. All metadata now stored in RocksDB system tables (system_namespaces, system_storage_locations, system_tables, system_table_schemas).

---

## Decision 8: RocksDB-Only for Metadata Storage (Updated Architecture)

**Context**: Original design stored namespaces and storage_locations in JSON files (`conf/namespaces.json`, `conf/storage_locations.json`). This created dual persistence paths (JSON + RocksDB).

**Decision**: Store ALL metadata in RocksDB system tables - eliminate JSON config files

**Rationale**:
- **Single source of truth**: All metadata in one storage engine
- **Transactional consistency**: Atomic updates across related metadata
- **No file system sync issues**: Eliminates race conditions from dual persistence
- **Simplified backup/restore**: Single RocksDB snapshot captures everything
- **Consistent query interface**: kalamdb-sql provides unified SQL access to all metadata

**Architecture Changes**:
- **Removed**: `conf/namespaces.json`, `conf/storage_locations.json`
- **Added**: `system_namespaces` CF, `system_storage_locations` CF (already existed)
- **Added**: `system_tables` CF (new - table metadata registry)
- **Added**: `system_table_schemas` CF (new - Arrow schema versions)

**Column Family Strategy**:
```
system_namespaces        # Namespace registry
system_storage_locations # Storage location definitions
system_tables            # Table metadata (type, storage, flush policy)
system_table_schemas     # Arrow schema versions
system_users             # User authentication
system_live_queries      # Active subscriptions
system_jobs              # Background job history
user_table_counters      # Per-user-per-table row counts for flush tracking
```

**Alternatives Considered**:
1. **Keep JSON + RocksDB hybrid** - Rejected: Dual persistence adds complexity, sync issues
2. **Use external database** - Rejected: Adds deployment dependency
3. **Store in separate RocksDB instance** - Rejected: Unnecessary separation

**Clarification Source**: Q2 from spec clarification workflow - "namespaces should be only a rocksdb columnfamily table and not in disk"

---

## Decision 9: Unified kalamdb-sql Crate for System Tables

**Context**: With 7 system tables (users, live_queries, storage_locations, jobs, namespaces, tables, table_schemas), we'd need 7 separate CRUD implementations. This creates code duplication and maintenance burden.

**Decision**: Create kalamdb-sql crate with unified SQL interface for all system tables

**Rationale**:
- **Eliminates duplication**: Single implementation serves 7 tables
- **Consistent API**: All system tables accessed via SQL (SELECT, INSERT, UPDATE, DELETE)
- **Type safety**: Rust models for each table with compile-time validation
- **Maintainability**: Changes to system table logic happen in one place
- **Future-ready**: Designed for distributed replication via kalamdb-raft

**Architecture**:

```rust
// kalamdb-sql crate structure
kalamdb-sql/
├── src/
│   ├── lib.rs          # Public API exports
│   ├── models.rs       # Rust structs for 7 system tables
│   │   ├── User { user_id, username, permissions }
│   │   ├── LiveQuery { subscription_id, user_id, table_name, ... }
│   │   ├── StorageLocation { location_name, location_type, ... }
│   │   ├── Job { job_id, job_type, node_id, ... }
│   │   ├── Namespace { namespace_id, name, options }
│   │   ├── Table { table_id, namespace, table_name, ... }
│   │   └── TableSchema { schema_id, table_id, version, ... }
│   ├── parser.rs       # SQL parsing (sqlparser-rs)
│   ├── executor.rs     # SQL execution logic
│   └── adapter.rs      # RocksDB read/write operations
```

**Usage Pattern**:
```rust
// kalamdb-core uses kalamdb-sql for system table operations
let sql_engine = KalamSql::new(rocksdb_handle);

// Query system tables
let users = sql_engine.execute("SELECT * FROM system.users WHERE username = 'alice'")?;

// Insert into system tables
sql_engine.execute("INSERT INTO system.namespaces (namespace_id, name, options) VALUES (?, ?, ?)")?;
```

**Benefits**:
1. **~7x less code**: Single SQL engine vs 7 manual implementations
2. **Consistent behavior**: All tables follow same query semantics
3. **Easy testing**: Test SQL engine once, all tables validated
4. **Simple integration**: kalamdb-core just calls execute()
5. **Raft-ready**: SQL operations can be replicated as log entries

**Alternatives Considered**:
1. **Manual CRUD for each table** - Rejected: 7x code duplication
2. **Generic macro-based solution** - Rejected: Harder to debug, less flexible
3. **ORM library (diesel, sea-orm)** - Rejected: Designed for SQL databases, not RocksDB

**Future Integration** (kalamdb-raft):
- System table mutations become Raft log entries
- Each entry contains: `(operation_id, sql_statement, parameters)`
- Leader replicates to followers before applying to RocksDB
- Followers apply committed entries to their local RocksDB
- Strong consistency for all metadata operations

**Clarification Source**: Q3 from spec clarification workflow - "create a separate crate called kalamdb-sql...unified usage of system table"

**References**:
- sqlparser-rs: https://github.com/sqlparser-rs/sqlparser-rs
- Raft consensus: https://raft.github.io/

---

## Summary

All technical unknowns resolved. Key decisions:
1. ✅ DataFusion for SQL execution (user/shared tables)
2. ✅ RocksDB column families for table isolation
3. ✅ Actix actors for WebSocket sessions
4. ✅ Hybrid in-memory + RocksDB for live query routing
5. ✅ DataFusion schema JSON serialization
6. ✅ Parquet bloom filters on _updated
7. ✅ JWT for authentication
8. ✅ **NEW**: RocksDB-only metadata (eliminated JSON config files)
9. ✅ **NEW**: kalamdb-sql unified crate (eliminates system table code duplication)

**Major Architecture Changes**:
- 7 system tables instead of 4 (added: namespaces, tables, table_schemas)
- All metadata in RocksDB (no JSON files)
- Unified SQL engine for all system tables
- Future-ready for distributed replication (kalamdb-raft)

**Next Step**: Update data-model.md to reflect 7 system tables + kalamdb-sql architecture
