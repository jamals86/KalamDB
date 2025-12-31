# ADR-017: WebSocket Subscription Memory Optimization

**Date**: 2025-11-29  
**Status**: Accepted  
**Context**: Phase 14 (014-live-queries-websocket)

## Context

KalamDB supports real-time live queries via WebSocket connections. Each client connection can have multiple active subscriptions, and we need to scale to **hundreds of thousands of concurrent connections** with multiple subscriptions each.

Initial implementation stored full subscription state in multiple places:
- Once in `ConnectionState.subscriptions` (per-connection storage)
- Again in `user_table_subscriptions` index (for O(1) notification routing)

This duplication, combined with large fields like SQL strings and parsed AST expressions, resulted in ~1,600 bytes per subscription—a significant memory overhead that limits scalability.

### Memory Breakdown (Before Optimization)

```
SubscriptionState (stored TWICE):
├── live_id: LiveQueryId           ~192 bytes (3 Strings)
├── table_id: TableId              ~128 bytes (2 Strings)
├── options: SubscriptionOptions    ~48 bytes (unused!)
├── sql: String                    ~224 bytes (avg 200 char query)
├── filter_expr: Option<Expr>      ~200-500 bytes (AST can be large)
├── projections: Option<Vec<...>>   ~0-100 bytes (always None!)
├── batch_size: usize                ~8 bytes
├── snapshot_end_seq: Option<SeqId> ~16 bytes
└── notification_tx: Arc<...>        ~8 bytes
                                   ───────────
                            Total: ~800+ bytes × 2 = ~1,600 bytes/subscription
```

## Decision

We implemented a **two-tier storage model** that separates full subscription state from lightweight routing handles:

### 1. SubscriptionState (Full State - Stored Once)

Stored only in `ConnectionState.subscriptions`. Contains all data needed for batch fetching and state management.

```rust
pub struct SubscriptionState {
    pub live_id: LiveQueryId,
    pub table_id: TableId,
    pub sql: Arc<str>,                    // Arc for zero-copy sharing
    pub filter_expr: Option<Arc<Expr>>,   // Arc for sharing with handle
    pub batch_size: usize,
    pub snapshot_end_seq: Option<SeqId>,
    pub notification_tx: Arc<NotificationSender>,
}
```

**Optimizations Applied:**
- `Arc<str>` for SQL (zero-copy, no cloning)
- `Arc<Expr>` for filter expression (shared with SubscriptionHandle)
- Removed unused `options` field (batch_size already extracted)
- Removed unused `projections` field (was always None)

### 2. SubscriptionHandle (Lightweight - For Indices)

Used in `user_table_subscriptions` index for O(1) notification routing. Contains only the minimum data needed to filter and route notifications.

```rust
pub struct SubscriptionHandle {
    pub live_id: LiveQueryId,
    pub filter_expr: Option<Arc<Expr>>,   // Shared with SubscriptionState
    pub notification_tx: Arc<NotificationSender>,
}
```

**Size: ~48 bytes** (vs ~800+ bytes for full clone)

## Memory Layout

```
┌─────────────────────────────────────────────────────────────┐
│                    ConnectionState                           │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ subscriptions: DashMap<String, SubscriptionState>      │ │
│  │   "sub-1" → SubscriptionState { ... }  (~200 bytes)    │ │
│  │   "sub-2" → SubscriptionState { ... }                  │ │
│  └────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
                              │
                              │ Arc<Expr> shared reference
                              ▼
┌─────────────────────────────────────────────────────────────┐
│              ConnectionsManager Indices                      │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ user_table_subscriptions:                              │ │
│  │   DashMap<(UserId, TableId), Vec<SubscriptionHandle>>  │ │
│  │                                                        │ │
│  │   (user1, ns.table) → [                                │ │
│  │     SubscriptionHandle { live_id, filter_expr, tx }    │ │
│  │   ]  (~48 bytes per handle)                            │ │
│  └────────────────────────────────────────────────────────┘ │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ live_id_to_connection: DashMap<LiveQueryId, ConnId>    │ │
│  └────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

## Memory Usage Summary

### Per Subscription

| Component | Before | After | Savings |
|-----------|--------|-------|---------|
| SubscriptionState (1st copy) | ~800 bytes | ~200 bytes | 75% |
| SubscriptionState (2nd copy) | ~800 bytes | ~48 bytes (Handle) | 94% |
| **Total per Subscription** | **~1,600 bytes** | **~248 bytes** | **85%** |

### Per Connection

| Component | Size | Notes |
|-----------|------|-------|
| ConnectionId | ~56 bytes | String wrapper |
| UserId | ~56 bytes | Optional, post-auth |
| ConnectionInfo (client_ip) | ~56 bytes | String wrapper |
| is_authenticated | 1 byte | bool |
| auth_started | 1 byte | bool |
| connected_at | 16 bytes | Instant |
| last_heartbeat | 16 bytes | Instant |
| notification_tx | 8 bytes | Arc pointer |
| event_tx | 24 bytes | Channel sender |
| subscriptions (DashMap base) | ~64 bytes | Per-shard overhead |
| **Base Connection Overhead** | **~300 bytes** | Without subscriptions |

### Scalability Projections

| Scenario | Before | After | Memory Saved |
|----------|--------|-------|--------------|
| 10K connections × 1 sub each | 19 MB | 5.5 MB | 13.5 MB |
| 10K connections × 10 subs each | 163 MB | 28 MB | 135 MB |
| 100K connections × 1 sub each | 190 MB | 55 MB | 135 MB |
| 100K connections × 10 subs each | 1.63 GB | 280 MB | 1.35 GB |
| 1M connections × 1 sub each | 1.9 GB | 550 MB | 1.35 GB |

## Consequences

### Positive

1. **6x More Concurrent Subscriptions**: Same memory supports 6x more live queries
2. **Zero-Copy Sharing**: Arc<str> and Arc<Expr> eliminate redundant allocations
3. **O(1) Notification Routing**: Lightweight handles in indices maintain performance
4. **Reduced GC Pressure**: Fewer allocations means less memory churn
5. **Better Cache Locality**: Smaller handles fit more entries per cache line

### Negative

1. **Slight Complexity**: Two struct types (State vs Handle) instead of one
2. **Arc Overhead**: 8 bytes per Arc + atomic ref counting (negligible)

### Neutral

1. **Same API**: External callers unaffected by internal restructuring
2. **Same Functionality**: All features preserved (filtering, batching, etc.)

## Implementation Details

### Files Changed

- `kalamdb-core/src/live/connections_manager.rs`
  - Added `SubscriptionHandle` struct
  - Modified `SubscriptionState` to use `Arc<str>` and `Arc<Expr>`
  - Removed unused `options` and `projections` fields
  - Changed `user_table_subscriptions` to store `Vec<SubscriptionHandle>`

- `kalamdb-core/src/live/subscription.rs`
  - Updated `register_subscription()` to create both State and Handle
  - Removed `projections` parameter (unused)

- `kalamdb-core/src/live/manager/core.rs`
  - Updated `register_subscription()` signature

### Key Code Patterns

```rust
// Creating subscription with shared Arc references
let filter_expr_arc = filter_expr.map(Arc::new);

let subscription_state = SubscriptionState {
    live_id: live_id.clone(),
    table_id: table_id.clone(),
    sql: request.sql.as_str().into(),  // Arc<str>
    filter_expr: filter_expr_arc.clone(),  // Shared Arc
    batch_size,
    snapshot_end_seq: None,
    notification_tx: notification_tx.clone(),
};

let subscription_handle = SubscriptionHandle {
    live_id: live_id.clone(),
    filter_expr: filter_expr_arc,  // Same Arc, zero-copy
    notification_tx,
};

// Store full state in connection
state.subscriptions.insert(id, subscription_state);

// Store lightweight handle in index
registry.index_subscription(user_id, conn_id, live_id, table_id, subscription_handle);
```

## Related ADRs

- [ADR-001: Table-Per-User Architecture](./ADR-001-table-per-user-architecture.md) - Foundation for scalable subscriptions
- [ADR-004: Commons Crate](./ADR-004-commons-crate.md) - Shared types like LiveQueryId

## References

- [Stress Tests and Memory Leak Detection](../../development/testing-strategy.md#3-stress-tests-and-memory-leak-detection) - Memory testing approach
- Phase 14 Live Queries WebSocket implementation
