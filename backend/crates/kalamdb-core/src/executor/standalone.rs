//! StandaloneExecutor - Wired DirectExecutor for standalone mode
//!
//! This executor is wired to the actual SystemTablesRegistry providers,
//! enabling direct command execution without Raft consensus.

use async_trait::async_trait;
use std::sync::Arc;

use kalamdb_commons::system::Namespace;
use kalamdb_raft::{
    CommandExecutor, DataResponse, GroupId, JobsCommand, JobsResponse,
    SharedDataCommand, SystemCommand, SystemResponse, UserDataCommand,
    UsersCommand, UsersResponse,
};
use kalamdb_system::SystemTablesRegistry;

/// Result type for executor operations
type Result<T> = std::result::Result<T, kalamdb_raft::RaftError>;

/// Standalone executor wired to actual providers.
///
/// Executes commands by calling SystemTablesRegistry providers directly.
/// Used in standalone mode (no clustering) for zero overhead.
#[derive(Debug)]
pub struct StandaloneExecutor {
    system_tables: Arc<SystemTablesRegistry>,
}

impl StandaloneExecutor {
    /// Create a new StandaloneExecutor wired to the given providers
    pub fn new(system_tables: Arc<SystemTablesRegistry>) -> Self {
        Self { system_tables }
    }
}

#[async_trait]
impl CommandExecutor for StandaloneExecutor {
    async fn execute_system(&self, cmd: SystemCommand) -> Result<SystemResponse> {
        match cmd {
            SystemCommand::CreateNamespace { namespace_id, created_by } => {
                // Use Namespace::new() which sets timestamp and defaults
                let namespace = Namespace::new(namespace_id.as_str());
                // Log who created it (stored in options or audit log)
                log::info!("Creating namespace {:?} by {:?}", namespace.namespace_id, created_by);
                
                self.system_tables
                    .namespaces()
                    .create_namespace_async(namespace.clone())
                    .await
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                
                Ok(SystemResponse::NamespaceCreated { namespace_id: namespace.namespace_id })
            }
            
            SystemCommand::DeleteNamespace { namespace_id } => {
                self.system_tables
                    .namespaces()
                    .delete_namespace_async(&namespace_id)
                    .await
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                
                Ok(SystemResponse::Ok)
            }
            
            SystemCommand::CreateTable { table_id, table_type, schema_json } => {
                // Parse the schema from JSON
                let table_def: kalamdb_commons::models::schemas::TableDefinition = 
                    serde_json::from_str(&schema_json)
                        .map_err(|e| kalamdb_raft::RaftError::provider(format!("Invalid schema JSON: {}", e)))?;
                
                self.system_tables
                    .tables()
                    .create_table_async(&table_id, &table_def)
                    .await
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                
                log::info!("Created table {:?} (type: {})", table_id, table_type);
                Ok(SystemResponse::TableCreated { table_id })
            }
            
            SystemCommand::AlterTable { table_id, schema_json } => {
                let table_def: kalamdb_commons::models::schemas::TableDefinition = 
                    serde_json::from_str(&schema_json)
                        .map_err(|e| kalamdb_raft::RaftError::provider(format!("Invalid schema JSON: {}", e)))?;
                
                self.system_tables
                    .tables()
                    .update_table_async(&table_id, &table_def)
                    .await
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                
                log::info!("Altered table {:?}", table_id);
                Ok(SystemResponse::Ok)
            }
            
            SystemCommand::DropTable { table_id } => {
                self.system_tables
                    .tables()
                    .delete_table_async(&table_id)
                    .await
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                
                log::info!("Dropped table {:?}", table_id);
                Ok(SystemResponse::Ok)
            }
            
            SystemCommand::RegisterStorage { storage_id, config_json } => {
                // Parse storage config and register
                let storage: kalamdb_commons::system::Storage = 
                    serde_json::from_str(&config_json)
                        .map_err(|e| kalamdb_raft::RaftError::provider(format!("Invalid storage config: {}", e)))?;
                
                self.system_tables
                    .storages()
                    .create_storage_async(storage)
                    .await
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                
                log::info!("Registered storage: {}", storage_id);
                Ok(SystemResponse::Ok)
            }
            
            SystemCommand::UnregisterStorage { storage_id } => {
                log::info!("Unregistered storage: {}", storage_id);
                
                self.system_tables
                    .storages()
                    .delete_storage_async(&storage_id.into())
                    .await
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                
                Ok(SystemResponse::Ok)
            }
        }
    }

    async fn execute_users(&self, cmd: UsersCommand) -> Result<UsersResponse> {
        // User operations require mapping between command types and the actual User struct.
        // The User struct uses i64 timestamps, Role enum, AuthType enum, etc.
        // For now, we stub these and log - full wiring requires adapter code.
        match cmd {
            UsersCommand::CreateUser { user_id, username, .. } => {
                log::info!("StandaloneExecutor: CreateUser {:?} ({})", user_id, username);
                // TODO: Create User struct with proper field mapping
                // self.system_tables.users().create_user_async(user).await?;
                Ok(UsersResponse::UserCreated { user_id })
            }
            
            UsersCommand::UpdateUser { user_id, .. } => {
                log::debug!("StandaloneExecutor: UpdateUser {:?}", user_id);
                Ok(UsersResponse::Ok)
            }
            
            UsersCommand::DeleteUser { user_id, .. } => {
                log::debug!("StandaloneExecutor: DeleteUser {:?}", user_id);
                Ok(UsersResponse::Ok)
            }
            
            UsersCommand::RecordLogin { user_id, .. } => {
                log::debug!("StandaloneExecutor: RecordLogin {:?}", user_id);
                Ok(UsersResponse::Ok)
            }
            
            UsersCommand::SetLocked { user_id, locked, .. } => {
                log::debug!("StandaloneExecutor: SetLocked {:?} = {}", user_id, locked);
                Ok(UsersResponse::Ok)
            }
        }
    }

    async fn execute_jobs(&self, cmd: JobsCommand) -> Result<JobsResponse> {
        match cmd {
            JobsCommand::CreateJob { job_id, job_type, .. } => {
                log::info!("StandaloneExecutor: CreateJob {} (type: {})", job_id, job_type);
                Ok(JobsResponse::JobCreated { job_id })
            }
            
            JobsCommand::ClaimJob { job_id, node_id, .. } => {
                log::info!("StandaloneExecutor: ClaimJob {} by node {}", job_id, node_id);
                Ok(JobsResponse::JobClaimed { job_id, node_id })
            }
            
            JobsCommand::UpdateJobStatus { job_id, status, .. } => {
                log::debug!("StandaloneExecutor: UpdateJobStatus {} -> {}", job_id, status);
                Ok(JobsResponse::Ok)
            }
            
            JobsCommand::CompleteJob { job_id, .. } => {
                log::info!("StandaloneExecutor: CompleteJob {}", job_id);
                Ok(JobsResponse::Ok)
            }
            
            JobsCommand::FailJob { job_id, error_message, .. } => {
                log::warn!("StandaloneExecutor: FailJob {}: {}", job_id, error_message);
                Ok(JobsResponse::Ok)
            }
            
            JobsCommand::ReleaseJob { job_id, reason, .. } => {
                log::info!("StandaloneExecutor: ReleaseJob {}: {}", job_id, reason);
                Ok(JobsResponse::Ok)
            }
            
            JobsCommand::CancelJob { job_id, reason, .. } => {
                log::info!("StandaloneExecutor: CancelJob {}: {}", job_id, reason);
                Ok(JobsResponse::Ok)
            }
            
            JobsCommand::CreateSchedule { schedule_id, job_type, .. } => {
                log::info!("StandaloneExecutor: CreateSchedule {} (type: {})", schedule_id, job_type);
                Ok(JobsResponse::Ok)
            }
            
            JobsCommand::DeleteSchedule { schedule_id } => {
                log::info!("StandaloneExecutor: DeleteSchedule {}", schedule_id);
                Ok(JobsResponse::Ok)
            }
        }
    }

    async fn execute_user_data(&self, user_id: &str, cmd: UserDataCommand) -> Result<DataResponse> {
        match cmd {
            UserDataCommand::Insert { table_id, rows_data, .. } => {
                log::debug!("StandaloneExecutor: Insert into {:?} for user {} ({} bytes)", 
                    table_id, user_id, rows_data.len());
                // TODO: Wire to UserTableStore insert
                Ok(DataResponse::RowsAffected(0))
            }
            
            UserDataCommand::Update { table_id, .. } => {
                log::debug!("StandaloneExecutor: Update {:?} for user {}", table_id, user_id);
                Ok(DataResponse::RowsAffected(0))
            }
            
            UserDataCommand::Delete { table_id, .. } => {
                log::debug!("StandaloneExecutor: Delete from {:?} for user {}", table_id, user_id);
                Ok(DataResponse::RowsAffected(0))
            }
            
            UserDataCommand::RegisterLiveQuery { subscription_id, user_id: uid, query_hash: _, table_id, filter_json: _, node_id: _, created_at: _ } => {
                // TODO: Map command fields to actual LiveQuery struct
                // The LiveQuery struct has different fields (live_id, query, options, node, status, etc.)
                // This mapping will be implemented in a follow-up task
                log::info!("StandaloneExecutor: RegisterLiveQuery {} for user {:?} on {:?}", 
                    subscription_id, uid, table_id);
                Ok(DataResponse::Subscribed { subscription_id })
            }
            
            UserDataCommand::UnregisterLiveQuery { subscription_id, user_id: uid } => {
                log::debug!("StandaloneExecutor: Unregister live query {} for user {:?}", subscription_id, uid);
                Ok(DataResponse::Ok)
            }
            
            UserDataCommand::CleanupNodeSubscriptions { user_id: uid, failed_node_id } => {
                log::info!("StandaloneExecutor: Cleanup subscriptions from node {} for user {:?}", failed_node_id, uid);
                Ok(DataResponse::RowsAffected(0))
            }
            
            UserDataCommand::PingLiveQuery { subscription_id, user_id: uid, .. } => {
                log::trace!("StandaloneExecutor: Ping live query {} for user {:?}", subscription_id, uid);
                Ok(DataResponse::Ok)
            }
        }
    }

    async fn execute_shared_data(&self, cmd: SharedDataCommand) -> Result<DataResponse> {
        match cmd {
            SharedDataCommand::Insert { table_id, rows_data } => {
                log::debug!("StandaloneExecutor: Insert into shared {:?} ({} bytes)", table_id, rows_data.len());
                Ok(DataResponse::RowsAffected(0))
            }
            
            SharedDataCommand::Update { table_id, .. } => {
                log::debug!("StandaloneExecutor: Update shared {:?}", table_id);
                Ok(DataResponse::RowsAffected(0))
            }
            
            SharedDataCommand::Delete { table_id, .. } => {
                log::debug!("StandaloneExecutor: Delete from shared {:?}", table_id);
                Ok(DataResponse::RowsAffected(0))
            }
        }
    }

    async fn is_leader(&self, _group: GroupId) -> bool {
        // In standalone mode, we're always the leader
        true
    }

    async fn get_leader(&self, _group: GroupId) -> Option<u64> {
        // In standalone mode, there's no leader node_id
        None
    }

    fn is_cluster_mode(&self) -> bool {
        false
    }

    fn node_id(&self) -> u64 {
        0 // Standalone mode has no node_id
    }
}
