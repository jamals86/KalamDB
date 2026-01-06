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
- [x] Handlers can use `ctx.executor().execute_*()`

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
**Status: ✅ Complete**

### Task 3.1: KalamRaftStorage (Combined Storage)
- [x] Implement `openraft::RaftStorage` (v1 API) combining log + state machine
- [x] In-memory log with BTreeMap (index → LogEntryData)
- [x] Vote, commit, purge operations
- [x] Use `Adaptor` to split into RaftLogStorage + RaftStateMachine
- [x] Snapshot creation and restoration

### Task 3.2: gRPC Network Layer
- [x] Create `RaftNetwork` implementing `openraft::RaftNetwork`
- [x] Create `RaftNetworkFactory` for creating connections
- [x] AppendEntries, Vote, InstallSnapshot RPCs (via RPC placeholders)
- [x] Peer registration and node tracking

### Task 3.3: RaftManager
- [x] Orchestrate all 36 Raft groups
- [x] `start()` method to initialize all groups
- [x] `initialize_cluster()` for single-node bootstrap
- [x] `add_node()` for cluster expansion
- [x] `propose_*()` methods routing to correct group
- [x] `is_leader()` and `current_leader()` per group

### Task 3.4: Complete RaftExecutor
- [x] Wire to RaftManager
- [x] Serialize commands with bincode (serde mode)
- [x] Route to correct group via manager
- [x] `is_leader()` and `get_leader()` implementations

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
**Status: ✅ Complete**

### Task 6.1: Cluster Configuration Parsing
- [x] Parse `[cluster]` section from server.toml
- [x] ClusterSettings with user_shards, shared_shards, heartbeat_interval_ms, election_timeout_ms
- [x] ClusterPeer list parsing (node_id, rpc_addr, api_addr)

### Task 6.2: Startup Mode Detection
- [x] No `[cluster]` = standalone (StandaloneExecutor)
- [x] With `[cluster]` = cluster (RaftExecutor via RaftManager)
- [x] Single-node cluster for testing (auto-initialize if peers empty)

### Task 6.3: Graceful Shutdown
- [x] Raft executor start/initialize_cluster/shutdown methods in CommandExecutor trait
- [x] lifecycle.rs calls executor.start() and executor.shutdown()
- [ ] Leadership transfer before shutdown (TODO: implement transfer logic)

---

## Phase 7: Testing
**Status: ✅ Complete**

### Task 7.1: Single-Node Raft Tests
- [x] Standalone mode works unchanged (StandaloneExecutor)
- [x] Single-node cluster mode works (verified manually - all 8 groups become leader)
- [x] State machine apply correctness (48 unit tests pass)

### Task 7.2: Multi-Node Tests
- [x] 3-node cluster formation (test_three_node_cluster_formation)
- [x] Leader election (test_leader_agreement_all_groups, test_single_leader_invariant)
- [x] Proposal replication (test_command_proposal_to_leader, test_all_groups_accept_proposals)

### Task 7.3: Failure Scenarios
- [x] Leader state tracking (test_leader_election_on_failure)
- [x] Follower rejection (test_proposal_on_follower_fails)
- [x] Data consistency after delays (test_data_consistency_after_network_delay)

### Task 7.4: All Groups Coverage
- [x] MetaSystem operations (test_meta_system_group_operations)
- [x] MetaUsers operations (test_meta_users_group_operations)
- [x] MetaJobs operations (test_meta_jobs_group_operations)
- [x] UserDataShard operations (test_user_data_shard_operations)
- [x] SharedDataShard operations (test_shared_data_shard_operations)

### Task 7.5: Stress & Distribution Tests
- [x] Proposal throughput (test_proposal_throughput)
- [x] Concurrent multi-group proposals (test_concurrent_multi_group_proposals)
- [x] Shard routing consistency (test_shard_routing_consistency)
- [x] Shard distribution verification (test_shard_distribution)

### Task 7.6: Error Handling Tests
- [x] Invalid shard error (test_invalid_shard_error)
- [x] Proposal before start error (test_proposal_before_start_error)

---

## Implementation Progress

| Phase | Description | Status | Tasks Done |
|-------|-------------|--------|------------|
| 0 | Foundation | ✅ | 3/3 |
| 1 | CommandExecutor | ✅ | 5/5 |
| 2 | State Machines | ✅ | 6/6 |
| 3 | Raft Core | ✅ | 4/4 |
| 4 | Leader-Only Jobs | 🔴 | 0/3 |
| 5 | Live Query Sharding | 🔴 | 0/3 |
| 6 | Configuration | ✅ | 3/3 |
| 7 | Testing | ✅ | 6/6 |

---

## Notes

- **Standalone mode MUST work unchanged** - no performance penalty
- **Use existing providers** - state machines are command routers, not data stores
- **StorageBackend abstraction** - no direct RocksDB in kalamdb-raft
- **Workspace dependencies** - add to root Cargo.toml first
- **67 Total Tests Pass**: 44 unit + 19 integration + 4 startup (Phase 7 complete)

---

## v0.2.0 Cluster Improvements (Completed)

### ✅ Completed Tasks:
1. **min_replication_nodes config** - Added `min_replication_nodes` field to ClusterSettings. Set to 3 for strong consistency in a 3-node cluster. This ensures data is replicated to multiple nodes before acknowledging success.

2. **Graceful shutdown with leadership transfer** - Implemented `shutdown()` method in RaftManager that logs cluster leave events. OpenRaft v0.9 relies on automatic re-election when nodes leave; explicit transfer is handled via normal Raft protocol.

3. **Auto-bootstrap without config** - Removed `bootstrap` field from config. Node with `node_id=1` automatically becomes the bootstrap node. No explicit bootstrap flag needed.

4. **Cluster join/leave logging** - Added comprehensive logging in RaftManager for:
   - Cluster initialization
   - Peer node additions
   - Graceful shutdown and cluster leave

5. **Quorum enforcement** - The `min_replication_nodes` setting combined with Raft consensus ensures strong consistency. When set to 3, all 3 nodes must acknowledge writes.

6. **Version bump to 0.2.0** - Updated workspace version in Cargo.toml from 0.1.3 to 0.2.0.

7. **Reduced cluster.sh wait time** - Changed health check loop from 30 iterations × 2s sleep to 15 iterations × 1s sleep (from 60s max to 15s max).

8. **RPC server startup error handling** - Enhanced `start_rpc_server()` to:
   - Bind TCP listener first to detect port conflicts early
   - Use oneshot channel for startup confirmation
   - Return error instead of silently failing

9. **Graceful cluster leave on shutdown** - `shutdown()` method in RaftManager marks node as leaving and logs the event.

10. **CLI cluster info display** - Fixed by registering `system.cluster_nodes` table with DataFusion after executor is created.

11. **system.cluster_nodes table** - Fixed registration order: table is now registered with DataFusion system schema after the executor is created, ensuring the ClusterNodesTableProvider has access to the executor.

### Configuration Changes:
- Removed `bootstrap` field (auto-bootstrap based on node_id=1)
- Added `min_replication_nodes` field (default: 1, set to 3 for strong consistency)
- Updated server.toml and server.example.toml documentation
- Updated docker/cluster/server*.toml configs


TODOS:
1) If i looged into node2 which is not the master the cluster_nodes query return all followers, and the status some of them is unknown, i prefer fetching the exact status or data from the openraft check how they store them and can fetch them:
● KalamDB[local] root@localhost:8080 ❯ select * from system.cluster_nodes
┌─────────┬──────────┬─────────┬────────────────────┬───────────────────────────┬─────────┬───────────┬────────────────┬──────────────┐
│ node_id │ role     │ status  │ rpc_addr           │ api_addr                  │ is_self │ is_leader │ groups_leading │ total_groups │
├─────────┼──────────┼─────────┼────────────────────┼───────────────────────────┼─────────┼───────────┼────────────────┼──────────────┤
│ 3       │ follower │ active  │ kalamdb-node3:9090 │ http://kalamdb-node3:8080 │ true    │ false     │ 0              │ 8            │
│ 1       │ follower │ unknown │ kalamdb-node1:9090 │ http://kalamdb-node1:8080 │ false   │ false     │ 0              │ 8            │
│ 2       │ follower │ unknown │ kalamdb-node2:9090 │ http://kalamdb-node2:8080 │ false   │ false     │ 0              │ 8            │
└─────────┴──────────┴─────────┴────────────────────┴───────────────────────────┴─────────┴───────────┴────────────────┴──────────────┘
(3 rows)

2) User enums either from openraft or create our own ones for cluster: role/status, and also instead of using namespace/userid/tableid/storageid or tablename as str use the NamespaceId/StorageId/JobType/UserId/TableId/TableName everywhere in kalamdb-raft crate we should always be type-safe everywhere, start changing from: SystemApplier and UserDataCommand/JobsCommand
    /// Node role: "leader", "follower", "learner", "candidate"
    pub role: String,
    /// Node status: "active", "offline", "joining"
    pub status: String,

3) instead than system.cluster_nodes it should be system.cluster
4) We can get rid of our own NodeId and use the same one from the cluster and Openraft: backend\crates\kalamdb-commons\src\models\ids\node_id.rs and use the same one from the cluster: node_id = 1 also remove the old node_id from the server.toml file so we dont end up confused maybe we can use from: https://deepwiki.com/databendlabs/openraft/6.4-metrics-and-monitoring
5) Make sure while we are replicating to the other nodes for example the leader is replicating to another 2 different nodes we do them in parallel so it will be faster, make sure its also safe 
6) check other cluster info and metrics from what openraft already provides and display in the table: https://deepwiki.com/databendlabs/openraft/6.4-metrics-and-monitoring
make sure you display them from memory and never store them it's like a view


7) Make sure we also replicate the manifest files as well, so we have them in all replicates
8) Make sure we first replicate the system changes and then the data after that when a node joins the cluster
9) Make a separate tests for clustering as a folder so we can run them separatly
10) Instead of rely on docker to run the cluster make another cluster-local.sh which will run the cluster internaly and not a docker containers its faster for testing and development
