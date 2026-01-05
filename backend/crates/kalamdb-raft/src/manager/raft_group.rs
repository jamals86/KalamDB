//! Individual Raft Group
//!
//! Represents a single Raft consensus group with its own log, state machine, and network.

use std::sync::Arc;

use openraft::storage::Adaptor;
use openraft::{Config, Raft};
use parking_lot::RwLock;

use crate::storage::{KalamRaftStorage, KalamTypeConfig, KalamNode};
use crate::network::RaftNetworkFactory;
use crate::state_machine::KalamStateMachine;
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
    /// Create a new Raft group (not yet started)
    pub fn new(group_id: GroupId, state_machine: SM) -> Self {
        Self {
            group_id,
            raft: RwLock::new(None),
            storage: Arc::new(KalamRaftStorage::new(group_id, state_machine)),
            network_factory: RaftNetworkFactory::new(group_id),
        }
    }
    
    /// Start the Raft group with the given node ID
    ///
    /// This initializes the Raft instance and begins participating in consensus.
    pub async fn start(&self, node_id: u64) -> Result<(), RaftError> {
        // Check if already started
        if self.is_started() {
            return Ok(());
        }
        
        // Create Raft configuration
        let config = Config {
            cluster_name: format!("kalamdb-{}", self.group_id),
            election_timeout_min: 150,
            election_timeout_max: 300,
            heartbeat_interval: 50,
            ..Default::default()
        };
        
        let config = Arc::new(config.validate().map_err(|e| RaftError::Config(e.to_string()))?);
        
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
        
        log::info!("Started Raft group {} on node {}", self.group_id, node_id);
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
        
        log::info!("Initialized Raft group {} cluster with node {}", self.group_id, node_id);
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
    
    /// Propose a command to this Raft group
    ///
    /// Returns the response data after the command is committed and applied.
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state_machine::SystemStateMachine;

    #[test]
    fn test_raft_group_creation() {
        let sm = SystemStateMachine::new();
        let group = RaftGroup::new(GroupId::MetaSystem, sm);
        
        assert_eq!(group.group_id(), GroupId::MetaSystem);
        assert!(!group.is_started());
        assert!(!group.is_leader());
    }
}
