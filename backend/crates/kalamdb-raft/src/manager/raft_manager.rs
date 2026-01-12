//! Raft Manager - Central orchestration for all Raft groups
//!
//! Manages N Raft groups (configurable shards):
//! - Meta: Unified metadata (namespaces, tables, storages, users, jobs)
//! - DataUserShard(0..N): User table data shards (default 32)
//! - DataSharedShard(0..M): Shared table data shards (default 1)

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use kalamdb_commons::models::{NodeId, TableId};
use kalamdb_store::raft_storage::RAFT_PARTITION_NAME;
use kalamdb_store::{Partition, StorageBackend};
use openraft::RaftMetrics;
use parking_lot::RwLock;

use crate::manager::config::RaftManagerConfig;
use crate::manager::RaftGroup;
use crate::state_machine::{
    MetaStateMachine, SharedDataStateMachine, UserDataStateMachine,
};
use crate::state_machine::KalamStateMachine;
use crate::storage::KalamNode;
use crate::{GroupId, RaftError};

/// Central manager for all Raft groups
///
/// Orchestrates:
/// - Group lifecycle (creation, startup, shutdown)
/// - Command routing to correct group
/// - Leader tracking and forwarding
pub struct RaftManager {
    /// This node's ID
    node_id: NodeId,
    
    /// Unified metadata group (namespaces, tables, storages, users, jobs)
    meta: Arc<RaftGroup<MetaStateMachine>>,
    
    /// User data shards (configurable, default 32)
    user_data_shards: Vec<Arc<RaftGroup<UserDataStateMachine>>>,
    
    /// Shared data shards (configurable, default 1)
    shared_data_shards: Vec<Arc<RaftGroup<SharedDataStateMachine>>>,
    
    /// Whether the manager has been started
    started: RwLock<bool>,
    
    /// Cluster configuration
    config: RaftManagerConfig,
    
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
    /// Create a new Raft manager with in-memory storage (for testing or standalone mode)
    pub fn new(config: RaftManagerConfig) -> Self {
        let user_shards_count = config.user_shards;
        let shared_shards_count = config.shared_shards;

        // Create unified meta group
        let meta = Arc::new(RaftGroup::new(
            GroupId::Meta,
            MetaStateMachine::new(),
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
            meta,
            user_data_shards,
            shared_data_shards,
            started: RwLock::new(false),
            config,
            user_shards_count,
            shared_shards_count,
        }
    }

    /// Create a new Raft manager with persistent storage
    ///
    /// This mode persists Raft log entries, votes, and metadata to durable storage.
    /// On restart, state is recovered from the persistent store.
    ///
    /// The `raft_data` partition will be created if it doesn't exist.
    pub fn new_persistent(
        config: RaftManagerConfig,
        backend: Arc<dyn StorageBackend>,
        snapshots_dir: std::path::PathBuf,
    ) -> Result<Self, RaftError> {
        let user_shards_count = config.user_shards;
        let shared_shards_count = config.shared_shards;

        // Ensure the raft_data partition exists
        let partition = Partition::new(RAFT_PARTITION_NAME);
        if !backend.partition_exists(&partition) {
            backend
                .create_partition(&partition)
                .map_err(|e| RaftError::Storage(format!("Failed to create raft partition: {}", e)))?;
        }

        std::fs::create_dir_all(&snapshots_dir).map_err(|e| {
            RaftError::Storage(format!("Failed to create snapshots directory {}: {}", snapshots_dir.display(), e))
        })?;

        // Create unified meta group with persistent storage
        let meta = Arc::new(RaftGroup::new_persistent(
            GroupId::Meta,
            MetaStateMachine::new(),
            backend.clone(),
            snapshots_dir.clone(),
        )?);

        // Create user data shards with persistent storage
        let user_data_shards: Vec<_> = (0..user_shards_count)
            .map(|shard_id| {
                RaftGroup::new_persistent(
                    GroupId::DataUserShard(shard_id),
                    UserDataStateMachine::new(shard_id),
                    backend.clone(),
                    snapshots_dir.clone(),
                )
                .map(Arc::new)
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Create shared data shards with persistent storage
        let shared_data_shards: Vec<_> = (0..shared_shards_count)
            .map(|shard_id| {
                RaftGroup::new_persistent(
                    GroupId::DataSharedShard(shard_id),
                    SharedDataStateMachine::new(shard_id),
                    backend.clone(),
                    snapshots_dir.clone(),
                )
                .map(Arc::new)
            })
            .collect::<Result<Vec<_>, _>>()?;

        log::info!(
            "Created RaftManager with persistent storage: {} user shards, {} shared shards",
            user_shards_count,
            shared_shards_count
        );

        Ok(Self {
            meta,
            node_id: config.node_id,
            user_data_shards,
            shared_data_shards,
            started: RwLock::new(false),
            config,
            user_shards_count,
            shared_shards_count,
        })
    }
    
    /// Get this node's ID
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }
    
    /// Get this node's ID as u64 (for OpenRaft API compatibility)
    pub fn node_id_u64(&self) -> u64 {
        self.node_id.as_u64()
    }

    /// Get OpenRaft metrics for the Meta group
    pub fn meta_metrics(&self) -> Option<RaftMetrics<u64, KalamNode>> {
        self.meta.metrics()
    }
    
    /// Check if the manager has been started
    pub fn is_started(&self) -> bool {
        *self.started.read()
    }
    
    /// Start all Raft groups
    ///
    /// This initializes all Raft groups and begins participating in consensus.
    pub async fn start(&self) -> Result<(), RaftError> {
        if self.is_started() {
            log::warn!("RaftManager already started, skipping");
            return Ok(());
        }
        
        log::info!("Starting Raft Cluster: node={} rpc={} api={}", self.node_id, self.config.rpc_addr, self.config.api_addr);
        // log::info!("Groups: {} (1 meta + {}u + {}s) │ Peers: {}", 
        //     self.group_count(), self.user_shards_count, self.shared_shards_count, 
        //     self.config.peers.len());
        for peer in &self.config.peers {
            log::info!("[CLUSTER] Peer node_id={}: rpc={}, api={}", 
                peer.node_id, peer.rpc_addr, peer.api_addr);
        }
        
        // Register peers from config
        log::debug!("Registering {} peers...", self.config.peers.len());
        for peer in &self.config.peers {
            self.register_peer(peer.node_id, peer.rpc_addr.clone(), peer.api_addr.clone());
        }
        
        // Start unified meta group
        log::debug!("Starting unified meta group...");
        self.meta.start(self.node_id.as_u64(), &self.config).await?;
        log::debug!("  ✓ Meta group started");
        
        // Start all user data shards
        log::debug!("Starting {} user data shards...", self.user_data_shards.len());
        for (i, shard) in self.user_data_shards.iter().enumerate() {
            shard.start(self.node_id.as_u64(), &self.config).await?;
            log::debug!("  ✓ UserDataShard[{}] started", i);
        }
        
        // Start all shared data shards
        log::debug!("Starting {} shared data shards...", self.shared_data_shards.len());
        for (i, shard) in self.shared_data_shards.iter().enumerate() {
            shard.start(self.node_id.as_u64(), &self.config).await?;
            log::debug!("  ✓ SharedDataShard[{}] started", i);
        }
        
        // Mark as started
        {
            let mut started = self.started.write();
            *started = true;
        }
        
        log::debug!("✓ Raft cluster started: {} groups on node {}", self.group_count(), self.node_id);
        Ok(())
    }
    
    /// Initialize the cluster (call on first node only)
    ///
    /// This bootstraps all Raft groups with initial membership containing this node.
    pub async fn initialize_cluster(&self) -> Result<(), RaftError> {
        if !self.is_started() {
            return Err(RaftError::NotStarted("RaftManager not started".to_string()));
        }

        // IMPORTANT:
        // - OpenRaft cluster initialization is a one-time operation.
        // - On restart, a node should NOT re-run initialize() or membership changes.
        // We detect whether this node has already initialized the meta group by checking
        // whether OpenRaft has applied any log entry (including membership entries).
        // When last_applied is Some(..), the Raft state is persisted and initialization
        // has already happened at least once.
        let meta_metrics = self.meta.metrics();
        let meta_last_applied = meta_metrics
            .as_ref()
            .and_then(|m| m.last_applied.map(|log_id| log_id.index))
            .unwrap_or(0);
        let meta_voters: BTreeSet<u64> = meta_metrics
            .as_ref()
            .map(|m| m.membership_config.voter_ids().collect())
            .unwrap_or_default();

        let already_initialized = meta_last_applied > 0;
        
        let self_node = KalamNode {
            rpc_addr: self.config.rpc_addr.clone(),
            api_addr: self.config.api_addr.clone(),
        };

        if already_initialized {
            log::info!(
                "Cluster already initialized (meta last_applied={}); skipping group initialization",
                meta_last_applied
            );
        } else {
            log::info!(
                "Bootstrapping cluster: node {} as LEADER for {} groups",
                self.node_id,
                self.group_count()
            );

            // Initialize unified meta group
            log::debug!("Initializing unified meta group...");
            self.meta.initialize(self.node_id.as_u64(), self_node.clone()).await?;
            log::debug!("  ✓ Meta initialized");

            log::debug!("Initializing user data shards...");
            for (i, shard) in self.user_data_shards.iter().enumerate() {
                shard.initialize(self.node_id.as_u64(), self_node.clone()).await?;
                log::debug!("  ✓ UserDataShard[{}] initialized", i);
            }

            log::debug!("Initializing shared data shards...");
            for (i, shard) in self.shared_data_shards.iter().enumerate() {
                shard.initialize(self.node_id.as_u64(), self_node.clone()).await?;
                log::debug!("  ✓ SharedDataShard[{}] initialized", i);
            }

            log::info!(
                "✓ Cluster initialized: node {} is LEADER for all {} groups",
                self.node_id,
                self.group_count()
            );
        }
        
        // After initialization, wait for peer nodes to come online before adding them to the cluster.
        // This prevents OpenRaft from generating thousands of connection errors when trying to
        // replicate to offline nodes. We wait for each peer's RPC endpoint to respond before
        // calling add_node(), which ensures a clean cluster formation with minimal error logs.
        let should_attempt_peer_join = if !already_initialized {
            // First boot: always attempt to add configured peers.
            true
        } else {
            // Restart: only attempt to continue initial formation if the cluster is still
            // single-node (only this node is a voter in the meta group).
            meta_voters.len() == 1 && meta_voters.contains(&self.node_id.as_u64())
        };

        if should_attempt_peer_join && !self.config.peers.is_empty() {
            // Only perform membership operations when we lead all groups.
            let leader_for_all_groups = self.meta.is_leader()
                && self.user_data_shards.iter().all(|g| g.is_leader())
                && self.shared_data_shards.iter().all(|g| g.is_leader());

            if !leader_for_all_groups {
                log::info!(
                    "Skipping peer join: node {} is not leader for all groups",
                    self.node_id
                );
                return Ok(());
            }

            log::info!("Waiting for {} peer nodes to come online...", self.config.peers.len());
            
            const MAX_RETRIES: u32 = 60;  // 60 retries × 0.5s initial = ~30s max wait
            const INITIAL_DELAY_MS: u64 = 500;
            const MAX_DELAY_MS: u64 = 2000;
            
            for peer in &self.config.peers {
                log::info!("  Waiting for peer node_id={} (rpc={}) to be online...", 
                    peer.node_id, peer.rpc_addr);
                
                // Wait for the peer's RPC endpoint to respond
                match self.wait_for_peer_online(&peer.rpc_addr, MAX_RETRIES, INITIAL_DELAY_MS, MAX_DELAY_MS).await {
                    Ok(_) => {
                        log::info!("    ✓ Peer {} is online, adding to cluster...", peer.node_id);
                        
                        // Now add the node - should succeed immediately since it's online
                        match self.add_node(peer.node_id, peer.rpc_addr.clone(), peer.api_addr.clone()).await {
                            Ok(_) => {
                                log::info!("    ✓ Peer {} joined cluster successfully", peer.node_id);
                            }
                            Err(e) => {
                                log::error!("    ✗ Failed to add peer {} to cluster: {}", peer.node_id, e);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("    ✗ Peer {} did not come online: {}", peer.node_id, e);
                    }
                }
            }
            log::info!("Cluster formation complete");
        }
        
        Ok(())
    }
    
    /// Wait for a peer node to be online and ready to join the cluster
    /// 
    /// This checks if the peer's RPC endpoint is responding before attempting to add it.
    /// This prevents OpenRaft from generating thousands of connection errors when trying
    /// to replicate to an offline node.
    async fn wait_for_peer_online(&self, rpc_addr: &str, max_retries: u32, initial_delay_ms: u64, max_delay_ms: u64) -> Result<(), RaftError> {
        let mut attempt = 0;
        let mut delay_ms = initial_delay_ms;
        
        loop {
            attempt += 1;
            
            // Try to connect to the peer's RPC endpoint using tonic
            let uri = format!("http://{}", rpc_addr);
            match tonic::transport::Endpoint::from_shared(uri.clone())
                .map_err(|e| RaftError::Internal(format!("Invalid RPC address {}: {}", rpc_addr, e)))?
                .connect()
                .await
            {
                Ok(_channel) => {
                    // Connection successful - peer is online
                    log::debug!("[CLUSTER] Peer {} is online and accepting connections", rpc_addr);
                    return Ok(());
                }
                Err(_) => {
                    // Connection failed - peer not ready
                    if attempt >= max_retries {
                        return Err(RaftError::Internal(format!("Peer {} not reachable after {} attempts", rpc_addr, max_retries)));
                    }
                    if attempt == 1 || attempt % 10 == 0 {
                        log::debug!("[CLUSTER] Waiting for peer {} to be online (attempt {}/{})...", rpc_addr, attempt, max_retries);
                    }
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    delay_ms = (delay_ms * 2).min(max_delay_ms);
                }
            }
        }
    }
    
    /// Add a new node to the cluster
    pub async fn add_node(&self, node_id: NodeId, rpc_addr: String, api_addr: String) -> Result<(), RaftError> {
        let node_id_u64 = node_id.as_u64();
        log::info!("[CLUSTER] Node {} joining cluster (rpc={}, api={})", node_id, rpc_addr, api_addr);
        let node = KalamNode { rpc_addr: rpc_addr.clone(), api_addr: api_addr.clone() };

        async fn add_learner_and_wait<SM: KalamStateMachine + Send + Sync + 'static>(
            group: &Arc<RaftGroup<SM>>,
            node_id_u64: u64,
            node: &KalamNode,
            timeout: Duration,
        ) -> Result<(), RaftError> {
            if !group.is_leader() {
                return Err(RaftError::not_leader(
                    group.group_id().to_string(),
                    group.current_leader(),
                ));
            }
            group.add_learner(node_id_u64, node.clone()).await?;
            group.wait_for_learner_catchup(node_id_u64, timeout).await?;
            Ok(())
        }

        async fn promote_learner<SM: KalamStateMachine + Send + Sync + 'static>(
            group: &Arc<RaftGroup<SM>>,
            node_id_u64: u64,
        ) -> Result<(), RaftError> {
            group.promote_learner(node_id_u64).await
        }
        
        // Add to all groups as learner first
        log::info!("[CLUSTER] Adding node {} as learner to all {} Raft groups...", node_id, self.group_count());
        
        // Add to unified meta group
        add_learner_and_wait(&self.meta, node_id_u64, &node, self.config.replication_timeout).await?;
        
        for shard in &self.user_data_shards {
            add_learner_and_wait(shard, node_id_u64, &node, self.config.replication_timeout).await?;
        }
        
        for shard in &self.shared_data_shards {
            add_learner_and_wait(shard, node_id_u64, &node, self.config.replication_timeout).await?;
        }

        log::info!(
            "[CLUSTER] Promoting node {} to voter on all groups...",
            node_id
        );
        
        // Promote on unified meta group
        promote_learner(&self.meta, node_id_u64).await?;

        for shard in &self.user_data_shards {
            promote_learner(shard, node_id_u64).await?;
        }
        for shard in &self.shared_data_shards {
            promote_learner(shard, node_id_u64).await?;
        }
        
        log::info!("[CLUSTER] ✓ Node {} joined cluster successfully (added to {} groups)", 
            node_id, self.group_count());
        Ok(())
    }
    
    /// Get the cluster configuration
    pub fn config(&self) -> &RaftManagerConfig {
        &self.config
    }
    
    /// Get the group for a given GroupId
    fn _get_group_id(&self, group_id: GroupId) -> Result<(), RaftError> {
        match group_id {
            GroupId::Meta => Ok(()),
            GroupId::DataUserShard(shard) if shard < self.user_shards_count => Ok(()),
            GroupId::DataSharedShard(shard) if shard < self.shared_shards_count => Ok(()),
            _ => Err(RaftError::InvalidGroup(group_id.to_string())),
        }
    }
    
    /// Check if this node is leader for a given group
    pub fn is_leader(&self, group_id: GroupId) -> bool {
        match group_id {
            GroupId::Meta => self.meta.is_leader(),
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
    pub fn current_leader(&self, group_id: GroupId) -> Option<NodeId> {
        let leader_u64 = match group_id {
            GroupId::Meta => self.meta.current_leader(),
            GroupId::DataUserShard(shard) if shard < self.user_shards_count => {
                self.user_data_shards[shard as usize].current_leader()
            }
            GroupId::DataSharedShard(shard) if shard < self.shared_shards_count => {
                self.shared_data_shards[shard as usize].current_leader()
            }
            _ => None,
        };
        leader_u64.map(NodeId::from)
    }
    
    /// Get the current Meta group's last applied index
    /// 
    /// This is the watermark used for Meta→Data ordering:
    /// - Leaders stamp data commands with this index at proposal time
    /// - Followers buffer data commands until their local meta catches up
    pub fn current_meta_index(&self) -> u64 {
        self.meta.storage().state_machine().last_applied_index()
    }
    
    /// Internal helper to propose to a group with leader forwarding
    async fn propose_to_group<SM: crate::state_machine::KalamStateMachine + Send + Sync + 'static>(
        &self,
        group: &Arc<RaftGroup<SM>>,
        command: Vec<u8>,
    ) -> Result<Vec<u8>, RaftError> {
        // Use standard quorum-based replication (OpenRaft default)
        group.propose_with_forward(command).await
    }
    
    /// Propose a command to the unified Meta group (with leader forwarding)
    ///
    /// If this node is a follower, the request is automatically forwarded to the leader.
    /// Uses standard quorum-based replication.
    pub async fn propose_meta(&self, command: Vec<u8>) -> Result<Vec<u8>, RaftError> {
        self.propose_to_group(&self.meta, command).await
    }
    
    /// Propose a command to a user data shard (with leader forwarding)
    ///
    /// If this node is a follower, the request is automatically forwarded to the leader.
    /// Uses standard quorum-based replication.
    pub async fn propose_user_data(&self, shard: u32, command: Vec<u8>) -> Result<Vec<u8>, RaftError> {
        if shard >= self.user_shards_count {
            return Err(RaftError::InvalidGroup(format!("DataUserShard({})", shard)));
        }
        self.propose_to_group(&self.user_data_shards[shard as usize], command).await
    }
    
    /// Propose a command to a shared data shard (with leader forwarding)
    ///
    /// If this node is a follower, the request is automatically forwarded to the leader.
    /// Uses standard quorum-based replication.
    pub async fn propose_shared_data(&self, shard: u32, command: Vec<u8>) -> Result<Vec<u8>, RaftError> {
        if shard >= self.shared_shards_count {
            return Err(RaftError::InvalidGroup(format!("DataSharedShard({})", shard)));
        }
        self.propose_to_group(&self.shared_data_shards[shard as usize], command).await
    }
    
    /// Propose a command to any group by GroupId (for RPC server handling)
    ///
    /// Used by the RaftService when receiving a forwarded proposal.
    /// Does NOT forward - should only be called when we are the leader.
    /// Uses standard quorum-based replication.
    pub async fn propose_for_group(&self, group_id: GroupId, command: Vec<u8>) -> Result<Vec<u8>, RaftError> {
        let (data, _log_index) = self.propose_for_group_with_index(group_id, command).await?;
        Ok(data)
    }
    
    /// Propose a command to any group and return both response data and log index
    ///
    /// Used by the RaftService when receiving a forwarded proposal.
    /// Does NOT forward - should only be called when we are the leader.
    /// Returns (response_data, log_index) for read-your-writes consistency.
    pub async fn propose_for_group_with_index(&self, group_id: GroupId, command: Vec<u8>) -> Result<(Vec<u8>, u64), RaftError> {
        match group_id {
            GroupId::Meta => self.meta.propose_with_index(command).await,
            GroupId::DataUserShard(shard) => {
                if shard >= self.user_shards_count {
                    return Err(RaftError::InvalidGroup(format!("DataUserShard({})", shard)));
                }
                self.user_data_shards[shard as usize].propose_with_index(command).await
            }
            GroupId::DataSharedShard(shard) => {
                if shard >= self.shared_shards_count {
                    return Err(RaftError::InvalidGroup(format!("DataSharedShard({})", shard)));
                }
                self.shared_data_shards[shard as usize].propose_with_index(command).await
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
    pub fn register_peer(&self, node_id: NodeId, rpc_addr: String, api_addr: String) {
        let node_id_u64 = node_id.as_u64();
        let node = KalamNode { rpc_addr, api_addr };
        
        // Register with unified meta group
        self.meta.register_peer(node_id_u64, node.clone());
        
        // Register with all data shards
        for shard in &self.user_data_shards {
            shard.register_peer(node_id_u64, node.clone());
        }
        for shard in &self.shared_data_shards {
            shard.register_peer(node_id_u64, node.clone());
        }
    }
    
    /// Get all group IDs
    pub fn all_group_ids(&self) -> Vec<GroupId> {
        let mut groups = vec![GroupId::Meta];
        
        for shard in 0..self.user_shards_count {
            groups.push(GroupId::DataUserShard(shard));
        }
        
        for shard in 0..self.shared_shards_count {
            groups.push(GroupId::DataSharedShard(shard));
        }
        
        groups
    }
    
    /// Get the total number of groups (1 meta + user shards + shared shards)
    pub fn group_count(&self) -> usize {
        1 + self.user_shards_count as usize + self.shared_shards_count as usize
    }
    
    /// Get the number of user data shards
    pub fn user_shards(&self) -> u32 {
        self.user_shards_count
    }
    
    /// Get the number of shared data shards
    pub fn shared_shards(&self) -> u32 {
        self.shared_shards_count
    }
    
    /// Get the unified Meta group
    pub fn meta(&self) -> &Arc<RaftGroup<MetaStateMachine>> {
        &self.meta
    }
    
    /// Get a user data shard
    pub fn user_data_shard(&self, shard: u32) -> Option<&Arc<RaftGroup<UserDataStateMachine>>> {
        self.user_data_shards.get(shard as usize)
    }
    
    /// Get a shared data shard
    pub fn shared_data_shard(&self, shard: u32) -> Option<&Arc<RaftGroup<SharedDataStateMachine>>> {
        self.shared_data_shards.get(shard as usize)
    }
    
    /// Set the meta applier for persisting unified metadata to providers
    ///
    /// This should be called after RaftManager creation once providers are available.
    /// The applier will be called on all nodes (leader and followers) when commands
    /// are applied, ensuring consistent state across the cluster.
    pub fn set_meta_applier(&self, applier: std::sync::Arc<dyn crate::applier::MetaApplier>) {
        let sm = self.meta.storage().state_machine();
        sm.set_applier(applier);
        log::info!("RaftManager: Meta applier registered for metadata replication");
    }

    /// Set the user data applier for persisting per-user data to providers
    ///
    /// This should be called after RaftManager creation once providers are available.
    /// The same applier is used for all user data shards.
    pub fn set_user_data_applier(&self, applier: std::sync::Arc<dyn crate::applier::UserDataApplier>) {
        for (shard_id, shard) in self.user_data_shards.iter().enumerate() {
            let sm = shard.storage().state_machine();
            sm.set_applier(applier.clone());
            log::debug!("RaftManager: User data applier set for shard {}", shard_id);
        }
        log::debug!(
            "RaftManager: User data applier registered for {} shards",
            self.user_data_shards.len()
        );
    }

    /// Set the shared data applier for persisting shared data to providers
    ///
    /// This should be called after RaftManager creation once providers are available.
    /// The same applier is used for all shared data shards.
    pub fn set_shared_data_applier(&self, applier: std::sync::Arc<dyn crate::applier::SharedDataApplier>) {
        for (shard_id, shard) in self.shared_data_shards.iter().enumerate() {
            let sm = shard.storage().state_machine();
            sm.set_applier(applier.clone());
            log::debug!("RaftManager: Shared data applier set for shard {}", shard_id);
        }
        log::debug!(
            "RaftManager: Shared data applier registered for {} shards",
            self.shared_data_shards.len()
        );
    }
    
    // === Raft RPC Handlers (for receiving RPCs from other nodes) ===
    
    /// Get the Raft instance for a specific group
    fn get_raft_instance(&self, group_id: GroupId) -> Result<crate::manager::raft_group::RaftInstance, RaftError> {
        match group_id {
            GroupId::Meta => self.meta.raft()
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
    /// 2. Shuts down all Raft groups (calls OpenRaft's Raft::shutdown())
    /// 3. Marks the manager as stopped
    pub async fn shutdown(&self) -> Result<(), RaftError> {
        if !self.is_started() {
            log::warn!("RaftManager not started, nothing to shutdown");
            return Ok(());
        }
        
        log::info!("╔═══════════════════════════════════════════════════════════════════╗");
        log::info!("║           Graceful Cluster Shutdown Starting                      ║");
        log::info!("╚═══════════════════════════════════════════════════════════════════╝");
        log::info!("[CLUSTER] Node {} leaving cluster...", self.node_id);
        
        // Count how many groups we're leading
        let mut groups_leading = 0;
        for group_id in self.all_group_ids() {
            if self.is_leader(group_id) {
                groups_leading += 1;
            }
        }
        
        if groups_leading > 0 {
            log::info!("[CLUSTER] This node is LEADER of {} groups, attempting leadership transfer...", groups_leading);
            
            // Attempt leadership transfer for each group where we're leader
            // Find the first available peer to transfer leadership to
            if let Some(target_node) = self.config.peers.first() {
                let target_node_id = target_node.node_id.as_u64();
                log::info!("[CLUSTER] Transferring leadership to node {}...", target_node.node_id);
                
                // Transfer leadership for Meta group
                if self.meta.is_leader() {
                    match self.meta.transfer_leadership(target_node_id).await {
                        Ok(_) => log::info!("[CLUSTER] ✓ Meta leadership transferred to node {}", target_node.node_id),
                        Err(e) => log::warn!("[CLUSTER] ⚠ Failed to transfer Meta leadership: {}", e),
                    }
                }
                
                // Transfer leadership for user data shards
                for (i, shard) in self.user_data_shards.iter().enumerate() {
                    if shard.is_leader() {
                        match shard.transfer_leadership(target_node_id).await {
                            Ok(_) => log::debug!("[CLUSTER] ✓ UserDataShard[{}] leadership transferred", i),
                            Err(e) => log::warn!("[CLUSTER] ⚠ Failed to transfer UserDataShard[{}] leadership: {}", i, e),
                        }
                    }
                }
                
                // Transfer leadership for shared data shards
                for (i, shard) in self.shared_data_shards.iter().enumerate() {
                    if shard.is_leader() {
                        match shard.transfer_leadership(target_node_id).await {
                            Ok(_) => log::debug!("[CLUSTER] ✓ SharedDataShard[{}] leadership transferred", i),
                            Err(e) => log::warn!("[CLUSTER] ⚠ Failed to transfer SharedDataShard[{}] leadership: {}", i, e),
                        }
                    }
                }
                
                // Give time for leadership transfer to complete
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                log::info!("[CLUSTER] Leadership transfer completed");
            } else {
                log::warn!("[CLUSTER] No peers available for leadership transfer - cluster may experience brief unavailability");
            }
        }
        
        // Shutdown all Raft groups (calls OpenRaft's Raft::shutdown())
        log::info!("[CLUSTER] Shutting down all Raft groups...");
        
        // Shutdown Meta group
        if let Err(e) = self.meta.shutdown().await {
            log::warn!("[CLUSTER] ⚠ Failed to shutdown Meta group: {}", e);
        }
        
        // Shutdown user data shards
        for (i, shard) in self.user_data_shards.iter().enumerate() {
            if let Err(e) = shard.shutdown().await {
                log::warn!("[CLUSTER] ⚠ Failed to shutdown UserDataShard[{}]: {}", i, e);
            }
        }
        
        // Shutdown shared data shards
        for (i, shard) in self.shared_data_shards.iter().enumerate() {
            if let Err(e) = shard.shutdown().await {
                log::warn!("[CLUSTER] ⚠ Failed to shutdown SharedDataShard[{}]: {}", i, e);
            }
        }
        
        log::info!("[CLUSTER] All Raft groups shutdown complete");
        
        // Mark as stopped
        {
            let mut started = self.started.write();
            *started = false;
        }
        
        log::info!("╔═══════════════════════════════════════════════════════════════════╗");
        log::info!("║   [CLUSTER] Node {} Left Cluster Successfully                     ║", self.node_id);
        log::info!("╚═══════════════════════════════════════════════════════════════════╝");
        
        Ok(())
    }
    
    /// Get the replication timeout (used for learner catchup)
    pub fn replication_timeout(&self) -> Duration {
        self.config.replication_timeout
    }
    
    /// Get the total number of cluster nodes (self + peers)
    pub fn total_nodes(&self) -> usize {
        1 + self.config.peers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::models::{NamespaceId, TableName};

    fn test_config() -> RaftManagerConfig {
        RaftManagerConfig {
            node_id: NodeId::new(1),
            rpc_addr: "127.0.0.1:5001".to_string(),
            api_addr: "127.0.0.1:3001".to_string(),
            peers: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn test_raft_manager_creation() {
        let manager = RaftManager::new(test_config());
        
        assert_eq!(manager.node_id(), NodeId::new(1));
        assert!(!manager.is_started());
        
        // Should have 34 groups total by default: 1 meta + 32 user data + 1 shared
        assert_eq!(manager.group_count(), 34);
    }

    #[test]
    fn test_all_group_ids() {
        let manager = RaftManager::new(test_config());
        let groups = manager.all_group_ids();
        
        assert_eq!(groups.len(), 34);
        assert!(groups.contains(&GroupId::Meta));
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
            NodeId::new(2),
            "127.0.0.1:5002".to_string(),
            "127.0.0.1:3002".to_string(),
        );
    }

    #[test]
    fn test_is_leader_before_start() {
        let manager = RaftManager::new(test_config());
        
        // Before start, no group should have a leader
        assert!(!manager.is_leader(GroupId::Meta));
        assert!(!manager.is_leader(GroupId::DataUserShard(0)));
        assert!(!manager.is_leader(GroupId::DataSharedShard(0)));
    }

    #[test]
    fn test_current_leader_before_start() {
        let manager = RaftManager::new(test_config());
        
        // Before start, no leader should be known
        assert!(manager.current_leader(GroupId::Meta).is_none());
        assert!(manager.current_leader(GroupId::DataUserShard(0)).is_none());
    }
}
