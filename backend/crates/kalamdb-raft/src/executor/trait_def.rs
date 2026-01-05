//! CommandExecutor trait definition

use async_trait::async_trait;
use std::fmt::Debug;

use crate::commands::{
    DataResponse, JobsCommand, JobsResponse, SharedDataCommand, SystemCommand, 
    SystemResponse, UserDataCommand, UsersCommand, UsersResponse,
};
use crate::error::Result;
use crate::group_id::GroupId;

/// Unified interface for executing commands in both standalone and cluster modes.
///
/// This trait eliminates the need for if/else checks throughout the codebase.
/// Handlers simply call `ctx.executor().execute_*()` and the correct implementation
/// is invoked based on the server's configuration.
///
/// # Implementations
///
/// - [`DirectExecutor`]: Standalone mode - calls providers directly, zero overhead
/// - `RaftExecutor`: Cluster mode - routes commands through Raft consensus
///
/// # Example
///
/// ```rust,ignore
/// // In a DDL handler:
/// async fn create_table(ctx: &AppContext, table: TableDefinition) -> Result<()> {
///     let cmd = SystemCommand::CreateTable { 
///         table_id: table.id.clone(),
///         table_type: "user".to_string(),
///         schema_json: serde_json::to_string(&table)?,
///     };
///     ctx.executor().execute_system(cmd).await?;
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait CommandExecutor: Send + Sync + Debug {
    /// Execute a system metadata command (namespaces, tables, storages)
    async fn execute_system(&self, cmd: SystemCommand) -> Result<SystemResponse>;

    /// Execute a users command (user accounts, authentication)
    async fn execute_users(&self, cmd: UsersCommand) -> Result<UsersResponse>;

    /// Execute a jobs command (background job coordination)
    async fn execute_jobs(&self, cmd: JobsCommand) -> Result<JobsResponse>;

    /// Execute a user data command (routed by user_id to correct shard)
    async fn execute_user_data(&self, user_id: &str, cmd: UserDataCommand) -> Result<DataResponse>;

    /// Execute a shared data command (routed to shared shard)
    async fn execute_shared_data(&self, cmd: SharedDataCommand) -> Result<DataResponse>;

    /// Check if this node is the leader for a specific group
    ///
    /// In standalone mode, always returns true.
    /// In cluster mode, checks Raft leadership.
    async fn is_leader(&self, group: GroupId) -> bool;

    /// Get the leader node ID for a group, if known
    ///
    /// In standalone mode, returns None (there's only one node).
    /// In cluster mode, returns the leader's node_id.
    async fn get_leader(&self, group: GroupId) -> Option<u64>;

    /// Returns true if running in cluster mode
    fn is_cluster_mode(&self) -> bool;

    /// Get the current node's ID (0 for standalone)
    fn node_id(&self) -> u64;
}
