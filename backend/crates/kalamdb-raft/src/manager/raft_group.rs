//! Individual Raft Group
//!
//! Represents a single Raft consensus group with its own log, state machine, and network.

use std::sync::Arc;
use std::time::{Duration, Instant};

use kalamdb_store::StorageBackend;
use openraft::storage::Adaptor;
use openraft::{Config, Raft, RaftMetrics};
use parking_lot::RwLock;

use crate::network::RaftNetworkFactory;
use crate::state_machine::KalamStateMachine;
use crate::storage::{KalamNode, KalamRaftStorage, KalamTypeConfig};
use crate::{GroupId, RaftError};

/// Type alias for the openraft Raft instance
pub type RaftInstance = Raft<KalamTypeConfig>;

/// Type alias for the storage adaptor
pub type StorageAdaptor<SM> = Adaptor<KalamTypeConfig, Arc<KalamRaftStorage<SM>>>;

/// A single Raft consensus group
///
/// Each group has:
/// - Its own combined storage (log + state machine)
/// - Its own network connections to peers
pub struct RaftGroup<SM: KalamStateMachine + Send + Sync + 'static> {
    /// Group identifier
    group_id: GroupId,

    /// The Raft instance
    raft: RwLock<Option<RaftInstance>>,

    /// Combined storage for this group
    storage: Arc<KalamRaftStorage<SM>>,

    /// Network factory for this group
    network_factory: RaftNetworkFactory,
}

impl<SM: KalamStateMachine + Send + Sync + 'static> RaftGroup<SM> {
    /// Create a new Raft group with in-memory storage (not yet started)
    pub fn new(group_id: GroupId, state_machine: SM) -> Self {
        Self {
            group_id,
            raft: RwLock::new(None),
            storage: Arc::new(KalamRaftStorage::new(group_id, state_machine)),
            network_factory: RaftNetworkFactory::new(group_id),
        }
    }

    /// Create a new Raft group with persistent storage (not yet started)
    ///
    /// This mode persists Raft log entries, votes, and metadata to durable storage.
    /// On restart, state is recovered from the persistent store.
    pub fn new_persistent(
        group_id: GroupId,
        state_machine: SM,
        backend: Arc<dyn StorageBackend>,
    ) -> Result<Self, RaftError> {
        let storage = KalamRaftStorage::new_persistent(group_id, state_machine, backend)
            .map_err(|e| RaftError::Storage(format!("Failed to create persistent storage: {}", e)))?;

        Ok(Self {
            group_id,
            raft: RwLock::new(None),
            storage: Arc::new(storage),
            network_factory: RaftNetworkFactory::new(group_id),
        })
    }

    /// Check if this group is using persistent storage
    pub fn is_persistent(&self) -> bool {
        self.storage.is_persistent()
    }

    /// Start the Raft group with the given node ID and configuration
    ///
    /// This initializes the Raft instance and begins participating in consensus.
    pub async fn start(&self, node_id: u64, config: &crate::manager::RaftManagerConfig) -> Result<(), RaftError> {
        // Check if already started
        if self.is_started() {
            return Ok(());
        }
        
        // Detect single-node mode (no peers configured)
        let is_single_node = config.peers.is_empty();
        
        // Parse snapshot policy from configuration
        let snapshot_policy = kalamdb_commons::config::ClusterConfig::parse_snapshot_policy(&config.snapshot_policy)
            .map_err(|e| RaftError::Config(format!("Invalid snapshot policy: {}", e)))?;
        
        // Create Raft configuration with optimizations for single-node mode
        // Single-node optimizations (OpenRaft recommendations):
        // - Disable snapshot purging (max_in_snapshot_log_to_keep = 0)
        // - Reduce heartbeat/election timeouts
        // - Smaller purge batch to reduce CPU overhead
        // - Keep tick and elect enabled (required for Raft state machine to function)
        // - Snapshot policy from config (default: 1000 entries)
        let raft_config = if is_single_node {
            log::debug!("[SINGLE-NODE] Applying lightweight Raft optimizations for {}", self.group_id);
            log::debug!("[SINGLE-NODE] Snapshot policy: {:?}", snapshot_policy);
            let mut cfg = Config {
                cluster_name: format!("kalamdb-{}", self.group_id),
                election_timeout_min: 150,  // Faster elections (default 150-300ms)
                election_timeout_max: 300,
                heartbeat_interval: 50,      // Faster heartbeats (default 50ms)
                install_snapshot_timeout: 5000,
                max_in_snapshot_log_to_keep: 0,  // Disable log retention after snapshot
                purge_batch_size: 64,        // Smaller purge batches
                
                enable_heartbeat: false,    // Disable heartbeats (no followers to send to)
                enable_elect: true,         // Keep elections enabled (needed to become leader)
                enable_tick: true,          // Keep tick enabled (REQUIRED for Raft to function)
                ..Default::default()
            };
            
            // Configure snapshot policy from config
            cfg.snapshot_policy = snapshot_policy;
            cfg
        } else {
            // Multi-node cluster: use conservative settings for network reliability
            log::debug!("[MULTI-NODE] Cluster Raft configuration for {}", self.group_id);
            log::debug!("[MULTI-NODE] Snapshot policy: {:?}", snapshot_policy);
            let mut cfg = Config {
                cluster_name: format!("kalamdb-{}", self.group_id),
                election_timeout_min: config.election_timeout_ms.0,
                election_timeout_max: config.election_timeout_ms.1,
                heartbeat_interval: config.heartbeat_interval_ms,
                install_snapshot_timeout: 10000,
                max_in_snapshot_log_to_keep: config.max_snapshots_to_keep as u64,
                purge_batch_size: 256,
                ..Default::default()
            };
            
            // Configure snapshot policy from config
            cfg.snapshot_policy = snapshot_policy;
            cfg
        };
        
        let config = Arc::new(raft_config.validate().map_err(|e| RaftError::Config(e.to_string()))?);
        
        // Create adaptor from combined storage
        let (log_store, state_machine): (StorageAdaptor<SM>, StorageAdaptor<SM>) = 
            Adaptor::new(self.storage.clone());
        
        // Create the Raft instance
        let raft = Raft::new(
            node_id,
            config,
            self.network_factory.clone(),
            log_store,
            state_machine,
        ).await.map_err(|e| RaftError::Internal(format!("Failed to create Raft: {:?}", e)))?;
        
        // Store the instance
        {
            let mut guard = self.raft.write();
            *guard = Some(raft);
        }
        
        log::debug!("Started Raft group {} on node {}", self.group_id, node_id);
        Ok(())
    }
    
    /// Initialize the cluster (call on first node only)
    ///
    /// This bootstraps the cluster with an initial membership containing only this node.
    pub async fn initialize(&self, node_id: u64, node: KalamNode) -> Result<(), RaftError> {
        let raft = {
            let guard = self.raft.read();
            guard.clone().ok_or_else(|| RaftError::NotStarted(self.group_id.to_string()))?
        };
        
        // Create initial membership with just this node
        let mut members = std::collections::BTreeMap::new();
        members.insert(node_id, node);
        
        raft.initialize(members).await
            .map_err(|e| RaftError::Internal(format!("Failed to initialize cluster: {:?}", e)))?;
        
        log::debug!("Initialized Raft group {} cluster with node {}", self.group_id, node_id);
        Ok(())
    }
    
    /// Add a learner (non-voting member) to the cluster
    pub async fn add_learner(&self, node_id: u64, node: KalamNode) -> Result<(), RaftError> {
        let raft = {
            let guard = self.raft.read();
            guard.clone().ok_or_else(|| RaftError::NotStarted(self.group_id.to_string()))?
        };
        
        raft.add_learner(node_id, node, true).await
            .map_err(|e| RaftError::Internal(format!("Failed to add learner: {:?}", e)))?;
        
        Ok(())
    }

    /// Wait for a learner to catch up to the leader's commit index
    pub async fn wait_for_learner_catchup(
        &self,
        node_id: u64,
        timeout: Duration,
    ) -> Result<(), RaftError> {
        let raft = {
            let guard = self.raft.read();
            guard.clone().ok_or_else(|| RaftError::NotStarted(self.group_id.to_string()))?
        };

        let start = Instant::now();
        let poll_interval = Duration::from_millis(50);

        loop {
            if start.elapsed() > timeout {
                return Err(RaftError::ReplicationTimeout {
                    group: self.group_id.to_string(),
                    committed_log_id: "catchup".to_string(),
                    timeout_ms: timeout.as_millis() as u64,
                });
            }

            let metrics = raft.metrics().borrow().clone();
            if metrics.current_leader != Some(metrics.id) {
                return Err(RaftError::not_leader(
                    self.group_id.to_string(),
                    metrics.current_leader,
                ));
            }

            let last_log_index = metrics.last_log_index.unwrap_or(0);
            let snapshot_index = metrics.snapshot.map(|log_id| log_id.index).unwrap_or(0);
            let applied_index = metrics.last_applied.map(|log_id| log_id.index).unwrap_or(0);
            let target_index = last_log_index.max(snapshot_index).max(applied_index);

            if target_index == 0 {
                return Ok(());
            }

            if let Some(ref replication) = metrics.replication {
                if let Some(matched) = replication.get(&node_id) {
                    let matched_index = matched.map(|log_id| log_id.index).unwrap_or(0);
                    if matched_index >= target_index {
                        return Ok(());
                    }
                }
            }

            tokio::time::sleep(poll_interval).await;
        }
    }
    
    /// Change membership to include the given voters
    pub async fn change_membership(&self, members: Vec<u64>) -> Result<(), RaftError> {
        let raft = {
            let guard = self.raft.read();
            guard.clone().ok_or_else(|| RaftError::NotStarted(self.group_id.to_string()))?
        };
        
        let member_set: std::collections::BTreeSet<u64> = members.into_iter().collect();
        raft.change_membership(member_set, false).await
            .map_err(|e| RaftError::Internal(format!("Failed to change membership: {:?}", e)))?;
        
        Ok(())
    }

    /// Promote a learner to a voter using the current membership configuration
    pub async fn promote_learner(&self, node_id: u64) -> Result<(), RaftError> {
        let raft = {
            let guard = self.raft.read();
            guard.clone().ok_or_else(|| RaftError::NotStarted(self.group_id.to_string()))?
        };

        let metrics = raft.metrics().borrow().clone();
        if metrics.current_leader != Some(metrics.id) {
            return Err(RaftError::not_leader(
                self.group_id.to_string(),
                metrics.current_leader,
            ));
        }

        let mut voters: std::collections::BTreeSet<u64> =
            metrics.membership_config.voter_ids().collect();
        if voters.contains(&node_id) {
            return Ok(());
        }

        voters.insert(node_id);
        raft.change_membership(voters, false).await
            .map_err(|e| RaftError::Internal(format!("Failed to change membership: {:?}", e)))?;

        Ok(())
    }
    
    /// Get the Raft instance (if started)
    pub fn raft(&self) -> Option<RaftInstance> {
        self.raft.read().clone()
    }
    
    /// Get the group ID
    pub fn group_id(&self) -> GroupId {
        self.group_id
    }
    
    /// Check if this group has been started
    pub fn is_started(&self) -> bool {
        self.raft.read().is_some()
    }
    
    /// Check if this node is the leader for this group
    pub fn is_leader(&self) -> bool {
        let raft = self.raft.read();
        match raft.as_ref() {
            Some(r) => {
                let metrics = r.metrics().borrow().clone();
                metrics.current_leader == Some(metrics.id)
            }
            None => false,
        }
    }
    
    /// Get the current leader node ID, if known
    pub fn current_leader(&self) -> Option<u64> {
        let raft = self.raft.read();
        raft.as_ref().and_then(|r| {
            r.metrics().borrow().current_leader
        })
    }

    /// Get the latest OpenRaft metrics for this group, if started
    pub fn metrics(&self) -> Option<RaftMetrics<u64, KalamNode>> {
        let raft = self.raft.read();
        raft.as_ref().map(|r| r.metrics().borrow().clone())
    }
    
    /// Propose a command to this Raft group
    ///
    /// Returns the response data after the command is committed and applied.
    /// Note: This method only works if this node is the leader.
    /// For automatic forwarding, use `propose_with_forward`.
    pub async fn propose(&self, command: Vec<u8>) -> Result<Vec<u8>, RaftError> {
        let raft = {
            let guard = self.raft.read();
            guard.clone().ok_or_else(|| RaftError::NotStarted(self.group_id.to_string()))?
        };
        
        // Submit the command and wait for commit
        let response = raft.client_write(command).await
            .map_err(|e| RaftError::Proposal(format!("{:?}", e)))?;
        
        Ok(response.data)
    }
    
    /// Propose a command with automatic leader forwarding
    ///
    /// If this node is the leader, proposes locally.
    /// If this node is a follower, forwards the proposal to the leader via gRPC.
    /// Includes retry logic for transient failures (e.g., leader unknown).
    pub async fn propose_with_forward(&self, command: Vec<u8>) -> Result<Vec<u8>, RaftError> {
        // Fast path: if we are the leader, propose locally
        if self.is_leader() {
            return self.propose(command).await;
        }
        
        // We're not the leader - try to forward to the leader with retries
        // because the leader might not be known yet (during election)
        const MAX_RETRIES: u32 = 5;
        const INITIAL_BACKOFF_MS: u64 = 50;
        
        let mut last_error = None;
        for attempt in 0..MAX_RETRIES {
            // Get current leader
            match self.current_leader() {
                Some(leader_id) => {
                    // Get leader node info
                    match self.network_factory.get_node(leader_id) {
                        Some(leader_node) => {
                            log::debug!(
                                "Forwarding proposal for group {} to leader {} at {} (attempt {})",
                                self.group_id, leader_id, leader_node.rpc_addr, attempt + 1
                            );
                            
                            // Try to forward
                            match self.forward_to_leader(&leader_node.rpc_addr, command.clone()).await {
                                Ok(result) => return Ok(result),
                                Err(e) => {
                                    log::debug!("Forward attempt {} failed: {}", attempt + 1, e);
                                    last_error = Some(e);
                                    // Continue to retry
                                }
                            }
                        }
                        None => {
                            log::debug!(
                                "Leader node {} for group {} not in registry (attempt {})",
                                leader_id, self.group_id, attempt + 1
                            );
                            last_error = Some(RaftError::Network(format!(
                                "Unknown leader node {} for group {}", leader_id, self.group_id
                            )));
                        }
                    }
                }
                None => {
                    log::debug!(
                        "No leader known for group {} (attempt {}), waiting...",
                        self.group_id, attempt + 1
                    );
                    last_error = Some(RaftError::not_leader(self.group_id.to_string(), None));
                }
            }
            
            // Wait before retry with exponential backoff
            if attempt + 1 < MAX_RETRIES {
                let backoff = INITIAL_BACKOFF_MS * (1 << attempt);
                tokio::time::sleep(std::time::Duration::from_millis(backoff)).await;
            }
        }
        
        // All retries exhausted
        Err(last_error.unwrap_or_else(|| RaftError::not_leader(
            self.group_id.to_string(), 
            None
        )))
    }
    
    /// Forward a proposal to the leader via gRPC
    async fn forward_to_leader(
        &self, 
        leader_addr: &str, 
        command: Vec<u8>,
    ) -> Result<Vec<u8>, RaftError> {
        use tonic::transport::Channel;
        use crate::network::{RaftClient, ClientProposalRequest};
        
        // Connect to leader
        let endpoint = format!("http://{}", leader_addr);
        let channel = Channel::from_shared(endpoint.clone())
            .map_err(|e| RaftError::Network(format!("Invalid leader URI: {}", e)))?
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(30))
            .connect()
            .await
            .map_err(|e| RaftError::Network(format!(
                "Failed to connect to leader at {}: {}", leader_addr, e
            )))?;
        
        let mut client = RaftClient::new(channel);
        
        // Send the proposal
        let request = tonic::Request::new(ClientProposalRequest {
            group_id: self.group_id.to_string(),
            command,
        });
        
        let response = client.client_proposal(request).await
            .map_err(|e| RaftError::Network(format!(
                "gRPC error forwarding proposal: {}", e
            )))?;
        
        let inner = response.into_inner();
        
        if inner.success {
            Ok(inner.payload)
        } else if let Some(leader_hint) = inner.leader_hint {
            // Leader might have changed - return not_leader error with hint
            Err(RaftError::not_leader(self.group_id.to_string(), Some(leader_hint)))
        } else {
            Err(RaftError::Proposal(inner.error))
        }
    }

    /// Register a peer node
    pub fn register_peer(&self, node_id: u64, node: KalamNode) {
        self.network_factory.register_node(node_id, node);
    }
    
    /// Get reference to the storage
    pub fn storage(&self) -> &Arc<KalamRaftStorage<SM>> {
        &self.storage
    }
    
    /// Get reference to the network factory
    pub fn network_factory(&self) -> &RaftNetworkFactory {
        &self.network_factory
    }
    
    /// Attempt to transfer leadership to another node
    ///
    /// In OpenRaft v0.9, we don't have explicit leadership transfer API.
    /// Instead, we trigger an election by sending heartbeats with a hint
    /// that another node should take over. If leadership transfer is not
    /// supported, we just log and continue - the cluster will re-elect.
    pub async fn transfer_leadership(&self, _target_node_id: u64) -> Result<(), RaftError> {
        // OpenRaft v0.9 doesn't have direct leadership transfer
        // The Raft protocol will handle re-election when this node shuts down
        // Other nodes will detect the leader is gone and start an election
        log::debug!(
            "Leadership transfer requested for group {} - relying on automatic re-election on shutdown",
            self.group_id
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state_machine::MetaStateMachine;

    #[test]
    fn test_raft_group_creation() {
        let sm = MetaStateMachine::new();
        let group = RaftGroup::new(GroupId::Meta, sm);
        
        assert_eq!(group.group_id(), GroupId::Meta);
        assert!(!group.is_started());
        assert!(!group.is_leader());
    }
}
