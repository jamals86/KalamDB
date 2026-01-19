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
use crate::state_machine::KalamStateMachine;
use crate::state_machine::{MetaStateMachine, SharedDataStateMachine, UserDataStateMachine};
use crate::storage::KalamNode;
use crate::{GroupId, RaftError};

/// Information about a snapshot operation result
#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    /// The Raft group ID
    pub group_id: GroupId,
    /// The snapshot index (if available)
    pub snapshot_index: Option<u64>,
    /// Whether the snapshot was triggered successfully
    pub success: bool,
    /// Error message if the snapshot failed
    pub error: Option<String>,
}

/// Information about a cluster action result
#[derive(Debug, Clone)]
pub struct ClusterActionResult {
    /// The Raft group ID
    pub group_id: GroupId,
    /// Whether the action was successful
    pub success: bool,
    /// Error message if the action failed
    pub error: Option<String>,
}

/// Summary of all snapshots in the cluster
#[derive(Debug, Clone)]
pub struct SnapshotsSummary {
    /// Total number of Raft groups
    pub total_groups: usize,
    /// Number of groups with snapshots
    pub groups_with_snapshots: usize,
    /// Directory where snapshots are stored
    pub snapshots_dir: String,
    /// Details for each group
    pub group_details: Vec<(GroupId, Option<u64>)>,
}

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
        let meta = Arc::new(RaftGroup::new(GroupId::Meta, MetaStateMachine::new()));

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
            backend.create_partition(&partition).map_err(|e| {
                RaftError::Storage(format!("Failed to create raft partition: {}", e))
            })?;
        }

        std::fs::create_dir_all(&snapshots_dir).map_err(|e| {
            RaftError::Storage(format!(
                "Failed to create snapshots directory {}: {}",
                snapshots_dir.display(),
                e
            ))
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

        log::debug!(
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

    /// Get OpenRaft metrics for a specific Raft group (if started)
    pub fn group_metrics(&self, group_id: crate::GroupId) -> Option<RaftMetrics<u64, KalamNode>> {
        match group_id {
            crate::GroupId::Meta => self.meta.metrics(),
            crate::GroupId::DataUserShard(shard) if shard < self.user_shards_count => {
                self.user_data_shards[shard as usize].metrics()
            },
            crate::GroupId::DataSharedShard(shard) if shard < self.shared_shards_count => {
                self.shared_data_shards[shard as usize].metrics()
            },
            _ => None,
        }
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

        log::info!(
            "Starting Raft Cluster: node={} rpc={} api={}",
            self.node_id,
            self.config.rpc_addr,
            self.config.api_addr
        );
        // log::info!("Groups: {} (1 meta + {}u + {}s) │ Peers: {}",
        //     self.group_count(), self.user_shards_count, self.shared_shards_count,
        //     self.config.peers.len());
        for peer in &self.config.peers {
            log::info!(
                "[CLUSTER] Peer node_id={}: rpc={}, api={}",
                peer.node_id,
                peer.rpc_addr,
                peer.api_addr
            );
        }

        // Register this node for leader forwarding (covers self-forward when leader detection lags).
        self.register_peer(
            self.node_id,
            self.config.rpc_addr.clone(),
            self.config.api_addr.clone(),
        );

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

        log::debug!(
            "✓ Raft cluster started: {} groups on node {}",
            self.group_count(),
            self.node_id
        );
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
        //
        // We detect if this is a restart by checking if we have persisted Raft state
        // in the storage layer. This is more reliable than checking OpenRaft metrics
        // because metrics may not be immediately available after Raft::new().
        //
        // The storage-level check looks for: vote, last_applied, committed, or log entries.
        // If any of these exist, the cluster was previously initialized.
        let already_initialized = self.meta.has_persisted_state();

        // Get membership info from metrics (for peer join logic)
        let meta_metrics = self.meta.metrics();
        let meta_voters: BTreeSet<u64> = meta_metrics
            .as_ref()
            .map(|m| m.membership_config.voter_ids().collect())
            .unwrap_or_default();

        // Create self node with auto-detected system metadata (hostname, version, memory, os, arch)
        let self_node = KalamNode::with_auto_metadata(
            self.config.rpc_addr.clone(),
            self.config.api_addr.clone(),
        );

        if already_initialized {
            let meta_last_applied = self.meta.get_last_applied().map(|id| id.index).unwrap_or(0);
            log::info!(
                "Cluster already initialized (meta last_applied={}); skipping group initialization",
                meta_last_applied
            );
        } else {
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
            let peers = self.config.peers.clone();
            let meta = self.meta.clone();
            let user_data_shards = self.user_data_shards.clone();
            let shared_data_shards = self.shared_data_shards.clone();
            let replication_timeout = self.config.replication_timeout;
            let node_id = self.node_id;

            tokio::spawn(async move {
                // Only perform membership operations when we lead all groups.
                let leader_for_all_groups = meta.is_leader()
                    && user_data_shards.iter().all(|g| g.is_leader())
                    && shared_data_shards.iter().all(|g| g.is_leader());

                if !leader_for_all_groups {
                    log::info!("Skipping peer join: node {} is not leader for all groups", node_id);
                    return;
                }

                log::info!("Waiting for {} peer nodes to come online...", peers.len());

                const MAX_RETRIES: u32 = 60; // 60 retries × 0.5s initial = ~30s max wait
                const INITIAL_DELAY_MS: u64 = 500;
                const MAX_DELAY_MS: u64 = 2000;

                for peer in &peers {
                    log::info!(
                        "  Waiting for peer node_id={} (rpc={}) to be online...",
                        peer.node_id,
                        peer.rpc_addr
                    );

                    // Wait for the peer's RPC endpoint to respond
                    match RaftManager::wait_for_peer_online(
                        &peer.rpc_addr,
                        MAX_RETRIES,
                        INITIAL_DELAY_MS,
                        MAX_DELAY_MS,
                    )
                    .await
                    {
                        Ok(_) => {
                            log::info!(
                                "    ✓ Peer {} is online, adding to cluster...",
                                peer.node_id
                            );

                            // Now add the node - should succeed immediately since it's online
                            match RaftManager::add_node_with_groups(
                                peer.node_id,
                                peer.rpc_addr.clone(),
                                peer.api_addr.clone(),
                                meta.clone(),
                                user_data_shards.clone(),
                                shared_data_shards.clone(),
                                replication_timeout,
                            )
                            .await
                            {
                                Ok(_) => {
                                    log::info!(
                                        "    ✓ Peer {} joined cluster successfully",
                                        peer.node_id
                                    );
                                },
                                Err(e) => {
                                    log::error!(
                                        "    ✗ Failed to add peer {} to cluster: {}",
                                        peer.node_id,
                                        e
                                    );
                                },
                            }
                        },
                        Err(e) => {
                            log::error!("    ✗ Peer {} did not come online: {}", peer.node_id, e);
                        },
                    }
                }
                log::info!("Cluster formation complete");
            });
        }

        Ok(())
    }

    /// Wait for a peer node to be online and ready to join the cluster
    ///
    /// This checks if the peer's RPC endpoint is responding before attempting to add it.
    /// This prevents OpenRaft from generating thousands of connection errors when trying
    /// to replicate to an offline node.
    async fn wait_for_peer_online(
        rpc_addr: &str,
        max_retries: u32,
        initial_delay_ms: u64,
        max_delay_ms: u64,
    ) -> Result<(), RaftError> {
        let mut attempt = 0;
        let mut delay_ms = initial_delay_ms;

        loop {
            attempt += 1;

            // Try to connect to the peer's RPC endpoint using tonic
            let uri = format!("http://{}", rpc_addr);
            match tonic::transport::Endpoint::from_shared(uri.clone())
                .map_err(|e| {
                    RaftError::Internal(format!("Invalid RPC address {}: {}", rpc_addr, e))
                })?
                .connect()
                .await
            {
                Ok(_channel) => {
                    // Connection successful - peer is online
                    log::debug!("[CLUSTER] Peer {} is online and accepting connections", rpc_addr);
                    return Ok(());
                },
                Err(_) => {
                    // Connection failed - peer not ready
                    if attempt >= max_retries {
                        return Err(RaftError::Internal(format!(
                            "Peer {} not reachable after {} attempts",
                            rpc_addr, max_retries
                        )));
                    }
                    if attempt == 1 || attempt % 10 == 0 {
                        log::debug!(
                            "[CLUSTER] Waiting for peer {} to be online (attempt {}/{})...",
                            rpc_addr,
                            attempt,
                            max_retries
                        );
                    }
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    delay_ms = (delay_ms * 2).min(max_delay_ms);
                },
            }
        }
    }

    async fn add_node_with_groups(
        node_id: NodeId,
        rpc_addr: String,
        api_addr: String,
        meta: Arc<RaftGroup<MetaStateMachine>>,
        user_data_shards: Vec<Arc<RaftGroup<UserDataStateMachine>>>,
        shared_data_shards: Vec<Arc<RaftGroup<SharedDataStateMachine>>>,
        replication_timeout: Duration,
    ) -> Result<(), RaftError> {
        let node_id_u64 = node_id.as_u64();
        log::info!(
            "[CLUSTER] Node {} joining cluster (rpc={}, api={})",
            node_id,
            rpc_addr,
            api_addr
        );
        // Note: Node metadata is not available during add_learner (only rpc/api addrs)
        // The node's own metadata is populated when it initializes via with_auto_metadata
        let node = KalamNode::new(rpc_addr.clone(), api_addr.clone());

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
        log::info!(
            "[CLUSTER] Adding node {} as learner to all {} Raft groups...",
            node_id,
            1 + user_data_shards.len() + shared_data_shards.len()
        );

        // Add to unified meta group
        add_learner_and_wait(&meta, node_id_u64, &node, replication_timeout).await?;

        for shard in &user_data_shards {
            add_learner_and_wait(shard, node_id_u64, &node, replication_timeout).await?;
        }

        for shard in &shared_data_shards {
            add_learner_and_wait(shard, node_id_u64, &node, replication_timeout).await?;
        }

        log::info!("[CLUSTER] Promoting node {} to voter on all groups...", node_id);

        promote_learner(&meta, node_id_u64).await?;

        for shard in &user_data_shards {
            promote_learner(shard, node_id_u64).await?;
        }

        for shard in &shared_data_shards {
            promote_learner(shard, node_id_u64).await?;
        }

        Ok(())
    }

    /// Add a new node to the cluster
    pub async fn add_node(
        &self,
        node_id: NodeId,
        rpc_addr: String,
        api_addr: String,
    ) -> Result<(), RaftError> {
        let node_id_u64 = node_id.as_u64();
        log::info!(
            "[CLUSTER] Node {} joining cluster (rpc={}, api={})",
            node_id,
            rpc_addr,
            api_addr
        );
        // Note: Node metadata is not available during add_learner (only rpc/api addrs)
        // The node's own metadata is populated when it initializes via with_auto_metadata
        let node = KalamNode::new(rpc_addr.clone(), api_addr.clone());

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
        log::info!(
            "[CLUSTER] Adding node {} as learner to all {} Raft groups...",
            node_id,
            self.group_count()
        );

        // Add to unified meta group
        add_learner_and_wait(&self.meta, node_id_u64, &node, self.config.replication_timeout)
            .await?;

        for shard in &self.user_data_shards {
            add_learner_and_wait(shard, node_id_u64, &node, self.config.replication_timeout)
                .await?;
        }

        for shard in &self.shared_data_shards {
            add_learner_and_wait(shard, node_id_u64, &node, self.config.replication_timeout)
                .await?;
        }

        log::info!("[CLUSTER] Promoting node {} to voter on all groups...", node_id);

        // Promote on unified meta group
        promote_learner(&self.meta, node_id_u64).await?;

        for shard in &self.user_data_shards {
            promote_learner(shard, node_id_u64).await?;
        }
        for shard in &self.shared_data_shards {
            promote_learner(shard, node_id_u64).await?;
        }

        log::info!(
            "[CLUSTER] ✓ Node {} joined cluster successfully (added to {} groups)",
            node_id,
            self.group_count()
        );
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
            },
            GroupId::DataSharedShard(shard) if shard < self.shared_shards_count => {
                self.shared_data_shards[shard as usize].is_leader()
            },
            _ => false,
        }
    }

    /// Get the current leader for a group
    pub fn current_leader(&self, group_id: GroupId) -> Option<NodeId> {
        let leader_u64 = match group_id {
            GroupId::Meta => self.meta.current_leader(),
            GroupId::DataUserShard(shard) if shard < self.user_shards_count => {
                self.user_data_shards[shard as usize].current_leader()
            },
            GroupId::DataSharedShard(shard) if shard < self.shared_shards_count => {
                self.shared_data_shards[shard as usize].current_leader()
            },
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
    async fn propose_to_group<
        SM: crate::state_machine::KalamStateMachine + Send + Sync + 'static,
    >(
        &self,
        group: &Arc<RaftGroup<SM>>,
        command: crate::RaftCommand,
    ) -> Result<crate::RaftResponse, RaftError> {
        // Use standard quorum-based replication (OpenRaft default)
        group.propose_with_forward(command).await
    }

    /// Propose a command to the unified Meta group (with leader forwarding)
    ///
    /// If this node is a follower, the request is automatically forwarded to the leader.
    /// Uses standard quorum-based replication.
    pub async fn propose_meta(
        &self,
        command: crate::MetaCommand,
    ) -> Result<crate::MetaResponse, RaftError> {
        let cmd = crate::RaftCommand::Meta(command);
        let response = self.propose_to_group(&self.meta, cmd).await?;
        match response {
            crate::RaftResponse::Meta(r) => Ok(r),
            _ => Err(RaftError::Internal("Unexpected response type for Meta command".to_string())),
        }
    }

    /// Propose a command to a user data shard (with leader forwarding)
    ///
    /// If this node is a follower, the request is automatically forwarded to the leader.
    /// Uses standard quorum-based replication.
    pub async fn propose_user_data(
        &self,
        shard: u32,
        command: crate::UserDataCommand,
    ) -> Result<crate::DataResponse, RaftError> {
        if shard >= self.user_shards_count {
            return Err(RaftError::InvalidGroup(format!("DataUserShard({})", shard)));
        }
        let cmd = crate::RaftCommand::UserData(command);
        let response = self.propose_to_group(&self.user_data_shards[shard as usize], cmd).await?;
        match response {
            crate::RaftResponse::Data(r) => Ok(r),
            _ => Err(RaftError::Internal(
                "Unexpected response type for UserData command".to_string(),
            )),
        }
    }

    /// Propose a command to a shared data shard (with leader forwarding)
    ///
    /// If this node is a follower, the request is automatically forwarded to the leader.
    /// Uses standard quorum-based replication.
    pub async fn propose_shared_data(
        &self,
        shard: u32,
        command: crate::SharedDataCommand,
    ) -> Result<crate::DataResponse, RaftError> {
        if shard >= self.shared_shards_count {
            return Err(RaftError::InvalidGroup(format!("DataSharedShard({})", shard)));
        }
        let cmd = crate::RaftCommand::SharedData(command);
        let response: crate::RaftResponse =
            self.propose_to_group(&self.shared_data_shards[shard as usize], cmd).await?;
        match response {
            crate::RaftResponse::Data(r) => Ok(r),
            _ => Err(RaftError::Internal(
                "Unexpected response type for SharedData command".to_string(),
            )),
        }
    }

    /// Propose a command to any group by GroupId (for RPC server handling)
    ///
    /// Used by the RaftService when receiving a forwarded proposal.
    /// Does NOT forward - should only be called when we are the leader.
    /// Uses standard quorum-based replication.
    pub async fn propose_for_group(
        &self,
        group_id: GroupId,
        command: Vec<u8>,
    ) -> Result<Vec<u8>, RaftError> {
        let (data, _log_index) = self.propose_for_group_with_index(group_id, command).await?;
        Ok(data)
    }

    /// Propose a command to any group and return both response data and log index
    ///
    /// Used by the RaftService when receiving a forwarded proposal.
    /// Does NOT forward - should only be called when we are the leader.
    /// Returns (response_data, log_index) for read-your-writes consistency.
    pub async fn propose_for_group_with_index(
        &self,
        group_id: GroupId,
        command: Vec<u8>,
    ) -> Result<(Vec<u8>, u64), RaftError> {
        // Deserialize the command
        let raft_cmd: crate::RaftCommand = crate::state_machine::serde_helpers::decode(&command)
            .map_err(|e| RaftError::Internal(format!("Failed to deserialize command: {}", e)))?;

        // Route to appropriate group
        let (response, log_index) = match group_id {
            GroupId::Meta => self.meta.propose_with_index(raft_cmd).await?,
            GroupId::DataUserShard(shard) => {
                if shard >= self.user_shards_count {
                    return Err(RaftError::InvalidGroup(format!("DataUserShard({})", shard)));
                }
                self.user_data_shards[shard as usize].propose_with_index(raft_cmd).await?
            },
            GroupId::DataSharedShard(shard) => {
                if shard >= self.shared_shards_count {
                    return Err(RaftError::InvalidGroup(format!("DataSharedShard({})", shard)));
                }
                self.shared_data_shards[shard as usize].propose_with_index(raft_cmd).await?
            },
        };

        // Serialize the response
        let response_bytes = crate::state_machine::serde_helpers::encode(&response)
            .map_err(|e| RaftError::Internal(format!("Failed to serialize response: {}", e)))?;

        Ok((response_bytes, log_index))
    }

    /// Compute the shard ID for a table
    ///
    /// Uses consistent hashing on the table ID to determine shard placement.
    pub fn compute_shard(&self, table_id: &TableId) -> u32 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        table_id.namespace_id().as_str().hash(&mut hasher);
        table_id.table_name().as_str().hash(&mut hasher);
        (hasher.finish() % self.user_shards_count as u64) as u32
    }

    /// Register a peer node with all groups
    pub fn register_peer(&self, node_id: NodeId, rpc_addr: String, api_addr: String) {
        let node_id_u64 = node_id.as_u64();
        let node = KalamNode::new(rpc_addr, api_addr);

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
        log::debug!("RaftManager: Meta applier registered for metadata replication");
    }

    /// Set the user data applier for persisting per-user data to providers
    ///
    /// This should be called after RaftManager creation once providers are available.
    /// The same applier is used for all user data shards.
    pub fn set_user_data_applier(
        &self,
        applier: std::sync::Arc<dyn crate::applier::UserDataApplier>,
    ) {
        for (_shard_id, shard) in self.user_data_shards.iter().enumerate() {
            let sm = shard.storage().state_machine();
            sm.set_applier(applier.clone());
            // log::trace!("RaftManager: User data applier set for shard {}", shard_id);
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
    pub fn set_shared_data_applier(
        &self,
        applier: std::sync::Arc<dyn crate::applier::SharedDataApplier>,
    ) {
        for (_shard_id, shard) in self.shared_data_shards.iter().enumerate() {
            let sm = shard.storage().state_machine();
            sm.set_applier(applier.clone());
            // log::trace!("RaftManager: Shared data applier set for shard {}", shard_id);
        }
        log::debug!(
            "RaftManager: Shared data applier registered for {} shards",
            self.shared_data_shards.len()
        );
    }

    /// Restore all state machines from their persisted snapshots
    ///
    /// This should be called AFTER all appliers are set. It restores the state machines'
    /// internal state from persisted snapshots to ensure idempotency checks work correctly
    /// on restart, preventing duplicate application of log entries.
    ///
    /// Without this, state machines would start with `last_applied_index = 0`, and log
    /// entries would be re-applied even if they were already applied before the restart.
    pub async fn restore_state_machines_from_snapshots(&self) -> Result<(), RaftError> {
        let start = std::time::Instant::now();
        let mut restored_count = 0;

        // Restore meta state machine
        if self.meta.has_snapshot() {
            self.meta.restore_state_machine_from_snapshot().await?;
            restored_count += 1;
        }

        // Restore user data state machines
        for shard in &self.user_data_shards {
            if shard.has_snapshot() {
                shard.restore_state_machine_from_snapshot().await?;
                restored_count += 1;
            }
        }

        // Restore shared data state machines
        for shard in &self.shared_data_shards {
            if shard.has_snapshot() {
                shard.restore_state_machine_from_snapshot().await?;
                restored_count += 1;
            }
        }

        if restored_count > 0 {
            log::info!(
                "RaftManager: Restored {} state machines from snapshots in {:.2}ms",
                restored_count,
                start.elapsed().as_secs_f64() * 1000.0
            );
        }

        Ok(())
    }

    // === Raft RPC Handlers (for receiving RPCs from other nodes) ===

    /// Get the Raft instance for a specific group
    fn get_raft_instance(
        &self,
        group_id: GroupId,
    ) -> Result<crate::manager::raft_group::RaftInstance, RaftError> {
        match group_id {
            GroupId::Meta => self
                .meta
                .raft()
                .ok_or_else(|| RaftError::NotStarted(format!("Group {:?} not started", group_id))),
            GroupId::DataUserShard(shard) => {
                let group = self
                    .user_data_shards
                    .get(shard as usize)
                    .ok_or_else(|| RaftError::Internal(format!("Invalid user shard: {}", shard)))?;
                group.raft().ok_or_else(|| {
                    RaftError::NotStarted(format!("Group {:?} not started", group_id))
                })
            },
            GroupId::DataSharedShard(shard) => {
                let group = self.shared_data_shards.get(shard as usize).ok_or_else(|| {
                    RaftError::Internal(format!("Invalid shared shard: {}", shard))
                })?;
                group.raft().ok_or_else(|| {
                    RaftError::NotStarted(format!("Group {:?} not started", group_id))
                })
            },
        }
    }

    /// Handle incoming vote request
    pub async fn handle_vote(
        &self,
        group_id: GroupId,
        payload: &[u8],
    ) -> Result<Vec<u8>, RaftError> {
        use crate::state_machine::{decode, encode};
        use openraft::raft::VoteRequest;

        let raft = self.get_raft_instance(group_id)?;
        let request: VoteRequest<u64> = decode(payload)?;

        let response = raft
            .vote(request)
            .await
            .map_err(|e| RaftError::Internal(format!("Vote RPC failed: {:?}", e)))?;

        encode(&response)
    }

    /// Handle incoming append entries request
    pub async fn handle_append_entries(
        &self,
        group_id: GroupId,
        payload: &[u8],
    ) -> Result<Vec<u8>, RaftError> {
        use crate::state_machine::{decode, encode};
        use crate::storage::KalamTypeConfig;
        use openraft::raft::AppendEntriesRequest;

        let raft = self.get_raft_instance(group_id)?;
        let request: AppendEntriesRequest<KalamTypeConfig> = decode(payload)?;

        let response = raft
            .append_entries(request)
            .await
            .map_err(|e| RaftError::Internal(format!("AppendEntries RPC failed: {:?}", e)))?;

        encode(&response)
    }

    /// Handle incoming install snapshot request
    pub async fn handle_install_snapshot(
        &self,
        group_id: GroupId,
        payload: &[u8],
    ) -> Result<Vec<u8>, RaftError> {
        use crate::state_machine::{decode, encode};
        use crate::storage::KalamTypeConfig;
        use openraft::raft::InstallSnapshotRequest;

        let raft = self.get_raft_instance(group_id)?;
        let request: InstallSnapshotRequest<KalamTypeConfig> = decode(payload)?;

        let response = raft
            .install_snapshot(request)
            .await
            .map_err(|e| RaftError::Internal(format!("InstallSnapshot RPC failed: {:?}", e)))?;

        encode(&response)
    }

    /// Trigger snapshots for all Raft groups
    ///
    /// Forces OpenRaft to create snapshots for Meta, all User shards, and all Shared shards.
    /// Returns information about the snapshots created.
    pub async fn trigger_all_snapshots(&self) -> Result<Vec<SnapshotInfo>, RaftError> {
        if !self.is_started() {
            return Err(RaftError::NotStarted("RaftManager not started".to_string()));
        }

        let mut results = Vec::new();
        let mut errors = Vec::new();

        // Trigger Meta group snapshot
        log::info!("[SNAPSHOT] Triggering snapshot for Meta group...");
        match self.meta.trigger_snapshot().await {
            Ok(()) => {
                let snapshot_idx = self.meta.snapshot_index();
                results.push(SnapshotInfo {
                    group_id: GroupId::Meta,
                    snapshot_index: snapshot_idx,
                    success: true,
                    error: None,
                });
                log::info!("[SNAPSHOT] ✓ Meta snapshot triggered (index: {:?})", snapshot_idx);
            },
            Err(e) => {
                errors.push(format!("Meta: {}", e));
                results.push(SnapshotInfo {
                    group_id: GroupId::Meta,
                    snapshot_index: None,
                    success: false,
                    error: Some(e.to_string()),
                });
            },
        }

        // Trigger User data shard snapshots
        for (i, shard) in self.user_data_shards.iter().enumerate() {
            let group_id = GroupId::DataUserShard(i as u32);
            match shard.trigger_snapshot().await {
                Ok(()) => {
                    let snapshot_idx = shard.snapshot_index();
                    results.push(SnapshotInfo {
                        group_id,
                        snapshot_index: snapshot_idx,
                        success: true,
                        error: None,
                    });
                    log::debug!(
                        "[SNAPSHOT] ✓ UserShard[{}] snapshot triggered (index: {:?})",
                        i,
                        snapshot_idx
                    );
                },
                Err(e) => {
                    errors.push(format!("UserShard[{}]: {}", i, e));
                    results.push(SnapshotInfo {
                        group_id,
                        snapshot_index: None,
                        success: false,
                        error: Some(e.to_string()),
                    });
                },
            }
        }

        // Trigger Shared data shard snapshots
        for (i, shard) in self.shared_data_shards.iter().enumerate() {
            let group_id = GroupId::DataSharedShard(i as u32);
            match shard.trigger_snapshot().await {
                Ok(()) => {
                    let snapshot_idx = shard.snapshot_index();
                    results.push(SnapshotInfo {
                        group_id,
                        snapshot_index: snapshot_idx,
                        success: true,
                        error: None,
                    });
                    log::debug!(
                        "[SNAPSHOT] ✓ SharedShard[{}] snapshot triggered (index: {:?})",
                        i,
                        snapshot_idx
                    );
                },
                Err(e) => {
                    errors.push(format!("SharedShard[{}]: {}", i, e));
                    results.push(SnapshotInfo {
                        group_id,
                        snapshot_index: None,
                        success: false,
                        error: Some(e.to_string()),
                    });
                },
            }
        }

        let success_count = results.iter().filter(|r| r.success).count();
        let total = results.len();

        if errors.is_empty() {
            log::info!("[SNAPSHOT] All {} snapshots triggered successfully", total);
        } else {
            log::warn!(
                "[SNAPSHOT] Triggered {}/{} snapshots, {} errors: {:?}",
                success_count,
                total,
                errors.len(),
                errors
            );
        }

        Ok(results)
    }

    /// Trigger elections for all Raft groups
    pub async fn trigger_all_elections(&self) -> Result<Vec<ClusterActionResult>, RaftError> {
        let mut results = Vec::new();

        match self.meta.trigger_election().await {
            Ok(_) => results.push(ClusterActionResult {
                group_id: GroupId::Meta,
                success: true,
                error: None,
            }),
            Err(e) => results.push(ClusterActionResult {
                group_id: GroupId::Meta,
                success: false,
                error: Some(format!("{}", e)),
            }),
        }

        for (i, shard) in self.user_data_shards.iter().enumerate() {
            let group_id = GroupId::DataUserShard(i as u32);
            match shard.trigger_election().await {
                Ok(_) => results.push(ClusterActionResult {
                    group_id,
                    success: true,
                    error: None,
                }),
                Err(e) => results.push(ClusterActionResult {
                    group_id,
                    success: false,
                    error: Some(format!("{}", e)),
                }),
            }
        }

        for (i, shard) in self.shared_data_shards.iter().enumerate() {
            let group_id = GroupId::DataSharedShard(i as u32);
            match shard.trigger_election().await {
                Ok(_) => results.push(ClusterActionResult {
                    group_id,
                    success: true,
                    error: None,
                }),
                Err(e) => results.push(ClusterActionResult {
                    group_id,
                    success: false,
                    error: Some(format!("{}", e)),
                }),
            }
        }

        Ok(results)
    }

    /// Purge logs up to the given index for all Raft groups
    pub async fn purge_all_logs(&self, upto: u64) -> Result<Vec<ClusterActionResult>, RaftError> {
        let mut results = Vec::new();

        match self.meta.purge_log(upto).await {
            Ok(_) => results.push(ClusterActionResult {
                group_id: GroupId::Meta,
                success: true,
                error: None,
            }),
            Err(e) => results.push(ClusterActionResult {
                group_id: GroupId::Meta,
                success: false,
                error: Some(format!("{}", e)),
            }),
        }

        for (i, shard) in self.user_data_shards.iter().enumerate() {
            let group_id = GroupId::DataUserShard(i as u32);
            match shard.purge_log(upto).await {
                Ok(_) => results.push(ClusterActionResult {
                    group_id,
                    success: true,
                    error: None,
                }),
                Err(e) => results.push(ClusterActionResult {
                    group_id,
                    success: false,
                    error: Some(format!("{}", e)),
                }),
            }
        }

        for (i, shard) in self.shared_data_shards.iter().enumerate() {
            let group_id = GroupId::DataSharedShard(i as u32);
            match shard.purge_log(upto).await {
                Ok(_) => results.push(ClusterActionResult {
                    group_id,
                    success: true,
                    error: None,
                }),
                Err(e) => results.push(ClusterActionResult {
                    group_id,
                    success: false,
                    error: Some(format!("{}", e)),
                }),
            }
        }

        Ok(results)
    }

    /// Attempt to transfer leadership for all Raft groups
    pub async fn transfer_leadership_all(
        &self,
        target_node_id: u64,
    ) -> Result<Vec<ClusterActionResult>, RaftError> {
        let mut results = Vec::new();

        match self.meta.transfer_leadership(target_node_id).await {
            Ok(_) => results.push(ClusterActionResult {
                group_id: GroupId::Meta,
                success: true,
                error: None,
            }),
            Err(e) => results.push(ClusterActionResult {
                group_id: GroupId::Meta,
                success: false,
                error: Some(format!("{}", e)),
            }),
        }

        for (i, shard) in self.user_data_shards.iter().enumerate() {
            let group_id = GroupId::DataUserShard(i as u32);
            match shard.transfer_leadership(target_node_id).await {
                Ok(_) => results.push(ClusterActionResult {
                    group_id,
                    success: true,
                    error: None,
                }),
                Err(e) => results.push(ClusterActionResult {
                    group_id,
                    success: false,
                    error: Some(format!("{}", e)),
                }),
            }
        }

        for (i, shard) in self.shared_data_shards.iter().enumerate() {
            let group_id = GroupId::DataSharedShard(i as u32);
            match shard.transfer_leadership(target_node_id).await {
                Ok(_) => results.push(ClusterActionResult {
                    group_id,
                    success: true,
                    error: None,
                }),
                Err(e) => results.push(ClusterActionResult {
                    group_id,
                    success: false,
                    error: Some(format!("{}", e)),
                }),
            }
        }

        Ok(results)
    }

    /// Attempt to step down leaders for all Raft groups
    pub async fn step_down_all(&self) -> Result<Vec<ClusterActionResult>, RaftError> {
        let mut results = Vec::new();

        match self.meta.step_down().await {
            Ok(_) => results.push(ClusterActionResult {
                group_id: GroupId::Meta,
                success: true,
                error: None,
            }),
            Err(e) => results.push(ClusterActionResult {
                group_id: GroupId::Meta,
                success: false,
                error: Some(format!("{}", e)),
            }),
        }

        for (i, shard) in self.user_data_shards.iter().enumerate() {
            let group_id = GroupId::DataUserShard(i as u32);
            match shard.step_down().await {
                Ok(_) => results.push(ClusterActionResult {
                    group_id,
                    success: true,
                    error: None,
                }),
                Err(e) => results.push(ClusterActionResult {
                    group_id,
                    success: false,
                    error: Some(format!("{}", e)),
                }),
            }
        }

        for (i, shard) in self.shared_data_shards.iter().enumerate() {
            let group_id = GroupId::DataSharedShard(i as u32);
            match shard.step_down().await {
                Ok(_) => results.push(ClusterActionResult {
                    group_id,
                    success: true,
                    error: None,
                }),
                Err(e) => results.push(ClusterActionResult {
                    group_id,
                    success: false,
                    error: Some(format!("{}", e)),
                }),
            }
        }

        Ok(results)
    }

    /// Get summary information about existing snapshots
    pub fn get_snapshots_summary(&self) -> SnapshotsSummary {
        let mut total_groups = 0;
        let mut groups_with_snapshots = 0;
        let mut group_details = Vec::new();

        // Check Meta group
        total_groups += 1;
        if let Some(snapshot_idx) = self.meta.snapshot_index() {
            groups_with_snapshots += 1;
            group_details.push((GroupId::Meta, Some(snapshot_idx)));
        } else {
            group_details.push((GroupId::Meta, None));
        }

        // Check User data shards
        for (i, shard) in self.user_data_shards.iter().enumerate() {
            total_groups += 1;
            let group_id = GroupId::DataUserShard(i as u32);
            if let Some(snapshot_idx) = shard.snapshot_index() {
                groups_with_snapshots += 1;
                group_details.push((group_id, Some(snapshot_idx)));
            } else {
                group_details.push((group_id, None));
            }
        }

        // Check Shared data shards
        for (i, shard) in self.shared_data_shards.iter().enumerate() {
            total_groups += 1;
            let group_id = GroupId::DataSharedShard(i as u32);
            if let Some(snapshot_idx) = shard.snapshot_index() {
                groups_with_snapshots += 1;
                group_details.push((group_id, Some(snapshot_idx)));
            } else {
                group_details.push((group_id, None));
            }
        }

        SnapshotsSummary {
            total_groups,
            groups_with_snapshots,
            snapshots_dir: self
                .config
                .peers
                .is_empty()
                .then(|| "data/snapshots".to_string())
                .unwrap_or_else(|| "data/snapshots".to_string()),
            group_details,
        }
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

        log::info!("[CLUSTER] Node {} leaving cluster...", self.node_id);

        // Count how many groups we're leading
        let mut groups_leading = 0;
        for group_id in self.all_group_ids() {
            if self.is_leader(group_id) {
                groups_leading += 1;
            }
        }

        if groups_leading > 0 {
            log::info!(
                "[CLUSTER] This node is LEADER of {} groups, attempting leadership transfer...",
                groups_leading
            );

            // Attempt leadership transfer for each group where we're leader
            // Find the first available peer to transfer leadership to
            if let Some(target_node) = self.config.peers.first() {
                let target_node_id = target_node.node_id.as_u64();
                log::info!("[CLUSTER] Transferring leadership to node {}...", target_node.node_id);

                // Transfer leadership for Meta group
                if self.meta.is_leader() {
                    match self.meta.transfer_leadership(target_node_id).await {
                        Ok(_) => log::info!(
                            "[CLUSTER] ✓ Meta leadership transferred to node {}",
                            target_node.node_id
                        ),
                        Err(e) => {
                            log::warn!("[CLUSTER] ⚠ Failed to transfer Meta leadership: {}", e)
                        },
                    }
                }

                // Transfer leadership for user data shards
                for (i, shard) in self.user_data_shards.iter().enumerate() {
                    if shard.is_leader() {
                        match shard.transfer_leadership(target_node_id).await {
                            Ok(_) => log::debug!(
                                "[CLUSTER] ✓ UserDataShard[{}] leadership transferred",
                                i
                            ),
                            Err(e) => log::warn!(
                                "[CLUSTER] ⚠ Failed to transfer UserDataShard[{}] leadership: {}",
                                i,
                                e
                            ),
                        }
                    }
                }

                // Transfer leadership for shared data shards
                for (i, shard) in self.shared_data_shards.iter().enumerate() {
                    if shard.is_leader() {
                        match shard.transfer_leadership(target_node_id).await {
                            Ok(_) => log::debug!(
                                "[CLUSTER] ✓ SharedDataShard[{}] leadership transferred",
                                i
                            ),
                            Err(e) => log::warn!(
                                "[CLUSTER] ⚠ Failed to transfer SharedDataShard[{}] leadership: {}",
                                i,
                                e
                            ),
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
        log::debug!("[CLUSTER] Shutting down all Raft groups...");

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
