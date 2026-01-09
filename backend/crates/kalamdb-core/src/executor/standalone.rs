//! StandaloneExecutor - Wired DirectExecutor for standalone mode
//!
//! This executor is wired to the actual SystemTablesRegistry providers,
//! enabling direct command execution without Raft consensus.

use async_trait::async_trait;
use std::sync::Arc;

use kalamdb_commons::models::{NodeId, UserId};
use kalamdb_commons::system::{Job, Namespace};
use kalamdb_commons::JobStatus;
use kalamdb_raft::{
    ClusterInfo, ClusterNodeInfo, CommandExecutor, DataResponse, GroupId,
    MetaCommand, MetaResponse, NodeRole, NodeStatus, SharedDataCommand, UserDataCommand,
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
    async fn execute_meta(&self, cmd: MetaCommand) -> Result<MetaResponse> {
        match cmd {
            // =================================================================
            // Namespace Operations
            // =================================================================
            MetaCommand::CreateNamespace { namespace_id, created_by } => {
                let namespace = Namespace::new(namespace_id.as_str());
                log::info!("Creating namespace {:?} by {:?}", namespace.namespace_id, created_by);
                
                self.system_tables
                    .namespaces()
                    .create_namespace(namespace.clone())
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                
                Ok(MetaResponse::NamespaceCreated { namespace_id: namespace.namespace_id })
            }
            
            MetaCommand::DeleteNamespace { namespace_id } => {
                self.system_tables
                    .namespaces()
                    .delete_namespace(&namespace_id)
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                
                Ok(MetaResponse::Ok)
            }
            
            // =================================================================
            // Table Operations
            // =================================================================
            MetaCommand::CreateTable { table_id, table_type, schema_json } => {
                let table_def: kalamdb_commons::models::schemas::TableDefinition = 
                    serde_json::from_str(&schema_json)
                        .map_err(|e| kalamdb_raft::RaftError::provider(format!("Invalid schema JSON: {}", e)))?;
                
                self.system_tables
                    .tables()
                    .create_table(&table_id, &table_def)
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                
                log::info!("Created table {:?} (type: {:?})", table_id, table_type);
                Ok(MetaResponse::TableCreated { table_id })
            }
            
            MetaCommand::AlterTable { table_id, schema_json } => {
                let table_def: kalamdb_commons::models::schemas::TableDefinition = 
                    serde_json::from_str(&schema_json)
                        .map_err(|e| kalamdb_raft::RaftError::provider(format!("Invalid schema JSON: {}", e)))?;
                
                self.system_tables
                    .tables()
                    .update_table(&table_id, &table_def)
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                
                log::info!("Altered table {:?}", table_id);
                Ok(MetaResponse::Ok)
            }
            
            MetaCommand::DropTable { table_id } => {
                self.system_tables
                    .tables()
                    .delete_table(&table_id)
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                
                log::info!("Dropped table {:?}", table_id);
                Ok(MetaResponse::Ok)
            }
            
            // =================================================================
            // Storage Operations
            // =================================================================
            MetaCommand::RegisterStorage { storage_id, config_json } => {
                let storage: kalamdb_commons::system::Storage = 
                    serde_json::from_str(&config_json)
                        .map_err(|e| kalamdb_raft::RaftError::provider(format!("Invalid storage config: {}", e)))?;
                
                self.system_tables
                    .storages()
                    .create_storage(storage)
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                
                log::info!("Registered storage: {}", storage_id);
                Ok(MetaResponse::Ok)
            }
            
            MetaCommand::UnregisterStorage { storage_id } => {
                self.system_tables
                    .storages()
                    .delete_storage(&storage_id)
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                
                log::info!("Unregistered storage: {}", storage_id);
                Ok(MetaResponse::Ok)
            }
            
            // =================================================================
            // User Operations
            // =================================================================
            MetaCommand::CreateUser { user } => {
                log::info!("StandaloneExecutor: CreateUser {:?} ({})", user.id, user.username);
                
                self.system_tables
                    .users()
                    .create_user(user.clone())
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                
                Ok(MetaResponse::UserCreated { user_id: user.id })
            }
            
            MetaCommand::UpdateUser { user } => {
                log::debug!("StandaloneExecutor: UpdateUser {:?}", user.id);
                
                self.system_tables
                    .users()
                    .update_user(user)
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                
                Ok(MetaResponse::Ok)
            }
            
            MetaCommand::DeleteUser { user_id, deleted_at: _ } => {
                log::debug!("StandaloneExecutor: DeleteUser {:?}", user_id);
                
                self.system_tables
                    .users()
                    .delete_user(&user_id)
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                
                Ok(MetaResponse::Ok)
            }
            
            MetaCommand::RecordLogin { user_id, logged_in_at } => {
                log::debug!("StandaloneExecutor: RecordLogin {:?} at {:?}", user_id, logged_in_at);
                // Get the user, update last_login_at, and save
                if let Some(mut user) = self.system_tables.users().get_user_by_id(&user_id)
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))? 
                {
                    user.last_login_at = Some(logged_in_at.timestamp());
                    self.system_tables
                        .users()
                        .update_user(user)
                        .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                }
                Ok(MetaResponse::Ok)
            }
            
            MetaCommand::SetUserLocked { user_id, locked_until, updated_at: _ } => {
                log::debug!("StandaloneExecutor: SetUserLocked {:?} until {:?}", user_id, locked_until);
                // Get the user, update locked_until, and save
                if let Some(mut user) = self.system_tables.users().get_user_by_id(&user_id)
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?
                {
                    user.locked_until = locked_until;
                    self.system_tables
                        .users()
                        .update_user(user)
                        .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                }
                Ok(MetaResponse::Ok)
            }
            
            // =================================================================
            // Job Operations
            // =================================================================
            MetaCommand::CreateJob { job_id, job_type, namespace_id, table_name, config_json, created_at } => {
                log::info!("StandaloneExecutor: CreateJob {} (type: {:?})", job_id, job_type);
                
                // Build parameters JSON from namespace_id, table_name, and config
                let mut params = serde_json::Map::new();
                if let Some(ns_id) = &namespace_id {
                    params.insert("namespace_id".to_string(), serde_json::Value::String(ns_id.as_str().to_string()));
                }
                if let Some(tbl_name) = &table_name {
                    params.insert("table_name".to_string(), serde_json::Value::String(tbl_name.as_str().to_string()));
                }
                if let Some(cfg) = &config_json {
                    if let Ok(cfg_val) = serde_json::from_str::<serde_json::Value>(cfg) {
                        if let serde_json::Value::Object(obj) = cfg_val {
                            for (k, v) in obj {
                                params.insert(k, v);
                            }
                        }
                    }
                }
                let parameters = if params.is_empty() { None } else { Some(serde_json::to_string(&params).unwrap_or_default()) };
                let now = created_at.timestamp_millis();
                
                let job = Job {
                    job_id: job_id.clone(),
                    job_type,
                    status: JobStatus::New,
                    node_id: NodeId::default(),
                    parameters,
                    idempotency_key: None,
                    retry_count: 0,
                    max_retries: 3,
                    message: None,
                    exception_trace: None,
                    queue: None,
                    priority: None,
                    created_at: now,
                    updated_at: now,
                    started_at: None,
                    finished_at: None,
                    memory_used: None,
                    cpu_used: None,
                };
                
                self.system_tables
                    .jobs()
                    .create_job(job)
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                
                Ok(MetaResponse::JobCreated { job_id })
            }
            
            MetaCommand::ClaimJob { job_id, node_id, claimed_at } => {
                log::info!("StandaloneExecutor: ClaimJob {} by node {}", job_id, node_id);
                
                if let Some(mut job) = self.system_tables.jobs().get_job(&job_id)
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?
                {
                    job.node_id = node_id;
                    job.started_at = Some(claimed_at.timestamp_millis());
                    job.status = JobStatus::Running;
                    job.updated_at = chrono::Utc::now().timestamp_millis();
                    
                    self.system_tables
                        .jobs()
                        .update_job(job)
                        .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                }
                
                Ok(MetaResponse::JobClaimed { job_id, node_id })
            }
            
            MetaCommand::UpdateJobStatus { job_id, status, updated_at: _ } => {
                log::debug!("StandaloneExecutor: UpdateJobStatus {} -> {}", job_id, status);
                
                if let Some(mut job) = self.system_tables.jobs().get_job(&job_id)
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?
                {
                    job.status = status.parse().unwrap_or(JobStatus::Failed);
                    job.updated_at = chrono::Utc::now().timestamp_millis();
                    
                    self.system_tables
                        .jobs()
                        .update_job(job)
                        .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                }
                
                Ok(MetaResponse::Ok)
            }
            
            MetaCommand::CompleteJob { job_id, result_json, completed_at } => {
                log::info!("StandaloneExecutor: CompleteJob {}", job_id);
                
                if let Some(mut job) = self.system_tables.jobs().get_job(&job_id)
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?
                {
                    job.status = JobStatus::Completed;
                    job.message = result_json;
                    job.finished_at = Some(completed_at.timestamp_millis());
                    job.updated_at = chrono::Utc::now().timestamp_millis();
                    
                    self.system_tables
                        .jobs()
                        .update_job(job)
                        .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                }
                
                Ok(MetaResponse::Ok)
            }
            
            MetaCommand::FailJob { job_id, error_message, failed_at: _ } => {
                log::warn!("StandaloneExecutor: FailJob {}: {}", job_id, error_message);
                
                if let Some(mut job) = self.system_tables.jobs().get_job(&job_id)
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?
                {
                    job.status = JobStatus::Failed;
                    job.message = Some(error_message);
                    job.finished_at = Some(chrono::Utc::now().timestamp_millis());
                    job.updated_at = chrono::Utc::now().timestamp_millis();
                    
                    self.system_tables
                        .jobs()
                        .update_job(job)
                        .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                }
                
                Ok(MetaResponse::Ok)
            }
            
            MetaCommand::ReleaseJob { job_id, reason: _, released_at: _ } => {
                log::info!("StandaloneExecutor: ReleaseJob {}", job_id);
                
                if let Some(mut job) = self.system_tables.jobs().get_job(&job_id)
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?
                {
                    job.status = JobStatus::New;
                    job.node_id = NodeId::default();
                    job.started_at = None;
                    job.updated_at = chrono::Utc::now().timestamp_millis();
                    
                    self.system_tables
                        .jobs()
                        .update_job(job)
                        .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                }
                
                Ok(MetaResponse::Ok)
            }
            
            MetaCommand::CancelJob { job_id, reason, cancelled_at } => {
                log::info!("StandaloneExecutor: CancelJob {}: {}", job_id, reason);
                
                if let Some(mut job) = self.system_tables.jobs().get_job(&job_id)
                    .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?
                {
                    job.status = JobStatus::Cancelled;
                    job.finished_at = Some(cancelled_at.timestamp_millis());
                    job.message = Some(reason);
                    job.updated_at = chrono::Utc::now().timestamp_millis();
                    
                    self.system_tables
                        .jobs()
                        .update_job(job)
                        .map_err(|e| kalamdb_raft::RaftError::provider(e.to_string()))?;
                }
                
                Ok(MetaResponse::Ok)
            }
            
            MetaCommand::CreateSchedule { schedule_id, .. } => {
                log::info!("StandaloneExecutor: CreateSchedule {}", schedule_id);
                Ok(MetaResponse::Ok)
            }
            
            MetaCommand::DeleteSchedule { schedule_id } => {
                log::info!("StandaloneExecutor: DeleteSchedule {}", schedule_id);
                Ok(MetaResponse::Ok)
            }
        }
    }

    async fn execute_user_data(&self, user_id: &UserId, cmd: UserDataCommand) -> Result<DataResponse> {
        match cmd {
            UserDataCommand::Insert { table_id, rows_data, .. } => {
                log::debug!("StandaloneExecutor: Insert into {:?} for user {} ({} bytes)", 
                    table_id, user_id, rows_data.len());
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
            
            UserDataCommand::RegisterLiveQuery { subscription_id, user_id: uid, table_id, .. } => {
                log::info!("StandaloneExecutor: RegisterLiveQuery {} for user {:?} on {:?}", 
                    subscription_id, uid, table_id);
                Ok(DataResponse::Subscribed { subscription_id })
            }
            
            UserDataCommand::UnregisterLiveQuery { subscription_id, user_id: uid, .. } => {
                log::debug!("StandaloneExecutor: Unregister live query {} for user {:?}", subscription_id, uid);
                Ok(DataResponse::Ok)
            }
            
            UserDataCommand::CleanupNodeSubscriptions { user_id: uid, failed_node_id, .. } => {
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
            SharedDataCommand::Insert { table_id, rows_data, .. } => {
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
        true
    }

    async fn get_leader(&self, _group: GroupId) -> Option<NodeId> {
        None
    }

    fn is_cluster_mode(&self) -> bool {
        false
    }

    fn node_id(&self) -> NodeId {
        NodeId::default()
    }
    
    fn get_cluster_info(&self) -> ClusterInfo {
        ClusterInfo {
            cluster_id: "standalone".to_string(),
            current_node_id: NodeId::default(),
            is_cluster_mode: false,
            nodes: vec![ClusterNodeInfo {
                node_id: NodeId::default(),
                role: NodeRole::Leader,
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
        Ok(())
    }
    
    async fn initialize_cluster(&self) -> Result<()> {
        Ok(())
    }
    
    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}
