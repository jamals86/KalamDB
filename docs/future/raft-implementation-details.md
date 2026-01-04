# KalamDB Multi-Raft Implementation Details

This document supplements `raft-replication-design.md` with detailed implementation specifics using **OpenRaft** with **gRPC networking** and **Multi-Raft groups**, based on:
- [raft-kv-memstore-grpc](https://github.com/databendlabs/openraft/tree/main/examples/raft-kv-memstore-grpc) - gRPC networking pattern
- [multi-raft-kv](https://github.com/databendlabs/openraft/tree/main/examples/multi-raft-kv) - Multi-Raft group pattern

## 0. Existing Infrastructure Analysis

**Key Finding**: KalamDB already has the metadata storage infrastructure in `kalamdb-system`. The Raft state machines should **delegate to existing providers**, not duplicate their logic.

### 0.1 What Already Exists in `kalamdb-system`

| Provider | Location | Key Methods | Storage |
|----------|----------|-------------|---------|
| `NamespacesTableProvider` | `kalamdb-system/src/providers/namespaces/` | `create_namespace()`, `get_namespace()`, `delete_namespace()` | EntityStore via RocksDB |
| `TablesTableProvider` | `kalamdb-system/src/providers/tables/` | `create_table()`, `update_table()`, `delete_table()`, `get_table_by_id()` | Versioned store via RocksDB |
| `StoragesTableProvider` | `kalamdb-system/src/providers/storages/` | `create_storage()`, `update_storage()`, `delete_storage()` | EntityStore via RocksDB |
| `UsersTableProvider` | `kalamdb-system/src/providers/users/` | `create_user()`, `update_user()`, `delete_user()` | EntityStore via RocksDB |
| `JobsTableProvider` | `kalamdb-system/src/providers/jobs/` | `create_job()`, `update_job_status()`, `complete_job()` | EntityStore via RocksDB |

### 0.2 What `SchemaRegistry` Does

The `SchemaRegistry` in `kalamdb-core` is an **in-memory cache** on top of `TablesTableProvider`:

```rust
// kalamdb-core/src/schema_registry/registry/core.rs
pub struct SchemaRegistry {
    table_cache: TableCache,  // In-memory cache for fast lookups
    base_session_context: OnceLock<Arc<SessionContext>>,
}
```

It uses `SchemaPersistence` for read-through/write-through to `TablesTableProvider`:
- `SchemaPersistence::put_table_definition()` → `TablesTableProvider::create_table()`
- `SchemaPersistence::get_table_definition()` → `TablesTableProvider::get_table_by_id()`

### 0.3 Architecture Decision: State Machines as Command Routers

**The Raft state machines should NOT duplicate storage logic.** Instead:

```
Raft Command → State Machine → kalamdb-system Provider → RocksDB
```

This means:
- `SystemStateMachine` uses `TablesTableProvider`, `NamespacesTableProvider`, `StoragesTableProvider`
- `UsersStateMachine` uses `UsersTableProvider`
- `JobsStateMachine` uses `JobsTableProvider`

The Raft log stores only **commands** (CreateNamespace, CreateTable, etc.), not the actual data. The data is stored by the existing providers.

### 0.4 Crate Organization Strategy

Current state: Too much code in `kalamdb-core`. Proposed refactoring:

| Crate | Purpose | Key Types |
|-------|---------|-----------|
| `kalamdb-raft` (NEW) | Raft consensus, Multi-Raft groups, gRPC networking | `RaftManager`, `RaftRouter`, `GenericLogStore` |
| `kalamdb-system` (EXISTS) | System table providers, EntityStore | `TablesTableProvider`, `NamespacesTableProvider`, `UsersTableProvider` |
| `kalamdb-core` (EXISTS) | Orchestration, SchemaRegistry cache | `AppContext`, `SchemaRegistry` |
| `kalamdb-store` (EXISTS) | StorageBackend abstraction | `StorageBackend`, `RocksDBBackend` |

The Raft state machines live in `kalamdb-raft` but take `SystemTablesRegistry` as a dependency to access the providers.

## 1. Raft Group Design

KalamDB uses **Multi-Raft** with two categories of groups:

### 1.1 Metadata Groups (3 groups)

| Group ID | Name | Replicated Entities | Churn Level | Rationale |
|----------|------|---------------------|-------------|-----------|
| `meta:system` | System Catalog | Namespaces, Tables, Storages, Schema versions | Low | Core DDL metadata, must be strongly consistent |
| `meta:users` | Authentication | Users, Roles, Permissions, OAuth tokens | Low | Security-critical, separate from catalog for isolation |
| `meta:jobs` | Job Coordination | Jobs, Job status, Scheduled tasks | Medium | Job state changes frequently but needs cluster-wide coordination |

### 1.2 Data Groups (32 user shards + 1 shared shard = 33 groups)

| Group ID Pattern | Name | Replicated Data | Rationale |
|------------------|------|-----------------|-----------|
| `data:user:0` .. `data:user:31` | User Table Shards | INSERT/UPDATE/DELETE for user tables | Partition by `user_id` for horizontal scaling |
| `data:shared:0` | Shared Table Shard | INSERT/UPDATE/DELETE for shared tables | Single shard for now (future: multi-shard) |

**Total Raft Groups: 36** (3 metadata + 33 data)

### 1.3 Shard Assignment

```rust
/// Determines which Raft group handles a data operation
pub struct ShardRouter {
    num_user_shards: u32,      // Default: 32
    num_shared_shards: u32,    // Default: 1 (future: configurable)
}

impl ShardRouter {
    /// Get shard for user table operation
    pub fn user_shard(&self, user_id: &UserId) -> GroupId {
        let hash = xxhash64(user_id.as_bytes());
        let shard = (hash % self.num_user_shards as u64) as u32;
        GroupId::DataUserShard(shard)
    }
    
    /// Get shard for shared table operation
    pub fn shared_shard(&self, namespace_id: &NamespaceId) -> GroupId {
        // For now: single shard
        // Future: hash namespace_id to distribute across shards
        let _hash = xxhash64(namespace_id.as_bytes());
        let shard = 0; // Single shard for Phase 1
        GroupId::DataSharedShard(shard)
    }
}
```

### 1.4 GroupId Enum

```rust
/// Identifies a Raft group
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GroupId {
    // Metadata groups (3)
    MetaSystem,
    MetaUsers,
    MetaJobs,
    
    // Data groups - user table shards (32)
    DataUserShard(u32),  // 0..31
    
    // Data groups - shared table shards (1 for now)
    DataSharedShard(u32),  // 0 for Phase 1
}

impl GroupId {
    pub fn as_str(&self) -> String {
        match self {
            GroupId::MetaSystem => "meta:system".to_string(),
            GroupId::MetaUsers => "meta:users".to_string(),
            GroupId::MetaJobs => "meta:jobs".to_string(),
            GroupId::DataUserShard(n) => format!("data:user:{}", n),
            GroupId::DataSharedShard(n) => format!("data:shared:{}", n),
        }
    }
    
    pub fn all_groups(config: &ShardConfig) -> Vec<GroupId> {
        let mut groups = vec![
            GroupId::MetaSystem,
            GroupId::MetaUsers,
            GroupId::MetaJobs,
        ];
        for i in 0..config.num_user_shards {
            groups.push(GroupId::DataUserShard(i));
        }
        for i in 0..config.num_shared_shards {
            groups.push(GroupId::DataSharedShard(i));
        }
        groups
    }
}
```

### 1.5 Architecture Diagram (Updated)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              KalamDB Node 1                                  │
│                                                                              │
│  ╔═══════════════════════════════════════════════════════════════════════╗  │
│  ║                        METADATA GROUPS (3)                             ║  │
│  ║  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                 ║  │
│  ║  │ meta:system  │  │ meta:users   │  │ meta:jobs    │                 ║  │
│  ║  │ • Namespaces │  │ • Users      │  │ • Jobs       │                 ║  │
│  ║  │ • Tables     │  │ • Roles      │  │ • Status     │                 ║  │
│  ║  │ • Storages   │  │ • Perms      │  │ • Schedules  │                 ║  │
│  ║  └──────────────┘  └──────────────┘  └──────────────┘                 ║  │
│  ╚═══════════════════════════════════════════════════════════════════════╝  │
│                                                                              │
│  ╔═══════════════════════════════════════════════════════════════════════╗  │
│  ║                     DATA GROUPS - USER SHARDS (32)                     ║  │
│  ║  ┌────────────┐ ┌────────────┐ ┌────────────┐       ┌────────────┐    ║  │
│  ║  │data:user:0 │ │data:user:1 │ │data:user:2 │  ...  │data:user:31│    ║  │
│  ║  │ user_id%32 │ │ user_id%32 │ │ user_id%32 │       │ user_id%32 │    ║  │
│  ║  │   == 0     │ │   == 1     │ │   == 2     │       │   == 31    │    ║  │
│  ║  └────────────┘ └────────────┘ └────────────┘       └────────────┘    ║  │
│  ╚═══════════════════════════════════════════════════════════════════════╝  │
│                                                                              │
│  ╔═══════════════════════════════════════════════════════════════════════╗  │
│  ║                   DATA GROUPS - SHARED SHARDS (1)                      ║  │
│  ║  ┌──────────────┐                                                      ║  │
│  ║  │data:shared:0 │  (Future: data:shared:1, data:shared:2, ...)        ║  │
│  ║  │ All shared   │                                                      ║  │
│  ║  │ table writes │                                                      ║  │
│  ║  └──────────────┘                                                      ║  │
│  ╚═══════════════════════════════════════════════════════════════════════╝  │
│                                                                              │
│                    ┌────────────────────────────┐                            │
│                    │        RaftRouter          │                            │
│                    │  (routes by group_id)      │                            │
│                    └────────────┬───────────────┘                            │
│                                 │                                            │
│                    ┌────────────┴───────────────┐                            │
│                    │      gRPC Server           │                            │
│                    │   (tonic, port 9090)       │                            │
│                    └────────────────────────────┘                            │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 1.6 Why Multiple Raft Groups?

1. **Failure Isolation**: A bug or slow operation in job management won't block catalog DDL
2. **Independent Leadership**: Each group can have a different leader, distributing load
3. **Log Compaction**: Each group's log grows independently, snapshots are smaller
4. **Security Boundary**: User/auth data is isolated from catalog mutations
5. **Data Scalability**: 32 user shards allow horizontal write scaling
└─────────────────────────────────┼───────────────────────────────────────────┘
                                  │
                          Network (gRPC)
                                  │
┌─────────────────────────────────┼───────────────────────────────────────────┐
│                              KalamDB Node 2                                  │
│                    ┌────────────┴────────────┐                               │
│                    │    gRPC Server          │                               │
│                    └─────────────────────────┘                               │
│                              ...                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Why Multiple Raft Groups?

1. **Failure Isolation**: A bug or slow operation in job management won't block catalog DDL
2. **Independent Leadership**: Each group can have a different leader, distributing load
3. **Log Compaction**: Each group's log grows independently, snapshots are smaller
4. **Security Boundary**: User/auth data is isolated from catalog mutations

## 2. Generic Design Principles

### 2.1 StorageBackend Integration

**Critical Rule**: All Raft log and state machine persistence MUST use `kalamdb-store::StorageBackend`.

```rust
// ✅ CORRECT: Use StorageBackend abstraction
pub struct RaftLogStore<S: StorageBackend> {
    backend: Arc<S>,
    group_id: GroupId,
    partition: Partition, // e.g., "raft_log_system", "raft_log_users"
}

impl<S: StorageBackend> RaftLogStorage for RaftLogStore<S> {
    async fn append(&mut self, entries: &[Entry]) -> Result<()> {
        let ops: Vec<Operation> = entries.iter()
            .map(|e| Operation::Put {
                partition: self.partition.clone(),
                key: log_key(e.log_id),
                value: bincode::serialize(e)?,
            })
            .collect();
        self.backend.batch(ops)?;
        Ok(())
    }
}

// ❌ WRONG: Direct RocksDB usage
pub struct RaftLogStore {
    db: Arc<rocksdb::DB>,  // Violates crate boundary!
}
```

### 2.2 Generic State Machine Trait

Define a trait for state machine operations that all groups implement:

```rust
/// Generic trait for Raft state machine operations
/// Each group (system, users, jobs) implements this
pub trait RaftStateMachine: Send + Sync + 'static {
    /// The command type this state machine accepts
    type Command: Serialize + DeserializeOwned + Send + Sync;
    
    /// The response type returned after applying a command
    type Response: Serialize + DeserializeOwned + Send + Sync;
    
    /// Apply a committed command to the state machine
    async fn apply(&mut self, cmd: Self::Command) -> Result<Self::Response>;
    
    /// Build a snapshot of current state
    async fn build_snapshot(&self) -> Result<SnapshotData>;
    
    /// Restore state from a snapshot
    async fn restore_snapshot(&mut self, snapshot: SnapshotData) -> Result<()>;
}
```

### 2.3 Partition Naming Convention

Each Raft group uses dedicated RocksDB partitions (column families):

| Group | Log Partition | State Partition | Vote Partition |
|-------|---------------|-----------------|----------------|
| `system` | `raft_log_system` | `raft_state_system` | `raft_vote_system` |
| `users` | `raft_log_users` | `raft_state_users` | `raft_vote_users` |
| `jobs` | `raft_log_jobs` | `raft_state_jobs` | `raft_vote_jobs` |

## 3. Crate Structure: `kalamdb-raft`

```
backend/crates/kalamdb-raft/
├── Cargo.toml
├── build.rs                           # protobuf compilation
├── proto/
│   └── raft.proto                     # gRPC service definitions
└── src/
    ├── lib.rs                         # Public API exports
    ├── config.rs                      # RaftConfig, GroupConfig
    ├── types.rs                       # TypeConfig, NodeId, GroupId
    │
    ├── groups/                        # Group definitions
    │   ├── mod.rs
    │   ├── group_id.rs                # GroupId enum (System, Users, Jobs)
    │   ├── system_group.rs            # System catalog group setup
    │   ├── users_group.rs             # Users/auth group setup
    │   └── jobs_group.rs              # Jobs coordination group setup
    │
    ├── storage/                       # Raft log storage (uses StorageBackend)
    │   ├── mod.rs
    │   ├── log_store.rs               # RaftLogStorage implementation
    │   ├── vote_store.rs              # Vote/HardState persistence
    │   └── snapshot_store.rs          # Snapshot persistence
    │
    ├── state_machine/                 # State machine implementations
    │   ├── mod.rs
    │   ├── trait.rs                   # RaftStateMachine trait
    │   ├── system_sm.rs               # System catalog state machine
    │   ├── users_sm.rs                # Users state machine
    │   └── jobs_sm.rs                 # Jobs state machine
    │
    ├── network/                       # gRPC networking
    │   ├── mod.rs
    │   ├── router.rs                  # Multi-Raft message router
    │   ├── grpc_network.rs            # RaftNetwork implementation
    │   └── connection_pool.rs         # Connection management
    │
    ├── grpc/                          # gRPC services
    │   ├── mod.rs
    │   ├── raft_service.rs            # Raft internal RPCs
    │   └── management_service.rs      # Cluster management RPCs
    │
    ├── commands/                      # Command types per group
    │   ├── mod.rs
    │   ├── system_commands.rs         # CreateNamespace, CreateTable, etc.
    │   ├── users_commands.rs          # CreateUser, UpdateRole, etc.
    │   └── jobs_commands.rs           # CreateJob, UpdateJobStatus, etc.
    │
    └── manager.rs                     # RaftManager: orchestrates all groups
```

## 4. Command Definitions

### 4.1 System Group Commands

```rust
/// Commands for the system catalog Raft group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemCommand {
    // Namespace operations
    CreateNamespace {
        namespace: Namespace,
    },
    DropNamespace {
        namespace_id: NamespaceId,
    },
    
    // Table operations
    CreateTable {
        table: TableDefinition,
    },
    AlterTable {
        table_id: TableId,
        changes: Vec<AlterTableChange>,
        new_version: u64,
    },
    DropTable {
        table_id: TableId,
    },
    
    // Storage operations
    CreateStorage {
        storage: Storage,
    },
    UpdateStorage {
        storage: Storage,
    },
    DeleteStorage {
        storage_id: StorageId,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemResponse {
    Ok,
    NamespaceCreated { namespace_id: NamespaceId },
    TableCreated { table_id: TableId },
    Error { message: String },
}
```

### 4.2 Users Group Commands

```rust
/// Commands for the users/auth Raft group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UsersCommand {
    CreateUser {
        user: User,
    },
    UpdateUser {
        user_id: UserId,
        changes: UserChanges,
    },
    DeleteUser {
        user_id: UserId,
    },
    CreateRole {
        role: Role,
    },
    AssignRole {
        user_id: UserId,
        role_id: RoleId,
    },
    RevokeRole {
        user_id: UserId,
        role_id: RoleId,
    },
    // OAuth tokens (if replicated)
    StoreOAuthToken {
        user_id: UserId,
        token: OAuthToken,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UsersResponse {
    Ok,
    UserCreated { user_id: UserId },
    Error { message: String },
}
```

### 4.3 Jobs Group Commands

```rust
/// Commands for the jobs coordination Raft group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JobsCommand {
    CreateJob {
        job: Job,
    },
    UpdateJobStatus {
        job_id: JobId,
        status: JobStatus,
        updated_at: DateTime<Utc>,
    },
    CompleteJob {
        job_id: JobId,
        result: JobResult,
    },
    CancelJob {
        job_id: JobId,
        reason: String,
    },
    // Scheduled jobs
    CreateSchedule {
        schedule: JobSchedule,
    },
    DeleteSchedule {
        schedule_id: ScheduleId,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JobsResponse {
    Ok,
    JobCreated { job_id: JobId },
    Error { message: String },
}
```

## 5. gRPC Protocol Definition

```protobuf
// proto/raft.proto
syntax = "proto3";
package kalamdb.raft;

// ============ Raft Internal Service ============

service RaftService {
    // AppendEntries RPC (log replication)
    rpc AppendEntries(AppendEntriesRequest) returns (AppendEntriesResponse);
    
    // RequestVote RPC (leader election)
    rpc Vote(VoteRequest) returns (VoteResponse);
    
    // InstallSnapshot RPC (catch-up)
    rpc InstallSnapshot(stream SnapshotChunk) returns (InstallSnapshotResponse);
    
    // Pipeline streaming for efficient replication
    rpc StreamAppend(stream AppendEntriesRequest) returns (stream AppendEntriesResponse);
}

// ============ Cluster Management Service ============

service ClusterService {
    // Add a learner node to a group
    rpc AddLearner(AddLearnerRequest) returns (AddLearnerResponse);
    
    // Promote learner to voter
    rpc ChangeMembership(ChangeMembershipRequest) returns (ChangeMembershipResponse);
    
    // Get cluster status
    rpc GetStatus(GetStatusRequest) returns (GetStatusResponse);
    
    // Forward client request to leader
    rpc ForwardToLeader(ForwardRequest) returns (ForwardResponse);
}

// ============ Common Types ============

message NodeInfo {
    uint64 node_id = 1;
    string raft_addr = 2;
    string api_addr = 3;
}

// All messages include group_id for routing
message AppendEntriesRequest {
    string group_id = 1;  // "system", "users", "jobs"
    uint64 term = 2;
    uint64 leader_id = 3;
    uint64 prev_log_index = 4;
    uint64 prev_log_term = 5;
    repeated LogEntry entries = 6;
    uint64 leader_commit = 7;
}

message AppendEntriesResponse {
    string group_id = 1;
    uint64 term = 2;
    bool success = 3;
    uint64 conflict_index = 4;
}

message VoteRequest {
    string group_id = 1;
    uint64 term = 2;
    uint64 candidate_id = 3;
    uint64 last_log_index = 4;
    uint64 last_log_term = 5;
}

message VoteResponse {
    string group_id = 1;
    uint64 term = 2;
    bool vote_granted = 3;
}

message LogEntry {
    uint64 index = 1;
    uint64 term = 2;
    bytes payload = 3;  // Serialized command (SystemCommand, UsersCommand, etc.)
}

message SnapshotChunk {
    string group_id = 1;
    uint64 term = 2;
    uint64 leader_id = 3;
    uint64 last_included_index = 4;
    uint64 last_included_term = 5;
    uint64 offset = 6;
    bytes data = 7;
    bool done = 8;
}

// ============ Management Types ============

message AddLearnerRequest {
    string group_id = 1;
    NodeInfo node = 2;
}

message ChangeMembershipRequest {
    string group_id = 1;
    repeated uint64 voter_ids = 2;
}

message GetStatusRequest {
    optional string group_id = 1;  // If empty, return all groups
}

message GetStatusResponse {
    repeated GroupStatus groups = 1;
}

message GroupStatus {
    string group_id = 1;
    uint64 leader_id = 2;
    uint64 term = 3;
    uint64 commit_index = 4;
    uint64 last_applied = 5;
    repeated MemberStatus members = 6;
}

message MemberStatus {
    uint64 node_id = 1;
    string role = 2;  // "leader", "follower", "learner"
    uint64 match_index = 3;
    bool reachable = 4;
}

message ForwardRequest {
    string group_id = 1;
    bytes command = 2;  // Serialized command
}

message ForwardResponse {
    bool success = 1;
    bytes response = 2;  // Serialized response
    optional string error = 3;
}
```

## 6. Router Implementation

The router dispatches incoming gRPC messages to the correct Raft group:

```rust
/// Routes Raft messages to the correct group
pub struct RaftRouter {
    /// Map from GroupId to Raft instance
    groups: HashMap<GroupId, RaftHandle>,
}

impl RaftRouter {
    pub fn new() -> Self {
        Self {
            groups: HashMap::new(),
        }
    }
    
    /// Register a Raft group
    pub fn register(&mut self, group_id: GroupId, raft: RaftHandle) {
        self.groups.insert(group_id, raft);
    }
    
    /// Route an AppendEntries request to the correct group
    pub async fn route_append_entries(
        &self,
        req: AppendEntriesRequest,
    ) -> Result<AppendEntriesResponse> {
        let group_id = GroupId::from_str(&req.group_id)?;
        let raft = self.groups.get(&group_id)
            .ok_or_else(|| Error::UnknownGroup(req.group_id.clone()))?;
        raft.append_entries(req.into()).await
    }
    
    /// Route a Vote request to the correct group
    pub async fn route_vote(&self, req: VoteRequest) -> Result<VoteResponse> {
        let group_id = GroupId::from_str(&req.group_id)?;
        let raft = self.groups.get(&group_id)
            .ok_or_else(|| Error::UnknownGroup(req.group_id.clone()))?;
        raft.vote(req.into()).await
    }
    
    /// Get leader for a specific group
    pub fn get_leader(&self, group_id: &GroupId) -> Option<NodeId> {
        self.groups.get(group_id)?.current_leader()
    }
}
```

## 7. Log Storage Implementation (Generic over StorageBackend)

```rust
use kalamdb_store::{StorageBackend, Partition, Operation};

/// Raft log storage backed by StorageBackend
/// 
/// This is GENERIC and works with any StorageBackend implementation
/// (RocksDB, Sled, in-memory, etc.)
pub struct GenericLogStore<S: StorageBackend> {
    backend: Arc<S>,
    group_id: GroupId,
    
    // Partition names derived from group_id
    log_partition: Partition,
    vote_partition: Partition,
    state_partition: Partition,
}

impl<S: StorageBackend> GenericLogStore<S> {
    pub fn new(backend: Arc<S>, group_id: GroupId) -> Result<Self> {
        let log_partition = Partition::new(format!("raft_log_{}", group_id.as_str()));
        let vote_partition = Partition::new(format!("raft_vote_{}", group_id.as_str()));
        let state_partition = Partition::new(format!("raft_state_{}", group_id.as_str()));
        
        // Ensure partitions exist
        backend.create_partition(&log_partition)?;
        backend.create_partition(&vote_partition)?;
        backend.create_partition(&state_partition)?;
        
        Ok(Self {
            backend,
            group_id,
            log_partition,
            vote_partition,
            state_partition,
        })
    }
    
    /// Encode log index as big-endian bytes for lexicographic ordering
    fn log_key(index: u64) -> [u8; 8] {
        index.to_be_bytes()
    }
}

impl<S: StorageBackend> RaftLogStorage<TypeConfig> for GenericLogStore<S> {
    async fn get_log_entries(&self, range: Range<u64>) -> Result<Vec<Entry>> {
        let start_key = Self::log_key(range.start);
        let end_key = Self::log_key(range.end);
        
        let iter = self.backend.scan(
            &self.log_partition,
            Some(&start_key),
            None,
            None,
        )?;
        
        iter.take_while(|(k, _)| k.as_slice() < end_key.as_slice())
            .map(|(_, v)| bincode::deserialize(&v).map_err(Into::into))
            .collect()
    }
    
    async fn append(&mut self, entries: &[Entry]) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        
        let ops: Vec<Operation> = entries
            .iter()
            .map(|entry| {
                let key = Self::log_key(entry.log_id.index);
                let value = bincode::serialize(entry)?;
                Ok(Operation::Put {
                    partition: self.log_partition.clone(),
                    key: key.to_vec(),
                    value,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        
        self.backend.batch(ops)
    }
    
    async fn truncate_after(&mut self, after: u64) -> Result<()> {
        // Collect keys to delete
        let start_key = Self::log_key(after + 1);
        let keys_to_delete: Vec<Vec<u8>> = self.backend
            .scan(&self.log_partition, Some(&start_key), None, None)?
            .map(|(k, _)| k)
            .collect();
        
        if keys_to_delete.is_empty() {
            return Ok(());
        }
        
        let ops: Vec<Operation> = keys_to_delete
            .into_iter()
            .map(|key| Operation::Delete {
                partition: self.log_partition.clone(),
                key,
            })
            .collect();
        
        self.backend.batch(ops)
    }
    
    async fn save_vote(&mut self, vote: &Vote) -> Result<()> {
        let value = bincode::serialize(vote)?;
        self.backend.put(&self.vote_partition, b"current_vote", &value)
    }
    
    async fn read_vote(&self) -> Result<Option<Vote>> {
        match self.backend.get(&self.vote_partition, b"current_vote")? {
            Some(v) => Ok(Some(bincode::deserialize(&v)?)),
            None => Ok(None),
        }
    }
}
```

## 8. State Machine Implementation (Using Existing Providers)

**Key Design**: State machines delegate to `kalamdb-system` providers instead of managing their own storage. This avoids duplication and leverages the existing, tested infrastructure.

```rust
use kalamdb_system::{TablesTableProvider, NamespacesTableProvider, StoragesTableProvider};
use kalamdb_commons::system::Namespace;
use kalamdb_commons::schemas::TableDefinition;

/// System catalog state machine
/// 
/// Applies catalog commands by delegating to existing kalamdb-system providers.
/// The Raft log only stores commands; actual data lives in the providers.
pub struct SystemStateMachine {
    /// Reference to existing namespaces provider (from SystemTablesRegistry)
    namespaces: Arc<NamespacesTableProvider>,
    
    /// Reference to existing tables provider (from SystemTablesRegistry)
    tables: Arc<TablesTableProvider>,
    
    /// Reference to existing storages provider (from SystemTablesRegistry)
    storages: Arc<StoragesTableProvider>,
    
    /// Last applied log ID for idempotency (stored in RocksDB partition)
    last_applied_store: GenericLogStore<dyn StorageBackend>,
}

impl SystemStateMachine {
    /// Create from existing SystemTablesRegistry (no duplication!)
    pub fn new(
        registry: Arc<SystemTablesRegistry>,
        backend: Arc<dyn StorageBackend>,
    ) -> Result<Self> {
        Ok(Self {
            namespaces: registry.namespaces(),
            tables: registry.tables(),
            storages: registry.storages(),
            last_applied_store: GenericLogStore::new(backend, GroupId::System)?,
        })
    }
}

impl RaftStateMachine for SystemStateMachine {
    type Command = SystemCommand;
    type Response = SystemResponse;
    
    async fn apply(&mut self, log_id: LogId, cmd: SystemCommand) -> Result<SystemResponse> {
        // Idempotency check
        if self.already_applied(&log_id).await? {
            return Ok(SystemResponse::Ok);
        }
        
        let response = match cmd {
            // Namespace operations - delegate to NamespacesTableProvider
            SystemCommand::CreateNamespace { namespace } => {
                self.namespaces.create_namespace_async(namespace.clone()).await
                    .map_err(|e| Error::Apply(e.to_string()))?;
                SystemResponse::NamespaceCreated { namespace_id: namespace.namespace_id }
            }
            
            SystemCommand::DropNamespace { namespace_id } => {
                self.namespaces.delete_namespace_async(&namespace_id).await
                    .map_err(|e| Error::Apply(e.to_string()))?;
                SystemResponse::Ok
            }
            
            // Table operations - delegate to TablesTableProvider
            SystemCommand::CreateTable { table } => {
                let table_id = TableId::new(
                    table.namespace_id.clone(),
                    table.table_name.clone(),
                );
                self.tables.create_table_async(&table_id, &table).await
                    .map_err(|e| Error::Apply(e.to_string()))?;
                SystemResponse::TableCreated { table_id }
            }
            
            SystemCommand::AlterTable { table_id, table_def } => {
                self.tables.update_table_async(&table_id, &table_def).await
                    .map_err(|e| Error::Apply(e.to_string()))?;
                SystemResponse::Ok
            }
            
            SystemCommand::DropTable { table_id } => {
                self.tables.delete_table_async(&table_id).await
                    .map_err(|e| Error::Apply(e.to_string()))?;
                SystemResponse::Ok
            }
            
            // Storage operations - delegate to StoragesTableProvider
            SystemCommand::CreateStorage { storage } => {
                self.storages.create_storage_async(storage).await
                    .map_err(|e| Error::Apply(e.to_string()))?;
                SystemResponse::Ok
            }
            
            SystemCommand::UpdateStorage { storage } => {
                self.storages.update_storage_async(storage).await
                    .map_err(|e| Error::Apply(e.to_string()))?;
                SystemResponse::Ok
            }
            
            SystemCommand::DeleteStorage { storage_id } => {
                self.storages.delete_storage_async(&storage_id).await
                    .map_err(|e| Error::Apply(e.to_string()))?;
                SystemResponse::Ok
            }
        };
        
        // Record last applied
        self.record_applied(log_id).await?;
        
        Ok(response)
    }
    
    async fn build_snapshot(&self) -> Result<SnapshotData> {
        // Snapshot = scan all data from providers
        let namespaces = self.namespaces.scan_all_async().await?;
        let tables = self.tables.list_tables_async().await?;
        let storages = self.storages.scan_all_async().await?;
        
        let snapshot = SystemSnapshot {
            namespaces,
            tables,
            storages,
            last_applied: self.last_applied_store.read_last_applied().await?,
        };
        
        Ok(bincode::serialize(&snapshot)?.into())
    }
    
    async fn restore_snapshot(&mut self, snapshot: SnapshotData) -> Result<()> {
        let data: SystemSnapshot = bincode::deserialize(&snapshot)?;
        
        // Clear and restore via providers
        // Note: This requires adding clear_all() methods to providers
        // OR using a transaction-like approach
        
        for ns in data.namespaces {
            // Upsert pattern
            let _ = self.namespaces.delete_namespace_async(&ns.namespace_id).await;
            self.namespaces.create_namespace_async(ns).await?;
        }
        
        for table in data.tables {
            let table_id = TableId::new(table.namespace_id.clone(), table.table_name.clone());
            let _ = self.tables.delete_table_async(&table_id).await;
            self.tables.create_table_async(&table_id, &table).await?;
        }
        
        for storage in data.storages {
            let _ = self.storages.delete_storage_async(&storage.storage_id).await;
            self.storages.create_storage_async(storage).await?;
        }
        
        self.last_applied_store.save_last_applied(data.last_applied).await?;
        
        Ok(())
    }
}
```

### 8.1 Users State Machine (Delegates to UsersTableProvider)

```rust
use kalamdb_system::UsersTableProvider;

pub struct UsersStateMachine {
    users: Arc<UsersTableProvider>,
    last_applied_store: GenericLogStore<dyn StorageBackend>,
}

impl UsersStateMachine {
    pub fn new(
        registry: Arc<SystemTablesRegistry>,
        backend: Arc<dyn StorageBackend>,
    ) -> Result<Self> {
        Ok(Self {
            users: registry.users(),
            last_applied_store: GenericLogStore::new(backend, GroupId::Users)?,
        })
    }
}

impl RaftStateMachine for UsersStateMachine {
    type Command = UsersCommand;
    type Response = UsersResponse;
    
    async fn apply(&mut self, log_id: LogId, cmd: UsersCommand) -> Result<UsersResponse> {
        if self.already_applied(&log_id).await? {
            return Ok(UsersResponse::Ok);
        }
        
        let response = match cmd {
            UsersCommand::CreateUser { user } => {
                let user_id = user.user_id.clone();
                self.users.create_user_async(user).await
                    .map_err(|e| Error::Apply(e.to_string()))?;
                UsersResponse::UserCreated { user_id }
            }
            
            UsersCommand::UpdateUser { user_id, changes } => {
                if let Some(mut user) = self.users.get_user_async(&user_id).await? {
                    user.apply_changes(changes);
                    self.users.update_user_async(user).await
                        .map_err(|e| Error::Apply(e.to_string()))?;
                }
                UsersResponse::Ok
            }
            
            UsersCommand::DeleteUser { user_id } => {
                self.users.delete_user_async(&user_id).await
                    .map_err(|e| Error::Apply(e.to_string()))?;
                UsersResponse::Ok
            }
            
            // ... other commands
        };
        
        self.record_applied(log_id).await?;
        Ok(response)
    }
    
    // ... snapshot methods similar to SystemStateMachine
}
```

### 8.2 Jobs State Machine (Delegates to JobsTableProvider)

```rust
use kalamdb_system::JobsTableProvider;

pub struct JobsStateMachine {
    jobs: Arc<JobsTableProvider>,
    last_applied_store: GenericLogStore<dyn StorageBackend>,
}

impl JobsStateMachine {
    pub fn new(
        registry: Arc<SystemTablesRegistry>,
        backend: Arc<dyn StorageBackend>,
    ) -> Result<Self> {
        Ok(Self {
            jobs: registry.jobs(),
            last_applied_store: GenericLogStore::new(backend, GroupId::Jobs)?,
        })
    }
}

impl RaftStateMachine for JobsStateMachine {
    type Command = JobsCommand;
    type Response = JobsResponse;
    
    async fn apply(&mut self, log_id: LogId, cmd: JobsCommand) -> Result<JobsResponse> {
        if self.already_applied(&log_id).await? {
            return Ok(JobsResponse::Ok);
        }
        
        let response = match cmd {
            JobsCommand::CreateJob { job } => {
                let job_id = job.job_id.clone();
                self.jobs.create_job_async(job).await
                    .map_err(|e| Error::Apply(e.to_string()))?;
                JobsResponse::JobCreated { job_id }
            }
            
            JobsCommand::UpdateJobStatus { job_id, status, updated_at } => {
                self.jobs.update_job_status_async(&job_id, status, updated_at).await
                    .map_err(|e| Error::Apply(e.to_string()))?;
                JobsResponse::Ok
            }
            
            JobsCommand::CompleteJob { job_id, result } => {
                self.jobs.complete_job_async(&job_id, result).await
                    .map_err(|e| Error::Apply(e.to_string()))?;
                JobsResponse::Ok
            }
            
            JobsCommand::CancelJob { job_id, reason } => {
                self.jobs.cancel_job_async(&job_id, &reason).await
                    .map_err(|e| Error::Apply(e.to_string()))?;
                JobsResponse::Ok
            }
            
            // ... other commands
        };
        
        self.record_applied(log_id).await?;
        Ok(response)
    }
}
```

### 8.3 Common Trait for All State Machines

All three state machines share significant code. We define a common trait to reduce duplication:

```rust
use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};

/// Common trait for all Raft state machines in KalamDB
/// 
/// This trait abstracts the common patterns:
/// - Idempotency tracking (last_applied)
/// - Snapshot build/restore
/// - Error mapping
/// 
/// Each state machine only needs to implement the command-specific logic.
#[async_trait]
pub trait KalamStateMachine: Send + Sync + 'static {
    /// The command type for this state machine
    type Command: Serialize + DeserializeOwned + Send + Sync + Clone;
    
    /// The response type for this state machine  
    type Response: Serialize + DeserializeOwned + Send + Sync + Default;
    
    /// The snapshot data type
    type Snapshot: Serialize + DeserializeOwned + Send + Sync;
    
    /// Get the group ID for this state machine
    fn group_id(&self) -> GroupId;
    
    /// Apply a single command (called after idempotency check)
    async fn apply_command(&mut self, cmd: Self::Command) -> Result<Self::Response>;
    
    /// Build a snapshot of current state
    async fn build_snapshot_data(&self) -> Result<Self::Snapshot>;
    
    /// Restore state from a snapshot
    async fn restore_from_snapshot(&mut self, snapshot: Self::Snapshot) -> Result<()>;
    
    /// Get the last applied log ID storage
    fn last_applied_store(&self) -> &GenericLogStore;
    fn last_applied_store_mut(&mut self) -> &mut GenericLogStore;
}

/// Blanket implementation that provides idempotency and snapshot wrappers
#[async_trait]
impl<T: KalamStateMachine> RaftStateMachine for T {
    type Command = T::Command;
    type Response = T::Response;
    
    async fn apply(&mut self, log_id: LogId, cmd: Self::Command) -> Result<Self::Response> {
        // Idempotency check (common to all state machines)
        if let Some(last) = self.last_applied_store().read_last_applied().await? {
            if log_id <= last {
                return Ok(Self::Response::default());
            }
        }
        
        // Apply the command (state machine specific)
        let response = self.apply_command(cmd).await?;
        
        // Record last applied (common to all state machines)
        self.last_applied_store_mut().save_last_applied(log_id).await?;
        
        Ok(response)
    }
    
    async fn build_snapshot(&self) -> Result<SnapshotData> {
        let snapshot = self.build_snapshot_data().await?;
        Ok(bincode::serialize(&snapshot)?.into())
    }
    
    async fn restore_snapshot(&mut self, data: SnapshotData) -> Result<()> {
        let snapshot: T::Snapshot = bincode::deserialize(&data)?;
        self.restore_from_snapshot(snapshot).await
    }
}
```

### 8.4 Simplified State Machine Implementations

With the common trait, each state machine becomes much simpler:

```rust
/// System state machine - only implements command-specific logic
pub struct SystemStateMachine {
    namespaces: Arc<NamespacesTableProvider>,
    tables: Arc<TablesTableProvider>,
    storages: Arc<StoragesTableProvider>,
    log_store: GenericLogStore,
}

#[async_trait]
impl KalamStateMachine for SystemStateMachine {
    type Command = SystemCommand;
    type Response = SystemResponse;
    type Snapshot = SystemSnapshot;
    
    fn group_id(&self) -> GroupId { GroupId::System }
    fn last_applied_store(&self) -> &GenericLogStore { &self.log_store }
    fn last_applied_store_mut(&mut self) -> &mut GenericLogStore { &mut self.log_store }
    
    async fn apply_command(&mut self, cmd: SystemCommand) -> Result<SystemResponse> {
        // Only command-specific logic here - no idempotency boilerplate!
        match cmd {
            SystemCommand::CreateNamespace { namespace } => {
                self.namespaces.create_namespace_async(namespace.clone()).await?;
                Ok(SystemResponse::NamespaceCreated { namespace_id: namespace.namespace_id })
            }
            SystemCommand::CreateTable { table } => {
                let table_id = TableId::new(table.namespace_id.clone(), table.table_name.clone());
                self.tables.create_table_async(&table_id, &table).await?;
                Ok(SystemResponse::TableCreated { table_id })
            }
            // ... other commands ...
        }
    }
    
    async fn build_snapshot_data(&self) -> Result<SystemSnapshot> {
        Ok(SystemSnapshot {
            namespaces: self.namespaces.scan_all_async().await?,
            tables: self.tables.list_tables_async().await?,
            storages: self.storages.scan_all_async().await?,
        })
    }
    
    async fn restore_from_snapshot(&mut self, snapshot: SystemSnapshot) -> Result<()> {
        // Clear existing data first
        self.namespaces.clear_all_async().await?;
        self.tables.clear_all_async().await?;
        self.storages.clear_all_async().await?;
        
        // Restore from snapshot
        for ns in snapshot.namespaces {
            self.namespaces.create_namespace_async(ns).await?;
        }
        for table in snapshot.tables {
            let table_id = TableId::new(table.namespace_id.clone(), table.table_name.clone());
            self.tables.create_table_async(&table_id, &table).await?;
        }
        for storage in snapshot.storages {
            self.storages.create_storage_async(storage).await?;
        }
        Ok(())
    }
}

/// Users state machine - same simplified pattern
pub struct UsersStateMachine {
    users: Arc<UsersTableProvider>,
    log_store: GenericLogStore,
}

#[async_trait]
impl KalamStateMachine for UsersStateMachine {
    type Command = UsersCommand;
    type Response = UsersResponse;
    type Snapshot = UsersSnapshot;
    
    fn group_id(&self) -> GroupId { GroupId::Users }
    fn last_applied_store(&self) -> &GenericLogStore { &self.log_store }
    fn last_applied_store_mut(&mut self) -> &mut GenericLogStore { &mut self.log_store }
    
    async fn apply_command(&mut self, cmd: UsersCommand) -> Result<UsersResponse> {
        match cmd {
            UsersCommand::CreateUser { user } => {
                let user_id = user.user_id.clone();
                self.users.create_user_async(user).await?;
                Ok(UsersResponse::UserCreated { user_id })
            }
            // ... other commands ...
        }
    }
    
    async fn build_snapshot_data(&self) -> Result<UsersSnapshot> {
        Ok(UsersSnapshot {
            users: self.users.scan_all_async().await?,
        })
    }
    
    async fn restore_from_snapshot(&mut self, snapshot: UsersSnapshot) -> Result<()> {
        self.users.clear_all_async().await?;
        for user in snapshot.users {
            self.users.create_user_async(user).await?;
        }
        Ok(())
    }
}

/// Jobs state machine - same pattern
pub struct JobsStateMachine {
    jobs: Arc<JobsTableProvider>,
    log_store: GenericLogStore,
}

#[async_trait]
impl KalamStateMachine for JobsStateMachine {
    type Command = JobsCommand;
    type Response = JobsResponse;
    type Snapshot = JobsSnapshot;
    
    fn group_id(&self) -> GroupId { GroupId::Jobs }
    fn last_applied_store(&self) -> &GenericLogStore { &self.log_store }
    fn last_applied_store_mut(&mut self) -> &mut GenericLogStore { &mut self.log_store }
    
    async fn apply_command(&mut self, cmd: JobsCommand) -> Result<JobsResponse> {
        match cmd {
            JobsCommand::CreateJob { job } => {
                let job_id = job.job_id.clone();
                self.jobs.create_job_async(job).await?;
                Ok(JobsResponse::JobCreated { job_id })
            }
            // ... other commands ...
        }
    }
    
    async fn build_snapshot_data(&self) -> Result<JobsSnapshot> {
        Ok(JobsSnapshot {
            jobs: self.jobs.scan_all_async().await?,
        })
    }
    
    async fn restore_from_snapshot(&mut self, snapshot: JobsSnapshot) -> Result<()> {
        self.jobs.clear_all_async().await?;
        for job in snapshot.jobs {
            self.jobs.create_job_async(job).await?;
        }
        Ok(())
    }
}
```

### 8.5 Benefits of Common Trait

| Benefit | Description |
|---------|-------------|
| **Code Reuse** | ~60% less code per state machine |
| **Consistency** | Idempotency logic in one place |
| **Testing** | Test trait once, test commands individually |
| **New Groups** | Adding a 4th group requires minimal boilerplate |
| **Bug Fixes** | Fix idempotency bugs in one place |

### 8.6 Leader-Only Job Execution (Flushing, Compaction, etc.)

Background jobs like **flushing**, **compaction**, **retention cleanup**, and **backup** must run on exactly **one node at a time** to prevent data corruption or duplicate work. In a cluster, we leverage the Raft leader for this coordination.

#### The Problem

Without coordination, multiple nodes might:
- Flush the same table simultaneously → duplicate Parquet files
- Run retention cleanup concurrently → race conditions
- Create overlapping backups → wasted resources

#### The Solution: Leader-Only Execution

```rust
/// Job executor that only runs on the Raft leader
pub struct LeaderOnlyJobExecutor {
    raft_manager: Arc<RaftManager>,
    job_manager: Arc<UnifiedJobManager>,
}

impl LeaderOnlyJobExecutor {
    /// Check if this node should run background jobs
    pub async fn should_run_jobs(&self) -> bool {
        // Only the Jobs group leader runs background jobs
        self.raft_manager.is_leader(GroupId::MetaJobs).await
    }
    
    /// Main job execution loop - only executes if we're the leader
    pub async fn run_job_loop(&self) {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            
            // Skip if not leader
            if !self.should_run_jobs().await {
                continue;
            }
            
            // Process pending jobs (flushing, compaction, etc.)
            self.process_pending_jobs().await;
        }
    }
    
    async fn process_pending_jobs(&self) {
        // Get queued jobs from Jobs Raft group
        let jobs = self.job_manager.get_queued_jobs().await;
        
        for job in jobs {
            // Claim job via Raft (persists node_id in jobs table)
            let claim_cmd = JobsCommand::ClaimJob {
                job_id: job.job_id.clone(),
                node_id: self.raft_manager.node_id(),
                claimed_at: Utc::now(),
            };
            
            // This goes through Raft - all nodes see the claim
            self.raft_manager
                .propose(GroupId::MetaJobs, claim_cmd)
                .await
                .ok();
            
            // Execute the job
            self.execute_job(&job).await;
        }
    }
}
```

#### Jobs Table Schema (Already Exists)

The `system.jobs` table already tracks which node is running a job:

```sql
CREATE TABLE system.jobs (
    job_id TEXT PRIMARY KEY,
    job_type TEXT NOT NULL,
    status TEXT NOT NULL,           -- 'queued', 'running', 'completed', 'failed'
    node_id BIGINT,                 -- Which node is running this job (NULL if queued)
    namespace_id TEXT,
    table_name TEXT,
    created_at TIMESTAMP,
    started_at TIMESTAMP,
    completed_at TIMESTAMP,
    ...
);
```

#### Leader Failover Handling

When a leader fails mid-job:

1. **New leader elected** via Raft
2. New leader scans for jobs with `status = 'running'` and `node_id = <old_leader>`
3. Those jobs are marked as `'failed'` or re-queued based on job type
4. Retry logic kicks in for retryable jobs

```rust
impl LeaderOnlyJobExecutor {
    /// Called when this node becomes leader
    async fn on_become_leader(&self) {
        // Find orphaned jobs from previous leader
        let orphaned = self.job_manager
            .find_jobs_by_status(JobStatus::Running)
            .await;
        
        for job in orphaned {
            // Re-queue or fail based on job type
            if job.is_idempotent() {
                self.job_manager.requeue_job(&job.job_id).await;
            } else {
                self.job_manager.fail_job(
                    &job.job_id,
                    "Previous leader failed during execution"
                ).await;
            }
        }
    }
}
```

#### Extended JobsCommand for Claiming

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JobsCommand {
    // ... existing commands ...
    
    /// Claim a job for execution (leader-only)
    ClaimJob {
        job_id: JobId,
        node_id: u64,
        claimed_at: DateTime<Utc>,
    },
    
    /// Release a claimed job (on failure or leader change)
    ReleaseJob {
        job_id: JobId,
        reason: String,
    },
}
```

#### Summary: Job Coordination

| Aspect | Mechanism |
|--------|----------|
| **Who runs jobs** | Only the Jobs group leader |
| **Job claiming** | Via Raft proposal (all nodes see claim) |
| **Tracking** | `node_id` column in `system.jobs` |
| **Failover** | New leader re-queues orphaned jobs |
| **Idempotency** | Job IDs prevent duplicate execution |

---

## 9. Node Catchup and Late Joiner Handling

### 9.1 How Raft Handles Late Joiners

OpenRaft (and Raft in general) has built-in mechanisms for node catchup:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Late Joiner Catchup Flow                             │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  NEW NODE JOINS                                                              │
│  ══════════════                                                              │
│                                                                              │
│  1. Node starts with empty state                                             │
│  2. Contacts cluster via configured seed members                             │
│  3. Leader adds node as "learner" (non-voting member)                        │
│  4. Leader sends log entries OR snapshot (depending on log availability)     │
│                                                                              │
│  ┌──────────────────────────────────────────────────────────────────┐       │
│  │ If leader has all required log entries:                          │       │
│  │                                                                   │       │
│  │   Leader Log: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]                    │       │
│  │   New Node:   []                                                  │       │
│  │                                                                   │       │
│  │   → Leader sends entries 1-10 via AppendEntries                   │       │
│  │   → New node applies each entry to state machine                  │       │
│  │   → New node catches up to index 10                               │       │
│  └──────────────────────────────────────────────────────────────────┘       │
│                                                                              │
│  ┌──────────────────────────────────────────────────────────────────┐       │
│  │ If leader has compacted log (entries 1-5 deleted):               │       │
│  │                                                                   │       │
│  │   Leader Log: [snapshot@5, 6, 7, 8, 9, 10]                       │       │
│  │   New Node:   []                                                  │       │
│  │                                                                   │       │
│  │   → Leader sends snapshot (state at index 5) via InstallSnapshot  │       │
│  │   → New node restores snapshot to state machine                   │       │
│  │   → Leader sends entries 6-10 via AppendEntries                   │       │
│  │   → New node catches up to index 10                               │       │
│  └──────────────────────────────────────────────────────────────────┘       │
│                                                                              │
│  5. Once caught up, leader promotes learner to voter                         │
│  6. Node participates in elections and commits                               │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 9.2 Node Was Down for Days - Resync Logic

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                     Stale Node Resync Flow                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  SCENARIO: Node 2 was down for 3 days                                        │
│  ════════════════════════════════════════                                    │
│                                                                              │
│  Before downtime:                                                            │
│    Node 1 (leader):  Log [1..100], Commit: 100                              │
│    Node 2:           Log [1..100], Commit: 100                              │
│    Node 3:           Log [1..100], Commit: 100                              │
│                                                                              │
│  After 3 days:                                                               │
│    Node 1 (leader):  Log [snapshot@500, 501..1000], Commit: 1000            │
│    Node 2 (offline): Log [1..100], Commit: 100 (stale)                      │
│    Node 3:           Log [snapshot@500, 501..1000], Commit: 1000            │
│                                                                              │
│  When Node 2 comes back online:                                              │
│  ════════════════════════════════                                            │
│                                                                              │
│  1. Node 2 sends vote request with last_log_index=100                        │
│  2. Leader rejects (stale term/index)                                        │
│  3. Leader detects Node 2 is behind                                          │
│  4. Leader checks: "Do I have entries 101-1000?"                            │
│                                                                              │
│     ┌─────────────────────────────────────────────────────────────┐         │
│     │ Case A: Log compacted (entries 101-500 deleted)             │         │
│     │                                                              │         │
│     │   → Leader sends snapshot@500 via InstallSnapshot RPC       │         │
│     │   → Node 2 calls state_machine.restore_snapshot()           │         │
│     │   → Node 2 state now matches snapshot (index 500)           │         │
│     │   → Leader sends entries 501-1000 via AppendEntries         │         │
│     │   → Node 2 applies entries 501-1000                         │         │
│     │   → Node 2 fully caught up                                  │         │
│     └─────────────────────────────────────────────────────────────┘         │
│                                                                              │
│     ┌─────────────────────────────────────────────────────────────┐         │
│     │ Case B: Log not compacted (all entries available)           │         │
│     │                                                              │         │
│     │   → Leader sends entries 101-1000 via AppendEntries         │         │
│     │   → Node 2 applies entries in order                         │         │
│     │   → Node 2 fully caught up                                  │         │
│     └─────────────────────────────────────────────────────────────┘         │
│                                                                              │
│  5. Node 2 resumes normal operation as follower                              │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 9.3 Implementation: Snapshot Building and Restore

The snapshot mechanism is CRITICAL for node catchup. Here's the detailed flow:

```rust
/// Snapshot metadata stored alongside the snapshot data
#[derive(Serialize, Deserialize, Clone)]
pub struct SnapshotMeta {
    /// The log index this snapshot represents
    pub last_included_index: u64,
    
    /// The log term at last_included_index  
    pub last_included_term: u64,
    
    /// Cluster membership at snapshot time
    pub membership: ClusterMembership,
    
    /// Timestamp when snapshot was created
    pub created_at: DateTime<Utc>,
    
    /// Checksum for integrity verification
    pub checksum: u64,
}

/// Complete snapshot including metadata and data for all groups
#[derive(Serialize, Deserialize)]
pub struct GroupSnapshot<T> {
    pub meta: SnapshotMeta,
    pub data: T,
}

impl<T: KalamStateMachine> T {
    /// Build snapshot - called by Raft when log grows too large
    async fn trigger_snapshot(&mut self) -> Result<GroupSnapshot<T::Snapshot>> {
        // 1. Get current applied index
        let last_applied = self.last_applied_store()
            .read_last_applied()
            .await?
            .ok_or(Error::NoAppliedEntries)?;
        
        // 2. Build snapshot data from state machine
        let data = self.build_snapshot_data().await?;
        let serialized = bincode::serialize(&data)?;
        
        // 3. Create metadata
        let meta = SnapshotMeta {
            last_included_index: last_applied.index,
            last_included_term: last_applied.term,
            membership: self.current_membership()?,
            created_at: Utc::now(),
            checksum: xxhash(&serialized),
        };
        
        // 4. Persist snapshot to storage
        self.persist_snapshot(&meta, &serialized).await?;
        
        // 5. Compact log entries before snapshot index
        self.log_store.compact_before(last_applied.index).await?;
        
        Ok(GroupSnapshot { meta, data })
    }
    
    /// Restore snapshot - called when receiving from leader
    async fn install_snapshot(&mut self, snapshot: GroupSnapshot<T::Snapshot>) -> Result<()> {
        // 1. Verify checksum
        let serialized = bincode::serialize(&snapshot.data)?;
        if xxhash(&serialized) != snapshot.meta.checksum {
            return Err(Error::SnapshotCorrupted);
        }
        
        // 2. Clear existing state and restore
        self.restore_from_snapshot(snapshot.data).await?;
        
        // 3. Update last applied to snapshot index
        let log_id = LogId {
            index: snapshot.meta.last_included_index,
            term: snapshot.meta.last_included_term,
        };
        self.last_applied_store_mut().save_last_applied(log_id).await?;
        
        // 4. Clear log entries before snapshot (they're now redundant)
        self.log_store.truncate_before(snapshot.meta.last_included_index).await?;
        
        Ok(())
    }
}
```

### 9.4 Snapshot Storage Layout

```
data/raft/
├── system/
│   ├── log/                    # Raft log entries (RocksDB partition)
│   ├── vote/                   # Vote state (RocksDB partition)
│   ├── snapshots/
│   │   ├── snapshot_500.meta   # Metadata for snapshot at index 500
│   │   └── snapshot_500.data   # Serialized state at index 500
│   └── state/                  # Last applied tracking
├── users/
│   ├── log/
│   ├── vote/
│   ├── snapshots/
│   │   └── ...
│   └── state/
└── jobs/
    ├── log/
    ├── vote/
    ├── snapshots/
    │   └── ...
    └── state/
```

### 9.5 Snapshot Transfer Protocol (gRPC Streaming)

For large snapshots, we use streaming to avoid memory issues:

```protobuf
// In proto/raft.proto

message InstallSnapshotRequest {
    string group_id = 1;
    uint64 term = 2;
    uint64 leader_id = 3;
    
    // Snapshot metadata
    uint64 last_included_index = 4;
    uint64 last_included_term = 5;
    
    // Chunked transfer
    uint64 offset = 6;          // Byte offset in snapshot
    bytes data = 7;             // Chunk data (64KB chunks)
    bool done = 8;              // Is this the last chunk?
}

message InstallSnapshotResponse {
    uint64 term = 1;
    bool success = 2;
    uint64 bytes_received = 3;  // For progress tracking
}

service RaftService {
    // Streaming snapshot install
    rpc InstallSnapshot(stream InstallSnapshotRequest) returns (InstallSnapshotResponse);
}
```

---

## 10. RaftManager: Orchestrating All 36 Groups

```rust
use kalamdb_system::SystemTablesRegistry;

/// Central manager for all Raft groups (36 total)
/// 
/// - 3 metadata groups: meta:system, meta:users, meta:jobs
/// - 32 user data shards: data:user:0 .. data:user:31
/// - 1 shared data shard: data:shared:0 (extensible)
pub struct RaftManager {
    config: RaftConfig,
    backend: Arc<dyn StorageBackend>,
    
    /// All Raft group instances, indexed by GroupId
    groups: HashMap<GroupId, RaftGroup>,
    
    /// Shared router for network
    router: Arc<RwLock<RaftRouter>>,
    
    /// Shard routing logic
    shard_router: Arc<ShardRouter>,
    
    /// gRPC server handle
    grpc_server: Option<JoinHandle<()>>,
}

/// A single Raft group with its state machine
struct RaftGroup {
    raft: Arc<Raft<TypeConfig>>,
    group_id: GroupId,
}

impl RaftManager {
    pub async fn new(
        config: RaftConfig,
        backend: Arc<dyn StorageBackend>,
        system_tables: Arc<SystemTablesRegistry>,
        user_stores: Arc<UserTableStores>,
        shared_stores: Arc<SharedTableStores>,
    ) -> Result<Self> {
        let router = Arc::new(RwLock::new(RaftRouter::new()));
        let network_factory = GrpcNetworkFactory::new(config.clone(), router.clone());
        
        let raft_config = Arc::new(Config {
            cluster_name: config.cluster_id.clone(),
            heartbeat_interval: config.heartbeat_interval_ms,
            election_timeout_min: config.election_timeout_min_ms,
            election_timeout_max: config.election_timeout_max_ms,
            ..Default::default()
        });
        
        let mut groups = HashMap::new();
        
        // ========== Create Metadata Groups (3) ==========
        
        // meta:system
        let system_sm = SystemStateMachine::new(system_tables.clone(), backend.clone())?;
        let system_raft = Self::create_raft_instance(
            &config, &raft_config, &network_factory, &backend,
            GroupId::MetaSystem, system_sm,
        ).await?;
        groups.insert(GroupId::MetaSystem, RaftGroup { 
            raft: system_raft.clone(), 
            group_id: GroupId::MetaSystem 
        });
        
        // meta:users
        let users_sm = UsersStateMachine::new(system_tables.clone(), backend.clone())?;
        let users_raft = Self::create_raft_instance(
            &config, &raft_config, &network_factory, &backend,
            GroupId::MetaUsers, users_sm,
        ).await?;
        groups.insert(GroupId::MetaUsers, RaftGroup { 
            raft: users_raft.clone(), 
            group_id: GroupId::MetaUsers 
        });
        
        // meta:jobs
        let jobs_sm = JobsStateMachine::new(system_tables.clone(), backend.clone())?;
        let jobs_raft = Self::create_raft_instance(
            &config, &raft_config, &network_factory, &backend,
            GroupId::MetaJobs, jobs_sm,
        ).await?;
        groups.insert(GroupId::MetaJobs, RaftGroup { 
            raft: jobs_raft.clone(), 
            group_id: GroupId::MetaJobs 
        });
        
        // ========== Create User Data Shards (32) ==========
        
        for shard_id in 0..config.sharding.num_user_shards {
            let group_id = GroupId::DataUserShard(shard_id);
            let sm = UserDataStateMachine::new(shard_id, user_stores.clone(), backend.clone())?;
            let raft = Self::create_raft_instance(
                &config, &raft_config, &network_factory, &backend,
                group_id.clone(), sm,
            ).await?;
            groups.insert(group_id.clone(), RaftGroup { raft, group_id });
        }
        
        // ========== Create Shared Data Shards (1 for now) ==========
        
        for shard_id in 0..config.sharding.num_shared_shards {
            let group_id = GroupId::DataSharedShard(shard_id);
            let sm = SharedDataStateMachine::new(shard_id, shared_stores.clone(), backend.clone())?;
            let raft = Self::create_raft_instance(
                &config, &raft_config, &network_factory, &backend,
                group_id.clone(), sm,
            ).await?;
            groups.insert(group_id.clone(), RaftGroup { raft, group_id });
        }
        
        // Register all groups with router
        {
            let mut router_mut = router.write().await;
            for (group_id, group) in &groups {
                router_mut.register(group_id.clone(), group.raft.clone());
            }
        }
        
        info!("Initialized {} Raft groups", groups.len());
        
        Ok(Self {
            config,
            backend,
            groups,
            router,
            shard_router: Arc::new(ShardRouter::new(&config.sharding)),
            grpc_server: None,
        })
    }
    
    /// Generic propose - works for any group
    pub async fn propose<C, R>(&self, group_id: GroupId, cmd: C) -> Result<R>
    where
        C: Serialize + Send + Sync,
        R: DeserializeOwned,
    {
        let group = self.groups.get(&group_id)
            .ok_or_else(|| Error::UnknownGroup(group_id.as_str()))?;
        
        let response = group.raft.client_write(cmd).await?;
        Ok(response.data)
    }
    
    /// Get shard router for determining which group to use
    pub fn shard_router(&self) -> &ShardRouter {
        &self.shard_router
    }
    
    /// Check if this node is leader for a group
    pub fn is_leader(&self, group_id: &GroupId) -> bool {
        self.groups.get(group_id)
            .map(|g| g.raft.current_leader() == Some(self.config.node_id))
            .unwrap_or(false)
    }
    
    /// Get leader node ID for a group
    pub fn get_leader(&self, group_id: &GroupId) -> Option<u64> {
        self.groups.get(group_id)
            .and_then(|g| g.raft.current_leader())
    }
    
    /// Get stats for all groups
    pub fn stats(&self) -> Vec<GroupStats> {
        self.groups.iter().map(|(id, g)| GroupStats {
            group_id: id.clone(),
            is_leader: self.is_leader(id),
            leader_id: g.raft.current_leader(),
            commit_index: g.raft.metrics().borrow().last_log_index,
        }).collect()
    }
    
    async fn create_raft_instance<SM>(
        config: &RaftConfig,
        raft_config: &Arc<Config>,
        network_factory: &GrpcNetworkFactory,
        backend: &Arc<dyn StorageBackend>,
        group_id: GroupId,
        state_machine: SM,
    ) -> Result<Arc<Raft<TypeConfig>>>
    where
        SM: RaftStateMachine + 'static,
    {
        let log_store = GenericLogStore::new(backend.clone(), group_id.clone())?;
        
        let raft = Raft::new(
            config.node_id,
            raft_config.clone(),
            network_factory.clone(),
            log_store,
            state_machine,
        ).await?;
        
        Ok(Arc::new(raft))
    }
    
    pub async fn start_server(&mut self) -> Result<()> {
        let addr = self.config.raft_bind_addr.parse()?;
        let router = self.router.clone();
        
        let server = Server::builder()
            .add_service(RaftServiceServer::new(RaftServiceImpl::new(router.clone())))
            .add_service(ClusterServiceServer::new(ClusterServiceImpl::new(router)))
            .serve(addr);
        
        self.grpc_server = Some(tokio::spawn(async move {
            if let Err(e) = server.await {
                error!("Raft gRPC server error: {}", e);
            }
        }));
        
        info!("Raft gRPC server started on {} with {} groups", 
              self.config.raft_bind_addr, self.groups.len());
        Ok(())
    }
}
```

---

## 11. Single-Node Mode (Zero Configuration)

**Key Design Goal**: Eliminate all `if cluster_mode { ... } else { ... }` scattered throughout the codebase. Use a single trait-based abstraction.

### 10.1 The CommandExecutor Trait

```rust
use async_trait::async_trait;

/// Unified command execution interface
/// 
/// Both standalone and cluster mode implement this trait.
/// Callers never need to check which mode they're in.
#[async_trait]
pub trait CommandExecutor: Send + Sync + 'static {
    // ========== Metadata Commands ==========
    
    /// Execute a system catalog command (DDL)
    async fn execute_system(&self, cmd: SystemCommand) -> Result<SystemResponse>;
    
    /// Execute a user/auth command
    async fn execute_users(&self, cmd: UsersCommand) -> Result<UsersResponse>;
    
    /// Execute a job command
    async fn execute_jobs(&self, cmd: JobsCommand) -> Result<JobsResponse>;
    
    // ========== Data Commands ==========
    
    /// Execute a user table data command (routed by user_id to correct shard)
    async fn execute_user_data(&self, user_id: &UserId, cmd: UserDataCommand) -> Result<DataResponse>;
    
    /// Execute a shared table data command (routed to shared shard)
    async fn execute_shared_data(&self, namespace_id: &NamespaceId, cmd: SharedDataCommand) -> Result<DataResponse>;
}
```

### 10.2 Standalone Executor (Direct Apply)

```rust
/// Standalone mode: commands go directly to providers
/// 
/// No Raft, no replication, no network overhead.
/// This is the current behavior.
pub struct DirectExecutor {
    system_tables: Arc<SystemTablesRegistry>,
    // For data operations
    user_table_stores: Arc<UserTableStores>,
    shared_table_stores: Arc<SharedTableStores>,
}

#[async_trait]
impl CommandExecutor for DirectExecutor {
    async fn execute_system(&self, cmd: SystemCommand) -> Result<SystemResponse> {
        // Direct apply - no Raft
        match cmd {
            SystemCommand::CreateNamespace { namespace } => {
                self.system_tables.namespaces()
                    .create_namespace_async(namespace.clone()).await?;
                Ok(SystemResponse::NamespaceCreated { 
                    namespace_id: namespace.namespace_id 
                })
            }
            SystemCommand::CreateTable { table } => {
                let table_id = TableId::new(
                    table.namespace_id.clone(), 
                    table.table_name.clone()
                );
                self.system_tables.tables()
                    .create_table_async(&table_id, &table).await?;
                Ok(SystemResponse::TableCreated { table_id })
            }
            // ... other commands
        }
    }
    
    async fn execute_users(&self, cmd: UsersCommand) -> Result<UsersResponse> {
        match cmd {
            UsersCommand::CreateUser { user } => {
                let user_id = user.user_id.clone();
                self.system_tables.users()
                    .create_user_async(user).await?;
                Ok(UsersResponse::UserCreated { user_id })
            }
            // ... other commands
        }
    }
    
    async fn execute_jobs(&self, cmd: JobsCommand) -> Result<JobsResponse> {
        match cmd {
            JobsCommand::CreateJob { job } => {
                let job_id = job.job_id.clone();
                self.system_tables.jobs()
                    .create_job_async(job).await?;
                Ok(JobsResponse::JobCreated { job_id })
            }
            // ... other commands
        }
    }
    
    async fn execute_user_data(
        &self, 
        user_id: &UserId, 
        cmd: UserDataCommand
    ) -> Result<DataResponse> {
        // Direct apply to user table store
        self.user_table_stores.apply(user_id, cmd).await
    }
    
    async fn execute_shared_data(
        &self, 
        namespace_id: &NamespaceId, 
        cmd: SharedDataCommand
    ) -> Result<DataResponse> {
        // Direct apply to shared table store
        self.shared_table_stores.apply(namespace_id, cmd).await
    }
}
```

### 10.3 Raft Executor (Replicated Apply)

```rust
/// Cluster mode: commands go through Raft for replication
/// 
/// All writes are replicated to majority before returning.
pub struct RaftExecutor {
    raft_manager: Arc<RaftManager>,
    shard_router: Arc<ShardRouter>,
}

#[async_trait]
impl CommandExecutor for RaftExecutor {
    async fn execute_system(&self, cmd: SystemCommand) -> Result<SystemResponse> {
        // Propose to meta:system Raft group
        self.raft_manager.propose(GroupId::MetaSystem, cmd).await
    }
    
    async fn execute_users(&self, cmd: UsersCommand) -> Result<UsersResponse> {
        // Propose to meta:users Raft group
        self.raft_manager.propose(GroupId::MetaUsers, cmd).await
    }
    
    async fn execute_jobs(&self, cmd: JobsCommand) -> Result<JobsResponse> {
        // Propose to meta:jobs Raft group
        self.raft_manager.propose(GroupId::MetaJobs, cmd).await
    }
    
    async fn execute_user_data(
        &self, 
        user_id: &UserId, 
        cmd: UserDataCommand
    ) -> Result<DataResponse> {
        // Route to correct user shard based on user_id hash
        let group = self.shard_router.user_shard(user_id);
        self.raft_manager.propose(group, cmd).await
    }
    
    async fn execute_shared_data(
        &self, 
        namespace_id: &NamespaceId, 
        cmd: SharedDataCommand
    ) -> Result<DataResponse> {
        // Route to shared shard (single shard for now)
        let group = self.shard_router.shared_shard(namespace_id);
        self.raft_manager.propose(group, cmd).await
    }
}
```

### 10.4 AppContext with CommandExecutor

```rust
pub struct AppContext {
    /// Storage backend
    backend: Arc<dyn StorageBackend>,
    
    /// System tables registry (providers)
    system_tables: Arc<SystemTablesRegistry>,
    
    /// Schema registry (cache)
    schema_registry: Arc<SchemaRegistry>,
    
    /// **THE KEY ABSTRACTION**: Unified command executor
    /// - Standalone mode: DirectExecutor
    /// - Cluster mode: RaftExecutor
    executor: Arc<dyn CommandExecutor>,
}

impl AppContext {
    /// Create AppContext - mode is determined by config
    pub async fn new(
        config: ServerConfig,
        backend: Arc<dyn StorageBackend>,
    ) -> Result<Arc<Self>> {
        let system_tables = Arc::new(SystemTablesRegistry::new(backend.clone())?);
        let schema_registry = Arc::new(SchemaRegistry::new(backend.clone())?);
        
        // Choose executor based on config - NO if/else anywhere else!
        let executor: Arc<dyn CommandExecutor> = match &config.cluster {
            None => {
                // Standalone mode
                Arc::new(DirectExecutor::new(
                    system_tables.clone(),
                    backend.clone(),
                ))
            }
            Some(cluster_config) => {
                // Cluster mode
                let raft_manager = RaftManager::new(
                    cluster_config.clone(),
                    backend.clone(),
                    system_tables.clone(),
                ).await?;
                Arc::new(RaftExecutor::new(
                    Arc::new(raft_manager),
                    Arc::new(ShardRouter::new(&cluster_config.sharding)),
                ))
            }
        };
        
        Ok(Arc::new(Self {
            backend,
            system_tables,
            schema_registry,
            executor,
        }))
    }
    
    /// Get the command executor - callers just use this
    pub fn executor(&self) -> &dyn CommandExecutor {
        self.executor.as_ref()
    }
}
```

### 10.5 Usage in Handlers (Clean, No If/Else)

```rust
// DDL Handler - same code works for standalone AND cluster
pub async fn handle_create_table(
    ctx: &AppContext,
    stmt: CreateTableStatement,
) -> Result<()> {
    let table_def = build_table_definition(&stmt)?;
    
    // Just call executor - no mode check needed!
    ctx.executor()
        .execute_system(SystemCommand::CreateTable { table: table_def })
        .await?;
    
    Ok(())
}

// DML Handler - same pattern
pub async fn handle_insert(
    ctx: &AppContext,
    user_id: &UserId,
    stmt: InsertStatement,
) -> Result<()> {
    let cmd = UserDataCommand::Insert { 
        table_id: stmt.table_id.clone(),
        rows: stmt.rows.clone(),
    };
    
    // Routes to correct shard automatically
    ctx.executor()
        .execute_user_data(user_id, cmd)
        .await?;
    
    Ok(())
}

// Auth Handler - same pattern
pub async fn handle_create_user(
    ctx: &AppContext,
    stmt: CreateUserStatement,
) -> Result<()> {
    let user = build_user(&stmt)?;
    
    ctx.executor()
        .execute_users(UsersCommand::CreateUser { user })
        .await?;
    
    Ok(())
}
```

### 10.6 Benefits of CommandExecutor Pattern

| Aspect | Before (if/else) | After (CommandExecutor) |
|--------|------------------|-------------------------|
| **Mode check** | Every handler | Once at startup |
| **Code duplication** | Command building duplicated | Single path |
| **Testing** | Mock both paths | Mock one trait |
| **New commands** | Add to both branches | Add to trait + impls |
| **Bugs** | Can differ between modes | Same interface |
| **Cognitive load** | "Am I in cluster mode?" | "Just call executor" |

---

## 12. Data Shard State Machines

### 12.1 UserDataStateMachine

User data shards handle data for **user tables** and **per-user system tables** like `system.live_queries`.

#### Why `system.live_queries` Lives in User Shards

- Each live query subscription belongs to a specific `user_id`
- The subscription tracks which node holds the WebSocket connection (`node_id` column)
- Routing by `user_id` keeps all of a user's subscriptions in the same shard
- On failover, the new leader can check which subscriptions belonged to the failed node

```sql
-- system.live_queries schema (user-scoped, in user shards)
CREATE TABLE system.live_queries (
    subscription_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,           -- Routes to shard via user_id % 32
    query_hash TEXT NOT NULL,
    table_id TEXT NOT NULL,
    filter_json TEXT,
    node_id BIGINT NOT NULL,         -- Which node holds the WebSocket connection
    created_at TIMESTAMP,
    last_ping_at TIMESTAMP,
);
```

#### Connection Failover

When a node fails, clients reconnect to another node. The new node:
1. Queries the user's shard for their active subscriptions
2. Cleans up stale subscriptions from the failed `node_id`
3. Re-registers new subscriptions under the new node

```rust
/// State machine for a single user data shard
/// 
/// Handles INSERT/UPDATE/DELETE for user tables where user_id hashes to this shard.
/// Also handles per-user system tables like system.live_queries.
pub struct UserDataStateMachine {
    shard_id: u32,  // 0..31
    
    /// Access to user table data stores
    user_stores: Arc<UserTableStores>,
    
    /// Access to live queries provider (per-user system table)
    live_queries: Arc<LiveQueriesProvider>,
    
    /// Log tracking
    log_store: GenericLogStore,
}

#[async_trait]
impl KalamStateMachine for UserDataStateMachine {
    type Command = UserDataCommand;
    type Response = DataResponse;
    type Snapshot = UserDataSnapshot;
    
    fn group_id(&self) -> GroupId { 
        GroupId::DataUserShard(self.shard_id) 
    }
    
    async fn apply_command(&mut self, cmd: UserDataCommand) -> Result<DataResponse> {
        match cmd {
            // === User Table Data ===
            UserDataCommand::Insert { table_id, user_id, rows } => {
                self.user_stores
                    .insert(&table_id, &user_id, rows)
                    .await?;
                Ok(DataResponse::RowsAffected(rows.len()))
            }
            
            UserDataCommand::Update { table_id, user_id, updates, filter } => {
                let count = self.user_stores
                    .update(&table_id, &user_id, updates, filter)
                    .await?;
                Ok(DataResponse::RowsAffected(count))
            }
            
            UserDataCommand::Delete { table_id, user_id, filter } => {
                let count = self.user_stores
                    .delete(&table_id, &user_id, filter)
                    .await?;
                Ok(DataResponse::RowsAffected(count))
            }
            
            // === Live Query Subscriptions ===
            UserDataCommand::RegisterLiveQuery { 
                subscription_id, user_id, query_hash, table_id, filter_json, node_id 
            } => {
                self.live_queries.register(
                    &subscription_id, &user_id, &query_hash, 
                    &table_id, filter_json.as_deref(), node_id
                ).await?;
                Ok(DataResponse::Ok)
            }
            
            UserDataCommand::UnregisterLiveQuery { subscription_id, user_id } => {
                self.live_queries.unregister(&subscription_id, &user_id).await?;
                Ok(DataResponse::Ok)
            }
            
            UserDataCommand::CleanupNodeSubscriptions { user_id, failed_node_id } => {
                let count = self.live_queries
                    .cleanup_by_node(&user_id, failed_node_id)
                    .await?;
                Ok(DataResponse::RowsAffected(count))
            }
            
            UserDataCommand::PingLiveQuery { subscription_id, user_id, pinged_at } => {
                self.live_queries
                    .update_ping(&subscription_id, &user_id, pinged_at)
                    .await?;
                Ok(DataResponse::Ok)
            }
        }
    }
    
    async fn build_snapshot_data(&self) -> Result<UserDataSnapshot> {
        // Snapshot all data for this shard (includes live_queries for this shard)
        self.user_stores.snapshot_shard(self.shard_id).await
    }
    
    async fn restore_from_snapshot(&mut self, snapshot: UserDataSnapshot) -> Result<()> {
        self.user_stores.restore_shard(self.shard_id, snapshot).await
    }
}
```

### 12.2 SharedDataStateMachine

```rust
/// State machine for shared table data shard
/// 
/// Handles INSERT/UPDATE/DELETE for shared tables.
/// Phase 1: Single shard. Future: Multiple shards.
pub struct SharedDataStateMachine {
    shard_id: u32,  // 0 for Phase 1
    
    /// Access to shared table data stores
    shared_stores: Arc<SharedTableStores>,
    
    /// Log tracking
    log_store: GenericLogStore,
}

#[async_trait]
impl KalamStateMachine for SharedDataStateMachine {
    type Command = SharedDataCommand;
    type Response = DataResponse;
    type Snapshot = SharedDataSnapshot;
    
    fn group_id(&self) -> GroupId { 
        GroupId::DataSharedShard(self.shard_id) 
    }
    
    async fn apply_command(&mut self, cmd: SharedDataCommand) -> Result<DataResponse> {
        match cmd {
            SharedDataCommand::Insert { table_id, rows } => {
                self.shared_stores.insert(&table_id, rows).await?;
                Ok(DataResponse::RowsAffected(rows.len()))
            }
            
            SharedDataCommand::Update { table_id, updates, filter } => {
                let count = self.shared_stores
                    .update(&table_id, updates, filter)
                    .await?;
                Ok(DataResponse::RowsAffected(count))
            }
            
            SharedDataCommand::Delete { table_id, filter } => {
                let count = self.shared_stores
                    .delete(&table_id, filter)
                    .await?;
                Ok(DataResponse::RowsAffected(count))
            }
        }
    }
    
    async fn build_snapshot_data(&self) -> Result<SharedDataSnapshot> {
        self.shared_stores.snapshot_shard(self.shard_id).await
    }
    
    async fn restore_from_snapshot(&mut self, snapshot: SharedDataSnapshot) -> Result<()> {
        self.shared_stores.restore_shard(self.shard_id, snapshot).await
    }
}
```

### 12.3 Data Commands

```rust
/// Commands for user table data operations
/// 
/// Includes both user table data AND per-user system tables like live_queries
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum UserDataCommand {
    // === User Table Data ===
    Insert {
        table_id: TableId,
        user_id: UserId,
        rows: Vec<Row>,
    },
    Update {
        table_id: TableId,
        user_id: UserId,
        updates: Vec<ColumnUpdate>,
        filter: Option<Filter>,
    },
    Delete {
        table_id: TableId,
        user_id: UserId,
        filter: Option<Filter>,
    },
    
    // === Live Query Subscriptions (per-user) ===
    RegisterLiveQuery {
        subscription_id: String,
        user_id: UserId,
        query_hash: String,
        table_id: TableId,
        filter_json: Option<String>,
        node_id: u64,  // Which node holds the WebSocket connection
    },
    UnregisterLiveQuery {
        subscription_id: String,
        user_id: UserId,
    },
    /// Called when a node fails - clean up its subscriptions
    CleanupNodeSubscriptions {
        user_id: UserId,
        failed_node_id: u64,
    },
    /// Heartbeat to keep subscription alive
    PingLiveQuery {
        subscription_id: String,
        user_id: UserId,
        pinged_at: DateTime<Utc>,
    },
}

/// Commands for shared table data operations
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SharedDataCommand {
    Insert {
        table_id: TableId,
        rows: Vec<Row>,
    },
    Update {
        table_id: TableId,
        updates: Vec<ColumnUpdate>,
        filter: Option<Filter>,
    },
    Delete {
        table_id: TableId,
        filter: Option<Filter>,
    },
}

/// Response for data operations
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub enum DataResponse {
    #[default]
    Ok,
    RowsAffected(usize),
    Error(String),
}
```

---

## 13. Minimal Changes to Existing Crates

**Critical Requirement**: Running KalamDB with a single node MUST work exactly as today, with NO extra configuration.

### 13.1 How It Works

```rust
/// Determines cluster mode based on configuration
pub enum ClusterMode {
    /// No [cluster] section in config = standalone mode
    Standalone,
    
    /// [cluster] section exists with 1 member = single-node Raft
    SingleNodeRaft,
    
    /// [cluster] section with 2+ members = multi-node cluster
    MultiNodeCluster,
}

impl ClusterMode {
    pub fn from_config(config: &ServerConfig) -> Self {
        match &config.cluster {
            None => ClusterMode::Standalone,
            Some(cluster) if cluster.members.len() <= 1 => ClusterMode::SingleNodeRaft,
            Some(_) => ClusterMode::MultiNodeCluster,
        }
    }
}
```

### 13.2 Standalone Mode (Default - No Changes Required)

When there's no `[cluster]` section in `server.toml`:

```toml
# server.toml - MINIMAL CONFIG (works exactly as today)
[server]
bind_addr = "0.0.0.0:8080"

[storage]
path = "./data"

# NO [cluster] section = standalone mode
```

In standalone mode:
- **No Raft**: `raft_manager = None`
- **Direct writes**: Commands go directly to providers
- **No gRPC server**: Port 9090 not opened
- **No overhead**: Zero Raft CPU/memory cost

```rust
impl AppContext {
    pub async fn new(config: ServerConfig, backend: Arc<dyn StorageBackend>) -> Result<Arc<Self>> {
        let mode = ClusterMode::from_config(&config);
        
        // Always create these (same as today)
        let system_tables = Arc::new(SystemTablesRegistry::new(backend.clone())?);
        let schema_registry = Arc::new(SchemaRegistry::new(backend.clone())?);
        
        // Only create RaftManager if cluster mode
        let raft_manager = match mode {
            ClusterMode::Standalone => None,  // <-- No Raft!
            ClusterMode::SingleNodeRaft | ClusterMode::MultiNodeCluster => {
                Some(Arc::new(RaftManager::new(
                    config.cluster.as_ref().unwrap().raft.clone(),
                    backend.clone(),
                    system_tables.clone(),
                ).await?))
            }
        };
        
        Ok(Arc::new(Self {
            backend,
            system_tables,
            schema_registry,
            raft_manager,
        }))
    }
}
```

### 13.3 Write Path Comparison

```
STANDALONE MODE (current behavior - unchanged):
┌─────────────────────────────────────────────────────────────────────┐
│  CREATE TABLE ...                                                    │
│       │                                                              │
│       ▼                                                              │
│  DDL Handler                                                         │
│       │                                                              │
│       ▼                                                              │
│  TablesTableProvider.create_table()  ──────────▶  RocksDB           │
│       │                                                              │
│       ▼                                                              │
│  SchemaRegistry.invalidate_cache()                                   │
│       │                                                              │
│       ▼                                                              │
│  Return OK                                                           │
└─────────────────────────────────────────────────────────────────────┘

CLUSTER MODE (new):
┌─────────────────────────────────────────────────────────────────────┐
│  CREATE TABLE ...                                                    │
│       │                                                              │
│       ▼                                                              │
│  DDL Handler                                                         │
│       │                                                              │
│       ▼                                                              │
│  ctx.propose_ddl(SystemCommand::CreateTable { ... })                │
│       │                                                              │
│       ▼                                                              │
│  RaftManager.propose_system()                                        │
│       │                                                              │
│       ▼                                                              │
│  Raft Consensus (replicate to majority)                             │
│       │                                                              │
│       ▼                                                              │
│  SystemStateMachine.apply()                                          │
│       │                                                              │
│       ▼                                                              │
│  TablesTableProvider.create_table()  ──────────▶  RocksDB           │
│       │                                                              │
│       ▼                                                              │
│  Return OK                                                           │
└─────────────────────────────────────────────────────────────────────┘
```

### 13.4 The Key Abstraction: `propose_ddl()`

```rust
impl AppContext {
    /// This is the ONLY change to DDL handlers!
    /// 
    /// Handlers call this instead of directly calling providers.
    /// In standalone mode: direct call to provider
    /// In cluster mode: goes through Raft first
    pub async fn propose_ddl(&self, cmd: SystemCommand) -> Result<SystemResponse> {
        if let Some(raft) = &self.raft_manager {
            // Cluster mode: replicate, then apply
            raft.propose_system(cmd).await
        } else {
            // Standalone mode: apply directly (same as today!)
            self.apply_system_command_direct(cmd).await
        }
    }
    
    /// Direct apply - used in standalone mode
    /// This is essentially what happens today, just extracted to a method
    async fn apply_system_command_direct(&self, cmd: SystemCommand) -> Result<SystemResponse> {
        match cmd {
            SystemCommand::CreateNamespace { namespace } => {
                self.system_tables.namespaces().create_namespace_async(namespace.clone()).await?;
                Ok(SystemResponse::NamespaceCreated { namespace_id: namespace.namespace_id })
            }
            SystemCommand::CreateTable { table } => {
                let table_id = TableId::new(table.namespace_id.clone(), table.table_name.clone());
                self.system_tables.tables().create_table_async(&table_id, &table).await?;
                Ok(SystemResponse::TableCreated { table_id })
            }
            // ... other commands
        }
    }
}
```

---

## 14. Design Validation Checklist

### 14.1 Node Scenarios

| Scenario | Handled? | Mechanism |
|----------|----------|-----------|
| New node joins cluster | ✅ | Leader adds as learner, sends log/snapshot |
| Node down for hours | ✅ | Leader replays log entries on reconnect |
| Node down for days | ✅ | Leader sends snapshot + recent entries |
| Node permanently removed | ✅ | Membership change via Raft |
| Network partition (minority) | ✅ | Minority cannot commit, majority continues |
| Network partition (healed) | ✅ | Minority catches up via log/snapshot |

### 14.2 Data Consistency

| Scenario | Handled? | Mechanism |
|----------|----------|-----------|
| Concurrent DDL on leader | ✅ | Raft serializes all commands |
| DDL during leader change | ✅ | Client retry, new leader has committed state |
| Read after write | ✅ | Wait for commit before returning |
| Stale read on follower | ✅ | Option: read-from-leader or read-index |

### 14.3 Single-Node Compatibility

| Scenario | Handled? | Mechanism |
|----------|----------|-----------|
| No `[cluster]` config | ✅ | `raft_manager = None`, direct writes |
| Upgrade standalone to cluster | ✅ | Add config, restart, node bootstraps |
| Performance overhead | ✅ | Zero overhead in standalone mode |
| Existing tests pass | ✅ | No behavior change in standalone |

### 14.4 Crate Boundaries

| Rule | Followed? | Notes |
|------|-----------|-------|
| No direct RocksDB in kalamdb-core | ✅ | Uses StorageBackend |
| State machines delegate to providers | ✅ | Uses SystemTablesRegistry |
| Minimal changes to existing crates | ✅ | ~100 lines outside kalamdb-raft |
| New code isolated in kalamdb-raft | ✅ | ~4000 lines in new crate |

---

## 15. Configuration Schema

```toml
# server.toml

# ============================================================
# STANDALONE MODE (no cluster section = current behavior)
# ============================================================
# Just don't include [cluster] section - everything works as today

# ============================================================
# CLUSTER MODE
# ============================================================

[cluster]
enabled = true
cluster_id = "kalamdb-cluster-prod"
node_id = 1  # Unique per node (1, 2, 3, ...)

[cluster.raft]
bind_addr = "0.0.0.0:9090"
advertise_addr = "10.0.0.1:9090"

# Timing (milliseconds)
heartbeat_interval = 100
election_timeout_min = 300
election_timeout_max = 500

# Log compaction
snapshot_threshold = 10000  # Entries before snapshot
log_compaction_batch = 1000

# Sharding configuration
[cluster.sharding]
num_user_shards = 32       # Default: 32 shards for user tables
num_shared_shards = 1      # Default: 1 shard for shared tables (future: increase)

# Members (static membership for Phase 1)
[[cluster.members]]
node_id = 1
raft_addr = "10.0.0.1:9090"
api_addr = "http://10.0.0.1:8080"

[[cluster.members]]
node_id = 2
raft_addr = "10.0.0.2:9090"
api_addr = "http://10.0.0.2:8080"

[[cluster.members]]
node_id = 3
raft_addr = "10.0.0.3:9090"
api_addr = "http://10.0.0.3:8080"
```

### 15.1 Configuration Structs

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct ClusterConfig {
    pub enabled: bool,
    pub cluster_id: String,
    pub node_id: u64,
    pub raft: RaftConfig,
    pub sharding: ShardingConfig,
    pub members: Vec<MemberConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RaftConfig {
    pub bind_addr: String,
    pub advertise_addr: String,
    pub heartbeat_interval: u64,
    pub election_timeout_min: u64,
    pub election_timeout_max: u64,
    pub snapshot_threshold: u64,
    pub log_compaction_batch: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShardingConfig {
    #[serde(default = "default_user_shards")]
    pub num_user_shards: u32,    // Default: 32
    
    #[serde(default = "default_shared_shards")]
    pub num_shared_shards: u32,  // Default: 1
}

fn default_user_shards() -> u32 { 32 }
fn default_shared_shards() -> u32 { 1 }

#[derive(Debug, Clone, Deserialize)]
pub struct MemberConfig {
    pub node_id: u64,
    pub raft_addr: String,
    pub api_addr: String,
}
```

---

## 16. Implementation Phases

### Phase 1A: Core Infrastructure (Week 1-2)

| Task | Description | Estimate |
|------|-------------|----------|
| Create `kalamdb-raft` crate | Basic structure, Cargo.toml, deps | 2h |
| Define protobuf schema | `proto/raft.proto` with all messages | 4h |
| Implement `GenericLogStore` | StorageBackend-based log storage | 6h |
| Implement `RaftRouter` | Multi-group message routing | 4h |
| Implement gRPC services | `RaftService`, `ClusterService` | 8h |
| Basic tests | Single-node, multi-group | 4h |

### Phase 1B: Command Executor & State Machines (Week 2-3)

| Task | Description | Estimate |
|------|-------------|----------|
| `CommandExecutor` trait | Generic executor interface | 4h |
| `DirectExecutor` | Standalone mode implementation | 4h |
| `RaftExecutor` | Cluster mode implementation | 6h |
| `KalamStateMachine` trait | Common trait with blanket impl | 4h |
| Metadata state machines | System, Users, Jobs | 8h |
| Data state machines | UserData, SharedData shards | 8h |
| Snapshot support | Build/restore for all SMs | 8h |

### Phase 1C: Integration (Week 3-4)

| Task | Description | Estimate |
|------|-------------|----------|
| `RaftManager` (36 groups) | Orchestrate all groups | 6h |
| `ShardRouter` | User/shared shard routing | 4h |
| AppContext integration | Wire executor into context | 4h |
| Update DDL handlers | Use `ctx.executor()` | 2h |
| Update DML handlers | Use `ctx.executor()` | 4h |
| Update auth handlers | Use `ctx.executor()` | 2h |
| Add `clear_all_async()` | Providers for snapshot restore | 2h |
| System tables | `cluster_members`, `raft_status`, `shard_stats` | 4h |
| CLI commands | `\cluster`, `\leader`, `\shards` | 2h |
| E2E tests | Full cluster scenarios | 8h |

### Total Estimate: ~100 hours (4 weeks)

### Total Estimate: ~86 hours (3-4 weeks)

## 17. Testing Strategy

### Unit Tests
- `KalamStateMachine` trait behavior
- Log store operations (append, truncate, vote)
- State machine apply/snapshot/restore
- Router dispatch

### Integration Tests
- Standalone mode works unchanged
- 3-node cluster formation
- Leader election and failover
- Command replication across groups
- Late joiner catchup (log replay)
- Stale node resync (snapshot transfer)
- Network partition handling

### Chaos Tests (Future)
- Random node failures
- Network delays/drops
- Slow followers
- Split-brain scenarios

---

## Summary

This spec defines a **Multi-Raft architecture** for KalamDB with:

| Aspect | Decision |
|--------|----------|
| **Total Raft Groups** | 36 (3 metadata + 32 user shards + 1 shared shard) |
| **Metadata Groups** | `meta:system`, `meta:users`, `meta:jobs` |
| **User Data Shards** | `data:user:0` .. `data:user:31` (by user_id hash) |
| **Shared Data Shard** | `data:shared:0` (extensible for future sharding) |
| **Common Trait** | `KalamStateMachine` with blanket `RaftStateMachine` impl |
| **Command Executor** | `CommandExecutor` trait - no if/else anywhere |
| **Networking** | gRPC with tonic, shared router |
| **Storage** | Generic over `StorageBackend` (RocksDB) |
| **State Machines** | Delegate to existing providers |
| **Single-Node Mode** | Zero config, `DirectExecutor`, no overhead |
| **Cluster Mode** | `RaftExecutor` with automatic shard routing |
| **Late Joiner** | Automatic catchup via log replay or snapshot |
| **Stale Node Resync** | Snapshot transfer + log replay |
| **Minimal Changes** | ~100 lines outside `kalamdb-raft` |

### Key Architectural Decisions

1. **CommandExecutor Trait (No If/Else)**: A single trait that both `DirectExecutor` (standalone) and `RaftExecutor` (cluster) implement. Handlers call `ctx.executor().execute_*()` without knowing which mode they're in.

2. **State Machines as Command Routers**: Raft state machines don't implement storage logic. They delegate to existing providers (`TablesTableProvider`, `UsersTableProvider`, etc.)

3. **Data Sharding**: 
   - User tables: 32 shards by `user_id % 32`
   - Shared tables: 1 shard (Phase 1), extensible for future
   - `ShardRouter` determines which Raft group to use

4. **Common `KalamStateMachine` Trait**: All state machines (metadata and data) implement a shared trait that provides idempotency, snapshot, and error handling. Only command-specific logic differs.

5. **Zero-Config Standalone**: No `[cluster]` section = standalone mode. Uses `DirectExecutor`, no Raft overhead, no gRPC server, works exactly as today.

6. **Automatic Node Catchup**: 
   - New nodes: Join as learner → receive log/snapshot → promote to voter
   - Stale nodes: Reconnect → receive snapshot if log compacted → replay entries

7. **Raft Log vs Entity Data**: Raft log stores commands (in RocksDB partitions). Entity data lives in existing providers (also RocksDB, different partitions).

8. **Crate Structure**:
   - `kalamdb-raft` (NEW, ~4000 lines): Raft protocol, gRPC, state machines, executors
   - `kalamdb-system` (EXISTS, +30 lines): Add `clear_all_async()` for snapshots
   - `kalamdb-core` (EXISTS, +20 lines): Add `executor: Arc<dyn CommandExecutor>`
   - `kalamdb-sql` (EXISTS, unchanged): Handlers use `ctx.executor()` (same interface)

The implementation reuses KalamDB's existing abstractions (`StorageBackend`, `Partition`, `EntityStore`) and follows the crate boundary rules (no direct RocksDB in `kalamdb-core`).
