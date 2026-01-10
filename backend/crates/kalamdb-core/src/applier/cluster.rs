//! Cluster Applier - Routes commands through Raft consensus
//!
//! In cluster mode, all commands flow through Raft:
//! - Leader: Proposes to Raft, waits for quorum commit  
//! - Follower: RaftManager automatically forwards to leader via gRPC
//!
//! Note: The `RaftManager::propose_meta()` and `RaftGroup::propose_with_forward()`
//! handle leader detection and forwarding internally - no separate forwarder needed.

use async_trait::async_trait;
use once_cell::sync::OnceCell;
use std::sync::Arc;

use kalamdb_raft::{GroupId, RaftExecutor, RaftManager};

use super::applier::{LeaderInfo, UnifiedApplier};
use super::commands::{
    AlterTableCommand, CreateNamespaceCommand, CreateStorageCommand, CreateTableCommand,
    CreateUserCommand, DeleteUserCommand, DropNamespaceCommand,
    DropStorageCommand, DropTableCommand, UpdateUserCommand,
};
use super::error::ApplierError;
use super::executor::CommandExecutorImpl;
use crate::app_context::AppContext;

/// Cluster applier - routes commands through Raft consensus
///
/// This applier delegates to `RaftManager::propose_meta()` which:
/// 1. If leader: Proposes command to Raft, waits for quorum commit
/// 2. If follower: Automatically forwards to leader via gRPC (handled by RaftGroup)
///
/// The actual command execution happens in `RaftStateMachine::apply()`
/// which delegates to `CommandExecutorImpl`.
pub struct ClusterApplier {
    executor: OnceCell<CommandExecutorImpl>,
}

impl Default for ClusterApplier {
    fn default() -> Self {
        Self::new()
    }
}

impl ClusterApplier {
    /// Create a new cluster applier
    pub fn new() -> Self {
        Self {
            executor: OnceCell::new(),
        }
    }

    /// Get the executor (panics if not initialized)
    fn executor(&self) -> &CommandExecutorImpl {
        self.executor
            .get()
            .expect("ClusterApplier not initialized - call set_app_context first")
    }

    /// Get the RaftManager from AppContext
    fn raft_manager(&self) -> Option<Arc<RaftManager>> {
        let app_ctx = self.executor().app_context();
        let executor = app_ctx.executor();

        // Try to downcast to RaftExecutor to get the manager
        executor
            .as_any()
            .downcast_ref::<RaftExecutor>()
            .map(|raft_exec| raft_exec.manager().clone())
    }

    /// Get leader info for meta group
    fn get_meta_leader_info(&self) -> Option<LeaderInfo> {
        let mgr = self.raft_manager()?;
        let leader_node_id = mgr.current_leader(GroupId::Meta)?;
        
        // Get the leader's address from the config peers
        let config = mgr.config();
        let leader_addr = if leader_node_id == config.node_id {
            // We are the leader (shouldn't happen if calling this as follower)
            config.rpc_addr.clone()
        } else {
            // Find peer address
            config.peers
                .iter()
                .find(|p| p.node_id == leader_node_id)
                .map(|p| p.rpc_addr.clone())?
        };
        
        Some(LeaderInfo {
            node_id: leader_node_id.as_u64(),
            address: leader_addr,
        })
    }

    /// Propose a command to meta Raft group
    ///
    /// RaftManager::propose_meta() handles:
    /// - Leader detection
    /// - Automatic forwarding to leader via gRPC if we're a follower
    /// - Retry logic with exponential backoff
    async fn propose_meta<T: serde::Serialize>(
        &self,
        command: &T,
        cmd_type: &str,
    ) -> Result<String, ApplierError> {
        // Serialize command
        let command_bytes = bincode::serde::encode_to_vec(command, bincode::config::standard())
            .map_err(|e| ApplierError::Serialization(e.to_string()))?;

        let raft_mgr = self.raft_manager().ok_or(ApplierError::NoLeader)?;

        log::debug!(
            "ClusterApplier: Proposing {} to meta Raft (leader={}, we_are_leader={})",
            cmd_type,
            raft_mgr.current_leader(GroupId::Meta).map(|n| n.as_u64()).unwrap_or(0),
            raft_mgr.is_leader(GroupId::Meta)
        );

        // propose_meta handles forwarding internally via propose_with_forward
        raft_mgr
            .propose_meta(command_bytes)
            .await
            .map_err(|e| ApplierError::Raft(e.to_string()))?;

        Ok(format!("{} completed via Raft consensus", cmd_type))
    }
}

#[async_trait]
impl UnifiedApplier for ClusterApplier {
    // =========================================================================
    // DDL Operations
    // =========================================================================

    async fn create_table(&self, cmd: CreateTableCommand) -> Result<String, ApplierError> {
        self.propose_meta(&cmd, "CREATE TABLE").await
    }

    async fn alter_table(&self, cmd: AlterTableCommand) -> Result<String, ApplierError> {
        self.propose_meta(&cmd, "ALTER TABLE").await
    }

    async fn drop_table(&self, cmd: DropTableCommand) -> Result<String, ApplierError> {
        self.propose_meta(&cmd, "DROP TABLE").await
    }

    // =========================================================================
    // Namespace Operations
    // =========================================================================

    async fn create_namespace(&self, cmd: CreateNamespaceCommand) -> Result<String, ApplierError> {
        self.propose_meta(&cmd, "CREATE NAMESPACE").await
    }

    async fn drop_namespace(&self, cmd: DropNamespaceCommand) -> Result<String, ApplierError> {
        self.propose_meta(&cmd, "DROP NAMESPACE").await
    }

    // =========================================================================
    // Storage Operations
    // =========================================================================

    async fn create_storage(&self, cmd: CreateStorageCommand) -> Result<String, ApplierError> {
        self.propose_meta(&cmd, "CREATE STORAGE").await
    }

    async fn drop_storage(&self, cmd: DropStorageCommand) -> Result<String, ApplierError> {
        self.propose_meta(&cmd, "DROP STORAGE").await
    }

    // =========================================================================
    // User Operations
    // =========================================================================

    async fn create_user(&self, cmd: CreateUserCommand) -> Result<String, ApplierError> {
        self.propose_meta(&cmd, "CREATE USER").await
    }

    async fn update_user(&self, cmd: UpdateUserCommand) -> Result<String, ApplierError> {
        self.propose_meta(&cmd, "UPDATE USER").await
    }

    async fn delete_user(&self, cmd: DeleteUserCommand) -> Result<String, ApplierError> {
        self.propose_meta(&cmd, "DELETE USER").await
    }

    // =========================================================================
    // Status Methods
    // =========================================================================

    fn can_accept_writes(&self) -> bool {
        // In cluster mode, we can always accept writes
        // They'll be forwarded to the leader if needed
        true
    }

    fn is_cluster_mode(&self) -> bool {
        true
    }

    fn get_leader_info(&self) -> Option<LeaderInfo> {
        self.get_meta_leader_info()
    }

    fn set_app_context(&self, app_context: Arc<AppContext>) {
        if self
            .executor
            .set(CommandExecutorImpl::new(app_context))
            .is_err()
        {
            log::warn!("ClusterApplier already initialized; ignoring duplicate set_app_context");
        }
    }
}
