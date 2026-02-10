# 025 — Publisher Crate Extraction & 100% Event Coverage

Date: 2025-07-14
Owner: KalamDB
Status: **Implemented** (Phases 1-4 complete, 100% event coverage achieved)

---

## Table of Contents

1. [System Overview](#1-system-overview)
2. [Root Cause Analysis — Why Events Are Lost](#2-root-cause-analysis)
3. [async-channel Evaluation](#3-async-channel-evaluation)
4. [Spec: 100% Unique Event Coverage](#4-spec-100-unique-event-coverage)
5. [Spec: `kalamdb-publisher` Crate Extraction](#5-spec-kalamdb-publisher-crate-extraction)
6. [Implementation Tasks](#6-implementation-tasks)
7. [Migration Guide](#7-migration-guide)
8. [Validation Checklist](#8-validation-checklist)

---

## 1. System Overview

### Current Architecture (as-is)

The topic publishing pipeline has **five stages**:

```
┌──────────────┐     ┌──────────────────┐     ┌───────────────────┐     ┌───────────────────┐     ┌──────────────┐
│ Table Write  │────▶│ NotificationSvc  │────▶│ TopicPublisherSvc │────▶│ TopicMessageStore │────▶│ HTTP Consume │
│ (Provider)   │     │ (tokio::mpsc 10K)│     │ (DashMap routing) │     │ (RocksDB)         │     │ (long-poll)  │
└──────────────┘     └──────────────────┘     └───────────────────┘     └───────────────────┘     └──────────────┘
      fire &              try_send()              single worker             EntityStore             stateless
      forget              DROPS on full           synchronous               durable                 polling
```

### Component Inventory

| Component | File | Lines | Responsibility |
|-----------|------|-------|----------------|
| `NotificationService` | `kalamdb-core/src/live/notification.rs` | 461 | Dispatches change events from table writes to topic publisher + live query subscribers |
| `TopicPublisherService` | `kalamdb-core/src/live/topic_publisher.rs` | 665 | Route matching, payload extraction, offset allocation, message persistence |
| `TopicMessageStore` | `kalamdb-tables/src/topics/topic_message_store.rs` | 289 | RocksDB-backed message storage with composite keys for range scans |
| `ConnectionsManager` | `kalamdb-core/src/live/manager/connections_manager.rs` | ~500 | WebSocket connection registry, per-subscriber notification dispatch |
| `SubscriptionService` | `kalamdb-core/src/live/subscription.rs` | 292 | Live query registration via Raft-replicated system table |
| `ConsumerPoller` (client) | `link/src/consumer/core/poller.rs` | 200 | HTTP client for `/v1/api/topics/consume` + `/v1/api/topics/ack` |
| `TopicConsumer` (client) | `link/src/consumer/core/topic_consumer.rs` | 385 | Client-side poll/commit loop with offset management |
| Consume handler | `kalamdb-api/src/handlers/topics/consume.rs` | 160 | HTTP endpoint: resolves offset, fetches messages, returns JSON |

### Channel Inventory

| Channel | Type | Capacity | Drop Behavior |
|---------|------|----------|---------------|
| Notification queue | `tokio::mpsc` (bounded) | 10,000 | `try_send` → **drops** on full |
| Per-connection notifications | `tokio::mpsc` (bounded) | 1,000 | `try_send` → **drops** on full |
| Per-connection events | `tokio::mpsc` (bounded) | 16 | `try_send` → **drops** on full |

### Key Observation

**All three channels use `try_send` (non-blocking) and silently drop messages when full.** This is the primary reason the smoke test achieves only ~42% unique event coverage under high load.

---

## 2. Root Cause Analysis

### RC-1: `try_send` drops notifications (CRITICAL)

**Location**: `notification.rs:~L231`
```rust
if let Err(tokio::sync::mpsc::error::TrySendError::Full(_)) = self.notify_tx.try_send(task) {
    log::warn!("Notification queue full, dropping notification");
}
```

When 24 publishers INSERT concurrently, the 10K queue fills and notifications are silently dropped. There is **no retry, no WAL, no backpressure** — the notification is gone forever.

**Impact**: The primary cause of missing events. Under sustained load, this can drop 50-60% of events.

### RC-2: Single notification worker (HIGH)

**Location**: `notification.rs:~L125` — one `tokio::spawn` processes ALL notifications sequentially.

The worker does:
1. Raft leadership check
2. `topic_publisher.publish_message()` — JSON serialization + RocksDB write (**blocking**)
3. Live query subscriber fan-out — per-subscription WHERE eval + JSON conversion + `try_send`

A single slow RocksDB write stalls the entire pipeline. With 24 concurrent publishers generating events faster than one worker can process, the queue backs up rapidly.

**Impact**: Amplifies RC-1 by keeping the queue full longer.

### RC-3: Non-atomic offset allocation (MEDIUM)

**Location**: `topic_publisher.rs:~L440`
```rust
fn next_offset(&self, topic_id: &str, partition_id: u32) -> u64 {
    let key = format!("{}:{}", topic_id, partition_id);
    let mut entry = self.offset_counters.entry(key).or_insert(0);
    let offset = *entry;
    *entry = offset + 1;
    offset
}
```

Uses `DashMap::entry()` which holds a shard lock. Under contention from the single worker this is safe, but if we add parallelism (fix for RC-2), concurrent `next_offset()` calls on the same topic could produce duplicate offsets since the increment is not truly atomic.

**Impact**: Latent bug — becomes critical when we parallelize.

### RC-4: Hash-based dedup is expensive and incomplete (LOW)

**Location**: `topic_publisher.rs:~L370` — `hash_row()` serializes the entire row to JSON and SHA-256 hashes it. Used for partition routing, not dedup. There is **no dedup mechanism** at all — if the same notification is somehow sent twice, it produces two messages with different offsets.

**Impact**: Duplication in the smoke test (1.9x ratio) is likely caused by notifications being generated twice at the table provider level, not by the publisher.

### RC-5: Consumer poll gap (LOW)

The consume endpoint returns immediately if no messages are available. With HTTP long-polling, there's an inherent gap between responses. However, since messages are durable in RocksDB, the consumer will catch up — this does **not** cause event loss, only latency.

**Impact**: Not a coverage issue. Consumers will eventually see all persisted messages.

### Summary

| Root Cause | Severity | Fix Complexity | Event Loss? |
|------------|----------|---------------|-------------|
| RC-1: `try_send` drops | CRITICAL | Medium | Yes — permanent loss |
| RC-2: Single worker | HIGH | Medium | Indirect — amplifies RC-1 |
| RC-3: Non-atomic offset | MEDIUM | Low | No (currently) — latent under parallelism |
| RC-4: No dedup | LOW | Low | No — causes duplication, not loss |
| RC-5: Poll gap | LOW | N/A | No — messages are durable |

---

## 3. async-channel Evaluation

### What is async-channel?

[async-channel](https://github.com/smol-rs/async-channel) is a lightweight MPMC (multi-producer, multi-consumer) async channel from the `smol-rs` ecosystem.

| Feature | `tokio::mpsc` | `async-channel` |
|---------|---------------|-----------------|
| Topology | MPSC (single consumer) | **MPMC** (multiple consumers) |
| Bounded send | `send()` (backpressure) + `try_send()` (drop) | `send()` (backpressure) + `try_send()` (drop) |
| Receiver cloning | ❌ Not clonable | ✅ Receiver is `Clone` |
| Runtime dependency | tokio-only | Runtime-agnostic (works with tokio, async-std, smol) |
| Close semantics | `Receiver` drop closes | Auto-closes when all senders OR all receivers are dropped |
| `is_full()` / `is_empty()` | ❌ | ✅ |
| Backpressure | `send().await` blocks until space | `send().await` blocks until space |
| Dependencies | Part of tokio | 1 dep (`event-listener-strategy`) |

### Recommendation: **Do NOT switch to `async-channel`**

Reasoning:
1. **We already depend on tokio** — adding `async-channel` creates a second async primitive for the same purpose.
2. **MPMC is not needed here** — the notification pipeline is fan-out (one queue → one worker → many subscribers), not many-consumers-from-one-queue.
3. **The core problem is `try_send`, not the channel type** — switching to `async-channel` with `try_send()` would exhibit the exact same drops. Switching to `send().await` fixes the problem regardless of channel library.
4. **`tokio::mpsc` already has `send().await`** with proper backpressure — we just need to use it instead of `try_send()`.

### What SHOULD change

Replace `try_send()` with `send().await` on the critical notification queue, which provides **bounded backpressure** — the table write will `await` until the queue has space rather than dropping the notification. This makes the write slightly slower under load (correct behavior — producers should slow down when the pipeline is saturated) but guarantees zero loss.

For per-subscriber live query channels (1K capacity), `try_send` with drops is acceptable since those are best-effort WebSocket pushes and messages are also persisted in topic logs.

---

## 4. Spec: 100% Unique Event Coverage

### Goal

Every table write (INSERT/UPDATE/DELETE) that matches a topic route MUST be published to the topic log exactly once, durably, before the write is acknowledged to the client.

### Design Principles

1. **Synchronous-path publishing**: Publish to the topic log in the same code path as the table write, not via an async side-channel.
2. **Backpressure over drops**: Under load, producers slow down instead of dropping events.
3. **Atomic offset allocation**: Use `AtomicU64` instead of `DashMap` entry locks.
4. **Ordered delivery**: Within a partition, consumers see events in offset order (guaranteed by RocksDB key ordering).
5. **At-least-once semantics**: Consumers may see duplicates (e.g., after crash recovery); dedup is consumer-side via idempotent processing or offset tracking.

### Architecture Change: Synchronous Publishing

**Current flow** (async, lossy):
```
table_write() → notify_table_change() → try_send(queue) → [single worker] → publish_message()
                                              ↑ DROPS HERE
```

**New flow** (synchronous, durable):
```
table_write() → topic_publisher.publish_for_table() → RocksDB write → return
                     ↓ (async, best-effort)
               notification_service.notify_live_queries()
```

The critical change: **topic publishing happens synchronously inside the table write path, NOT via an async queue.** The table write does not return until the topic message is persisted. Live query notifications remain async/best-effort (they have WebSocket push semantics and are stateless).

### Detailed Changes

#### 4.1 — `TopicPublisherService` gains a synchronous publish method

```rust
/// Called directly from table providers BEFORE returning from insert/update/delete.
/// This ensures the topic message is durable before the write is ack'd.
pub async fn publish_for_table(
    &self,
    table_id: &TableId,
    op: TopicOp,
    row_data: &Row,
    old_data: Option<&Row>,
    user_id: Option<&UserId>,
    schema: &SchemaRef,
) -> Result<Vec<PublishResult>, KalamDbError> {
    // 1. Look up routes for (table_id, op) from in-memory DashMap cache
    // 2. For each matching route:
    //    a. Evaluate optional filter_expr against row_data
    //    b. Extract payload per PayloadMode
    //    c. Compute partition_id (hash-based or default 0)
    //    d. Allocate offset atomically via AtomicU64
    //    e. Write TopicMessage to RocksDB via TopicMessageStore
    // 3. Return list of (topic_id, partition_id, offset) for each published message
}
```

#### 4.2 — Atomic offset counters

Replace `DashMap<String, u64>` with `DashMap<String, Arc<AtomicU64>>`:

```rust
fn next_offset(&self, topic_id: &str, partition_id: u32) -> u64 {
    let key = format!("{}:{}", topic_id, partition_id);
    let counter = self.offset_counters
        .entry(key)
        .or_insert_with(|| Arc::new(AtomicU64::new(0)));
    counter.fetch_add(1, Ordering::SeqCst)
}
```

This is safe for concurrent access from multiple table providers without the DashMap shard lock held during the increment.

#### 4.3 — Table providers call `publish_for_table` directly

In `shared_table_provider.rs`, `user_table_provider.rs`, `stream_table_provider.rs`:

```rust
// BEFORE (async, lossy):
self.notification_service.notify_table_change(user_id, &table_id, notification);

// AFTER (sync, durable — for topics):
if self.topic_publisher.has_routes_for_table(&table_id) {
    self.topic_publisher.publish_for_table(
        &table_id, TopicOp::Insert, &row, None, user_id.as_ref(), &schema
    ).await?;
}
// Live query notification remains async/best-effort:
self.notification_service.notify_live_queries(user_id, &table_id, notification);
```

#### 4.4 — `NotificationService` splits responsibilities

The current `NotificationService` handles BOTH topic publishing and live query fan-out. Split it:

| Concern | Owner | Delivery | Durability |
|---------|-------|----------|------------|
| Topic publishing | `TopicPublisherService` (called directly) | Synchronous | Durable (RocksDB) |
| Live query fan-out | `NotificationService` (renamed to `LiveNotificationService`) | Async queue + worker | Best-effort (WebSocket push) |

The 10K `tokio::mpsc` queue remains for live query notifications only, where `try_send` drops are acceptable.

#### 4.5 — Deduplication

Add optional message dedup at the publisher level using a content hash:

```rust
pub struct TopicMessage {
    // ... existing fields ...
    pub content_hash: Option<u64>,  // FNV hash of (table_id, op, row_key)
}
```

The publisher can maintain a small LRU cache of recent `(topic_id, partition_id, content_hash)` to detect and skip duplicates within a time window. This is **optional** — the primary fix is synchronous publishing which eliminates the duplication source.

#### 4.6 — Consumer-side dedup

Consumers should be resilient to at-least-once delivery:
- Track `(topic_id, partition_id, offset)` as a dedup key
- The `kalam-link` SDK `TopicConsumer` should deduplicate by offset before returning records to the application

### Expected Results

| Metric | Before | After |
|--------|--------|-------|
| Unique event coverage | ~42% | **100%** |
| Duplication ratio | ~1.9x | **1.0x** (with publisher dedup) or ≤1.05x (without) |
| Write latency (p99) | ~1ms | ~1.5ms (synchronous topic write adds ~0.5ms) |
| Throughput under load | Higher (drops events) | Lower but correct (backpressure) |

---

## 5. Spec: `kalamdb-publisher` Crate Extraction

### Goal

Extract topic publishing logic from `kalamdb-core/src/live/` into a dedicated `kalamdb-publisher` crate, as originally planned in the [024-topic-pubsub spec](../024-topic-pubsub/README.md#ownership-by-crate).

### What Moves to `kalamdb-publisher`

| Module | Current Location | New Location |
|--------|-----------------|--------------|
| `TopicPublisherService` | `kalamdb-core/src/live/topic_publisher.rs` | `kalamdb-publisher/src/service.rs` |
| Route matching & caching | embedded in `topic_publisher.rs` | `kalamdb-publisher/src/routing.rs` |
| Payload extraction | embedded in `topic_publisher.rs` | `kalamdb-publisher/src/payload.rs` |
| Offset allocation | embedded in `topic_publisher.rs` | `kalamdb-publisher/src/offset.rs` |

### What Stays in `kalamdb-core/src/live/`

| Module | Reason |
|--------|--------|
| `notification.rs` → `LiveNotificationService` | WebSocket/live query fan-out is core orchestration |
| `subscription.rs` | Raft-replicated live query registration |
| `manager/connections_manager.rs` | WebSocket connection lifecycle |
| `manager/queries_manager.rs` | Live query tracking |
| `models/` | Shared types for live queries (connection, subscription) |
| `helpers/` | Filter eval, failover, initial data, query parser |

### Crate Structure

```
backend/crates/kalamdb-publisher/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Re-exports: TopicPublisherService, PublishResult
│   ├── service.rs          # TopicPublisherService (main entry point)
│   ├── routing.rs          # RouteEntry, route matching, DashMap<TableId, Vec<RouteEntry>>
│   ├── payload.rs          # PayloadMode extraction (Key/Full/Diff), row serialization
│   ├── offset.rs           # AtomicU64-based offset counters, restore_offset_counters()
│   └── models.rs           # PublishResult, PublishError, TopicCacheStats
```

### Dependency Graph

```
kalamdb-publisher
├── kalamdb-commons        (TableId, TopicId, UserId, TopicOp, PayloadMode, Row types)
├── kalamdb-tables         (TopicMessageStore — for persistence)
├── kalamdb-system         (TopicsTableProvider, TopicOffsetsTableProvider — for route/offset management)
├── kalamdb-store          (StorageBackend trait — passed through, not used directly)
├── dashmap                (route cache, offset counters)
├── arrow                  (SchemaRef, RecordBatch for payload extraction)
├── serde / serde_json     (payload serialization)
├── log                    (tracing)
└── tokio                  (async runtime, for async methods)
```

```
kalamdb-core
├── kalamdb-publisher      (NEW — replaces inline topic_publisher.rs)
├── kalamdb-commons
├── kalamdb-tables
├── kalamdb-system
├── kalamdb-store
├── ...
```

### Interface Design

```rust
// kalamdb-publisher/src/lib.rs

/// Result of publishing a single message to a topic partition.
#[derive(Debug, Clone)]
pub struct PublishResult {
    pub topic_id: String,
    pub partition_id: u32,
    pub offset: u64,
}

/// The core publisher service. Thread-safe, cheaply clonable via Arc internals.
pub struct TopicPublisherService {
    // Route cache: table_id → matching routes
    table_routes: DashMap<TableId, Vec<RouteEntry>>,
    // Offset counters: "topic_id:partition_id" → atomic counter
    offset_counters: DashMap<String, Arc<AtomicU64>>,
    // Persistence
    message_store: Arc<TopicMessageStore>,
    // System table for consumer offsets
    offsets_provider: Arc<TopicOffsetsTableProvider>,
    // System table for topic metadata (for cache refresh)
    topics_provider: Arc<TopicsTableProvider>,
}

impl TopicPublisherService {
    /// Create a new publisher service. Called once at server startup.
    pub fn new(
        message_store: Arc<TopicMessageStore>,
        offsets_provider: Arc<TopicOffsetsTableProvider>,
        topics_provider: Arc<TopicsTableProvider>,
    ) -> Self;

    /// Check if any topic routes exist for a table (fast DashMap lookup).
    pub fn has_routes_for_table(&self, table_id: &TableId) -> bool;

    /// Synchronously publish change events for a table write.
    /// Called directly from table providers in the write path.
    pub async fn publish_for_table(
        &self,
        table_id: &TableId,
        op: TopicOp,
        row_data: &Row,
        old_data: Option<&Row>,
        user_id: Option<&UserId>,
        schema: &SchemaRef,
    ) -> Result<Vec<PublishResult>, KalamDbError>;

    /// Fetch messages for a consumer (used by consume handler).
    pub fn fetch_messages(
        &self,
        topic_id: &str,
        partition_id: u32,
        start_offset: u64,
        limit: usize,
    ) -> Result<Vec<TopicMessage>, KalamDbError>;

    /// Acknowledge consumer offset.
    pub async fn ack_offset(
        &self,
        topic_id: &str,
        group_id: &str,
        partition_id: u32,
        offset: u64,
    ) -> Result<(), KalamDbError>;

    /// Refresh route cache from system.topics table.
    pub async fn refresh_routes(&self) -> Result<(), KalamDbError>;

    /// Restore offset counters from persisted messages (called at startup).
    pub async fn restore_offset_counters(&self) -> Result<(), KalamDbError>;

    /// Get cache statistics for observability.
    pub fn cache_stats(&self) -> TopicCacheStats;
}
```

### Key Differences from Current `topic_publisher.rs`

1. **`publish_for_table()` replaces `publish_message()`** — takes table-level context (schema, row, op) instead of pre-built `ChangeNotification`, so the publisher owns payload extraction.
2. **`AtomicU64` offsets** — replaces `DashMap<String, u64>` for lock-free concurrent increments.
3. **No `ChangeNotification` dependency** — the publisher works with raw `Row` + `SchemaRef` from Arrow, decoupled from the live query notification model.
4. **No notification queue** — called directly, no `try_send`, no drops.

---

## 6. Implementation Tasks

### Phase 1: Crate Scaffold (1 day) ✅ COMPLETE

- [x] Create `backend/crates/kalamdb-publisher/Cargo.toml` with workspace deps
- [x] Create `lib.rs`, `service.rs`, `routing.rs`, `payload.rs`, `offset.rs`, `models.rs`
- [x] Add `kalamdb-publisher` to workspace `Cargo.toml` members
- [x] Add `kalamdb-publisher = { path = "../kalamdb-publisher" }` to `kalamdb-core/Cargo.toml`

### Phase 2: Extract & Refactor (2-3 days) ✅ COMPLETE

- [x] Move route matching logic from `topic_publisher.rs` → `routing.rs`
- [x] Move payload extraction logic → `payload.rs`
- [x] Move offset allocation logic → `offset.rs` with `AtomicU64`
- [x] Build `TopicPublisherService` in `service.rs` composing the above modules
- [x] Add `publish_for_table()` method (synchronous publishing API)
- [x] Update `kalamdb-core/src/live/mod.rs` to re-export from `kalamdb-publisher`
- [x] Remove `topic_publisher.rs` from `kalamdb-core/src/live/`

### Phase 3: Synchronous Publishing (1-2 days) ✅ COMPLETE

- [x] Add `Arc<TopicPublisherService>` to table provider constructors (shared, user, stream)
- [x] In each provider's `insert()`, `update()`, `delete()`: call `publish_for_table()` before returning
- [x] Split `NotificationService` → `LiveNotificationService` (live queries only)
- [x] Remove topic publishing from the notification worker
- [x] Update `notify_table_change` → `notify_live_queries` in table providers

**Implementation Notes:**
- Created `TopicPublisher` trait in `kalamdb-system` to avoid circular deps (publisher depends on system, not core)
- Added `publish_to_topics()` helper on `TableProviderCore` — called from all 12 write sites across shared/user/stream providers
- `_table` metadata injected into Full/Diff payloads via `extract_full_row_with_metadata()` in `payload.rs`
- `has_subscribers()` in notification worker now only checks live queries (topic publishing fully removed)

### Phase 4: Testing & Validation (1-2 days) ✅ COMPLETE

- [x] Update existing topic integration tests
- [x] Re-run high-load smoke test — **100% unique event coverage achieved** (480/480 events)
- [x] Add unit tests for `routing.rs`, `payload.rs`, `offset.rs` — 10/10 publisher unit tests pass
- [ ] Benchmark: measure write latency impact of synchronous publishing *(deferred — not blocking)*
- [x] Verify consumer offset tracking still works correctly

**Test Results:**
- Unique event coverage: **100.0%** (up from 42.1% pre-spec)
- Duplication ratio: **1.0x** (up from 1.9x pre-spec)
- Test runtime: **17.6s** (down from 22s)
- Publisher unit tests: **10/10 PASS** in 0.049s

**Bugs fixed during validation:**
1. `_table` field missing from payload — consumers couldn't identify source table
2. `Int64`/BIGINT values serialized as JSON strings (for JS precision safety) — test ID parsing fixed
3. Stream table creation required `WITH (TTL_SECONDS = ...)` clause — `execute_sql` helper now validates response status

### Phase 5: Cleanup (0.5 day) — REMAINING

- [x] Remove dead code from `notification.rs` (topic publishing branches)
- [ ] Update `AGENTS.md` project structure section
- [ ] Update `024-topic-pubsub` implementation status

---

## 7. Migration Guide

### For `kalamdb-core` consumers

```rust
// BEFORE: Import from kalamdb-core::live
use kalamdb_core::live::TopicPublisherService;

// AFTER: Import from kalamdb-publisher (re-exported through kalamdb-core::live)
use kalamdb_core::live::TopicPublisherService;  // Still works (re-export)
// OR: Direct import
use kalamdb_publisher::TopicPublisherService;
```

### For table providers

```rust
// BEFORE:
// notification_service.notify_table_change(user_id, &table_id, notification);

// AFTER:
if self.topic_publisher.has_routes_for_table(&table_id) {
    self.topic_publisher.publish_for_table(
        &table_id, TopicOp::Insert, &row, None, user_id.as_ref(), &schema
    ).await?;
}
self.notification_service.notify_live_queries(user_id, &table_id, notification);
```

### For `AppContext`

```rust
// AppContext gains a direct reference to the publisher:
pub struct AppContext {
    // ...existing fields...
    pub topic_publisher: Arc<TopicPublisherService>,  // NEW — direct access
    pub live_notification_service: Arc<LiveNotificationService>,  // RENAMED
}
```

---

## 8. Validation Checklist

- [x] `cargo check` passes for all workspace crates
- [x] `cargo nextest run` — all existing tests pass
- [x] High-load smoke test achieves ≥99% unique event coverage — **100.0%** ✅
- [x] Duplication ratio ≤1.05x — **1.0x** ✅
- [x] No `try_send` in the topic publishing path
- [ ] Write latency p99 ≤2ms (from current ~1ms) *(benchmark deferred)*
- [x] `kalamdb-publisher` has no dependency on `kalamdb-core` (no circular deps)
- [x] `kalamdb-core` depends on `kalamdb-publisher` (not vice versa)
- [x] All topic-related types are in `kalamdb-commons` or `kalamdb-publisher`
- [x] `notification.rs` no longer contains topic publishing logic

---

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Synchronous publishing adds latency to writes | ~0.5ms per write to RocksDB is acceptable; can batch multiple routes into single RocksDB `WriteBatch` |
| Table providers need `Arc<TopicPublisherService>` | Already pattern-matched by `Arc<AppContext>` — add publisher to context |
| Offset counter restart race | `restore_offset_counters()` runs at startup before accepting writes; `AtomicU64` prevents runtime races |
| Circular dependency `kalamdb-publisher` ↔ `kalamdb-core` | Publisher depends only on `kalamdb-commons`, `kalamdb-tables`, `kalamdb-system` — no core dependency |
| Breaking change for `NotificationService` trait | Keep trait name, add `notify_live_queries()` method, deprecate `notify_table_change()` |
