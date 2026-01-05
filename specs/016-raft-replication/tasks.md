# Raft Replication Implementation Tasks

## Overview

This document tracks the implementation of Raft-based replication for KalamDB, enabling multi-node clusters with strong consistency for metadata and data operations.

**Target Architecture:**
- **36 Raft Groups**: 3 metadata + 32 user data shards + 1 shared data shard
- **CommandExecutor Pattern**: Generic abstraction (no if/else for cluster vs standalone)
- **Leader-Only Jobs**: Background jobs (flush, compaction) run only on leader
- **Live Query Sharding**: `system.live_queries` in user shards by `user_id`

---

## Phase 0: Foundation (Dependencies & Crate Setup)
**Status: ✅ Complete**

### Task 0.1: Add Raft Dependencies to Workspace
- [x] Add `openraft` to workspace Cargo.toml (v0.9.21)
- [x] Add `tonic` (gRPC) for network layer
- [x] Add `prost` for protobuf serialization

### Task 0.2: Create `kalamdb-raft` Crate
- [x] Create crate structure under `backend/crates/kalamdb-raft/`
- [x] Add Cargo.toml with workspace dependencies
- [x] Create basic module structure:
  ```
  kalamdb-raft/src/
  ├── lib.rs
  ├── config.rs          # RaftConfig, ClusterConfig, ShardingConfig
  ├── group_id.rs        # GroupId enum (36 groups)
  ├── error.rs           # RaftError types
  ├── commands/          # Command/Response enums
  │   ├── mod.rs
  │   ├── system.rs
  │   ├── users.rs
  │   ├── jobs.rs
  │   └── data.rs
  └── executor/          # CommandExecutor trait + DirectExecutor
      ├── mod.rs
      ├── trait_def.rs
      └── direct.rs
  ```

### Task 0.3: Define GroupId Enum
- [x] Create GroupId enum with 36 variants (MetaSystem, MetaUsers, MetaJobs, DataUserShard(0-31), DataSharedShard(0))
- [x] Implement shard routing helpers
- [x] Add `ShardRouter` for user_id → shard mapping

---

## Phase 1: CommandExecutor Abstraction
**Status: ✅ Complete**

### Task 1.1: Define CommandExecutor Trait
- [x] Create `CommandExecutor` trait in `kalamdb-raft`
- [x] Define command enums: `SystemCommand`, `UsersCommand`, `JobsCommand`, `UserDataCommand`, `SharedDataCommand`
- [x] Define response enums

### Task 1.2: Implement DirectExecutor (Standalone)
- [x] Create `DirectExecutor` that logs commands (stub implementation)
- [x] Zero overhead for standalone mode

### Task 1.3: Implement StandaloneExecutor (Wired)
- [x] Create `StandaloneExecutor` in `kalamdb-core` that wires to actual providers
- [x] Wire namespace/table/storage operations to SystemTablesRegistry
- [x] Stub user/job/live_query operations (TODO: model adapters)
- [x] Add executor module to kalamdb-core

### Task 1.4: Stub RaftExecutor
- [x] Create `RaftExecutor` struct (placeholder)
- [x] Returns Internal error with "stub" message
- [x] Test for error handling

### Task 1.5: Integrate with AppContext
- [x] Add `executor: Arc<dyn CommandExecutor>` to AppContext
- [x] Add `ClusterSettings` to ServerConfig (kalamdb-commons)
- [x] Factory logic: StandaloneExecutor (no cluster config) vs RaftExecutor (cluster config present)
- [x] Add `executor()` accessor method to AppContext
- [x] Add `is_cluster_mode()` helper method
- [ ] Update handlers to use `ctx.executor().execute_*()`

---

## Phase 2: State Machine Implementations
**Status: ✅ Complete**

### Task 2.1: KalamStateMachine Trait
- [x] Define common trait for all state machines
- [x] Include apply, snapshot, restore methods
- [x] Idempotency tracking (last_applied LogId)
- [x] bincode 2.x serde helpers (encode/decode)

### Task 2.2: SystemStateMachine
- [x] Handles namespaces, tables, storages metadata
- [x] Snapshot cache with RwLock
- [x] Snapshot/restore implementation

### Task 2.3: UsersStateMachine
- [x] Handles user CRUD operations
- [x] CreateUser, UpdateUser, DeleteUser, RecordLogin, SetLocked
- [x] Snapshot/restore implementation

### Task 2.4: JobsStateMachine
- [x] Handles job creation, status updates
- [x] Includes ClaimJob/ReleaseJob for leader-only execution
- [x] CreateSchedule/DeleteSchedule for scheduled jobs
- [x] Snapshot/restore implementation

### Task 2.5: UserDataStateMachine
- [x] Handles user table INSERT/UPDATE/DELETE
- [x] Handles live_queries for users in this shard
- [x] Routes by user_id % 32 (shard parameter)
- [x] RegisterLiveQuery, UnregisterLiveQuery, CleanupNodeSubscriptions, PingLiveQuery

### Task 2.6: SharedDataStateMachine
- [x] Handles shared table operations
- [x] Single shard for Phase 1
- [x] Tracks recent operations for metrics

---

## Phase 3: Raft Core Implementation
**Status: 🔴 Not Started**

### Task 3.1: GenericLogStore
- [ ] Implement `openraft::RaftLogStorage` over StorageBackend
- [ ] Partition naming: `raft_log_{group_id}`
- [ ] Log entry serialization with bincode

### Task 3.2: gRPC Network Layer
- [ ] Create `proto/raft.proto` definitions
- [ ] Implement `RaftNetworkFactory` for gRPC transport
- [ ] AppendEntries, Vote, InstallSnapshot RPCs

### Task 3.3: RaftManager
- [ ] Orchestrate all 36 Raft groups
- [ ] Leader election per group
- [ ] Generic `propose()` method routing to correct group

### Task 3.4: Complete RaftExecutor
- [ ] Implement actual Raft proposal logic
- [ ] Wait for commit before returning
- [ ] Handle leader redirection

---

## Phase 4: Leader-Only Job Execution
**Status: 🔴 Not Started**

### Task 4.1: LeaderOnlyJobExecutor
- [ ] Check `is_leader(GroupId::MetaJobs)` before running jobs
- [ ] Job claiming via Raft (ClaimJob command)
- [ ] node_id tracking in system.jobs

### Task 4.2: Leader Failover Handling
- [ ] Detect orphaned jobs from failed leader
- [ ] Re-queue or fail based on job type
- [ ] `on_become_leader()` hook

### Task 4.3: Update UnifiedJobManager
- [ ] Integrate with LeaderOnlyJobExecutor
- [ ] Ensure flush/compaction/retention run on leader only

---

## Phase 5: Live Query Sharding
**Status: 🔴 Not Started**

### Task 5.1: Add node_id to LiveQuery Model
- [ ] Add `node_id: u64` field to LiveQuery struct
- [ ] Update LiveQueriesProvider with node_id methods

### Task 5.2: Live Query Commands in UserDataStateMachine
- [ ] RegisterLiveQuery, UnregisterLiveQuery commands
- [ ] CleanupNodeSubscriptions for failover
- [ ] PingLiveQuery for heartbeats

### Task 5.3: Connection Failover
- [ ] Detect stale subscriptions from failed nodes
- [ ] Re-register on new node

---

## Phase 6: Configuration & Startup
**Status: 🔴 Not Started**

### Task 6.1: Cluster Configuration Parsing
- [ ] Parse `[cluster]` section from server.toml
- [ ] ShardingConfig with num_user_shards, num_shared_shards
- [ ] Member list parsing

### Task 6.2: Startup Mode Detection
- [ ] No `[cluster]` = standalone (DirectExecutor)
- [ ] With `[cluster]` = cluster (RaftExecutor)
- [ ] Single-node cluster for testing

### Task 6.3: Graceful Shutdown
- [ ] Raft group shutdown sequence
- [ ] Leadership transfer before shutdown

---

## Phase 7: Testing
**Status: 🔴 Not Started**

### Task 7.1: Single-Node Raft Tests
- [ ] Standalone mode works unchanged
- [ ] Single-node cluster mode works
- [ ] State machine apply correctness

### Task 7.2: Multi-Node Tests
- [ ] 3-node cluster formation
- [ ] Leader election
- [ ] Proposal replication

### Task 7.3: Failure Scenarios
- [ ] Leader failure and re-election
- [ ] Minority partition (cannot commit)
- [ ] Node catchup after downtime

### Task 7.4: Snapshot Tests
- [ ] Snapshot creation
- [ ] Snapshot transfer to new node
- [ ] Restore from snapshot

---

## Implementation Progress

| Phase | Description | Status | Tasks Done |
|-------|-------------|--------|------------|
| 0 | Foundation | ✅ | 3/3 |
| 1 | CommandExecutor | 🟡 | 2/4 |
| 2 | State Machines | 🔴 | 0/6 |
| 3 | Raft Core | 🔴 | 0/4 |
| 4 | Leader-Only Jobs | 🔴 | 0/3 |
| 5 | Live Query Sharding | 🔴 | 0/3 |
| 6 | Configuration | 🔴 | 0/3 |
| 7 | Testing | 🔴 | 0/4 |

---

## Notes

- **Standalone mode MUST work unchanged** - no performance penalty
- **Use existing providers** - state machines are command routers, not data stores
- **StorageBackend abstraction** - no direct RocksDB in kalamdb-raft
- **Workspace dependencies** - add to root Cargo.toml first
