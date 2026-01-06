//! CommandExecutor trait definition

use async_trait::async_trait;
use std::fmt::Debug;

use crate::cluster_types::{NodeRole, NodeStatus};
use crate::commands::{
    DataResponse, JobsCommand, JobsResponse, SharedDataCommand, SystemCommand, 
    SystemResponse, UserDataCommand, UsersCommand, UsersResponse,
};
use crate::error::Result;
use crate::group_id::GroupId;

/// Information about a cluster node
#[derive(Debug, Clone)]
pub struct ClusterNodeInfo {
    /// Node ID (1, 2, 3, ...)
    pub node_id: u64,
    /// Node role derived from OpenRaft ServerState
    pub role: NodeRole,
    /// Node status (active, offline, joining, unknown)
    pub status: NodeStatus,
    /// RPC address for Raft communication
    pub rpc_addr: String,
    /// HTTP API address
    pub api_addr: String,
    /// Whether this is the current node
    pub is_self: bool,
    /// Whether this node is leader for MetaSystem group
    pub is_leader: bool,
    /// Number of groups this node leads (for multi-group Raft)
    pub groups_leading: u32,
    /// Total number of Raft groups
    pub total_groups: u32,
    /// Current Raft term (from leader's perspective, if known)
    pub current_term: Option<u64>,
    /// Last applied log index (for this node if is_self, otherwise from replication metrics)
    pub last_applied_log: Option<u64>,
    /// Milliseconds since last heartbeat response (only for leader viewing followers)
    pub millis_since_last_heartbeat: Option<u64>,
    /// Replication lag in log entries (only for leader viewing followers)
    pub replication_lag: Option<u64>,
}

/// Cluster status information
#[derive(Debug, Clone)]
pub struct ClusterInfo {
    /// Cluster ID
    pub cluster_id: String,
    /// Current node ID
    pub current_node_id: u64,
    /// Whether in cluster mode
    pub is_cluster_mode: bool,
    /// All nodes in the cluster
    pub nodes: Vec<ClusterNodeInfo>,
    /// Total number of Raft groups
    pub total_groups: u32,
    /// Number of user data shards
    pub user_shards: u32,
    /// Number of shared data shards  
    pub shared_shards: u32,
    /// Current Raft term (from MetaSystem group)
    pub current_term: u64,
    /// Last log index (from MetaSystem group)
    pub last_log_index: Option<u64>,
    /// Last applied log index (from MetaSystem group)
    pub last_applied: Option<u64>,
    /// Milliseconds since quorum acknowledgment (leader health indicator)
    pub millis_since_quorum_ack: Option<u64>,
}

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
    
    /// Get cluster information including all nodes, their roles, and status
    ///
    /// In standalone mode, returns a single-node cluster info.
    /// In cluster mode, returns info about all configured nodes.
    fn get_cluster_info(&self) -> ClusterInfo;
    
    /// Start the executor (initialize Raft groups, begin consensus participation)
    ///
    /// In standalone mode, this is a no-op.
    /// In cluster mode, this starts all Raft groups.
    async fn start(&self) -> Result<()>;
    
    /// Initialize the cluster (bootstrap first node only)
    ///
    /// In standalone mode, this is a no-op.
    /// In cluster mode, this bootstraps all Raft groups with initial membership.
    /// Only call this on the first node when creating a new cluster.
    async fn initialize_cluster(&self) -> Result<()>;
    
    /// Gracefully shutdown the executor
    ///
    /// In standalone mode, this is a no-op.
    /// In cluster mode, this stops all Raft groups and optionally transfers leadership.
    async fn shutdown(&self) -> Result<()>;
}
