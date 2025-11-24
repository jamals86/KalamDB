# Research: Live Queries over WebSocket Connections

## Decisions

### Reuse single WebSocket per client with multiplexed subscriptions
- **Decision**: Use one WebSocket per logical client context and allow multiple live query subscriptions to share that connection.
- **Rationale**: Reduces connection overhead, aligns with the spec, and simplifies resource management while still supporting multiple connections per user when needed.
- **Alternatives considered**: One WebSocket per subscription (too many connections per client); HTTP long-polling or SSE (does not meet bidirectional requirements).

### Auto-resubscribe and SeqId-based resume on reconnect
- **Decision**: On reconnect, the SDK automatically re-subscribes all previously active subscriptions and passes the last received SeqId per subscription so the server can resume from that point.
- **Rationale**: Provides intuitive "live" semantics, avoids data gaps/duplicates, and hides transient network failures from application code.
- **Alternatives considered**: Require applications to recreate subscriptions manually; drop subscriptions on disconnect; ignore SeqIds and accept possible gaps/duplicates.

### Admin kill-connection semantics
- **Decision**: Killing a connection immediately closes the WebSocket and terminates all subscriptions on that connection, removing or inactivating their rows in `system.live_queries` promptly.
- **Rationale**: Gives operators a clear, strong control for misbehaving clients and simplifies reasoning about resource cleanup.
- **Alternatives considered**: Per-subscription admin termination only; soft limits or throttling without explicit kill; leaving cleanup to timeouts.

### Auth expiry behavior
- **Decision**: When auth for a connection expires or is revoked, active subscriptions on that connection are terminated promptly and no further updates are delivered until a new authenticated connection is established.
- **Rationale**: Maintains security guarantees by ensuring live data is not streamed over unauthorized channels.
- **Alternatives considered**: Allow subscriptions to continue until natural connection close; grace periods without clear enforcement.

### O(1) connection lookup tables
- **Decision**: Maintain hash maps keyed by `(user_id, connection_id)` (and optionally table identifiers) so locating a WebSocket handler is O(1) without scanning every user.
- **Rationale**: Ensures row-level change propagation remains fast even with thousands of connections, and fulfills the "no looping" requirement from product.
- **Alternatives considered**: Linear scans or per-user lists (too slow at scale); tree structures (unnecessary overhead).

### Immediate RocksDB cleanup on unsubscribe/close
- **Decision**: When a subscription is unsubscribed or a socket closes, delete all affected rows from the RocksDB-backed `system.live_queries` store immediately, not lazily.
- **Rationale**: Keeps metadata accurate for admins/kill commands and prevents stale subscriptions from accumulating.
- **Alternatives considered**: Deferred cleanup jobs; TTL-based expiration (adds latency and complexity).

### Shared Kalam-link library for lifecycle management
- **Decision**: Place connection lifecycle logic (connect/disconnect/subscribe/unsubscribe/resume) in the Kalam-link shared library so multiple SDKs (TypeScript, others) reuse the same implementation; the TypeScript SDK exposes wrappers to those helpers.
- **Rationale**: Avoids duplicated state machines, ensures consistent behavior across clients, and makes future SDKs cheaper to build.
- **Alternatives considered**: Keeping logic in each SDK (harder to maintain, divergence risk); server-only orchestration (less flexible offline/testing support).

## Open Questions

No outstanding technical clarifications remain from the feature specification at this stage.
