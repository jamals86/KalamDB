# Durable Topics (Pub/Sub) Spec

Date: 2026-01-28
Owner: KalamDB

## Goals
- Add durable, internal topics backed by RocksDB for change-event consumption.
- Support multiple consumer groups with at-least-once delivery semantics.
- Enable replay from earliest/latest/specific offset.
- Keep partitioning in the data model (single partition for MVP).
- Provide SQL surface for topic creation, routing, consume, and ack.

## Non-Goals
- Multi-partition routing and per-partition leaders (future).
- Streaming push delivery (handled by live queries/WebSocket).
- Global exactly-once semantics.

## Current State (Observed)
- Change events are produced for live queries in `kalamdb-core`.
- RocksDB is used via `kalamdb-store` with EntityStore abstractions.
- SQL parsing lives in `kalamdb-sql` with handlers in `kalamdb-core`.
- System tables live in `kalamdb-system`.

## Proposed Data Model
### System Tables
Create system tables (or system-managed metadata) to track topics, routes, and offsets:
- `system.topics`
  - `topic_id` (TopicId)
  - `name`
  - `alias` (optional, unique)
  - `partitions` (u32, default 1)
  - `retention_seconds` (optional)
  - `retention_max_bytes` (optional)
  - `routes[]` (embedded; one topic can have multiple routes)
    - `table_id`
    - `op` (Insert|Update|Delete)
    - `payload_mode` (Key|Full|Diff)
    - `filter_expr` (optional)
    - `partition_key_expr` (optional)
- `system.topic_offsets`
  - `topic_id`
  - `group_id`
  - `partition_id` (u32)
  - `last_acked_offset` (u64)
  - `updated_at`

### Indexing & Caching
- `system.topics`: unique index on `name`, unique index on `alias` (when present).
- Route lookup: build an in-memory index at startup and update on changes: `(table_id, op) -> [topic_id]`.
- Optional future: persist a secondary index in RocksDB if startup rebuild becomes too expensive.

### Topic Message Envelope
Stored in RocksDB `topic_logs`:
- `topic_id`
- `partition_id`
- `offset` (u64)
- `message_id` (optional, Snowflake for tracing)
- `source_table` (TableId)
- `op` (Insert|Update|Delete)
- `ts` (utc millis)
- `payload_mode` (Key|Full|Diff)
- `payload`:
  - `Key`: primary key values only
  - `Full`: full row snapshot (optional)
  - `Diff`: changed columns (future)

## Storage Layout (RocksDB)
Use a dedicated Column Family in `kalamdb-store` (preferred) or key prefixes. use StoredKey just like the rest.

### Topic Logs
Key: (similar to the other storagekey's we already have with tuples)
```
topic/<topic_id>/<partition_id>/<offset>
```
Value: binary envelope (bincode/serde).

### Offsets
Key: (similar to the other storagekey's we already have with tuples)
```
offset/<topic_id>/<group_id>/<partition_id>
```
Value:
```
last_acked_offset (u64)
```

### Offset Counters
Key: (similar to the other storagekey's we already have with tuples)
```
counter/<topic_id>/<partition_id>
```
Value:
```
last_offset (u64)
```

## SQL Surface
### Create Topic
```
CREATE TOPIC app.new_messages;
```

Future-ready partitions:
```
CREATE TOPIC app.new_messages PARTITIONS 4;
```

### Add Route
Default payload (key reference):
```
ALTER TOPIC app.new_messages
ADD SOURCE chat.messages
ON INSERT
WITH (payload = 'key');
```

Full snapshot:
```
ALTER TOPIC app.new_messages
ADD SOURCE chat.messages
ON INSERT
WITH (payload = 'full');
```

Filtered route (WHERE):
```
ALTER TOPIC app.new_messages
ADD SOURCE chat.messages
ON INSERT
WHERE (is_visible = true AND channel_id = 42)
WITH (payload = 'key');
```

Partition key (future-ready):
```
ALTER TOPIC app.new_messages
ADD SOURCE chat.messages
ON INSERT
PARTITION BY (user_id)
WITH (payload = 'key');
```

### Consume
```
CONSUME FROM app.new_messages
GROUP 'ai-service'
FROM LATEST
LIMIT 100;
```

```
CONSUME FROM app.new_messages
GROUP 'ai-service'
FROM EARLIEST
LIMIT 100;
```

```
CONSUME FROM app.new_messages
GROUP 'ai-service'
FROM OFFSET 12345
LIMIT 100;
```

Consume by alias:
```
CONSUME FROM app.new_messages_alias
GROUP 'ai-service'
FROM LATEST
LIMIT 100;
```

### Ack
```
ACK app.new_messages
GROUP 'ai-service'
PARTITION 0
UPTO OFFSET 220;
```

## API Surface (HTTP/SDK)
### Consume Request (Long Polling)
**Endpoint**: `POST /api/topics/consume`

**Authentication**: Required (Basic Auth or JWT Bearer)
**Authorization**: Role must be `service`, `dba`, or `system` (NOT `user`)

**Long Polling Behavior**:
- Actix-Web async handler keeps request open until messages are available or timeout expires
- No need for frequent polling - one long-lived request per consumer
- Configurable timeout via `server.toml` or request header
- Default timeout: 30 seconds (configurable)
- If no messages available before timeout, returns empty array
- Client should immediately reconnect after receiving response or timeout

**Request Body**:
```json
{
  "topic": "app.new_messages",
  "group_id": "ai-service",
  "start": "Latest" | "Earliest" | { "Offset": 12345 },
  "limit": 100,
  "partition_id": 0,
  "timeout_seconds": 30
}
```

**Request Fields**:
- `topic` (string): Topic name or alias
- `group_id` (string): Consumer group identifier
- `start` (enum): Starting position (Latest | Earliest | Offset(u64))
- `limit` (int): Maximum messages to return (default 100)
- `partition_id` (int, optional): Partition to consume from (default 0)
- `timeout_seconds` (int, optional): Long polling timeout (default from server.toml)

### Consume Response
**Response Body**:
```json
{
  "messages": [
    {
      "topic_id": "...",
      "partition_id": 0,
      "offset": 12345,
      "message_id": "...",
      "source_table": "chat.messages",
      "op": "Insert",
      "ts": 1730000000000,
      "payload_mode": "Key",
      "payload": "base64-encoded-bytes"
    }
  ],
  "next_offset": 12346,
  "has_more": true
}
```

### Ack Request
**Endpoint**: `POST /api/topics/ack`

**Authentication**: Required (Basic Auth or JWT Bearer)
**Authorization**: Role must be `service`, `dba`, or `system` (NOT `user`)

**Request Body**:
```json
{
  "topic": "app.new_messages",
  "group_id": "ai-service",
  "partition_id": 0,
  "upto_offset": 220
}
```

**Response**:
```json
{
  "success": true,
  "acknowledged_offset": 220
}
```

## Write Path Integration
1. On table write, existing change-event pipeline emits `ChangeEvent`.
2. `TopicRouter::on_change(event)` in `kalamdb-core`:
   - resolve routes for `(table_id, op)`
   - optional filter evaluation
   - compute `partition_id` (0 for MVP)
   - allocate next `offset`
   - append message to `topic_logs`
3. Consumers read from `topic_logs` by `topic_id/partition_id/offset`.
4. ACK updates `topic_offsets`.

### Live Query Reuse (Future Work)
- Reuse the existing live query change-event + filter evaluation path to avoid duplicating predicate and projection logic.
- Potential design: a shared `ChangeEventEvaluator` service that compiles filters/queries once and is used by both live queries and topic routes.
- For MVP, keep topic routing isolated but structure it to plug into this shared evaluator later.

## Offset Allocation (Single Node MVP)
- Read `counter/<topic>/<partition>` to get `last_offset`.
- `new_offset = last_offset + 1`.
- Write log entry and update counter atomically (single RocksDB batch).

## Configuration (server.toml)
```toml
[topics]
# Long polling timeout for CONSUME requests (seconds)
default_consume_timeout = 30
max_consume_timeout = 300

# Authorization settings
allow_user_role_consume = false  # Only service/dba/system can consume
```

## Retention & Cleanup
- Per-topic retention policy: `retention_seconds` and/or `retention_max_bytes`.
- Background job in `kalamdb-core`:
  - delete keys by prefix `topic/<topic>/<partition>/` beyond retention.

## Ownership by Crate
- `kalamdb-store`: RocksDB CFs and low-level get/put/batch APIs.
- `kalamdb-system`: system tables (`topics`, `topic_routes`, `topic_offsets`).
- `kalamdb-publisher` (new): routing engine, route evaluation, payload materialization, and offset allocation.
- `kalamdb-core`: orchestrates topic routing, retention job, and delegates to `kalamdb-publisher`.
- `kalamdb-sql`: SQL parsing/AST for CREATE/ALTER/CONSUME/ACK.
- `kalamdb-api`: HTTP endpoints for consume/ack and SQL execution.
- `kalamdb-commons`: types (`TopicId`, `ConsumerGroupId`, `TopicOp`, `PayloadMode`).

## Validation Checklist
- [ ] System tables exist and are queryable.
- [ ] CREATE TOPIC persists metadata.
- [ ] ALTER TOPIC ADD SOURCE creates route.
- [ ] Change events append to topic logs.
- [ ] CONSUME supports EARLIEST/LATEST/OFFSET.
- [ ] ACK updates offsets correctly.
- [ ] Retention job prunes old log entries.

## Risks & Mitigations
- **Hot key contention on counters**: start with single-node + low throughput; shard by partition in future.
- **Large payloads**: default to `payload = key` and keep `full` optional.
- **Orphaned offsets**: initialize group offsets on first consume, and treat missing offsets as `EARLIEST` or `LATEST` depending on request.
