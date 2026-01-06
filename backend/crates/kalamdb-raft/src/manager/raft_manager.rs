//! Raft Manager - Central orchestration for all Raft groups
//!
//! Manages N Raft groups (configurable shards):
//! - MetaSystem: System-wide metadata (namespaces, tables, storages)
//! - MetaUsers: User/auth metadata
//! - MetaJobs: Job queue and scheduling
//! - DataUserShard(0..N): User table data shards (default 32)
//! - DataSharedShard(0..M): Shared table data shards (default 1)

use std::sync::Arc;

use kalamdb_commons::models::TableId;
use openraft::RaftMetrics;
use parking_lot::RwLock;

use crate::storage::log_store::KalamNode;
use crate::state_machine::{
    SystemStateMachine, UsersStateMachine, JobsStateMachine,
    UserDataStateMachine, SharedDataStateMachine,
};
use crate::manager::RaftGroup;
use crate::{GroupId, RaftError};

/// Default number of user data shards
pub const DEFAULT_USER_DATA_SHARDS: u32 = 32;

/// Default number of shared data shards
pub const DEFAULT_SHARED_DATA_SHARDS: u32 = 1;

/// Configuration for the Raft cluster
#[derive(Debug, Clone)]
pub struct ClusterConfig {
    /// This node's ID (must be >= 1)
    pub node_id: u64,
    
    /// This node's RPC address for Raft communication
    pub rpc_addr: String,
    
    /// This node's API address for client requests
    pub api_addr: String,
    
    /// Peer nodes in the cluster
    pub peers: Vec<PeerConfig>,
    
    /// Number of user data shards (default: 32)
    pub user_shards: u32,
    
    /// Number of shared data shards (default: 1)
    pub shared_shards: u32,
    
    /// Raft heartbeat interval in milliseconds
    pub heartbeat_interval_ms: u64,
    
    /// Raft election timeout range (min, max) in milliseconds
    pub election_timeout_ms: (u64, u64),
    
    /// Minimum number of nodes that must acknowledge a write (default: 1)
    /// Set to 2 or 3 for strong consistency in a 3-node cluster.
    pub min_replication_nodes: u32,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            node_id: 1,
            rpc_addr: "127.0.0.1:9100".to_string(),
            api_addr: "127.0.0.1:8080".to_string(),
            peers: vec![],
            user_shards: DEFAULT_USER_DATA_SHARDS,
            shared_shards: DEFAULT_SHARED_DATA_SHARDS,
            heartbeat_interval_ms: 50,
            election_timeout_ms: (150, 300),
            min_replication_nodes: 1,
        }
    }
}

/// Configuration for a peer node
#[derive(Debug, Clone)]
pub struct PeerConfig {
    /// Peer's node ID
    pub node_id: u64,
    
    /// Peer's RPC address for Raft communication
    pub rpc_addr: String,
    
    /// Peer's API address for client requests
    pub api_addr: String,
}

/// Central manager for all Raft groups
///
/// Orchestrates:
/// - Group lifecycle (creation, startup, shutdown)
/// - Command routing to correct group
/// - Leader tracking and forwarding
pub struct RaftManager {
    /// This node's ID
    node_id: u64,
    
    /// System metadata group
    meta_system: Arc<RaftGroup<SystemStateMachine>>,
    
    /// Users metadata group
    meta_users: Arc<RaftGroup<UsersStateMachine>>,
    
    /// Jobs metadata group
    meta_jobs: Arc<RaftGroup<JobsStateMachine>>,
    
    /// User data shards (configurable, default 32)
    user_data_shards: Vec<Arc<RaftGroup<UserDataStateMachine>>>,
    
    /// Shared data shards (configurable, default 1)
    shared_data_shards: Vec<Arc<RaftGroup<SharedDataStateMachine>>>,
    
    /// Whether the manager has been started
    started: RwLock<bool>,
    
    /// Cluster configuration
    config: ClusterConfig,
    
    /// Number of user shards (cached from config)
    user_shards_count: u32,
    
    /// Number of shared shards (cached from config)
    shared_shards_count: u32,
}

impl std::fmt::Debug for RaftManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RaftManager")
            .field("node_id", &self.node_id)
            .field("started", &*self.started.read())
            .field("user_data_shards", &self.user_data_shards.len())
            .field("shared_data_shards", &self.shared_data_shards.len())
            .finish_non_exhaustive()
    }
}

impl RaftManager {
    /// Create a new Raft manager
    pub fn new(config: ClusterConfig) -> Self {
        let user_shards_count = config.user_shards;
        let shared_shards_count = config.shared_shards;
        
        // Create meta groups
        let meta_system = Arc::new(RaftGroup::new(
            GroupId::MetaSystem,
            SystemStateMachine::new(),
        ));
        
        let meta_users = Arc::new(RaftGroup::new(
            GroupId::MetaUsers,
            UsersStateMachine::new(),
        ));
        
        let meta_jobs = Arc::new(RaftGroup::new(
            GroupId::MetaJobs,
            JobsStateMachine::new(),
        ));
        
        // Create user data shards (configurable)
        let user_data_shards: Vec<_> = (0..user_shards_count)
            .map(|shard_id| {
                Arc::new(RaftGroup::new(
                    GroupId::DataUserShard(shard_id),
                    UserDataStateMachine::new(shard_id),
                ))
            })
            .collect();
        
        // Create shared data shards (configurable)
        let shared_data_shards: Vec<_> = (0..shared_shards_count)
            .map(|shard_id| {
                Arc::new(RaftGroup::new(
                    GroupId::DataSharedShard(shard_id),
                    SharedDataStateMachine::new(shard_id),
                ))
            })
            .collect();
        
        Self {
            node_id: config.node_id,
            meta_system,
            meta_users,
            meta_jobs,
            user_data_shards,
            shared_data_shards,
            started: RwLock::new(false),
            config,
            user_shards_count,
            shared_shards_count,
        }
    }
    
    /// Get this node's ID
    pub fn node_id(&self) -> u64 {
        self.node_id
    }

    /// Get OpenRaft metrics for the MetaSystem group
    pub fn meta_system_metrics(&self) -> Option<RaftMetrics<u64, KalamNode>> {
        self.meta_system.metrics()
    }
    
    /// Check if the manager has been started
    pub fn is_started(&self) -> bool {
        *self.started.read()
    }
    
    /// Start all Raft groups
    ///
    /// This initializes all 36 Raft groups and begins participating in consensus.
    pub async fn start(&self) -> Result<(), RaftError> {
        if self.is_started() {
            log::warn!("RaftManager already started, skipping");
            return Ok(());
        }
        
        log::info!("╔═══════════════════════════════════════════════════════════════════╗");
        log::info!("║               Starting Raft Cluster Mode                          ║");
        log::info!("╚═══════════════════════════════════════════════════════════════════╝");
        log::info!("Node ID: {}", self.node_id);
        log::info!("RPC Address: {}", self.config.rpc_addr);
        log::info!("API Address: {}", self.config.api_addr);
        log::info!("Total Raft Groups: {} (3 meta + {} user shards + {} shared shards)", 
            self.group_count(), self.user_shards_count, self.shared_shards_count);
        log::info!("Peers configured: {}", self.config.peers.len());
        for peer in &self.config.peers {
            log::info!("  - Peer node_id={}: rpc={}, api={}", 
                peer.node_id, peer.rpc_addr, peer.api_addr);
        }
        
        // Register peers from config
        log::debug!("Registering {} peers...", self.config.peers.len());
        for peer in &self.config.peers {
            self.register_peer(peer.node_id, peer.rpc_addr.clone(), peer.api_addr.clone());
        }
        
        // Start all meta groups
        log::info!("Starting meta groups...");
        self.meta_system.start(self.node_id).await?;
        log::debug!("  ✓ MetaSystem group started");
        self.meta_users.start(self.node_id).await?;
        log::debug!("  ✓ MetaUsers group started");
        self.meta_jobs.start(self.node_id).await?;
        log::debug!("  ✓ MetaJobs group started");
        
        // Start all user data shards
        log::info!("Starting {} user data shards...", self.user_data_shards.len());
        for (i, shard) in self.user_data_shards.iter().enumerate() {
            shard.start(self.node_id).await?;
            log::debug!("  ✓ UserDataShard[{}] started", i);
        }
        
        // Start all shared data shards
        log::info!("Starting {} shared data shards...", self.shared_data_shards.len());
        for (i, shard) in self.shared_data_shards.iter().enumerate() {
            shard.start(self.node_id).await?;
            log::debug!("  ✓ SharedDataShard[{}] started", i);
        }
        
        // Mark as started
        {
            let mut started = self.started.write();
            *started = true;
        }
        
        log::info!("╔═══════════════════════════════════════════════════════════════════╗");
        log::info!("║         Raft Cluster Started Successfully on Node {}              ║", self.node_id);
        log::info!("╚═══════════════════════════════════════════════════════════════════╝");
        Ok(())
    }
    
    /// Initialize the cluster (call on first node only)
    ///
    /// This bootstraps all Raft groups with initial membership containing this node.
    pub async fn initialize_cluster(&self) -> Result<(), RaftError> {
        if !self.is_started() {
            return Err(RaftError::NotStarted("RaftManager not started".to_string()));
        }
        
        let self_node = KalamNode {
            rpc_addr: self.config.rpc_addr.clone(),
            api_addr: self.config.api_addr.clone(),
        };
        
        log::info!("╔═══════════════════════════════════════════════════════════════════╗");
        log::info!("║           Initializing Raft Cluster (Bootstrap)                   ║");
        log::info!("╚═══════════════════════════════════════════════════════════════════╝");
        log::info!("Bootstrap node_id={}, rpc={}, api={}", 
            self.node_id, self_node.rpc_addr, self_node.api_addr);
        
        // Initialize all groups
        log::info!("Initializing meta groups...");
        self.meta_system.initialize(self.node_id, self_node.clone()).await?;
        log::debug!("  ✓ MetaSystem initialized");
        self.meta_users.initialize(self.node_id, self_node.clone()).await?;
        log::debug!("  ✓ MetaUsers initialized");
        self.meta_jobs.initialize(self.node_id, self_node.clone()).await?;
        log::debug!("  ✓ MetaJobs initialized");
        
        log::info!("Initializing user data shards...");
        for (i, shard) in self.user_data_shards.iter().enumerate() {
            shard.initialize(self.node_id, self_node.clone()).await?;
            log::debug!("  ✓ UserDataShard[{}] initialized", i);
        }
        
        log::info!("Initializing shared data shards...");
        for (i, shard) in self.shared_data_shards.iter().enumerate() {
            shard.initialize(self.node_id, self_node.clone()).await?;
            log::debug!("  ✓ SharedDataShard[{}] initialized", i);
        }
        
        log::info!("╔═══════════════════════════════════════════════════════════════════╗");
        log::info!("║         Cluster Initialized Successfully!                         ║");
        log::info!("║         This node is now the leader of all {} groups              ║", self.group_count());
        log::info!("╚═══════════════════════════════════════════════════════════════════╝");
        
        // After initialization, add peer nodes to the cluster
        if !self.config.peers.is_empty() {
            log::info!("Adding {} peer nodes to cluster...", self.config.peers.len());
            for peer in &self.config.peers {
                log::info!("  Adding peer node_id={} (rpc={}, api={})...", 
                    peer.node_id, peer.rpc_addr, peer.api_addr);
                match self.add_node(peer.node_id, peer.rpc_addr.clone(), peer.api_addr.clone()).await {
                    Ok(_) => log::info!("    ✓ Peer {} added successfully", peer.node_id),
                    Err(e) => log::error!("    ✗ Failed to add peer {}: {}", peer.node_id, e),
                }
            }
            log::info!("Peer nodes added to cluster");
        }
        
        Ok(())
    }
    
    /// Add a new node to the cluster
    pub async fn add_node(&self, node_id: u64, rpc_addr: String, api_addr: String) -> Result<(), RaftError> {
        log::info!("Adding node {} to cluster (rpc={}, api={})", node_id, rpc_addr, api_addr);
        let node = KalamNode { rpc_addr: rpc_addr.clone(), api_addr: api_addr.clone() };
        
        // Add to all groups as learner first
        log::debug!("Adding node {} as learner to all groups...", node_id);
        self.meta_system.add_learner(node_id, node.clone()).await?;
        self.meta_users.add_learner(node_id, node.clone()).await?;
        self.meta_jobs.add_learner(node_id, node.clone()).await?;
        
        for shard in &self.user_data_shards {
            shard.add_learner(node_id, node.clone()).await?;
        }
        
        for shard in &self.shared_data_shards {
            shard.add_learner(node_id, node.clone()).await?;
        }
        
        log::info!("Node {} added to cluster successfully", node_id);
        Ok(())
    }
    
    /// Get the cluster configuration
    pub fn config(&self) -> &ClusterConfig {
        &self.config
    }
    
    /// Get the group for a given GroupId
    fn _get_group_id(&self, group_id: GroupId) -> Result<(), RaftError> {
        match group_id {
            GroupId::MetaSystem => Ok(()),
            GroupId::MetaUsers => Ok(()),
            GroupId::MetaJobs => Ok(()),
            GroupId::DataUserShard(shard) if shard < self.user_shards_count => Ok(()),
            GroupId::DataSharedShard(shard) if shard < self.shared_shards_count => Ok(()),
            _ => Err(RaftError::InvalidGroup(group_id.to_string())),
        }
    }
    
    /// Check if this node is leader for a given group
    pub fn is_leader(&self, group_id: GroupId) -> bool {
        match group_id {
            GroupId::MetaSystem => self.meta_system.is_leader(),
            GroupId::MetaUsers => self.meta_users.is_leader(),
            GroupId::MetaJobs => self.meta_jobs.is_leader(),
            GroupId::DataUserShard(shard) if shard < self.user_shards_count => {
                self.user_data_shards[shard as usize].is_leader()
            }
            GroupId::DataSharedShard(shard) if shard < self.shared_shards_count => {
                self.shared_data_shards[shard as usize].is_leader()
            }
            _ => false,
        }
    }
    
    /// Get the current leader for a group
    pub fn current_leader(&self, group_id: GroupId) -> Option<u64> {
        match group_id {
            GroupId::MetaSystem => self.meta_system.current_leader(),
            GroupId::MetaUsers => self.meta_users.current_leader(),
            GroupId::MetaJobs => self.meta_jobs.current_leader(),
            GroupId::DataUserShard(shard) if shard < self.user_shards_count => {
                self.user_data_shards[shard as usize].current_leader()
            }
            GroupId::DataSharedShard(shard) if shard < self.shared_shards_count => {
                self.shared_data_shards[shard as usize].current_leader()
            }
            _ => None,
        }
    }
    
    /// Propose a command to the MetaSystem group (with leader forwarding)
    ///
    /// If this node is a follower, the request is automatically forwarded to the leader.
    pub async fn propose_system(&self, command: Vec<u8>) -> Result<Vec<u8>, RaftError> {
        self.meta_system.propose_with_forward(command).await
    }
    
    /// Propose a command to the MetaUsers group (with leader forwarding)
    ///
    /// If this node is a follower, the request is automatically forwarded to the leader.
    pub async fn propose_users(&self, command: Vec<u8>) -> Result<Vec<u8>, RaftError> {
        self.meta_users.propose_with_forward(command).await
    }
    
    /// Propose a command to the MetaJobs group (with leader forwarding)
    ///
    /// If this node is a follower, the request is automatically forwarded to the leader.
    pub async fn propose_jobs(&self, command: Vec<u8>) -> Result<Vec<u8>, RaftError> {
        self.meta_jobs.propose_with_forward(command).await
    }
    
    /// Propose a command to a user data shard (with leader forwarding)
    ///
    /// If this node is a follower, the request is automatically forwarded to the leader.
    pub async fn propose_user_data(&self, shard: u32, command: Vec<u8>) -> Result<Vec<u8>, RaftError> {
        if shard >= self.user_shards_count {
            return Err(RaftError::InvalidGroup(format!("DataUserShard({})", shard)));
        }
        self.user_data_shards[shard as usize].propose_with_forward(command).await
    }
    
    /// Propose a command to a shared data shard (with leader forwarding)
    ///
    /// If this node is a follower, the request is automatically forwarded to the leader.
    pub async fn propose_shared_data(&self, shard: u32, command: Vec<u8>) -> Result<Vec<u8>, RaftError> {
        if shard >= self.shared_shards_count {
            return Err(RaftError::InvalidGroup(format!("DataSharedShard({})", shard)));
        }
        self.shared_data_shards[shard as usize].propose_with_forward(command).await
    }
    
    /// Propose a command to any group by GroupId (for RPC server handling)
    ///
    /// Used by the RaftService when receiving a forwarded proposal.
    /// Does NOT forward - should only be called when we are the leader.
    pub async fn propose_for_group(&self, group_id: GroupId, command: Vec<u8>) -> Result<Vec<u8>, RaftError> {
        match group_id {
            GroupId::MetaSystem => self.meta_system.propose(command).await,
            GroupId::MetaUsers => self.meta_users.propose(command).await,
            GroupId::MetaJobs => self.meta_jobs.propose(command).await,
            GroupId::DataUserShard(shard) => {
                if shard >= self.user_shards_count {
                    return Err(RaftError::InvalidGroup(format!("DataUserShard({})", shard)));
                }
                self.user_data_shards[shard as usize].propose(command).await
            }
            GroupId::DataSharedShard(shard) => {
                if shard >= self.shared_shards_count {
                    return Err(RaftError::InvalidGroup(format!("DataSharedShard({})", shard)));
                }
                self.shared_data_shards[shard as usize].propose(command).await
            }
        }
    }
    
    /// Compute the shard ID for a table
    ///
    /// Uses consistent hashing on the table ID to determine shard placement.
    pub fn compute_shard(&self, table_id: &TableId) -> u32 {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;
        
        let mut hasher = DefaultHasher::new();
        table_id.namespace_id().as_str().hash(&mut hasher);
        table_id.table_name().as_str().hash(&mut hasher);
        (hasher.finish() % self.user_shards_count as u64) as u32
    }
    
    /// Register a peer node with all groups
    pub fn register_peer(&self, node_id: u64, rpc_addr: String, api_addr: String) {
        let node = KalamNode { rpc_addr, api_addr };
        
        // Register with all meta groups
        self.meta_system.register_peer(node_id, node.clone());
        self.meta_users.register_peer(node_id, node.clone());
        self.meta_jobs.register_peer(node_id, node.clone());
        
        // Register with all data shards
        for shard in &self.user_data_shards {
            shard.register_peer(node_id, node.clone());
        }
        for shard in &self.shared_data_shards {
            shard.register_peer(node_id, node.clone());
        }
    }
    
    /// Get all group IDs
    pub fn all_group_ids(&self) -> Vec<GroupId> {
        let mut groups = vec![
            GroupId::MetaSystem,
            GroupId::MetaUsers,
            GroupId::MetaJobs,
        ];
        
        for shard in 0..self.user_shards_count {
            groups.push(GroupId::DataUserShard(shard));
        }
        
        for shard in 0..self.shared_shards_count {
            groups.push(GroupId::DataSharedShard(shard));
        }
        
        groups
    }
    
    /// Get the total number of groups
    pub fn group_count(&self) -> usize {
        3 + self.user_shards_count as usize + self.shared_shards_count as usize
    }
    
    /// Get the number of user data shards
    pub fn user_shards(&self) -> u32 {
        self.user_shards_count
    }
    
    /// Get the number of shared data shards
    pub fn shared_shards(&self) -> u32 {
        self.shared_shards_count
    }
    
    /// Get the MetaSystem group
    pub fn meta_system(&self) -> &Arc<RaftGroup<SystemStateMachine>> {
        &self.meta_system
    }
    
    /// Get the MetaUsers group
    pub fn meta_users(&self) -> &Arc<RaftGroup<UsersStateMachine>> {
        &self.meta_users
    }
    
    /// Get the MetaJobs group
    pub fn meta_jobs(&self) -> &Arc<RaftGroup<JobsStateMachine>> {
        &self.meta_jobs
    }
    
    /// Get a user data shard
    pub fn user_data_shard(&self, shard: u32) -> Option<&Arc<RaftGroup<UserDataStateMachine>>> {
        self.user_data_shards.get(shard as usize)
    }
    
    /// Get a shared data shard
    pub fn shared_data_shard(&self, shard: u32) -> Option<&Arc<RaftGroup<SharedDataStateMachine>>> {
        self.shared_data_shards.get(shard as usize)
    }
    
    /// Set the system applier for persisting metadata to providers
    ///
    /// This should be called after RaftManager creation once providers are available.
    /// The applier will be called on all nodes (leader and followers) when commands
    /// are applied, ensuring consistent state across the cluster.
    pub fn set_system_applier(&self, applier: std::sync::Arc<dyn crate::applier::SystemApplier>) {
        // Get the state machine from the MetaSystem group's storage
        let sm = self.meta_system.storage().state_machine();
        sm.set_applier(applier);
        log::info!("RaftManager: System applier registered for metadata replication");
    }
    
    // === Raft RPC Handlers (for receiving RPCs from other nodes) ===
    
    /// Get the Raft instance for a specific group
    fn get_raft_instance(&self, group_id: GroupId) -> Result<crate::manager::raft_group::RaftInstance, RaftError> {
        match group_id {
            GroupId::MetaSystem => self.meta_system.raft()
                .ok_or_else(|| RaftError::NotStarted(format!("Group {:?} not started", group_id))),
            GroupId::MetaUsers => self.meta_users.raft()
                .ok_or_else(|| RaftError::NotStarted(format!("Group {:?} not started", group_id))),
            GroupId::MetaJobs => self.meta_jobs.raft()
                .ok_or_else(|| RaftError::NotStarted(format!("Group {:?} not started", group_id))),
            GroupId::DataUserShard(shard) => {
                let group = self.user_data_shards.get(shard as usize)
                    .ok_or_else(|| RaftError::Internal(format!("Invalid user shard: {}", shard)))?;
                group.raft()
                    .ok_or_else(|| RaftError::NotStarted(format!("Group {:?} not started", group_id)))
            }
            GroupId::DataSharedShard(shard) => {
                let group = self.shared_data_shards.get(shard as usize)
                    .ok_or_else(|| RaftError::Internal(format!("Invalid shared shard: {}", shard)))?;
                group.raft()
                    .ok_or_else(|| RaftError::NotStarted(format!("Group {:?} not started", group_id)))
            }
        }
    }
    
    /// Handle incoming vote request
    pub async fn handle_vote(&self, group_id: GroupId, payload: &[u8]) -> Result<Vec<u8>, RaftError> {
        use openraft::raft::VoteRequest;
        use crate::state_machine::{encode, decode};
        
        let raft = self.get_raft_instance(group_id)?;
        let request: VoteRequest<u64> = decode(payload)?;
        
        let response = raft.vote(request).await
            .map_err(|e| RaftError::Internal(format!("Vote RPC failed: {:?}", e)))?;
        
        encode(&response)
    }
    
    /// Handle incoming append entries request
    pub async fn handle_append_entries(&self, group_id: GroupId, payload: &[u8]) -> Result<Vec<u8>, RaftError> {
        use openraft::raft::AppendEntriesRequest;
        use crate::storage::KalamTypeConfig;
        use crate::state_machine::{encode, decode};
        
        let raft = self.get_raft_instance(group_id)?;
        let request: AppendEntriesRequest<KalamTypeConfig> = decode(payload)?;
        
        let response = raft.append_entries(request).await
            .map_err(|e| RaftError::Internal(format!("AppendEntries RPC failed: {:?}", e)))?;
        
        encode(&response)
    }
    
    /// Handle incoming install snapshot request
    pub async fn handle_install_snapshot(&self, group_id: GroupId, payload: &[u8]) -> Result<Vec<u8>, RaftError> {
        use openraft::raft::InstallSnapshotRequest;
        use crate::storage::KalamTypeConfig;
        use crate::state_machine::{encode, decode};
        
        let raft = self.get_raft_instance(group_id)?;
        let request: InstallSnapshotRequest<KalamTypeConfig> = decode(payload)?;
        
        let response = raft.install_snapshot(request).await
            .map_err(|e| RaftError::Internal(format!("InstallSnapshot RPC failed: {:?}", e)))?;
        
        encode(&response)
    }
    
    /// Gracefully shutdown the Raft manager
    ///
    /// This performs:
    /// 1. Leadership transfer if this node is leader (to minimize downtime)
    /// 2. Logs cluster leave event
    /// 3. Marks the manager as stopped
    pub async fn shutdown(&self) -> Result<(), RaftError> {
        if !self.is_started() {
            log::warn!("RaftManager not started, nothing to shutdown");
            return Ok(());
        }
        
        log::info!("╔═══════════════════════════════════════════════════════════════════╗");
        log::info!("║           Graceful Cluster Shutdown Starting                      ║");
        log::info!("╚═══════════════════════════════════════════════════════════════════╝");
        log::info!("Node {} leaving cluster...", self.node_id);
        
        // Count how many groups we're leading
        let mut groups_leading = 0;
        for group_id in self.all_group_ids() {
            if self.is_leader(group_id) {
                groups_leading += 1;
            }
        }
        
        if groups_leading > 0 {
            log::info!("This node is leader of {} groups, attempting leadership transfer...", groups_leading);
            
            // Attempt leadership transfer for each group where we're leader
            // Find the first available peer to transfer leadership to
            if let Some(target_node) = self.config.peers.first() {
                log::info!("Transferring leadership to node {}...", target_node.node_id);
                
                // Transfer leadership for MetaSystem
                if self.meta_system.is_leader() {
                    match self.meta_system.transfer_leadership(target_node.node_id).await {
                        Ok(_) => log::info!("  ✓ MetaSystem leadership transferred to node {}", target_node.node_id),
                        Err(e) => log::warn!("  ⚠ Failed to transfer MetaSystem leadership: {}", e),
                    }
                }
                
                // Transfer leadership for MetaUsers
                if self.meta_users.is_leader() {
                    match self.meta_users.transfer_leadership(target_node.node_id).await {
                        Ok(_) => log::info!("  ✓ MetaUsers leadership transferred to node {}", target_node.node_id),
                        Err(e) => log::warn!("  ⚠ Failed to transfer MetaUsers leadership: {}", e),
                    }
                }
                
                // Transfer leadership for MetaJobs
                if self.meta_jobs.is_leader() {
                    match self.meta_jobs.transfer_leadership(target_node.node_id).await {
                        Ok(_) => log::info!("  ✓ MetaJobs leadership transferred to node {}", target_node.node_id),
                        Err(e) => log::warn!("  ⚠ Failed to transfer MetaJobs leadership: {}", e),
                    }
                }
                
                // Transfer leadership for user data shards
                for (i, shard) in self.user_data_shards.iter().enumerate() {
                    if shard.is_leader() {
                        match shard.transfer_leadership(target_node.node_id).await {
                            Ok(_) => log::debug!("  ✓ UserDataShard[{}] leadership transferred", i),
                            Err(e) => log::warn!("  ⚠ Failed to transfer UserDataShard[{}] leadership: {}", i, e),
                        }
                    }
                }
                
                // Transfer leadership for shared data shards
                for (i, shard) in self.shared_data_shards.iter().enumerate() {
                    if shard.is_leader() {
                        match shard.transfer_leadership(target_node.node_id).await {
                            Ok(_) => log::debug!("  ✓ SharedDataShard[{}] leadership transferred", i),
                            Err(e) => log::warn!("  ⚠ Failed to transfer SharedDataShard[{}] leadership: {}", i, e),
                        }
                    }
                }
                
                // Give time for leadership transfer to complete
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                log::info!("Leadership transfer completed");
            } else {
                log::warn!("No peers available for leadership transfer - cluster may experience brief unavailability");
            }
        }
        
        // Mark as stopped
        {
            let mut started = self.started.write();
            *started = false;
        }
        
        log::info!("╔═══════════════════════════════════════════════════════════════════╗");
        log::info!("║           Node {} Left Cluster Successfully                       ║", self.node_id);
        log::info!("╚═══════════════════════════════════════════════════════════════════╝");
        
        Ok(())
    }
    
    /// Get the minimum number of replication nodes required
    pub fn min_replication_nodes(&self) -> u32 {
        self.config.min_replication_nodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::models::{NamespaceId, TableName};

    fn test_config() -> ClusterConfig {
        ClusterConfig {
            node_id: 1,
            rpc_addr: "127.0.0.1:5001".to_string(),
            api_addr: "127.0.0.1:3001".to_string(),
            peers: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn test_raft_manager_creation() {
        let manager = RaftManager::new(test_config());
        
        assert_eq!(manager.node_id(), 1);
        assert!(!manager.is_started());
        
        // Should have 36 groups total by default: 3 meta + 32 user data + 1 shared
        assert_eq!(manager.group_count(), 36);
    }

    #[test]
    fn test_all_group_ids() {
        let manager = RaftManager::new(test_config());
        let groups = manager.all_group_ids();
        
        assert_eq!(groups.len(), 36);
        assert!(groups.contains(&GroupId::MetaSystem));
        assert!(groups.contains(&GroupId::MetaUsers));
        assert!(groups.contains(&GroupId::MetaJobs));
        assert!(groups.contains(&GroupId::DataUserShard(0)));
        assert!(groups.contains(&GroupId::DataUserShard(31)));
        assert!(groups.contains(&GroupId::DataSharedShard(0)));
    }

    #[test]
    fn test_shard_computation() {
        let manager = RaftManager::new(test_config());
        
        let table1 = TableId::new(NamespaceId::from("ns1"), TableName::from("table1"));
        let table2 = TableId::new(NamespaceId::from("ns1"), TableName::from("table2"));
        let table3 = TableId::new(NamespaceId::from("ns2"), TableName::from("table1"));
        
        let shard1 = manager.compute_shard(&table1);
        let shard2 = manager.compute_shard(&table2);
        let shard3 = manager.compute_shard(&table3);
        
        // Shards should be in valid range
        assert!(shard1 < manager.user_shards());
        assert!(shard2 < manager.user_shards());
        assert!(shard3 < manager.user_shards());
        
        // Same table should always get same shard
        assert_eq!(manager.compute_shard(&table1), shard1);
        
        // Different tables may get different shards (likely but not guaranteed)
        // Just verify they're computed consistently
    }

    #[test]
    fn test_register_peer() {
        let manager = RaftManager::new(test_config());
        
        // Should not panic
        manager.register_peer(
            2,
            "127.0.0.1:5002".to_string(),
            "127.0.0.1:3002".to_string(),
        );
    }

    #[test]
    fn test_is_leader_before_start() {
        let manager = RaftManager::new(test_config());
        
        // Before start, no group should have a leader
        assert!(!manager.is_leader(GroupId::MetaSystem));
        assert!(!manager.is_leader(GroupId::MetaUsers));
        assert!(!manager.is_leader(GroupId::MetaJobs));
        assert!(!manager.is_leader(GroupId::DataUserShard(0)));
    }

    #[test]
    fn test_current_leader_before_start() {
        let manager = RaftManager::new(test_config());
        
        // Before start, no leader should be known
        assert!(manager.current_leader(GroupId::MetaSystem).is_none());
        assert!(manager.current_leader(GroupId::MetaUsers).is_none());
    }
}
