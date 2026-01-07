//! DirectExecutor - Standalone mode command execution
//!
//! Executes commands directly against providers without Raft consensus.
//! Zero overhead for standalone mode.

use async_trait::async_trait;
use openraft::ServerState;

use kalamdb_commons::models::{NodeId, UserId};

use crate::cluster_types::NodeStatus;
use crate::commands::{
    DataResponse, JobsCommand, JobsResponse, SharedDataCommand, SystemCommand,
    SystemResponse, UserDataCommand, UsersCommand, UsersResponse,
};
use crate::error::Result;
use crate::executor::{ClusterInfo, ClusterNodeInfo, CommandExecutor};
use crate::group_id::GroupId;

/// Marker trait for the system tables registry
/// 
/// This allows us to avoid a direct dependency on kalamdb-system in the trait,
/// making the crate more modular. The actual implementation is provided when
/// constructing DirectExecutor.
#[allow(dead_code)]
pub trait SystemTablesAccess: Send + Sync {
    // Namespace operations
    fn create_namespace(&self, namespace_id: &str, created_by: Option<&str>) -> Result<()>;
    fn delete_namespace(&self, namespace_id: &str) -> Result<()>;
    
    // Table operations (schema only, not data)
    fn create_table(&self, table_id: &str, table_type: &str, schema_json: &str) -> Result<()>;
    fn alter_table(&self, table_id: &str, schema_json: &str) -> Result<()>;
    fn drop_table(&self, table_id: &str) -> Result<()>;
    
    // Storage operations
    fn register_storage(&self, storage_id: &str, config_json: &str) -> Result<()>;
    fn unregister_storage(&self, storage_id: &str) -> Result<()>;
    
    // User operations
    fn create_user(&self, user_id: &str, username: &str, password_hash: &str, role: &str) -> Result<()>;
    fn update_user(&self, user_id: &str, username: Option<&str>, password_hash: Option<&str>, role: Option<&str>) -> Result<()>;
    fn delete_user(&self, user_id: &str) -> Result<()>;
    
    // Job operations
    fn create_job(&self, job_id: &str, job_type: &str, namespace_id: Option<&str>, table_name: Option<&str>) -> Result<()>;
    fn claim_job(&self, job_id: &str, node_id: NodeId) -> Result<()>;
    fn update_job_status(&self, job_id: &str, status: &str) -> Result<()>;
    fn complete_job(&self, job_id: &str, result_json: Option<&str>) -> Result<()>;
    fn fail_job(&self, job_id: &str, error_message: &str) -> Result<()>;
    fn release_job(&self, job_id: &str, reason: &str) -> Result<()>;
    fn cancel_job(&self, job_id: &str, reason: &str) -> Result<()>;
}

/// Direct executor for standalone mode.
///
/// Executes commands by calling providers directly, bypassing Raft.
/// This provides zero overhead for single-node deployments.
#[derive(Debug)]
pub struct DirectExecutor {
    // In a real implementation, this would be Arc<SystemTablesRegistry>
    // For now, we stub it to avoid circular dependencies
    _marker: std::marker::PhantomData<()>,
}

impl DirectExecutor {
    /// Create a new DirectExecutor
    ///
    /// In the full implementation, this would take Arc<SystemTablesRegistry>
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl Default for DirectExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CommandExecutor for DirectExecutor {
    async fn execute_system(&self, cmd: SystemCommand) -> Result<SystemResponse> {
        // In standalone mode, we would call the providers directly here.
        // For now, return a stub response until wiring is complete.
        match cmd {
            SystemCommand::CreateNamespace { namespace_id, .. } => {
                // TODO: Call self.registry.namespaces().create_namespace_async(...)
                log::debug!("DirectExecutor: CreateNamespace {:?}", namespace_id);
                Ok(SystemResponse::NamespaceCreated { namespace_id })
            }
            SystemCommand::DeleteNamespace { namespace_id } => {
                log::debug!("DirectExecutor: DeleteNamespace {:?}", namespace_id);
                Ok(SystemResponse::Ok)
            }
            SystemCommand::CreateTable { table_id, .. } => {
                log::debug!("DirectExecutor: CreateTable {:?}", table_id);
                Ok(SystemResponse::TableCreated { table_id })
            }
            SystemCommand::AlterTable { table_id, .. } => {
                log::debug!("DirectExecutor: AlterTable {:?}", table_id);
                Ok(SystemResponse::Ok)
            }
            SystemCommand::DropTable { table_id } => {
                log::debug!("DirectExecutor: DropTable {:?}", table_id);
                Ok(SystemResponse::Ok)
            }
            SystemCommand::RegisterStorage { storage_id, .. } => {
                log::debug!("DirectExecutor: RegisterStorage {}", storage_id);
                Ok(SystemResponse::Ok)
            }
            SystemCommand::UnregisterStorage { storage_id } => {
                log::debug!("DirectExecutor: UnregisterStorage {}", storage_id);
                Ok(SystemResponse::Ok)
            }
        }
    }

    async fn execute_users(&self, cmd: UsersCommand) -> Result<UsersResponse> {
        match cmd {
            UsersCommand::CreateUser { user } => {
                log::debug!("DirectExecutor: CreateUser {:?}", user.id);
                Ok(UsersResponse::UserCreated { user_id: user.id })
            }
            UsersCommand::UpdateUser { user } => {
                log::debug!("DirectExecutor: UpdateUser {:?}", user.id);
                Ok(UsersResponse::Ok)
            }
            UsersCommand::DeleteUser { user_id, .. } => {
                log::debug!("DirectExecutor: DeleteUser {:?}", user_id);
                Ok(UsersResponse::Ok)
            }
            UsersCommand::RecordLogin { user_id, .. } => {
                log::debug!("DirectExecutor: RecordLogin {:?}", user_id);
                Ok(UsersResponse::Ok)
            }
            UsersCommand::SetLocked { user_id, .. } => {
                log::debug!("DirectExecutor: SetLocked {:?}", user_id);
                Ok(UsersResponse::Ok)
            }
        }
    }

    async fn execute_jobs(&self, cmd: JobsCommand) -> Result<JobsResponse> {
        match cmd {
            JobsCommand::CreateJob { job_id, .. } => {
                log::debug!("DirectExecutor: CreateJob {}", job_id);
                Ok(JobsResponse::JobCreated { job_id })
            }
            JobsCommand::ClaimJob { job_id, node_id, .. } => {
                log::debug!("DirectExecutor: ClaimJob {} by node {}", job_id, node_id);
                Ok(JobsResponse::JobClaimed { job_id, node_id })
            }
            JobsCommand::UpdateJobStatus { job_id, .. } => {
                log::debug!("DirectExecutor: UpdateJobStatus {}", job_id);
                Ok(JobsResponse::Ok)
            }
            JobsCommand::CompleteJob { job_id, .. } => {
                log::debug!("DirectExecutor: CompleteJob {}", job_id);
                Ok(JobsResponse::Ok)
            }
            JobsCommand::FailJob { job_id, .. } => {
                log::debug!("DirectExecutor: FailJob {}", job_id);
                Ok(JobsResponse::Ok)
            }
            JobsCommand::ReleaseJob { job_id, .. } => {
                log::debug!("DirectExecutor: ReleaseJob {}", job_id);
                Ok(JobsResponse::Ok)
            }
            JobsCommand::CancelJob { job_id, .. } => {
                log::debug!("DirectExecutor: CancelJob {}", job_id);
                Ok(JobsResponse::Ok)
            }
            JobsCommand::CreateSchedule { schedule_id, .. } => {
                log::debug!("DirectExecutor: CreateSchedule {}", schedule_id);
                Ok(JobsResponse::Ok)
            }
            JobsCommand::DeleteSchedule { schedule_id } => {
                log::debug!("DirectExecutor: DeleteSchedule {}", schedule_id);
                Ok(JobsResponse::Ok)
            }
        }
    }

    async fn execute_user_data(&self, user_id: &UserId, cmd: UserDataCommand) -> Result<DataResponse> {
        match cmd {
            UserDataCommand::Insert { table_id, .. } => {
                log::debug!("DirectExecutor: Insert into {:?} for user {}", table_id, user_id);
                Ok(DataResponse::RowsAffected(0)) // Stub
            }
            UserDataCommand::Update { table_id, .. } => {
                log::debug!("DirectExecutor: Update {:?} for user {}", table_id, user_id);
                Ok(DataResponse::RowsAffected(0))
            }
            UserDataCommand::Delete { table_id, .. } => {
                log::debug!("DirectExecutor: Delete from {:?} for user {}", table_id, user_id);
                Ok(DataResponse::RowsAffected(0))
            }
            UserDataCommand::RegisterLiveQuery { subscription_id, .. } => {
                log::debug!("DirectExecutor: RegisterLiveQuery {} for user {}", subscription_id, user_id);
                Ok(DataResponse::Subscribed { subscription_id })
            }
            UserDataCommand::UnregisterLiveQuery { subscription_id, .. } => {
                log::debug!("DirectExecutor: UnregisterLiveQuery {} for user {}", subscription_id, user_id);
                Ok(DataResponse::Ok)
            }
            UserDataCommand::CleanupNodeSubscriptions { failed_node_id, .. } => {
                log::debug!("DirectExecutor: CleanupNodeSubscriptions node {} for user {}", failed_node_id, user_id);
                Ok(DataResponse::RowsAffected(0))
            }
            UserDataCommand::PingLiveQuery { subscription_id, .. } => {
                log::debug!("DirectExecutor: PingLiveQuery {} for user {}", subscription_id, user_id);
                Ok(DataResponse::Ok)
            }
        }
    }

    async fn execute_shared_data(&self, cmd: SharedDataCommand) -> Result<DataResponse> {
        match cmd {
            SharedDataCommand::Insert { table_id, .. } => {
                log::debug!("DirectExecutor: Insert into shared {:?}", table_id);
                Ok(DataResponse::RowsAffected(0))
            }
            SharedDataCommand::Update { table_id, .. } => {
                log::debug!("DirectExecutor: Update shared {:?}", table_id);
                Ok(DataResponse::RowsAffected(0))
            }
            SharedDataCommand::Delete { table_id, .. } => {
                log::debug!("DirectExecutor: Delete from shared {:?}", table_id);
                Ok(DataResponse::RowsAffected(0))
            }
        }
    }

    async fn is_leader(&self, _group: GroupId) -> bool {
        // In standalone mode, we're always the leader
        true
    }

    async fn get_leader(&self, _group: GroupId) -> Option<NodeId> {
        // In standalone mode, there's no leader node_id
        None
    }

    fn is_cluster_mode(&self) -> bool {
        false
    }

    fn node_id(&self) -> NodeId {
        NodeId::default() // Standalone mode uses default node_id
    }
    
    fn get_cluster_info(&self) -> ClusterInfo {
        // Standalone mode - single node, always leader
        ClusterInfo {
            cluster_id: "standalone".to_string(),
            current_node_id: NodeId::default(),
            is_cluster_mode: false,
            nodes: vec![ClusterNodeInfo {
                node_id: NodeId::default(),
                role: ServerState::Leader, // Standalone is effectively always leader
                status: NodeStatus::Active,
                rpc_addr: "".to_string(),
                api_addr: "localhost:8080".to_string(),
                is_self: true,
                is_leader: true,
                groups_leading: 0,
                total_groups: 0,
                current_term: None,
                last_applied_log: None,
                millis_since_last_heartbeat: None,
                replication_lag: None,
            }],
            total_groups: 0,
            user_shards: 0,
            shared_shards: 0,
            current_term: 0,
            last_log_index: None,
            last_applied: None,
            millis_since_quorum_ack: None,
        }
    }
    
    async fn start(&self) -> Result<()> {
        // No-op for standalone mode
        Ok(())
    }
    
    async fn initialize_cluster(&self) -> Result<()> {
        // No-op for standalone mode
        Ok(())
    }
    
    async fn shutdown(&self) -> Result<()> {
        // No-op for standalone mode
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_direct_executor_standalone() {
        let executor = DirectExecutor::new();
        
        assert!(!executor.is_cluster_mode());
        assert_eq!(executor.node_id(), NodeId::default());
        assert!(executor.is_leader(GroupId::MetaSystem).await);
        assert!(executor.get_leader(GroupId::MetaSystem).await.is_none());
    }

    #[tokio::test]
    async fn test_execute_system_create_namespace() {
        use kalamdb_commons::models::NamespaceId;
        
        let executor = DirectExecutor::new();
        let cmd = SystemCommand::CreateNamespace {
            namespace_id: NamespaceId::new("test_ns"),
            created_by: Some("user1".to_string()),
        };
        
        let response = executor.execute_system(cmd).await.unwrap();
        assert!(matches!(response, SystemResponse::NamespaceCreated { .. }));
    }

    #[tokio::test]
    async fn test_execute_jobs_create() {
        use kalamdb_commons::models::{JobType, NamespaceId, TableName};
        use kalamdb_commons::JobId;
        
        let executor = DirectExecutor::new();
        let cmd = JobsCommand::CreateJob {
            job_id: JobId::from("job_123"),
            job_type: JobType::Flush,
            namespace_id: Some(NamespaceId::from("ns")),
            table_name: Some(TableName::from("table")),
            config_json: None,
            created_at: chrono::Utc::now(),
        };
        
        let response = executor.execute_jobs(cmd).await.unwrap();
        assert!(matches!(response, JobsResponse::JobCreated { .. }));
    }
}
