//! RaftExecutor - Cluster mode executor using Raft consensus
//!
//! This executor routes commands through Raft groups for consensus
//! before applying them to the local state machine.
//!
//! **Status**: Stub implementation - returns errors until Phase 3.

use async_trait::async_trait;

use crate::{
    CommandExecutor, DataResponse, GroupId, JobsCommand, JobsResponse, RaftError,
    SharedDataCommand, SystemCommand, SystemResponse, UserDataCommand, UsersCommand,
    UsersResponse,
};

/// Result type for executor operations
type Result<T> = std::result::Result<T, RaftError>;

/// Cluster mode executor using Raft consensus.
///
/// Routes commands through the appropriate Raft group leader,
/// waits for consensus, then returns the result.
///
/// This is a stub implementation. Full implementation in Phase 3:
/// - Connect to Raft groups via gRPC (tonic)
/// - Forward to leader if not leader
/// - Wait for log commit before returning
#[derive(Debug)]
pub struct RaftExecutor {
    /// This node's ID in the cluster
    node_id: u64,
    /// Whether cluster mode is enabled
    cluster_enabled: bool,
}

impl RaftExecutor {
    /// Create a new RaftExecutor (stub).
    ///
    /// Returns a placeholder that errors on all operations.
    pub fn new(node_id: u64) -> Self {
        Self {
            node_id,
            cluster_enabled: false, // Will be enabled when wired
        }
    }
    
    fn not_implemented(&self, operation: &str) -> RaftError {
        RaftError::Internal(format!(
            "RaftExecutor is a stub. Operation '{}' not yet implemented. \
            See Phase 3 in specs/016-raft-replication/tasks.md",
            operation
        ))
    }
}

#[async_trait]
impl CommandExecutor for RaftExecutor {
    async fn execute_system(&self, cmd: SystemCommand) -> Result<SystemResponse> {
        log::warn!("RaftExecutor::execute_system called (stub): {:?}", cmd);
        Err(self.not_implemented("execute_system"))
    }

    async fn execute_users(&self, cmd: UsersCommand) -> Result<UsersResponse> {
        log::warn!("RaftExecutor::execute_users called (stub): {:?}", cmd);
        Err(self.not_implemented("execute_users"))
    }

    async fn execute_jobs(&self, cmd: JobsCommand) -> Result<JobsResponse> {
        log::warn!("RaftExecutor::execute_jobs called (stub): {:?}", cmd);
        Err(self.not_implemented("execute_jobs"))
    }

    async fn execute_user_data(&self, user_id: &str, cmd: UserDataCommand) -> Result<DataResponse> {
        log::warn!(
            "RaftExecutor::execute_user_data called (stub) for user {}: {:?}",
            user_id, cmd
        );
        Err(self.not_implemented("execute_user_data"))
    }

    async fn execute_shared_data(&self, cmd: SharedDataCommand) -> Result<DataResponse> {
        log::warn!("RaftExecutor::execute_shared_data called (stub): {:?}", cmd);
        Err(self.not_implemented("execute_shared_data"))
    }

    async fn is_leader(&self, _group: GroupId) -> bool {
        // In stub mode, we're never the leader (cluster not functional)
        false
    }

    async fn get_leader(&self, _group: GroupId) -> Option<u64> {
        // No known leader in stub mode
        None
    }

    fn is_cluster_mode(&self) -> bool {
        self.cluster_enabled
    }

    fn node_id(&self) -> u64 {
        self.node_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::models::NamespaceId;

    #[tokio::test]
    async fn test_raft_executor_stub_errors() {
        let executor = RaftExecutor::new(1);
        
        // Verify it's in cluster mode but returns errors
        assert_eq!(executor.node_id(), 1);
        assert!(!executor.is_cluster_mode()); // Not yet enabled
        assert!(!executor.is_leader(GroupId::MetaSystem).await);
        assert!(executor.get_leader(GroupId::MetaSystem).await.is_none());
        
        // All operations should error
        let result = executor.execute_system(SystemCommand::DeleteNamespace {
            namespace_id: NamespaceId::new("test"),
        }).await;
        
        assert!(result.is_err());
        match result.unwrap_err() {
            RaftError::Internal(msg) => {
                assert!(msg.contains("stub"));
            }
            other => panic!("Expected Internal error, got {:?}", other),
        }
    }
}
