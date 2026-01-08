# 017 — Raft Storage Persistence via kalamdb-store

## Summary

Migrate Raft log, vote, commit, and snapshot storage from in-memory structures to the `StorageBackend` abstraction in `kalamdb-store`. This enables durability across restarts, crash recovery without relying solely on appliers, and future portability to storage engines other than RocksDB.

## Current State

- `KalamRaftStorage` (in `kalamdb-raft/src/storage/raft_store.rs`) stores:
  - Raft log entries (`BTreeMap<u64, LogEntryData>`)
  - Vote (`Option<Vote<u64>>`)
  - Committed log id (`Option<LogId<u64>>`)
  - Last purged log id (`Option<LogId<u64>>`)
  - Snapshots (`Option<StoredSnapshot>`)
  - State machine in-memory cache
- All of the above is lost on process restart.
- Durability relies on appliers persisting real data to RocksDB/Parquet; Raft metadata itself is ephemeral.

## Goals

1. **Durability**: Raft log, vote, commit, and snapshots survive restarts.
2. **Abstraction**: Use `StorageBackend` trait from `kalamdb-store` (same code path as user/system data).
3. **Portability**: Switching RocksDB to Sled, FoundationDB, or S3-compatible stores requires changes only in `kalamdb-store`.
4. **Memory efficiency**: Keep only a bounded in-memory log window; older entries live on disk.
5. **Backward compatibility**: Existing applier-based persistence for real data remains unchanged.

## Design

### Partition Scheme

Single partition `raft_data` stores all data for all Raft groups to avoid Column Family explosion (RocksDB resource overhead).
Keys are prefixed with the Group ID.

| GroupId                | Partition Name           |
|------------------------|--------------------------|
| **All Groups**         | `raft_data`              |

### Key Layout (in `raft_data` partition)

Key format: `[GroupPrefix]:[KeyType]:[Suffix]`

**Group Prefixes**:
- `MetaSystem`: `sys`
- `MetaUsers`: `users`
- `MetaJobs`: `jobs`
- `DataUserShard(N)`: `u:{N:05}` (e.g., `u:00005`)
- `DataSharedShard(N)`: `s:{N:05}`

**Key Types**:

| Type         | Key Pattern (after group prefix) | Example Key (MetaSystem)     | Value Allocation |
|--------------|----------------------------------|------------------------------|------------------|
| **Log**      | `log:{index:020}`                | `sys:log:00000000000000000001` | KV Store (CF)    |
| **Vote**     | `meta:vote`                      | `sys:meta:vote`              | KV Store (CF)    |
| **Commit**   | `meta:commit`                    | `sys:meta:commit`            | KV Store (CF)    |
| **Purge**    | `meta:purge`                     | `sys:meta:purge`             | KV Store (CF)    |
| **SnapMeta** | `snap:meta`                      | `sys:snap:meta`              | KV Store (CF)    |
| **SnapData** | `snap:data`                      | `sys:snap:data`              | **Filesystem** (Path only) or KV if small |

### Storage Optimization

To avoid bloating the KV store (RocksDB Column Family):
1. **Single Partition**: Prevents write stalls from too many active MemTables.
2. **Snapshot Offloading**: Large snapshot payloads should NOT be stored in the KV store.
   - Store snapshot data in `kalamdb-filestore` (e.g., Parquet files or compressed blobs on disk/S3).
   - The KV store only holds the **path** or reference to the snapshot file in `snap:data`.
   - Small metadata snapshots can remain inline.

### New Types in kalamdb-store

```rust
// New file: kalamdb-store/src/raft_storage.rs

pub struct RaftLogEntry {
    pub index: u64,
    pub term: u64,
    pub payload: Vec<u8>, // serialized EntryPayload
}

impl KSerializable for RaftLogEntry {}

pub struct RaftMeta {
    pub vote: Option<Vote<u64>>,
    pub commit: Option<LogId<u64>>,
    pub purge: Option<LogId<u64>>,
}

impl KSerializable for RaftMeta {}

/// Generic Raft storage backed by StorageBackend
pub struct RaftPartitionStore {
    backend: Arc<dyn StorageBackend>,
    partition: Partition,
}

impl RaftPartitionStore {
    pub fn new(backend: Arc<dyn StorageBackend>, group_id: GroupId) -> Self;

    // Log operations
    pub fn append_log(&self, entries: &[RaftLogEntry]) -> Result<()>;
    pub fn get_log(&self, index: u64) -> Result<Option<RaftLogEntry>>;
    pub fn scan_logs(&self, start: u64, end: u64) -> Result<Vec<RaftLogEntry>>;
    pub fn delete_logs_before(&self, index: u64) -> Result<()>;
    pub fn last_log_index(&self) -> Result<Option<u64>>;

    // Vote/commit/purge
    pub fn save_vote(&self, vote: &Vote<u64>) -> Result<()>;
    pub fn read_vote(&self) -> Result<Option<Vote<u64>>>;
    pub fn save_commit(&self, commit: Option<LogId<u64>>) -> Result<()>;
    pub fn read_commit(&self) -> Result<Option<LogId<u64>>>;
    pub fn save_purge(&self, purge: Option<LogId<u64>>) -> Result<()>;
    pub fn read_purge(&self) -> Result<Option<LogId<u64>>>;

    // Snapshots
    pub fn save_snapshot_meta(&self, meta: &SnapshotMeta<...>) -> Result<()>;
    pub fn read_snapshot_meta(&self) -> Result<Option<SnapshotMeta<...>>>;
    pub fn save_snapshot_data(&self, data: &[u8]) -> Result<()>;
    pub fn read_snapshot_data(&self) -> Result<Option<Vec<u8>>>;
}
```

### Changes to kalamdb-raft

1. **Inject `StorageBackend`**: `KalamRaftStorage::new` takes `Arc<dyn StorageBackend>` in addition to state machine.
2. **Replace in-memory structures**: log, vote, commit, purge, snapshot use `RaftPartitionStore` internally.
3. **Keep bounded in-memory cache**: Last N log entries (e.g., 1000) cached for fast reads; older entries read from disk.
4. **Async considerations**: `RaftStorage` trait methods are sync; use `spawn_blocking` wrappers from `StorageBackendAsync` when needed.

### Partition Lifecycle

- On `RaftManager::start`, ensure each group's partition exists (`backend.create_partition`).
- On `initialize_cluster` (first node), partitions are empty; fresh bootstrap.
- On restart, read vote/commit/purge/snapshot from partition and resume Raft state.

### Snapshot Strategy

- Snapshots remain the same bincode blobs as today but stored under `snap:meta` and `snap:data` keys.
- Optional future: chunked snapshots for large state machines (split data across multiple keys).

### Migration Path

1. Default to in-memory mode if no `StorageBackend` injected (backward-compatible, useful for tests).
2. When backend is present, read existing state on startup; empty = fresh node.
3. No migration of in-memory logs needed (fresh cluster or rebuild via snapshot).

## Task Breakdown

### Phase 1: kalamdb-store additions

| Task | Description |
|------|-------------|
| 1.1  | Add `RaftLogEntry`, `RaftMeta` types with `KSerializable` |
| 1.2  | Implement `RaftPartitionStore` with log, meta, snapshot methods |
| 1.3  | Unit tests for `RaftPartitionStore` using in-memory backend |

### Phase 2: kalamdb-raft integration

| Task | Description |
|------|-------------|
| 2.1  | Add `Arc<dyn StorageBackend>` parameter to `KalamRaftStorage::new` |
| 2.2  | Replace log `BTreeMap` with `RaftPartitionStore` (fallback to in-memory if None) |
| 2.3  | Replace vote/commit/purge with `RaftPartitionStore` |
| 2.4  | Replace snapshot storage with `RaftPartitionStore` |
| 2.5  | Add bounded in-memory log cache (last N entries) |

### Phase 3: RaftManager wiring

| Task | Description |
|------|-------------|
| 3.1  | Pass `StorageBackend` from `AppContext` into `RaftManager` |
| 3.2  | Create raft partitions on startup (`create_partition` per group) |
| 3.3  | Resume from persisted state on restart |

### Phase 4: Testing & Validation

| Task | Description |
|------|-------------|
| 4.1  | Integration test: cluster restart recovers Raft state |
| 4.2  | Benchmark log append/read with RocksDB vs in-memory |
| 4.3  | Stress test: snapshot install + log compaction cycle |

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Performance regression | Bounded in-memory cache for hot entries; async batching for appends |
| Partition bloat | Log compaction via `delete_logs_before` after snapshot |
| Mixed-mode confusion | Clear error if backend missing in cluster mode; allow in-memory for standalone/tests |

## Future Work

- **Chunked snapshots**: Split large snapshots into multiple keys for streaming install.
- **Storage backend swap**: Test with Sled or S3-compatible backend once kalamdb-store supports them.
- **Encryption at rest**: Leverage future StorageBackend encryption hooks.

## References

- [kalamdb-store/src/storage_trait.rs](backend/crates/kalamdb-store/src/storage_trait.rs)
- [kalamdb-store/src/entity_store.rs](backend/crates/kalamdb-store/src/entity_store.rs)
- [kalamdb-raft/src/storage/raft_store.rs](backend/crates/kalamdb-raft/src/storage/raft_store.rs)
- [docs/architecture/raft-replication.md](docs/architecture/raft-replication.md)
