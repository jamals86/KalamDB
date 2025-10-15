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

## Decision 8: JSON Configuration Files (Not RocksDB)

**Context**: Where to store namespace and storage location metadata.

**Decision**: JSON files in `conf/` directory, loaded at startup

**Rationale**:
- **Infrequent updates**: Namespaces and storage locations rarely change
- **Human-readable**: Easy to inspect, edit, backup
- **Git-friendly**: Version control for configuration changes
- **Startup load**: Load into in-memory cache, no runtime RocksDB reads

**RocksDB is ONLY for**:
- Table data buffering (write path)
- System table data (users, live_queries, jobs)

**Configuration Files**:
- `conf/namespaces.json`: All namespaces
- `conf/storage_locations.json`: All storage locations
- `conf/{namespace}/schemas/{table_name}/`: Schema versioning

**namespaces.json Format**:
```json
{
  "namespaces": [
    {
      "name": "app",
      "created_at": "2025-10-14T10:00:00Z",
      "options": {},
      "table_count": 5
    }
  ]
}
```

**storage_locations.json Format**:
```json
{
  "locations": [
    {
      "location_name": "local_disk",
      "location_type": "filesystem",
      "path": "/data/${user_id}/tables",
      "credentials_ref": null,
      "usage_count": 3
    }
  ]
}
```

**Atomic Updates**:
- Write to temporary file
- Rename atomically (POSIX rename is atomic)
- Ensures crash-safe persistence

**Alternatives Considered**:
1. **Store in RocksDB** - Rejected: Harder to inspect, backup, version control
2. **TOML/YAML** - Rejected: JSON better for programmatic updates
3. **SQL database** - Rejected: Adds external dependency

---

## Summary

All technical unknowns resolved. Key decisions:
1. ✅ DataFusion for SQL execution
2. ✅ RocksDB column families for table isolation
3. ✅ Actix actors for WebSocket sessions
4. ✅ Hybrid in-memory + RocksDB for live query routing
5. ✅ DataFusion schema JSON serialization
6. ✅ Parquet bloom filters on _updated
7. ✅ JWT for authentication
8. ✅ JSON config files for metadata

**Next Step**: Proceed to Phase 1 (data-model.md, contracts/, quickstart.md)
