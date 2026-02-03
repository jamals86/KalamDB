# Kalam-Link Consumer Client (Rust) — Kafka-Style

Date: 2026-02-02  
Status: Draft  
Owner: KalamDB

## Overview
This document specifies a Kafka-style consumer client for the kalam-link Rust SDK. The consumer uses the existing Topic Pub/Sub API (`/api/topics/consume` + `/api/topics/ack`) and aligns with the current `CONSUME`/`ACK` SQL surface while adopting well-known Kafka semantics (poll, commit, auto-offset-reset, group id).

The consumer code must live in its own module inside the kalam-link crate to keep boundaries clear and to support future WebAssembly bindings.

## Goals
- Provide a Rust consumer that feels familiar to Kafka users (poll, commit, auto-commit, group id).
- Use the existing Topic Pub/Sub server API without introducing incompatible protocol changes.
- Ensure at-least-once delivery and explicit offset commits.
- Support optional long-polling, backoff, and retry behavior.
- Keep consumer code isolated under a dedicated module (models/utils/core split).

## Non-Goals
- Exactly-once processing semantics.
- Server-side rebalancing or multi-partition assignment (future work).
- Streaming push delivery (covered by live query/WebSocket).
- Multi-language consumer SDKs in this phase (Rust only; design allows extension).

## Alignment With Existing KalamDB Topic/ACK Semantics
Current server behavior uses `ACK` to advance offsets per group/partition. This is already equivalent to Kafka’s “commit offset” concept.

**Recommendation (Semantic Alignment):**
- Treat `ACK` as **commit** in the SDK terminology.
- Keep server API as-is for now, but expose Rust methods named `commit_*`.
- Optionally add SQL/HTTP aliases in the server later:
  - SQL: `COMMIT app.new_messages GROUP 'ai-service' PARTITION 0 UPTO OFFSET 220;`
  - HTTP: add `/api/topics/commit` as a synonym for `/api/topics/ack`.

This improves mental mapping to Kafka without breaking current behavior.

## Rust API Surface (Kafka-Style)
### Primary Types
- `TopicConsumer` — main consumer object.
- `ConsumerBuilder` — builder for configuration.
- `ConsumerConfig` — typed config with Kafka-compatible aliases.
- `ConsumerRecord` — record wrapper for topic messages.
- `ConsumerOffsets` — offset tracking and commit state.
- `AutoOffsetReset` — `Earliest | Latest | Offset(u64)`.
- `CommitMode` — `Auto | Manual`.

### Example Usage
```rust,no_run
use kalam_link::consumer::{TopicConsumer, ConsumerConfig, AutoOffsetReset};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let mut consumer = TopicConsumer::builder()
    .base_url("http://localhost:3000")
    .group_id("ai-service")
    .topic("app.new_messages")
    .auto_offset_reset(AutoOffsetReset::Latest)
    .enable_auto_commit(true)
    .build()?;

loop {
    let records = consumer.poll().await?;
    for record in records {
        // process record
        consumer.mark_processed(&record);
    }

    // If auto-commit disabled, explicitly commit:
    // consumer.commit_sync().await?;
}
# Ok(()) }
```

### `TopicConsumer` Methods (Proposed)
- `poll() -> Result<Vec<ConsumerRecord>>` — long-polls the server via `/api/topics/consume`.
- `poll_with_timeout(timeout: Duration) -> Result<Vec<ConsumerRecord>>` — overrides request timeout.
- `commit_sync() -> Result<CommitResult>` — calls `/api/topics/ack` using the highest processed offset.
- `commit_async() -> Result<()>` — fire-and-forget commit.
- `seek(offset: u64)` — next consume from offset (client-side state + request override).
- `position() -> u64` — current next offset.
- `mark_processed(record)` — track offsets that are safe to commit.
- `close()` — flush commits and mark consumer closed.

### Integration With Existing `KalamLinkClient`
- Provide a convenience: `KalamLinkClient::consumer()` returning `ConsumerBuilder`.
- `TopicConsumer` may hold a shared `KalamLinkClient` internally or build one from base URL + auth config.

## Config: Rust Names With Kafka Aliases
The Rust config uses idiomatic snake_case field names, while **accepting Kafka-style aliases** in `ConsumerConfig::from_map` or `ConsumerBuilder::from_properties`.

| Rust Field | Kafka Alias | Meaning |
|---|---|---|
| `group_id` | `group.id` | Consumer group identifier |
| `client_id` | `client.id` | Client identifier (optional) |
| `topic` | `topic` | Topic name/alias |
| `auto_offset_reset` | `auto.offset.reset` | `earliest` or `latest` |
| `enable_auto_commit` | `enable.auto.commit` | Auto commit offsets |
| `auto_commit_interval` | `auto.commit.interval.ms` | Commit interval |
| `max_poll_records` | `max.poll.records` | Max records per poll |
| `poll_timeout` | `fetch.max.wait.ms` | Long-poll timeout |
| `partition_id` | `partition.id` | Partition (MVP default 0) |
| `request_timeout` | `request.timeout.ms` | HTTP timeout |
| `retry_backoff` | `retry.backoff.ms` | Retry backoff |

**Alias Rules**:
- Both snake_case and dotted Kafka keys are accepted; snake_case takes priority on conflict.
- Values are normalized into a strongly-typed `ConsumerConfig`.
- Invalid values return `KalamLinkError::ConfigurationError`.

## Consumer Flow
1. **Initial Position**:
   - If `auto_offset_reset` is set and no committed offset exists, use it.
   - Otherwise, continue from the server-stored committed offset.
2. **Poll**:
   - Send `POST /api/topics/consume` with `group_id`, `topic`, `partition_id`, `limit`, and start position.
   - Long-poll until messages arrive or timeout.
3. **Process**:
   - User processes `ConsumerRecord` results.
4. **Commit**:
   - If `enable_auto_commit = true`, commit at interval based on highest processed offset.
   - If manual, call `commit_sync()`/`commit_async()`.

## Data Models
### `ConsumerRecord`
Maps the server response to a Rust type:
- `topic_id`, `topic_name` (resolved from request)
- `partition_id`
- `offset`
- `message_id` (optional)
- `source_table`
- `op` (Insert/Update/Delete)
- `timestamp_ms`
- `payload_mode` (Key|Full|Diff)
- `payload` (raw bytes)

### `CommitResult`
- `acknowledged_offset`
- `group_id`
- `partition_id`

## Errors & Retries
- Use `KalamLinkError` for public API.
- Consumer-specific errors wrap into `KalamLinkError::ConfigurationError`, `KalamLinkError::TimeoutError`, or `KalamLinkError::HttpError`.
- `poll()` retries transient HTTP errors with exponential backoff + jitter (configurable).

## Module Layout (Rust — Required)
Consumer code must be isolated under `link/src/consumer/`.

```
link/src/consumer/
├── mod.rs                 # public re-exports
├── core/
│   ├── mod.rs
│   ├── poller.rs           # long-poll logic, HTTP calls
│   ├── offset_manager.rs   # tracking/commit logic
│   └── coordinator.rs      # placeholder for future group coordination
├── models/
│   ├── mod.rs
│   ├── consumer_config.rs
│   ├── consumer_record.rs
│   ├── offsets.rs
│   ├── commit_result.rs
│   └── enums.rs            # AutoOffsetReset, CommitMode
└── utils/
    ├── mod.rs
    ├── backoff.rs
    └── time.rs
```

**Re-exports** in `link/src/consumer/mod.rs`:
- `pub use models::{ConsumerConfig, ConsumerRecord, AutoOffsetReset, CommitMode, CommitResult};`
- `pub use core::TopicConsumer;`

## HTTP Endpoint Usage
The consumer uses the existing API surface defined in [specs/024-topic-pubsub/README.md](README.md):
- `POST /api/topics/consume`
- `POST /api/topics/ack` (treated as commit)

## Suggested Server Improvements (Optional)
- Add `COMMIT` SQL alias for `ACK`.
- Add `/api/topics/commit` alias for `/api/topics/ack`.
- Include `topic_name` in consume response to avoid client-side mapping.
- Add `headers` field to the message envelope for future metadata.

## Implementation Tasks (Full Spec)
### SDK: Consumer Module (kalam-link)
1. Create module layout under `link/src/consumer/` exactly as specified.
2. Implement `models::enums` with `AutoOffsetReset` and `CommitMode`.
3. Implement `models::consumer_record` mapping the `/api/topics/consume` response.
4. Implement `models::commit_result` for `/api/topics/ack` response parsing.
5. Implement `models::consumer_config` with:
    - Snake_case fields + Kafka alias parsing (`from_map`, `from_properties`).
    - Validation + defaulting (including `auto_offset_reset`, timeouts, backoff).
    - `KalamLinkError::ConfigurationError` on invalid values.
6. Implement `core::poller`:
    - Long-polling with `poll()` and `poll_with_timeout()`.
    - HTTP request/response parsing and retry w/ exponential backoff + jitter.
7. Implement `core::offset_manager`:
    - Track current position, highest processed offsets, and safe commit offsets.
    - Support `seek()`, `position()`, `mark_processed()`.
8. Implement `core::TopicConsumer`:
    - Builder flow, `poll`, `commit_sync`, `commit_async`, `close`.
    - Auto-commit timer logic and flushing on `close`.
9. Implement `utils::backoff` and `utils::time` helpers.
10. Wire into `KalamLinkClient::consumer()` convenience API.
11. Add Rust doc examples for `TopicConsumer` and `ConsumerBuilder`.

### Server Compatibility (If Needed)
1. Confirm `/api/topics/consume` and `/api/topics/ack` support the required fields.
2. If missing, add optional `topic_name` in consume response (non-breaking).
3. (Optional) Add `/api/topics/commit` alias and SQL `COMMIT` alias.

## Test Plan (Full Coverage)
All tests must exercise the kalam-link consumer end-to-end against a running KalamDB server.

### Unit Tests (kalam-link)
- Config alias parsing:
  - `snake_case` wins over dotted Kafka keys.
  - Invalid types/values return `ConfigurationError`.
- `AutoOffsetReset` parsing: `earliest`, `latest`, explicit `Offset(u64)`.
- Backoff timing: exponential growth + jitter bounds.
- Offset manager:
  - `mark_processed()` advances highest processed offset.
  - `seek()` resets next position without losing processed markers.
  - `position()` reflects next fetch offset.

### Integration Tests (kalam-link + server)
1. Basic consume + manual commit:
    - Produce N messages.
    - `poll()` returns records in order.
    - `mark_processed()` then `commit_sync()` advances server-stored offset.
2. Auto-commit:
    - Enable auto-commit with short interval.
    - Verify committed offset advances without manual `commit_sync()`.
3. Auto-offset-reset:
    - With no committed offset, `Earliest` starts from offset 0.
    - With no committed offset, `Latest` starts at end and returns only new messages.
4. Timeout + long-poll:
    - `poll_with_timeout()` returns empty on timeout.
5. Retry/backoff:
    - Inject transient HTTP error (e.g., temporary server 5xx) and ensure retry then success.
6. Disconnection + reconnection:
    - Simulate client disconnect mid-session.
    - Reconnect and `poll()` continues from last committed offset.
    - Verify uncommitted messages are re-delivered (at-least-once).
7. Commit persistence:
    - Commit offset, restart client, consume again.
    - Verify server returns next messages after committed offset.
8. High load / throughput:
    - Produce large batches (e.g., 10k+ messages per topic).
    - Ensure `max_poll_records` respected and no data loss.
9. Multiple messages per topic:
    - Validate ordering and offsets within a single topic.
10. Multiple topics at once:
    - Run multiple consumers (one per topic) concurrently.
    - Ensure independent offsets per group/topic/partition.
    - Verify no cross-topic contamination in results.

### Stress/Soak Tests
- Sustained high-load polling for 5–15 minutes with auto-commit enabled.
- Random disconnect/reconnect during load; verify offsets are consistent and no skipped messages.

### Notes
- Integration tests should live under `link/tests/` and use real HTTP against a running server.
- Use unique `group_id`s per test to avoid cross-test offset contamination.
- Validate `ack`/commit persistence by querying server state or re-consuming.

## Open Questions
- Do we want a `Stream`-based API (`impl Stream<Item = Result<ConsumerRecord>>`) in addition to `poll()`?
- Should the consumer accept SQL `CONSUME` as a fallback path when HTTP is unavailable?
- Should `ACK` accept `offset` and `count` (commit a range) for batching?
