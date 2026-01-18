# Spec 021: Raft Replication Semantics and Consistency Model

## Status: Analysis & Design (REVISED)
## Created: 2026-01-18
## Updated: 2026-01-18 (v2 - with internal usage analysis)

---

## Executive Summary

This document analyzes KalamDB's current Raft replication model, identifies where leader-only reads are appropriate vs. where local reads are required, and provides a nuanced design that accounts for internal system operations.

**Key Finding**: Leader-only reads is NOT a universal solution. KalamDB has multiple internal use cases that REQUIRE local data access on followers.

---

## 1. Current KalamDB Architecture Analysis

### 1.1 What Currently Happens

Your assessment is **CORRECT**. Here's the precise flow:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    CURRENT KALAMDB FLOW                              │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  Client INSERT → Leader Node                                         │
│       │                                                              │
│       ▼                                                              │
│  1. RaftExecutor.execute_user_data(cmd)                              │
│       │                                                              │
│       ▼                                                              │
│  2. RaftManager.propose_user_data(shard, cmd)                        │
│       │                                                              │
│       ▼                                                              │
│  3. RaftGroup.propose(command)                                       │
│       │                                                              │
│       ▼                                                              │
│  4. OpenRaft.client_write(command_bytes)                             │
│       │                                                              │
│       ├──► Leader appends to local log                               │
│       ├──► OpenRaft replicates to followers (async, parallel)        │
│       ├──► Waits for QUORUM acknowledgment (log replication only!)   │
│       ▼                                                              │
│  5. Entry COMMITTED when quorum ACKs log append                      │
│       │                                                              │
│       ▼                                                              │
│  6. State machine apply() on LEADER ONLY at this point               │
│       │  └── UserDataStateMachine.apply(index, term, command)        │
│       │         └── UserDataApplier.insert() → RocksDB               │
│       │                                                              │
│  7. client_write() returns to caller                                 │
│       │                                                              │
│       ▼                                                              │
│  8. CLIENT GETS SUCCESS RESPONSE                                     │
│                                                                      │
│  ════════════════════════════════════════════════════════════════   │
│  MEANWHILE (async, no coordination with client):                     │
│                                                                      │
│  9. Followers eventually apply() to their state machines             │
│       └── May happen seconds later                                   │
│       └── Data NOT visible on followers until applied                │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### 1.2 Current Guarantees

| Guarantee | Status | Notes |
|-----------|--------|-------|
| **Durability (Log)** | ✅ Quorum | Log entry survives f failures (n=2f+1) |
| **Durability (Data)** | ⚠️ Leader only | Data in RocksDB only on leader at response time |
| **Read-after-write (Leader)** | ✅ Yes | Leader has applied before response |
| **Read-after-write (Followers)** | ❌ No | Followers haven't applied yet |
| **Cross-node consistency** | ❌ Eventual | Client may read stale data from followers |

### 1.3 The Core Problem

```rust
// In raft_group.rs lines 407-449:
pub async fn propose_with_index(&self, command: RaftCommand) -> Result<(RaftResponse, u64), RaftError> {
    // ...
    let response = raft
        .client_write(command_bytes)  // ← Waits for quorum log ACK
        .await                         // ← NOT quorum apply!
        .map_err(|e| RaftError::Proposal(...))?;
    
    // Response returned here, but followers haven't applied yet
}
```

OpenRaft's `client_write()` returns when:
1. Log entry is replicated to quorum of nodes ✅
2. Log entry is **applied** on the leader ✅
3. Log entry is **NOT necessarily applied** on followers ❌

---

## 2. CockroachDB's Approach

### 2.1 Architecture Overview

CockroachDB uses a **Raft + Range-based architecture** with per-range leases:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    COCKROACHDB ARCHITECTURE                          │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │                        Range                                 │    │
│  │  - Contains contiguous key span (e.g., /Table/1/Row/1-1000) │    │
│  │  - Has exactly ONE leaseholder (handles reads/writes)        │    │
│  │  - 3-5 replicas via Raft                                     │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                      │
│  Write Flow:                                                         │
│  ───────────                                                         │
│  1. Client → Gateway Node → Leaseholder (may be different nodes)    │
│  2. Leaseholder proposes write to Raft                               │
│  3. Raft replicates to quorum                                        │
│  4. Leaseholder applies AND returns success                          │
│                                                                      │
│  Read Flow (Default - Leaseholder reads):                            │
│  ─────────                                                           │
│  1. Client → Gateway → Leaseholder                                   │
│  2. Leaseholder checks lease validity                                │
│  3. Reads from local storage (no Raft needed!)                       │
│                                                                      │
│  Read Flow (Follower reads - opt-in):                                │
│  ─────────                                                           │
│  1. Client → Any replica with AS OF SYSTEM TIME                      │
│  2. Replica serves stale read (bounded staleness)                    │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### 2.2 Key Mechanisms

#### Lease-Based Reads
```
┌──────────────────────────────────────────────────────────────┐
│                      LEASEHOLDER                              │
│  - Single node per range holds the "lease"                   │
│  - ALL reads and writes go through leaseholder               │
│  - Lease is replicated via Raft (survives failures)          │
│  - Lease has expiration time (typically 9 seconds)           │
│  - Leaseholder can serve reads WITHOUT Raft round-trip       │
└──────────────────────────────────────────────────────────────┘
```

#### Closed Timestamps (for Follower Reads)
```
┌──────────────────────────────────────────────────────────────┐
│                   CLOSED TIMESTAMPS                           │
│  - Leaseholder periodically publishes "closed timestamp"     │
│  - T_closed = "no more writes will occur at or before T"     │
│  - Followers can safely read at T_closed without coordination│
│  - Default closed timestamp lag: 500ms                       │
│  - Enables AS OF SYSTEM TIME queries on any replica          │
└──────────────────────────────────────────────────────────────┘
```

### 2.3 CockroachDB Write Confirmation

```
Client sends: INSERT INTO foo VALUES (1, 'bar')

1. Gateway routes to leaseholder of range containing row
2. Leaseholder:
   a. Acquires latches (concurrency control)
   b. Checks for conflicts (write intents, etc.)
   c. Proposes Raft command
3. Raft replicates to quorum (log level)
4. Leaseholder applies command locally
5. Leaseholder returns SUCCESS to client
   └── At this point: quorum has log, leaseholder has applied
       Followers may not have applied yet (same as KalamDB!)

HOWEVER: CockroachDB's consistency is maintained because:
- ALL reads go to leaseholder (default)
- Leaseholder always has latest data
- Follower reads only allowed with explicit AS OF SYSTEM TIME
```

---

## 3. YugabyteDB's Approach

### 3.1 Architecture Overview

YugabyteDB uses **Raft per tablet** with leader-based reads by default:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    YUGABYTEDB ARCHITECTURE                           │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │                        Tablet                                │    │
│  │  - Horizontal partition of a table                           │    │
│  │  - Has Raft group with 3+ replicas                           │    │
│  │  - One LEADER handles reads/writes                           │    │
│  │  - RocksDB storage per replica                               │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                      │
│  DocDB Layer (Document/Key-Value):                                   │
│  ─────────────────────────────────                                   │
│  - Handles MVCC, transactions, encoding                              │
│  - Sits above Raft consensus layer                                   │
│                                                                      │
│  Write Flow:                                                         │
│  ───────────                                                         │
│  1. Client → YB-TServer → Tablet Leader                              │
│  2. Leader proposes Raft entry                                       │
│  3. Entry replicated to majority (log)                               │
│  4. Leader applies to RocksDB via DocDB                              │
│  5. SUCCESS returned to client                                       │
│                                                                      │
│  Read Flow (Default - Leader reads):                                 │
│  ─────────                                                           │
│  1. Client → Leader                                                  │
│  2. Leader reads from local RocksDB                                  │
│  3. Returns result (no Raft needed for reads!)                       │
│                                                                      │
│  Read Flow (Follower reads - opt-in via yb_read_from_followers):     │
│  ─────────                                                           │
│  1. Client → Any replica                                             │
│  2. Replica serves from local RocksDB                                │
│  3. May be stale (configurable staleness bounds)                     │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### 3.2 Key Mechanisms

#### Leader Leases (Raft Leader Leases)
```
┌──────────────────────────────────────────────────────────────┐
│                    LEADER LEASES                              │
│  - Raft leader holds a "leader lease"                        │
│  - Lease is time-bounded (default: 2 seconds)                │
│  - Leader can serve reads locally without quorum check       │
│  - On leader change, old leader's lease must expire first    │
│  - Prevents stale reads during leader transitions            │
└──────────────────────────────────────────────────────────────┘
```

#### Hybrid Logical Clocks (HLC)
```
┌──────────────────────────────────────────────────────────────┐
│               HYBRID LOGICAL CLOCKS                           │
│  - Combines physical time + logical counter                  │
│  - Enables MVCC with consistent ordering                     │
│  - Allows safe follower reads at specific timestamps         │
│  - Propagated across nodes to maintain causality             │
└──────────────────────────────────────────────────────────────┘
```

### 3.3 YugabyteDB Write Confirmation

```
Client sends: INSERT INTO foo VALUES (1, 'bar')

1. Client connects to any YB-TServer
2. TServer routes to tablet leader for the row's partition
3. Leader:
   a. Assigns HLC timestamp
   b. Proposes Raft entry with MVCC data
   c. Waits for quorum ACK (log replication)
   d. Applies to local RocksDB (via DocDB)
4. Returns SUCCESS to client
   └── Same as CockroachDB: quorum has log, leader has applied

Consistency maintained because:
- Reads go to leader by default
- Follower reads are opt-in with staleness bounds
- HLC ensures ordering even with follower reads
```

---

## 4. Comparison: KalamDB vs CockroachDB vs YugabyteDB

| Aspect | KalamDB (Current) | CockroachDB | YugabyteDB |
|--------|-------------------|-------------|------------|
| **Raft Scope** | Per-shard (32 user + 1 shared) | Per-range (key span) | Per-tablet (partition) |
| **Write Commit** | Quorum log ACK | Quorum log ACK | Quorum log ACK |
| **Leader Apply** | ✅ Before response | ✅ Before response | ✅ Before response |
| **Follower Apply** | ❌ Async (eventual) | ❌ Async (eventual) | ❌ Async (eventual) |
| **Default Reads** | ⚠️ Any node? | Leaseholder only | Leader only |
| **Follower Reads** | N/A | AS OF SYSTEM TIME | yb_read_from_followers |
| **Read Consistency** | ⚠️ Undefined | Strong (leaseholder) | Strong (leader) |
| **MVCC** | ❌ No | ✅ Yes (timestamp-based) | ✅ Yes (HLC-based) |

### 4.1 The Key Insight

**All three systems have the same Raft behavior:**
- Quorum acknowledges log replication
- Leader applies before returning success  
- Followers apply asynchronously

**The difference is in READ handling:**
- **CockroachDB/YugabyteDB**: Reads go to leader/leaseholder by default
- **KalamDB**: Reads may go to any node (undefined behavior currently)

---

## 5. Current KalamDB Read Path Analysis

Let me trace where reads actually go in KalamDB:

```rust
// In kalamdb-api, SQL queries are handled by:
// sql/executor/handlers/query.rs → DataFusion query

// DataFusion uses TableProviders which read from:
// - UserTableProvider → RocksDB via table providers
// - SharedTableProvider → RocksDB via table providers
// - StreamTableProvider → RocksDB via table providers

// The question: Does the TableProvider read from local RocksDB 
// regardless of whether this node is the leader?

// Looking at user_tables/provider.rs:
// It reads from the local RocksDB partition directly!
// There's NO check for "am I the leader?"
```

**This is the root cause of your consistency issue:**

1. Client writes to Leader Node 1 → Success returned
2. Client reads from Follower Node 2 → Reads local RocksDB
3. Follower Node 2 hasn't applied yet → **STALE READ**

---

## 5.1 CRITICAL: Internal Usage Patterns That REQUIRE Local Reads

Before jumping to "leader-only reads," we must analyze KalamDB's internal operations. Many **require local data access on all nodes**, not just the leader.

### 5.1.1 Job Executors - Local Phase Operations

From `executor_trait.rs`:
```rust
/// Execute local work phase (runs on ALL nodes in cluster)
///
/// This phase handles node-local operations that don't require cluster coordination:
/// - RocksDB flushes (each node flushes its own buffered data)
/// - Local cache eviction
/// - Local file cleanup
/// - RocksDB compaction
async fn execute_local(&self, ctx: &JobContext<Self::Params>) -> Result<JobDecision, KalamDbError>
```

**Jobs that MUST read local data:**

| Job Type | Why Local Reads Required |
|----------|-------------------------|
| **FlushExecutor** | Reads local RocksDB rows to write Parquet files. Each node flushes ITS OWN buffered data. |
| **StreamEvictionExecutor** | Scans local stream logs to delete expired records. Uses `store.delete_old_logs(cutoff_ms)`. |
| **RetentionExecutor** | Scans local table for soft-deleted rows past retention period. |
| **CleanupExecutor** | Deletes local table data from RocksDB stores. |
| **CompactExecutor** | Runs RocksDB compaction on local column families. |

**Key Insight**: These jobs read from `store.scan_*()` methods which access LOCAL RocksDB. If we forwarded these to the leader:
1. ❌ The leader would read ITS data, not the follower's buffered data
2. ❌ Followers would never flush/evict/compact their own RocksDB
3. ❌ Followers' RocksDB would grow unbounded

### 5.1.2 Flush Jobs - The Critical Case

From `flush.rs`:
```rust
let provider = provider_arc
    .as_any()
    .downcast_ref::<crate::providers::UserTableProvider>()?;
let store = provider.store.clone();  // <-- LOCAL store

let flush_job = UserTableFlushJob::new(
    app_ctx.clone(),
    table_id.clone(),
    store,  // <-- Reads from THIS node's RocksDB
    schema.clone(),
    schema_registry.clone(),
    app_ctx.manifest_service(),
);
```

**Current Problem (from Notes.md)**:
> "Flush/compaction: only the leader runs flush and compaction jobs today. Followers won't write Parquet, won't compact, and won't purge their local RocksDB data."

This is a **separate issue** - followers need to run local flush jobs too, but:
- Only leader should write Parquet to shared storage (S3)
- All nodes should delete flushed rows from their local RocksDB

### 5.1.3 Live Query Notifications

From `manager/core.rs`:
```rust
/// Notify subscribers about a table change (fire-and-forget async)
///
/// With Raft replication, each node handles its own live query notifications locally.
/// When data is applied via Raft on followers, the providers call this method directly.
pub fn notify_table_change_async(
    self: &Arc<Self>,
    user_id: UserId,
    table_id: TableId,
    notification: ChangeNotification,
) {
    self.notification_service.notify_async(user_id, table_id, notification)
}
```

**Why this works correctly**:
- When followers apply Raft log entries via `UserDataStateMachine.apply()`
- The applier calls `UserDataApplier.insert()` which writes to local RocksDB
- The provider fires `notify_table_change_async()` to local live queries
- Clients connected to that follower get notified immediately

**If we forwarded reads to leader**:
- ❌ Live query clients connected to followers wouldn't get notifications
- ❌ Or we'd need to route all WebSocket connections to leader only

### 5.1.4 Manifest Service - Local Cache

From `manifest/service.rs`:
```rust
/// Hot cache: moka::sync::Cache<(TableId, Option<UserId>), Arc<ManifestCacheEntry>> for fast reads
/// Persistent store: RocksDB manifest_cache column family for crash recovery
```

The manifest cache is **node-local** - each node maintains its own hot cache for query planning. This is correct because:
- Parquet files are in shared storage (S3), same for all nodes
- Cache is just an optimization layer
- Stale cache is OK (refresh on miss)

### 5.1.5 System Tables - Where Consistency Matters

| System Table | Consistency Requirement | Recommended Approach |
|--------------|------------------------|---------------------|
| `system.tables` | Strong (DDL operations) | Read from leader |
| `system.namespaces` | Strong (DDL operations) | Read from leader |
| `system.users` | Strong (auth operations) | Read from leader |
| `system.storages` | Strong (storage ops) | Read from leader |
| `system.jobs` | Eventual OK (monitoring) | Local read OK |
| `system.live_queries` | Node-local by design | Local read required |
| `system.audit_logs` | Eventual OK (analytics) | Local read OK |

---

## 5.2 Data Flow Categories

Based on analysis, KalamDB has THREE distinct data access patterns:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    DATA ACCESS CATEGORIES                            │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  CATEGORY 1: CLIENT SQL QUERIES (User-Facing)                       │
│  ─────────────────────────────────────────────                       │
│  Examples:                                                           │
│    - SELECT * FROM app.todos WHERE user_id = 'u1'                   │
│    - INSERT INTO app.messages VALUES (...)                          │
│                                                                      │
│  Consistency: STRONG (read-after-write required)                    │
│  Recommendation: Route to LEADER                                    │
│                                                                      │
│  ════════════════════════════════════════════════════════════════   │
│                                                                      │
│  CATEGORY 2: INTERNAL OPERATIONS (Node-Local)                       │
│  ─────────────────────────────────────────────                       │
│  Examples:                                                           │
│    - FlushExecutor reading buffered rows                            │
│    - StreamEvictionExecutor deleting expired logs                   │
│    - RetentionExecutor scanning for soft-deleted rows               │
│    - Live query notification after Raft apply                       │
│                                                                      │
│  Consistency: LOCAL (must read THIS node's data)                    │
│  Recommendation: ALWAYS read locally                                │
│                                                                      │
│  ════════════════════════════════════════════════════════════════   │
│                                                                      │
│  CATEGORY 3: METADATA LOOKUPS (Schema, Users, etc.)                 │
│  ─────────────────────────────────────────────────                   │
│  Examples:                                                           │
│    - SchemaRegistry.get_table_definition()                          │
│    - SystemTablesProvider.scan() for system.tables                  │
│    - Auth middleware checking user credentials                      │
│                                                                      │
│  Consistency: DEPENDS (see table above)                             │
│  Recommendation: Route critical to LEADER, others OK local          │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 5.3 Edge Cases and Special Scenarios

### Edge Case 1: Follower Becomes Leader During Query

```
1. Client sends SELECT to Node 2 (follower)
2. Node 2 checks "am I leader?" → NO
3. Node 2 forwards to Node 1 (leader)
4. While forwarding, Node 1 fails, Node 2 becomes leader
5. Node 2 receives forwarded response from... nowhere?

Solution:
- Forwarding should have timeout + retry to new leader
- Or: Return redirect hint, let client reconnect
```

### Edge Case 2: Stale Leader (Network Partition)

```
1. Network partition isolates Node 1 (old leader)
2. Node 2 becomes new leader
3. Client still connected to Node 1
4. Client writes → Node 1 accepts (thinks it's leader)
5. Write fails Raft quorum → Error returned
6. Client reads → Node 1 serves stale data

Solution (CockroachDB approach):
- Leader lease with expiration
- Old leader stops serving after lease expires
- Reads require valid lease
```

### Edge Case 3: Job Running During Leadership Change

```
1. FlushExecutor starts on Node 1 (leader)
2. Reads local RocksDB, starts writing Parquet
3. Node 1 loses leadership to Node 2
4. FlushExecutor continues (has data in memory)
5. FlushExecutor writes manifest entry

Question: Is this OK?
- Yes for local phase (reading own data)
- Maybe not for leader phase (manifest update)
- Solution: LeaderGuard checks leadership before leader-only actions
```

### Edge Case 4: Client Switches Nodes Between Write and Read

```
1. Client writes to Node 1 (leader) → Success
2. Client reconnects to Node 2 (follower) 
3. Client reads immediately → Stale?

Current Behavior: Yes, stale read possible
With Leader-Only Reads: Node 2 forwards to Node 1, consistent read

But: What if client PREFERS low latency over consistency?
Solution: Make it configurable (like Yugabyte's yb_read_from_followers)
```

### Edge Case 5: Cross-Shard Queries

```
Query: SELECT * FROM app.todos ORDER BY created_at

Sharding:
- User 'alice' → Shard 5 (leader: Node 1)
- User 'bob' → Shard 12 (leader: Node 2)

Problem: Query needs data from multiple shards with different leaders

Solutions:
1. Query coordinator collects from all shard leaders (distributed query)
2. For user tables: RLS guarantees single-shard access (simpler)
3. For shared tables: Single shard (no cross-shard)
```

### Edge Case 6: Live Query Initial Data

```
1. Client subscribes to live query on Node 2 (follower)
2. Initial data scan runs on Node 2
3. Meanwhile, writer inserts on Node 1 (leader)
4. Insert committed but not applied on Node 2 yet
5. Initial data returned to client (missing new row)
6. Node 2 applies insert, fires notification
7. Client receives "insert" notification for row they already have (from initial data)
   OR client misses the row entirely if notification was skipped

Current Behavior: Race condition possible
Solution: 
- Option A: Initial data from leader only
- Option B: Include "up to log index X" in initial data, replay from there
```

---

## 5.4 Performance Analysis: Why Raft is 4x Slower

The user reports **4x slower performance** after adding Raft. Here's the root cause analysis:

### Performance Bottleneck 1: Watermark Waiting (MAJOR)

Every user data command waits for Meta group to catch up:

```rust
// In user_data.rs:apply()
let required_meta = cmd.required_meta_index();
let current_meta = get_coordinator().current_index();

if required_meta > current_meta {
    get_coordinator().wait_for(required_meta).await;  // BLOCKING!
}
```

**Impact**: Even simple INSERT waits for Meta group if `required_meta_index > 0`.

**The Problem**: DML commands don't actually need Meta synchronization:
- INSERT/UPDATE/DELETE only need the table to exist
- Table existence was validated BEFORE the command was created
- By the time we're applying, the table definitely exists

**Fix**: Set `required_meta_index: 0` for all DML commands. Only DDL-dependent operations need watermark waiting.

### Performance Bottleneck 2: Lock Contention on Applier (MEDIUM)

```rust
// In UserDataStateMachine
applier: RwLock<Option<Arc<dyn UserDataApplier>>>,

// On EVERY apply():
let applier = {
    let guard = self.applier.read();  // Lock acquisition!
    guard.clone()
};
```

**Impact**: Lock contention on the hot path. The applier never changes after initialization.

**Fix**: Use `OnceLock` instead of `RwLock` - applier is set once, never changed.

### Performance Bottleneck 3: Serialization Overhead (MEDIUM)

```rust
// Every command goes through:
let cmd: UserDataCommand = decode(command)?;  // Deserialization
// ... apply ...
let response_data = encode(&response)?;       // Serialization
```

**Impact**: ~100-500µs per command for serde serialization.

**Potential Fix**: 
- Use `rkyv` for zero-copy deserialization
- Or pool/cache serialization buffers

### Performance Bottleneck 4: Drain Pending on Every Apply

```rust
async fn apply(&self, ...) -> Result<ApplyResult, RaftError> {
    // ...
    self.drain_pending().await?;  // Called EVERY apply!
    // ...
}
```

**Impact**: Even when there are no pending commands, this iterates through the pending buffer.

**Fix**: Check `pending_buffer.is_empty()` before draining.

### Estimated Performance Impact

| Bottleneck | Estimated Overhead | Priority |
|------------|-------------------|----------|
| Watermark waiting | 1-50ms (waits for Meta) | P0 - Critical |
| Applier RwLock | 1-10µs per op | P1 - Medium |
| Serialization | 100-500µs per op | P2 - Low |
| Drain pending | 1-5µs per op | P2 - Low |

**Total estimated overhead**: 1-50ms per operation (mostly watermark waiting)

### Performance Fix Summary

1. **P0**: Remove watermark wait for DML (`required_meta_index: 0`)
2. **P1**: Replace `RwLock<Option<Arc<dyn UserDataApplier>>>` with `OnceLock<Arc<dyn UserDataApplier>>`
3. **P2**: Add `if !pending_buffer.is_empty()` check before drain

---

## 5.5 Live Query Design: No Changes Needed ✅

**Good news**: The current live query design is already correct!

### How It Works Today

```
Client INSERT on Node 1 (Leader)
       │
       ▼
┌─────────────────────────────────────┐
│  Raft Consensus (log replication)   │
└─────────────────────────────────────┘
       │                    │
       ▼                    ▼
   Node 1 Apply         Node 2 Apply (async)
       │                    │
       ▼                    ▼
  UserTableProvider     UserTableProvider
    .insert()             .insert()
       │                    │
       ▼                    ▼
  RocksDB Write        RocksDB Write
       │                    │
       ▼                    ▼
  notify_table_change_async()   notify_table_change_async()
       │                    │
       ▼                    ▼
  NotificationService   NotificationService
       │                    │
       ▼                    ▼
  Push to WS clients    Push to WS clients
  connected to Node 1   connected to Node 2
```

### Key Points

1. **Each node notifies its own connected clients** after local apply
2. **No cross-node notification broadcast needed**
3. **No "holding" of notifications on other nodes**

### Timing Expectations

| Event | Node 1 (Leader) | Node 2 (Follower) |
|-------|-----------------|-------------------|
| Client submits INSERT | T=0 | - |
| Raft quorum ACK | T=2ms | T=2ms (log) |
| Leader apply | T=3ms | - |
| Client gets SUCCESS | T=3ms | - |
| Follower apply | - | T=5-50ms |
| WS notification | T=3ms (immediate) | T=5-50ms (after apply) |

**Clients connected to leader get notifications immediately.**
**Clients connected to followers get notifications after follower apply.**

This is the expected behavior and matches CockroachDB/YugabyteDB patterns.

---

## 6. Recommended Solutions

### Option A: Leader-Only Reads (YugabyteDB Style)

**Concept**: Route all reads to the Raft leader for the relevant shard.

```
┌─────────────────────────────────────────────────────────────────────┐
│                    OPTION A: LEADER-ONLY READS                       │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  TableProvider.scan():                                               │
│    1. Determine which shard owns this table/user                     │
│    2. Check: Am I the leader for this shard?                         │
│       ├── YES: Read from local RocksDB                               │
│       └── NO:  Forward read request to leader                        │
│                                                                      │
│  Pros:                                                               │
│    ✅ Strong consistency (read-after-write guaranteed)               │
│    ✅ Simple to implement                                            │
│    ✅ Same pattern as Yugabyte/Cockroach                             │
│                                                                      │
│  Cons:                                                               │
│    ⚠️ All read load on leader (hot spots)                           │
│    ⚠️ Leader failure = brief read unavailability                    │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

**Implementation Sketch**:

```rust
// In UserTableProvider::scan()
async fn scan(&self, ...) -> Result<SendableRecordBatchStream> {
    let shard = compute_shard(&self.user_id);
    let group_id = GroupId::DataUserShard(shard);
    
    if self.raft_manager.is_leader(group_id) {
        // Fast path: read locally
        self.read_from_rocksdb().await
    } else {
        // Forward to leader
        let leader_id = self.raft_manager.current_leader(group_id)
            .ok_or(Error::NoLeader)?;
        self.forward_read_to_leader(leader_id).await
    }
}
```

### Option B: Read Index / Linearizable Reads

**Concept**: Before reading, verify with Raft that we have all committed entries.

```
┌─────────────────────────────────────────────────────────────────────┐
│                    OPTION B: READ INDEX                              │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  OpenRaft provides: raft.ensure_linearizable()                       │
│                                                                      │
│  Flow:                                                               │
│    1. Before reading, call ensure_linearizable()                     │
│    2. This confirms all committed entries are applied locally        │
│    3. Then read from local RocksDB                                   │
│                                                                      │
│  Pros:                                                               │
│    ✅ Can read from any node (load distribution)                     │
│    ✅ Strong consistency                                             │
│                                                                      │
│  Cons:                                                               │
│    ⚠️ Extra round-trip to leader for read verification              │
│    ⚠️ Higher latency than leader-only reads                         │
│    ⚠️ More complex implementation                                   │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

**Implementation Sketch**:

```rust
// Before any read
async fn linearizable_read(&self) -> Result<()> {
    let raft = self.raft_group.raft();
    raft.ensure_linearizable().await?;
    // Now safe to read locally
    Ok(())
}
```

### Option C: Wait for Apply (Stronger Write Confirmation)

**Concept**: Wait for quorum APPLY, not just quorum log ACK.

```
┌─────────────────────────────────────────────────────────────────────┐
│                    OPTION C: QUORUM APPLY                            │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  Modify write path:                                                  │
│    1. Propose command via Raft (as today)                            │
│    2. Wait for quorum log ACK (as today)                             │
│    3. ALSO wait for quorum apply confirmation  ← NEW                 │
│    4. Then return success to client                                  │
│                                                                      │
│  Pros:                                                               │
│    ✅ Data visible on majority before client confirmation            │
│    ✅ Stronger durability guarantees                                 │
│                                                                      │
│  Cons:                                                               │
│    ❌ Significantly higher write latency                             │
│    ❌ OpenRaft doesn't natively support this                         │
│    ❌ Requires custom replication tracking                           │
│    ❌ CockroachDB/YugabyteDB don't do this either!                   │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

**Note**: This is NOT recommended. Neither CockroachDB nor YugabyteDB wait for follower apply. They solve read consistency at the read path instead.

### Option D: Lease-Based Reads (CockroachDB Style)

**Concept**: Designate a "leaseholder" per shard that handles all reads.

```
┌─────────────────────────────────────────────────────────────────────┐
│                    OPTION D: LEASEHOLDER                             │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  Leaseholder = Leader (simplified)                                   │
│  OR                                                                  │
│  Leaseholder = Separate concept (can be different from Raft leader) │
│                                                                      │
│  Flow:                                                               │
│    1. Each shard has one leaseholder                                 │
│    2. All reads AND writes go through leaseholder                    │
│    3. Writes: leaseholder proposes to Raft                           │
│    4. Reads: leaseholder reads locally (no Raft needed)              │
│                                                                      │
│  Pros:                                                               │
│    ✅ Strong consistency                                             │
│    ✅ Read-only operations don't touch Raft                          │
│    ✅ Industry-proven (CockroachDB)                                  │
│                                                                      │
│  Cons:                                                               │
│    ⚠️ More complex lease management                                 │
│    ⚠️ Need to handle lease expiration/transfer                      │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 6.5 REVISED Recommendation: Context-Aware Read Routing

Based on the analysis of internal usage patterns, **pure leader-only reads won't work** for KalamDB. Instead, we need **context-aware read routing**.

```
┌─────────────────────────────────────────────────────────────────────┐
│              CONTEXT-AWARE READ ROUTING (RECOMMENDED)                │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  Every read operation has a "context" that determines routing:      │
│                                                                      │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │  ReadContext::ClientQuery                                      │  │
│  │  ─────────────────────────                                     │  │
│  │  - Source: SQL query from API                                  │  │
│  │  - Routing: LEADER (forward if not leader)                     │  │
│  │  - Why: User expects read-after-write consistency              │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                      │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │  ReadContext::InternalJob                                      │  │
│  │  ─────────────────────────                                     │  │
│  │  - Source: FlushExecutor, RetentionExecutor, etc.              │  │
│  │  - Routing: LOCAL (always read this node's data)               │  │
│  │  - Why: Jobs operate on node-local RocksDB buffers             │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                      │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │  ReadContext::LiveQueryNotification                            │  │
│  │  ─────────────────────────────────                             │  │
│  │  - Source: After Raft apply on any node                        │  │
│  │  - Routing: LOCAL (notify clients connected to this node)      │  │
│  │  - Why: Notifications are node-local by design                 │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                      │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │  ReadContext::LiveQueryInitialData                             │  │
│  │  ─────────────────────────────────                             │  │
│  │  - Source: Live query subscription setup                       │  │
│  │  - Routing: LEADER (consistent with write path)                │  │
│  │  - Why: Avoid race between initial data and notifications      │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                      │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │  ReadContext::SystemTableMonitoring                            │  │
│  │  ─────────────────────────────────                             │  │
│  │  - Source: SELECT * FROM system.jobs, system.audit_logs        │  │
│  │  - Routing: LOCAL (eventual consistency OK for monitoring)     │  │
│  │  - Why: Performance over strong consistency for dashboards     │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                      │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │  ReadContext::SystemTableCritical                              │  │
│  │  ─────────────────────────────────                             │  │
│  │  - Source: Auth checks, DDL validation                         │  │
│  │  - Routing: LEADER (must be consistent)                        │  │
│  │  - Why: Security and schema consistency require strong reads   │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### 6.5.1 Implementation: Read Context in Session State

```rust
/// Read context determines how reads are routed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadContext {
    /// Client SQL query - route to leader for consistency
    ClientQuery,
    /// Internal job operation - always read locally
    InternalJob,
    /// Live query notification - local only
    LiveQueryNotification,
    /// Live query initial data - route to leader
    LiveQueryInitialData,
    /// System table for monitoring - local OK
    SystemMonitoring,
    /// System table critical (auth, DDL) - route to leader
    SystemCritical,
}

impl ReadContext {
    /// Should this read be routed to leader?
    pub fn requires_leader(&self) -> bool {
        matches!(self, 
            Self::ClientQuery | 
            Self::LiveQueryInitialData | 
            Self::SystemCritical
        )
    }
    
    /// Can this read be served locally regardless of leader status?
    pub fn can_read_locally(&self) -> bool {
        matches!(self,
            Self::InternalJob |
            Self::LiveQueryNotification |
            Self::SystemMonitoring
        )
    }
}
```

### 6.5.2 Where Context is Set

| Entry Point | Read Context | Notes |
|-------------|--------------|-------|
| `kalamdb-api` SQL handler | `ClientQuery` | Default for all SQL from API |
| `FlushExecutor.execute_local()` | `InternalJob` | Reading local RocksDB |
| `StreamEvictionExecutor.execute()` | `InternalJob` | Scanning local logs |
| `RetentionExecutor.execute()` | `InternalJob` | Scanning for expired rows |
| `UserDataApplier.insert()` | `LiveQueryNotification` | After Raft apply |
| `initial_data.rs` fetch | `LiveQueryInitialData` | Subscription setup |
| Admin dashboard queries | `SystemMonitoring` | Can be stale |
| Auth middleware | `SystemCritical` | Must be consistent |
| DDL validation | `SystemCritical` | Must be consistent |

### 6.5.3 Updated TableProvider Implementation

```rust
impl UserTableProvider {
    async fn scan_with_context(
        &self, 
        ctx: ReadContext,
        user_id: &UserId,
        filter: Option<&Expr>,
    ) -> Result<RecordBatch, KalamDbError> {
        
        // Check if we need to forward to leader
        if ctx.requires_leader() && !self.is_local_shard_leader(user_id) {
            return self.forward_scan_to_leader(user_id, filter).await;
        }
        
        // Local read (either we're leader, or context allows local)
        self.scan_local(user_id, filter).await
    }
    
    fn is_local_shard_leader(&self, user_id: &UserId) -> bool {
        let shard = self.compute_shard(user_id);
        let group_id = GroupId::DataUserShard(shard);
        self.core.app_context.raft_manager().is_leader(group_id)
    }
}
```

---

## 7. Phased Implementation Plan

### Phase 1: Leader-Only Reads (Immediate Fix)

This is the minimum viable fix and matches what Yugabyte/Cockroach do:

```rust
// Add to TableProvider implementations:

impl UserTableProvider {
    async fn ensure_can_read(&self) -> Result<()> {
        let shard = self.compute_shard();
        let group_id = GroupId::DataUserShard(shard);
        
        if !self.raft_manager.is_leader(group_id) {
            // Option 1: Forward read to leader
            return Err(Error::NotLeader(self.raft_manager.current_leader(group_id)));
            
            // Option 2: Return error (let client retry on leader)
            // return Err(Error::ReadOnlyFromLeader);
        }
        Ok(())
    }
    
    async fn scan(&self, ...) -> Result<...> {
        self.ensure_can_read().await?;
        // ... existing scan logic ...
    }
}
```

### Phase 2: Read Forwarding

Auto-forward reads to the appropriate leader:

```rust
// In kalamdb-api request handling:

async fn handle_query(req: SqlRequest, ctx: &AppContext) -> Result<Response> {
    // Determine which shards this query needs
    let required_shards = analyze_query_shards(&req.sql)?;
    
    for shard in required_shards {
        if !ctx.raft_manager().is_leader(shard) {
            // Forward entire query to a node that leads this shard
            // (or the node that leads the most required shards)
            return forward_query_to_leader(req, shard).await;
        }
    }
    
    // We're leader for all required shards - execute locally
    execute_query_locally(req, ctx).await
}
```

### Phase 3: Follower Reads (Optional, Future)

Add opt-in stale reads for read-heavy workloads:

```sql
-- PostgreSQL-compatible syntax (like CockroachDB)
SELECT * FROM users AS OF SYSTEM TIME '-10s';

-- Or session setting
SET default_transaction_read_only_staleness = '10s';
```

This requires:
1. MVCC / timestamp versioning
2. HLC or similar clock synchronization  
3. "Closed timestamp" propagation from leader to followers

---

## 8. API Changes for Read Forwarding

### 8.1 New Error Type

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueryError {
    // ... existing errors ...
    
    /// Query must be executed on leader node
    NotLeader {
        shard: GroupId,
        leader_node_id: Option<NodeId>,
        leader_api_addr: Option<String>,
    },
}
```

### 8.2 Client Behavior

```typescript
// Kalam-link SDK update:
class KalamClient {
    async query(sql: string): Promise<Result> {
        try {
            return await this.primaryNode.query(sql);
        } catch (e) {
            if (e.code === 'NOT_LEADER' && e.leaderApiAddr) {
                // Transparent redirect
                return await this.connectTo(e.leaderApiAddr).query(sql);
            }
            throw e;
        }
    }
}
```

---

## 9. Architecture Diagram: Target State

```
┌─────────────────────────────────────────────────────────────────────┐
│                    TARGET KALAMDB ARCHITECTURE                       │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  Client Query: SELECT * FROM app.users WHERE id = 'user123'         │
│       │                                                              │
│       ▼                                                              │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │  1. API Layer (any node)                                     │    │
│  │     - Parse query                                            │    │
│  │     - Determine required shard(s)                            │    │
│  └─────────────────────────────────────────────────────────────┘    │
│       │                                                              │
│       ▼                                                              │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │  2. Routing Check                                            │    │
│  │     - Am I leader for DataUserShard(hash(user123) % 32)?    │    │
│  │     ├── YES: Execute locally (step 3)                        │    │
│  │     └── NO:  Forward to leader node                          │    │
│  └─────────────────────────────────────────────────────────────┘    │
│       │                                                              │
│       ▼                                                              │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │  3. DataFusion Query Execution (on leader only)              │    │
│  │     - TableProvider reads from RocksDB                       │    │
│  │     - Guaranteed to see all committed writes                 │    │
│  └─────────────────────────────────────────────────────────────┘    │
│       │                                                              │
│       ▼                                                              │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │  4. Return Results                                           │    │
│  │     - Consistent view guaranteed                             │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                      │
│  ════════════════════════════════════════════════════════════════   │
│                                                                      │
│  Client Write: INSERT INTO app.users (id, name) VALUES (...)        │
│       │                                                              │
│       ▼                                                              │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │  1. API Layer → RaftExecutor                                 │    │
│  └─────────────────────────────────────────────────────────────┘    │
│       │                                                              │
│       ▼                                                              │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │  2. Route to Shard Leader                                    │    │
│  │     - If not leader, forward to leader via gRPC              │    │
│  └─────────────────────────────────────────────────────────────┘    │
│       │                                                              │
│       ▼                                                              │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │  3. Leader Proposes via Raft                                 │    │
│  │     - Quorum ACKs log entry                                  │    │
│  │     - Leader applies to RocksDB                              │    │
│  │     - Returns success                                        │    │
│  └─────────────────────────────────────────────────────────────┘    │
│       │                                                              │
│       ▼                                                              │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │  4. Client gets confirmation                                 │    │
│  │     ✅ Leader has data (can be read immediately)             │    │
│  │     ✅ Quorum has log (durable even if leader fails)         │    │
│  │     ⏳ Followers apply async (fine, reads go to leader)     │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 10. REVISED Implementation Tasks (Simplified v2)

### Phase 0: Performance Fix (P0 - Critical, Do First!)

Fix the 4x performance regression before anything else:

- [ ] **Task 0.1**: Remove watermark wait for DML commands (set `required_meta_index: 0`)
- [ ] **Task 0.2**: Replace `RwLock<Option<Arc<dyn Applier>>>` with `OnceLock<Arc<dyn Applier>>`
- [ ] **Task 0.3**: Add `is_empty()` check before `drain_pending()`
- [ ] **Task 0.4**: Add apply latency metrics for benchmarking

### Phase 1: Simple Read Routing (P1 - Important)

Keep it simple: 2 read contexts (Client vs Internal):

- [ ] **Task 1.1**: Add simple `ReadContext` enum (`Client`, `Internal`)
- [ ] **Task 1.2**: Add `is_leader_for_user(user_id)` to AppContext
- [ ] **Task 1.3**: Check leadership in `UserTableProvider::scan()` for Client reads
- [ ] **Task 1.4**: Return NOT_LEADER error with leader address hint

### Phase 2: Verification (P2 - Confidence)

Verify existing systems work correctly:

- [ ] **Task 2.1**: Add test: live query notification fires on follower after local apply
- [ ] **Task 2.2**: Add test: client read on follower returns NOT_LEADER
- [ ] **Task 2.3**: Add benchmark: compare write latency before/after performance fix

### Phase 3: Future (Optional)

Only if needed:

- [ ] Follower reads with bounded staleness (AS OF SYSTEM TIME)
- [ ] HLC/MVCC if needed for temporal queries

---

## 11. REVISED Summary (v2 - Simplified)

### What's Wrong

**Two distinct issues:**

1. **4x Performance Regression** (P0 - Fix First!)
   - Watermark waiting for DML commands (unnecessary)
   - RwLock contention on applier
   - Fix: Remove watermark wait, use OnceLock

2. **Stale Reads** (P1 - Fix After Performance)
   - Client reads can hit followers
   - Followers haven't applied yet
   - Fix: Route client reads to leader

### What's NOT Wrong ✅

1. **Live Query Notifications**: Already fire locally on each node after apply
2. **Job Executors**: Already read local data (correct behavior)
3. **Raft Replication**: Working correctly

### Live Query Design Confirmed

```
Leader Apply → RocksDB Write → notify_table_change_async()
                                       │
                                       ▼
                             Push to WS clients on THIS node

Follower Apply (async) → RocksDB Write → notify_table_change_async()
                                                  │
                                                  ▼
                                        Push to WS clients on THIS node
```

- **No cross-node notification broadcast needed**
- **No "holding" of notifications on other nodes**
- Clients on follower get notification a few ms after clients on leader

### The Solution (Simplified)

**Phase 0**: Fix performance (watermark, locks) → 4 hours
**Phase 1**: Simple read routing (Client vs Internal) → 6 hours  
**Phase 2**: Verification tests → 4 hours

**Total: ~14 hours** (vs original 40+ hours with 6-context design)

### Key Code Changes

```rust
// 1. Remove watermark wait for DML
UserDataCommand::Insert { 
    required_meta_index: 0,  // DML doesn't need Meta sync
    // ...
}

// 2. Simple ReadContext
enum ReadContext {
    Client,    // Route to leader
    Internal,  // Always local
}

// 3. Leadership check in scan()
fn scan(&self, read_context: ReadContext) -> Result<...> {
    if read_context == ReadContext::Client && !self.is_leader() {
        return Err(NotLeader { leader_addr });
    }
    self.scan_local()
}
```

This maintains simplicity and performance while ensuring consistency.
