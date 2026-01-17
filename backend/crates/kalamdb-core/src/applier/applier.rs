//! Unified Applier (Phase 20 - Single Code Path)
//!
//! All commands flow through Raft, even in single-node mode.
//! This ensures the same code path is tested in both modes.

use async_trait::async_trait;
use chrono::Utc;
use std::sync::Arc;

use kalamdb_commons::models::rows::Row;
use kalamdb_commons::models::schemas::{TableDefinition, TableType};
use kalamdb_commons::models::{NamespaceId, StorageId, TableId, UserId};
use kalamdb_commons::system::Storage;
use kalamdb_commons::types::User;
use kalamdb_raft::{
    DataResponse, GroupId, MetaCommand, RaftExecutor, SharedDataCommand, UserDataCommand,
};

use super::error::ApplierError;
use super::executor::CommandExecutorImpl;
use crate::app_context::AppContext;

/// Information about the current leader (for cluster mode)
#[derive(Debug, Clone)]
pub struct LeaderInfo {
    /// Node ID of the leader
    pub node_id: u64,
    /// Address for forwarding commands
    pub address: String,
}

/// Unified Applier trait - the single interface for all command execution
///
/// All commands flow through Raft consensus (even in single-node mode).
/// This ensures consistent behavior and testing across all deployment modes.
#[async_trait]
pub trait UnifiedApplier: Send + Sync {
    // =========================================================================
    // DDL Operations (Meta Raft Group)
    // =========================================================================

    /// Create a table
    async fn create_table(
        &self,
        table_id: TableId,
        table_type: TableType,
        table_def: TableDefinition,
    ) -> Result<String, ApplierError>;

    /// Alter a table
    async fn alter_table(
        &self,
        table_id: TableId,
        table_def: TableDefinition,
    ) -> Result<String, ApplierError>;

    /// Drop a table
    async fn drop_table(&self, table_id: TableId) -> Result<String, ApplierError>;

    // =========================================================================
    // Namespace Operations (Meta Raft Group)
    // =========================================================================

    /// Create a namespace
    async fn create_namespace(
        &self,
        namespace_id: NamespaceId,
        created_by: Option<UserId>,
    ) -> Result<String, ApplierError>;

    /// Drop a namespace
    async fn drop_namespace(&self, namespace_id: NamespaceId) -> Result<String, ApplierError>;

    // =========================================================================
    // Storage Operations (Meta Raft Group)
    // =========================================================================

    /// Create a storage backend
    async fn create_storage(&self, storage: Storage) -> Result<String, ApplierError>;

    /// Drop a storage backend
    async fn drop_storage(&self, storage_id: StorageId) -> Result<String, ApplierError>;

    // =========================================================================
    // User Operations (Meta Raft Group)
    // =========================================================================

    /// Create a user
    async fn create_user(&self, user: User) -> Result<String, ApplierError>;

    /// Update a user
    async fn update_user(&self, user: User) -> Result<String, ApplierError>;

    /// Delete a user (soft delete)
    async fn delete_user(&self, user_id: UserId) -> Result<String, ApplierError>;

    // =========================================================================
    // DML Operations - User Tables (User Data Raft Shards)
    // =========================================================================

    /// Insert rows into a user table
    async fn insert_user_data(
        &self,
        table_id: TableId,
        user_id: UserId,
        rows: Vec<Row>,
    ) -> Result<DataResponse, ApplierError>;

    /// Update rows in a user table
    async fn update_user_data(
        &self,
        table_id: TableId,
        user_id: UserId,
        updates: Vec<Row>,
        filter: Option<String>,
    ) -> Result<DataResponse, ApplierError>;

    /// Delete rows from a user table
    async fn delete_user_data(
        &self,
        table_id: TableId,
        user_id: UserId,
        pk_values: Option<Vec<String>>,
    ) -> Result<DataResponse, ApplierError>;

    // =========================================================================
    // DML Operations - Shared Tables (Shared Data Raft Shard)
    // =========================================================================

    /// Insert rows into a shared table
    async fn insert_shared_data(
        &self,
        table_id: TableId,
        rows: Vec<Row>,
    ) -> Result<DataResponse, ApplierError>;

    /// Update rows in a shared table
    async fn update_shared_data(
        &self,
        table_id: TableId,
        updates: Vec<Row>,
        filter: Option<String>,
    ) -> Result<DataResponse, ApplierError>;

    /// Delete rows from a shared table
    async fn delete_shared_data(
        &self,
        table_id: TableId,
        pk_values: Option<Vec<String>>,
    ) -> Result<DataResponse, ApplierError>;

    // =========================================================================
    // Status Methods
    // =========================================================================

    /// Check if this node can accept writes (always true - forwards to leader)
    fn can_accept_writes(&self) -> bool;

    /// Get leader info for forwarding (if not leader)
    fn get_leader_info(&self) -> Option<LeaderInfo>;
}

/// Raft Applier - routes all commands through Raft consensus
///
/// This is the ONLY applier implementation. All commands go through Raft:
/// - Meta commands (DDL, namespaces, users) → Meta Raft group
/// - User data commands → User data shards (by user_id)
/// - Shared data commands → Shared data shard
///
/// Even in single-node mode, we use a single-node Raft cluster for consistency.
pub struct RaftApplier {
    executor: CommandExecutorImpl,
}

impl RaftApplier {
    /// Create a new Raft applier
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self {
            executor: CommandExecutorImpl::new(app_context),
        }
    }

    /// Get the executor
    fn executor(&self) -> &CommandExecutorImpl {
        &self.executor
    }

    /// Get leader info for meta group
    fn get_meta_leader_info(&self) -> Option<LeaderInfo> {
        let app_ctx = self.executor().app_context();
        let executor = app_ctx.executor();
        let raft_exec = executor.as_any().downcast_ref::<RaftExecutor>()?;
        let mgr = raft_exec.manager();
        let leader_node_id = mgr.current_leader(GroupId::Meta)?;

        let config = mgr.config();
        let leader_addr = if leader_node_id == config.node_id {
            config.rpc_addr.clone()
        } else {
            config
                .peers
                .iter()
                .find(|p| p.node_id == leader_node_id)
                .map(|p| p.rpc_addr.clone())?
        };

        Some(LeaderInfo {
            node_id: leader_node_id.as_u64(),
            address: leader_addr,
        })
    }

    /// Propose a meta command to the Meta Raft group
    async fn propose_meta(
        &self,
        command: MetaCommand,
        cmd_type: &str,
    ) -> Result<String, ApplierError> {
        let app_ctx = self.executor().app_context();
        let executor = app_ctx.executor();
        let raft_exec =
            executor.as_any().downcast_ref::<RaftExecutor>().ok_or(ApplierError::NoLeader)?;
        let raft_mgr = raft_exec.manager();

        log::debug!(
            "RaftApplier: Proposing {} to meta Raft (leader={}, we_are_leader={})",
            cmd_type,
            raft_mgr.current_leader(GroupId::Meta).map(|n| n.as_u64()).unwrap_or(0),
            raft_mgr.is_leader(GroupId::Meta)
        );

        let response = raft_mgr
            .propose_meta(command)
            .await
            .map_err(|e| ApplierError::Raft(e.to_string()))?;

        Ok(response.get_message())
    }

    /// Execute a user data command through the appropriate shard
    async fn execute_user_data_cmd(
        &self,
        cmd: UserDataCommand,
    ) -> Result<DataResponse, ApplierError> {
        let app_ctx = self.executor().app_context();
        let user_id = cmd.user_id().clone();

        app_ctx
            .executor()
            .execute_user_data(&user_id, cmd)
            .await
            .map_err(|e| ApplierError::Raft(e.to_string()))
    }

    /// Execute a shared data command
    async fn execute_shared_data_cmd(
        &self,
        cmd: SharedDataCommand,
    ) -> Result<DataResponse, ApplierError> {
        let app_ctx = self.executor().app_context();

        app_ctx
            .executor()
            .execute_shared_data(cmd)
            .await
            .map_err(|e| ApplierError::Raft(e.to_string()))
    }
}

#[async_trait]
impl UnifiedApplier for RaftApplier {
    // =========================================================================
    // DDL Operations
    // =========================================================================

    async fn create_table(
        &self,
        table_id: TableId,
        table_type: TableType,
        table_def: TableDefinition,
    ) -> Result<String, ApplierError> {
        let schema_json = serde_json::to_string(&table_def)
            .map_err(|e| ApplierError::Serialization(e.to_string()))?;

        let cmd = MetaCommand::CreateTable {
            table_id,
            table_type,
            schema_json,
        };
        self.propose_meta(cmd, "CREATE TABLE").await
    }

    async fn alter_table(
        &self,
        table_id: TableId,
        table_def: TableDefinition,
    ) -> Result<String, ApplierError> {
        let schema_json = serde_json::to_string(&table_def)
            .map_err(|e| ApplierError::Serialization(e.to_string()))?;

        let cmd = MetaCommand::AlterTable {
            table_id,
            schema_json,
        };
        self.propose_meta(cmd, "ALTER TABLE").await
    }

    async fn drop_table(&self, table_id: TableId) -> Result<String, ApplierError> {
        let cmd = MetaCommand::DropTable { table_id };
        self.propose_meta(cmd, "DROP TABLE").await
    }

    // =========================================================================
    // Namespace Operations
    // =========================================================================

    async fn create_namespace(
        &self,
        namespace_id: NamespaceId,
        created_by: Option<UserId>,
    ) -> Result<String, ApplierError> {
        let cmd = MetaCommand::CreateNamespace {
            namespace_id,
            created_by,
        };
        self.propose_meta(cmd, "CREATE NAMESPACE").await
    }

    async fn drop_namespace(&self, namespace_id: NamespaceId) -> Result<String, ApplierError> {
        let cmd = MetaCommand::DeleteNamespace { namespace_id };
        self.propose_meta(cmd, "DROP NAMESPACE").await
    }

    // =========================================================================
    // Storage Operations
    // =========================================================================

    async fn create_storage(&self, storage: Storage) -> Result<String, ApplierError> {
        let config_json = serde_json::to_string(&storage)
            .map_err(|e| ApplierError::Serialization(e.to_string()))?;

        let cmd = MetaCommand::RegisterStorage {
            storage_id: storage.storage_id.clone(),
            config_json,
        };
        self.propose_meta(cmd, "CREATE STORAGE").await
    }

    async fn drop_storage(&self, storage_id: StorageId) -> Result<String, ApplierError> {
        let cmd = MetaCommand::UnregisterStorage { storage_id };
        self.propose_meta(cmd, "DROP STORAGE").await
    }

    // =========================================================================
    // User Operations
    // =========================================================================

    async fn create_user(&self, user: User) -> Result<String, ApplierError> {
        let cmd = MetaCommand::CreateUser { user };
        self.propose_meta(cmd, "CREATE USER").await
    }

    async fn update_user(&self, user: User) -> Result<String, ApplierError> {
        let cmd = MetaCommand::UpdateUser { user };
        self.propose_meta(cmd, "UPDATE USER").await
    }

    async fn delete_user(&self, user_id: UserId) -> Result<String, ApplierError> {
        let cmd = MetaCommand::DeleteUser {
            user_id,
            deleted_at: Utc::now(),
        };
        self.propose_meta(cmd, "DELETE USER").await
    }

    // =========================================================================
    // DML Operations - User Tables
    // =========================================================================

    async fn insert_user_data(
        &self,
        table_id: TableId,
        user_id: UserId,
        rows: Vec<Row>,
    ) -> Result<DataResponse, ApplierError> {
        let raft_cmd = UserDataCommand::Insert {
            required_meta_index: 0, // Will be set by RaftExecutor
            table_id,
            user_id,
            rows,
        };
        self.execute_user_data_cmd(raft_cmd).await
    }

    async fn update_user_data(
        &self,
        table_id: TableId,
        user_id: UserId,
        updates: Vec<Row>,
        filter: Option<String>,
    ) -> Result<DataResponse, ApplierError> {
        let raft_cmd = UserDataCommand::Update {
            required_meta_index: 0, // Will be set by RaftExecutor
            table_id,
            user_id,
            updates,
            filter,
        };
        self.execute_user_data_cmd(raft_cmd).await
    }

    async fn delete_user_data(
        &self,
        table_id: TableId,
        user_id: UserId,
        pk_values: Option<Vec<String>>,
    ) -> Result<DataResponse, ApplierError> {
        let raft_cmd = UserDataCommand::Delete {
            required_meta_index: 0, // Will be set by RaftExecutor
            table_id,
            user_id,
            pk_values,
        };
        self.execute_user_data_cmd(raft_cmd).await
    }

    // =========================================================================
    // DML Operations - Shared Tables
    // =========================================================================

    async fn insert_shared_data(
        &self,
        table_id: TableId,
        rows: Vec<Row>,
    ) -> Result<DataResponse, ApplierError> {
        let raft_cmd = SharedDataCommand::Insert {
            required_meta_index: 0, // Will be set by RaftExecutor
            table_id,
            rows,
        };
        self.execute_shared_data_cmd(raft_cmd).await
    }

    async fn update_shared_data(
        &self,
        table_id: TableId,
        updates: Vec<Row>,
        filter: Option<String>,
    ) -> Result<DataResponse, ApplierError> {
        let raft_cmd = SharedDataCommand::Update {
            required_meta_index: 0, // Will be set by RaftExecutor
            table_id,
            updates,
            filter,
        };
        self.execute_shared_data_cmd(raft_cmd).await
    }

    async fn delete_shared_data(
        &self,
        table_id: TableId,
        pk_values: Option<Vec<String>>,
    ) -> Result<DataResponse, ApplierError> {
        let raft_cmd = SharedDataCommand::Delete {
            required_meta_index: 0, // Will be set by RaftExecutor
            table_id,
            pk_values,
        };
        self.execute_shared_data_cmd(raft_cmd).await
    }

    // =========================================================================
    // Status Methods
    // =========================================================================

    fn can_accept_writes(&self) -> bool {
        // Always accept writes - they'll be forwarded to leader if needed
        true
    }

    fn get_leader_info(&self) -> Option<LeaderInfo> {
        self.get_meta_leader_info()
    }
}
