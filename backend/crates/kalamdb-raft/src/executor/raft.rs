//! RaftExecutor - Cluster mode executor using Raft consensus
//!
//! This executor routes commands through Raft groups for consensus
//! before applying them to the local state machine.

use std::sync::Arc;

use async_trait::async_trait;
use bincode::config;

use crate::{
    manager::RaftManager,
    ClusterInfo, ClusterNodeInfo, CommandExecutor, DataResponse, GroupId, JobsCommand, JobsResponse, RaftError,
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
        
        // Build info for this node
        let is_any_leader = self_groups_leading > 0;
        let self_role = if is_any_leader { "leader" } else { "follower" };
        
        let mut nodes = vec![ClusterNodeInfo {
            node_id: config.node_id,
            role: self_role.to_string(),
            status: "active".to_string(),
            rpc_addr: config.rpc_addr.clone(),
            api_addr: config.api_addr.clone(),
            is_self: true,
            is_leader: is_any_leader,
            groups_leading: self_groups_leading,
            total_groups,
        }];
        
        // Add peer nodes
        for peer in &config.peers {
            // For peers, we can't easily know their leader count without network calls
            // So we just report them as "unknown" role/status for now
            nodes.push(ClusterNodeInfo {
                node_id: peer.node_id,
                role: "follower".to_string(), // Assume follower unless we know otherwise
                status: "unknown".to_string(), // Would need health check
                rpc_addr: peer.rpc_addr.clone(),
                api_addr: peer.api_addr.clone(),
                is_self: false,
                is_leader: false,
                groups_leading: 0,
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
        self.manager.start().await
    }
    
    async fn initialize_cluster(&self) -> Result<()> {
        self.manager.initialize_cluster().await
    }
    
    async fn shutdown(&self) -> Result<()> {
        // TODO: Implement graceful shutdown with leadership transfer
        log::info!("RaftExecutor shutting down");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // Tests require a running RaftManager, which needs network setup
    // See integration tests in kalamdb-raft/tests/
}
