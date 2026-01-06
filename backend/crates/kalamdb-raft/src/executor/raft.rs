//! RaftExecutor - Cluster mode executor using Raft consensus
//!
//! This executor routes commands through Raft groups for consensus
//! before applying them to the local state machine.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use async_trait::async_trait;
use bincode::config;

use crate::{
    manager::RaftManager,
    ClusterInfo, ClusterNodeInfo, CommandExecutor, DataResponse, GroupId, JobsCommand, JobsResponse, KalamNode, RaftError,
    SharedDataCommand, SystemCommand, SystemResponse, UserDataCommand, UsersCommand,
    UsersResponse,
};

/// Result type for executor operations
type Result<T> = std::result::Result<T, RaftError>;

/// Cluster mode executor using Raft consensus.
///
/// Routes commands through the appropriate Raft group leader,
/// waits for consensus, then returns the result.
#[derive(Debug)]
pub struct RaftExecutor {
    /// Reference to the Raft manager
    manager: Arc<RaftManager>,
}

impl RaftExecutor {
    /// Create a new RaftExecutor with a RaftManager.
    pub fn new(manager: Arc<RaftManager>) -> Self {
        Self { manager }
    }
    
    /// Serialize a command to bytes using bincode with serde
    fn serialize<T: serde::Serialize>(cmd: &T) -> Result<Vec<u8>> {
        bincode::serde::encode_to_vec(cmd, config::standard()).map_err(|e| {
            RaftError::Internal(format!("Failed to serialize command: {}", e))
        })
    }
    
    /// Deserialize a response from bytes using bincode with serde
    fn deserialize<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T> {
        let (result, _) = bincode::serde::decode_from_slice(bytes, config::standard()).map_err(|e| {
            RaftError::Internal(format!("Failed to deserialize response: {}", e))
        })?;
        Ok(result)
    }
    
    /// Compute the shard for a user based on their ID
    fn user_shard(&self, user_id: &str) -> u32 {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;
        
        let mut hasher = DefaultHasher::new();
        user_id.hash(&mut hasher);
        (hasher.finish() % 32) as u32
    }
}

#[async_trait]
impl CommandExecutor for RaftExecutor {
    async fn execute_system(&self, cmd: SystemCommand) -> Result<SystemResponse> {
        let bytes = Self::serialize(&cmd)?;
        let result = self.manager.propose_system(bytes).await?;
        Self::deserialize(&result)
    }

    async fn execute_users(&self, cmd: UsersCommand) -> Result<UsersResponse> {
        let bytes = Self::serialize(&cmd)?;
        let result = self.manager.propose_users(bytes).await?;
        Self::deserialize(&result)
    }

    async fn execute_jobs(&self, cmd: JobsCommand) -> Result<JobsResponse> {
        let bytes = Self::serialize(&cmd)?;
        let result = self.manager.propose_jobs(bytes).await?;
        Self::deserialize(&result)
    }

    async fn execute_user_data(&self, user_id: &str, cmd: UserDataCommand) -> Result<DataResponse> {
        let shard = self.user_shard(user_id);
        let bytes = Self::serialize(&cmd)?;
        let result = self.manager.propose_user_data(shard, bytes).await?;
        Self::deserialize(&result)
    }

    async fn execute_shared_data(&self, cmd: SharedDataCommand) -> Result<DataResponse> {
        let bytes = Self::serialize(&cmd)?;
        // All shared data goes to shard 0
        let result = self.manager.propose_shared_data(0, bytes).await?;
        Self::deserialize(&result)
    }

    async fn is_leader(&self, group: GroupId) -> bool {
        self.manager.is_leader(group)
    }

    async fn get_leader(&self, group: GroupId) -> Option<u64> {
        self.manager.current_leader(group)
    }

    fn is_cluster_mode(&self) -> bool {
        true
    }

    fn node_id(&self) -> u64 {
        self.manager.node_id()
    }
    
    fn get_cluster_info(&self) -> ClusterInfo {
        let config = self.manager.config();
        let all_groups = self.manager.all_group_ids();
        let total_groups = all_groups.len() as u32;
        
        // Count how many groups this node leads
        let mut self_groups_leading = 0;
        for group in &all_groups {
            if self.manager.is_leader(*group) {
                self_groups_leading += 1;
            }
        }
        
        let meta_metrics = self.manager.meta_system_metrics();
        let mut voter_ids = BTreeSet::new();
        let mut nodes_map: BTreeMap<u64, KalamNode> = BTreeMap::new();

        let leader_id = if let Some(metrics) = meta_metrics.as_ref() {
            voter_ids.extend(metrics.membership_config.voter_ids());
            for (node_id, node) in metrics.membership_config.nodes() {
                nodes_map.insert(*node_id, node.clone());
            }
            metrics.current_leader
        } else {
            nodes_map.insert(
                config.node_id,
                KalamNode {
                    rpc_addr: config.rpc_addr.clone(),
                    api_addr: config.api_addr.clone(),
                },
            );
            for peer in &config.peers {
                nodes_map.insert(
                    peer.node_id,
                    KalamNode {
                        rpc_addr: peer.rpc_addr.clone(),
                        api_addr: peer.api_addr.clone(),
                    },
                );
            }
            voter_ids.extend(nodes_map.keys().copied());
            if self_groups_leading > 0 {
                Some(config.node_id)
            } else {
                None
            }
        };

        let self_status = if meta_metrics
            .as_ref()
            .map(|metrics| metrics.running_state.is_ok())
            .unwrap_or(true)
        {
            "active"
        } else {
            "offline"
        };

        let mut nodes = Vec::with_capacity(nodes_map.len());
        for (node_id, node) in nodes_map {
            let is_self = node_id == config.node_id;
            let is_leader = leader_id == Some(node_id);
            let role = if is_leader {
                "leader"
            } else if voter_ids.contains(&node_id) {
                "follower"
            } else {
                "learner"
            };
            let status = if is_self { self_status } else { "active" };

            nodes.push(ClusterNodeInfo {
                node_id,
                role: role.to_string(),
                status: status.to_string(),
                rpc_addr: node.rpc_addr,
                api_addr: node.api_addr,
                is_self,
                is_leader,
                groups_leading: if is_self { self_groups_leading } else { 0 },
                total_groups,
            });
        }
        
        ClusterInfo {
            cluster_id: "kalamdb-cluster".to_string(), // TODO: Get from config
            current_node_id: config.node_id,
            is_cluster_mode: true,
            nodes,
            total_groups,
            user_shards: config.user_shards,
            shared_shards: config.shared_shards,
        }
    }
    
    async fn start(&self) -> Result<()> {
        // First start the RPC server so we can receive incoming Raft RPCs
        let rpc_addr = self.manager.config().rpc_addr.clone();
        crate::network::start_rpc_server(self.manager.clone(), rpc_addr).await?;
        
        // Then start the Raft groups
        self.manager.start().await
    }
    
    async fn initialize_cluster(&self) -> Result<()> {
        self.manager.initialize_cluster().await
    }
    
    async fn shutdown(&self) -> Result<()> {
        log::info!("RaftExecutor shutting down with graceful cluster leave...");
        self.manager.shutdown().await
    }
}

#[cfg(test)]
mod tests {
    // Tests require a running RaftManager, which needs network setup
    // See integration tests in kalamdb-raft/tests/
}
