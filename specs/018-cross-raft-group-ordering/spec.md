# 018 — Unified Meta Group + Meta→Data Watermarking

## Summary

KalamDB currently uses multiple independent Raft groups. During node rejoin/catch-up, a data group can apply DML before the metadata it depends on has been applied locally (tables/users/storages/jobs). This manifests as “Provider NOT FOUND”/missing schema errors and can lead to dropped writes.

This spec locks in a single approach:
1. **Combine all system metadata Raft groups into one**: merge MetaSystem + MetaUsers + MetaJobs into a single `Meta` group.
2. **Watermark Meta→Data ordering**: every data command includes `required_meta_index: u64` captured from the `Meta` group at proposal time; followers buffer data commands until local `Meta` has applied at least that index.

## Goals

- Prevent any data apply that depends on metadata not yet applied locally.
- Ensure a rejoining node can safely catch up without dropping writes.
- Keep the mechanism simple: a single `u64` watermark (`required_meta_index`).

## Non-Goals

- Cross-data-group ordering (e.g., between two user shards).
- Global transactions spanning metadata + multiple data shards.

## Problem

With separate Raft logs, apply order across groups is undefined on followers during catch-up.

Example:

```
Leader timeline:
  Meta:  CREATE USER alice         (meta log index 100)
  Meta:  CREATE TABLE alice.orders (meta log index 101)
  Data:  INSERT alice.orders ...   (data log index 50)

Follower B rejoins:
  - Data shard replays faster and hits data index 50 first
  - Apply fails because local Meta is still < 101
  - Without protection: write is lost or becomes inconsistent
```

## Design

### 1) Unified `Meta` Raft Group

Replace the three metadata groups with one totally-ordered log.

**Group count** becomes: 1 (Meta) + 32 (DataUserShard) + 1 (DataSharedShard) = **34**.

Sketch:

```rust
// kalamdb-raft/src/group_id.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GroupId {
    /// Unified metadata group (replaces MetaSystem + MetaUsers + MetaJobs)
    Meta,
    DataUserShard(u32),
    DataSharedShard(u32),
}
```

All metadata commands become part of a single `MetaCommand` enum (schema + users + jobs). The key property is: **every metadata change has a single, monotonically increasing applied index** (`meta_index`).

### 2) The Watermark: `required_meta_index: u64`

Every data command carries a “watermark” which is simply the `Meta` group’s last applied index on the leader at the time the data command was proposed.

**Proposal rule (leader):**

1. Read `meta_index = Meta.last_applied_index()`.
2. Set `required_meta_index = meta_index` inside the data command.
3. Propose the data command to the target data shard.

Sketch:

```rust
// in RaftExecutor (leader path)

pub async fn execute_user_data(&self, mut cmd: UserDataCommand) -> Result<DataResponse, RaftError> {
    let meta_index = self.manager.current_meta_index();
    cmd.set_required_meta_index(meta_index);
    self.manager.propose_user_data(self.compute_user_shard(&cmd), encode(&cmd)?).await?
}
```

**Command shape:**

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum UserDataCommand {
    Insert {
        required_meta_index: u64,
        table_id: TableId,
        user_id: UserId,
        rows_data: Vec<u8>,
    },
    // Update/Delete/RegisterLiveQuery/... also carry required_meta_index
}
```

### 3) Apply Rule on Followers: buffer until Meta catches up

Each data state machine checks whether local meta is “fresh enough” before applying.

**Apply rule (follower):**

- If `Meta.last_applied_index() >= required_meta_index`: apply immediately.
- Else: buffer the command (do not call the provider/applier yet).

This must apply to both:
- `DataUserShard(0..31)`
- `DataSharedShard(0)`

**Important detail (Raft correctness):** buffering is still an *apply* of the Raft entry.

- The data state machine must store the command in its own durable state (the pending buffer persisted in the same Raft partition).
- After that, it is correct for the data state machine to advance its `last_applied_index` because its state now reflects the entry.
- The *provider side effects* (calling table providers, writing row batches, etc.) are deferred until the watermark is satisfied.

### 4) Pending Buffer + Drain

Data groups maintain a pending queue keyed by `required_meta_index` so they can efficiently drain commands as meta advances.

```rust
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingCommand {
    pub log_index: u64,
    pub log_term: u64,
    pub required_meta_index: u64,
    pub command_bytes: Vec<u8>,
}

pub struct PendingBuffer {
    by_required_meta: parking_lot::RwLock<BTreeMap<u64, Vec<PendingCommand>>>,
}

impl PendingBuffer {
    pub fn drain_satisfied(&self, current_meta_index: u64) -> Vec<PendingCommand> {
        // drain all keys <= current_meta_index
        // (implementation omitted)
        Vec::new()
    }
}
```

**Drain trigger:** when `Meta` applies a new entry, data shards should attempt to drain.

Implementation detail: use a `MetadataCoordinator` (atomic + `Notify`) to broadcast “meta advanced” to data shards.

**Ordering requirement (covers “catch up exactly in order”):**

Even though the pending buffer is keyed by `required_meta_index`, the drain must apply buffered commands in increasing **data log index order** per shard.

- Raft delivers entries to a state machine in increasing log index order.
- Buffering may temporarily “skip” provider side effects for an entry.
- When draining later, the shard must preserve the original order of operations for correctness.

Concretely, draining MUST behave as if it is replaying:

1. Pick all buffered entries whose `required_meta_index <= current_meta_index`.
2. Sort those entries by `log_index`.
3. Apply them in that order.

This ensures that within a shard the observed effect is identical to a perfect catch-up where Meta was always ahead.

### 5) Persistence (crash safety)

Buffered commands must survive process restarts.

- Persist each buffered command into the shard’s RocksDB partition under a `pending:{shard}:{log_index}` key.
- On shard startup, load pending entries back into memory and resume draining.
- After successfully applying a buffered command, delete its persisted pending key.

## Correctness Guarantees

This design guarantees the following, including during rejoin/catch-up:

1. **Meta-before-Data dependency barrier**
    - A data command with `required_meta_index = N` will not execute provider side effects until the node’s local `Meta.last_applied_index() >= N`.
    - This covers schema/table definitions, user existence/roles, storage configs, and job metadata because they all live in the unified `Meta` group.

2. **Per-shard deterministic ordering**
    - For each data shard, provider side effects are applied in the shard’s Raft log order (increasing data log index), even if entries were buffered.

3. **Shard independence (user data may be in different shards)**
    - Each `DataUserShard(k)` buffers and drains independently.
    - The single watermark works across all shards because it references the same global `Meta` log index.

4. **No “silent drop” on catch-up**
    - If Meta is behind, the follower buffers and later drains; it does not fail the apply path due to missing providers.

## Failure / Partial Replication Cases

This section is intentionally about recovery behavior (still within this single approach).

### Meta replication is slow, stalled, or temporarily unhealthy

- Data shards will keep buffering commands whose `required_meta_index` is not yet satisfied.
- The node should be treated as **not fully caught up** for serving reads that require up-to-date results.

### Data replication is ahead of Meta (the original bug)

- Safe: data commands are buffered until meta catches up.
- Once meta advances, buffered commands drain in shard log order.

### Meta never reaches the required index (replication “didn’t go well”)

If a node cannot advance Meta to satisfy a pending command’s `required_meta_index` (e.g., persistent Raft membership/config issue, corruption, or the node is permanently partitioned):

- The node will accumulate pending entries and remain in a **catching-up** state.
- The correct behavior is to **not pretend the node is healthy**:
  - It must not be elected leader for data shards where it can’t execute safely.
  - It must not serve “fresh” reads from shards with unresolved pending buffers.
- Operational recovery is to restore Meta replication health (snapshot install/rebuild the node / fix network) so `Meta.last_applied_index()` can progress.

### Restart/crash while buffering

- Safe if and only if pending entries are persisted in the shard’s Raft partition.
- On restart, the shard loads pending entries and resumes draining when Meta advances.

## Acceptance Criteria

Implementation is considered correct when all of the following hold:

1. **Rejoin ordering test (core)**
    - In a 3-node cluster, isolate one follower.
    - While isolated, execute `CREATE USER`, `CREATE TABLE`, then `INSERT` on the leader.
    - Rejoin the follower.
    - Expected: follower does not emit “provider not found” errors; the `INSERT` becomes visible on that node only after its local Meta has applied the required index.

2. **Shard coverage**
    - Repeat the above but target at least two users that hash to different `DataUserShard` values.
    - Expected: each shard buffers/drains independently; both converge correctly.

3. **Crash recovery**
    - Force a follower restart while it has pending buffered entries.
    - Expected: after restart, pending entries are reloaded and eventually drained once Meta catches up.

## Migration / Rollout

### Combine the meta groups

Prefer snapshot-based migration (simpler than log interleaving):

1. Stop the cluster (or perform a controlled upgrade window).
2. Export current metadata state from the three groups (system tables/users/jobs) into a single logical snapshot.
3. Bootstrap the new `Meta` group from that snapshot.
4. Start nodes with only the new `Meta` group enabled; keep data groups as-is.

### Backward compatibility during rollout

- Data commands without a watermark must behave as `required_meta_index = 0` (always satisfied).
- Gate the feature behind a config flag while rolling out.

## Implementation Tasks

### Phase 1: Merge MetaSystem + MetaUsers + MetaJobs

- Introduce `GroupId::Meta`, delete/stop using the three old group IDs.
- Add unified `MetaCommand` + `MetaResponse`.
- Implement a `MetaStateMachine` that applies metadata through a combined `MetaApplier`.
- Rewire `RaftManager`/`RaftExecutor` to route all metadata through the single `Meta` group.

### Phase 2: Add `required_meta_index` to data commands

- Extend both `UserDataCommand` and `SharedDataCommand` variants with `required_meta_index: u64`.
- On leaders, stamp it using `Meta.last_applied_index()` at proposal time.

### Phase 3: Buffering + draining on followers

- Add `PendingBuffer` to all data shards.
- Update data state machine apply logic to buffer when `required_meta_index > current_meta_index`.
- Add drain-on-meta-advance via coordinator notifications.

### Phase 4: Persistence + tests

- Persist pending commands in RocksDB per shard.
- Integration test: rejoin node where data log replays before meta log; Ensure buffered commands eventually apply.
