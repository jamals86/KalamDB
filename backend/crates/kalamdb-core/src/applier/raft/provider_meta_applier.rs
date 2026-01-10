//! ProviderMetaApplier - Raft state machine applier for metadata operations
//!
//! This applier implements the MetaApplier trait and delegates to the
//! CommandExecutorImpl for actual persistence. This ensures a SINGLE
//! code path for all mutations regardless of standalone/cluster mode.
//!
//! Used by the Raft state machine to apply replicated commands on followers.

use async_trait::async_trait;
use kalamdb_commons::models::schemas::TableDefinition;
use kalamdb_commons::models::{JobId, JobType, NamespaceId, NodeId, StorageId, TableId, TableName, UserId};
use kalamdb_commons::schemas::TableType;
use kalamdb_commons::system::{Job, Storage};
use kalamdb_commons::types::User;
use kalamdb_commons::JobStatus;
use kalamdb_raft::applier::MetaApplier;
use kalamdb_raft::RaftError;
use crate::app_context::AppContext;
use crate::applier::executor::CommandExecutorImpl;
use crate::applier::ApplierError;
use std::sync::Arc;

/// Unified applier that persists all metadata operations to system tables
///
/// This is used by the Raft state machine on follower nodes to apply
/// replicated commands locally. It delegates to CommandExecutorImpl to ensure
/// a single code path for all mutations.
///
/// ## Architecture
///
/// - DDL operations (create/alter/drop table) → DdlExecutor
/// - Namespace operations → NamespaceExecutor
/// - User operations → UserExecutor
/// - Storage operations → StorageExecutor
/// - Job operations → Local (jobs are Raft-only, no SQL handlers)
pub struct ProviderMetaApplier {
    executor: CommandExecutorImpl,
    app_context: Arc<AppContext>,
}

impl ProviderMetaApplier {
    /// Create a new ProviderMetaApplier
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self {
            executor: CommandExecutorImpl::new(Arc::clone(&app_context)),
            app_context,
        }
    }
}

#[async_trait]
impl MetaApplier for ProviderMetaApplier {
    // =========================================================================
    // Namespace Operations - Delegate to NamespaceExecutor
    // =========================================================================
    
    async fn create_namespace(
        &self,
        namespace_id: &NamespaceId,
        created_by: Option<&UserId>,
    ) -> Result<(), RaftError> {
        log::info!("ProviderMetaApplier: Creating namespace {} by {:?}", namespace_id, created_by);
        
        self.executor.namespace()
            .create_namespace(namespace_id)
            .await
            .map_err(|e: ApplierError| RaftError::Internal(e.to_string()))?;
        
        Ok(())
    }

    async fn delete_namespace(&self, namespace_id: &NamespaceId) -> Result<(), RaftError> {
        log::info!("ProviderMetaApplier: Deleting namespace {}", namespace_id);
        
        self.executor.namespace()
            .drop_namespace(namespace_id)
            .await
            .map_err(|e: ApplierError| RaftError::Internal(e.to_string()))?;
        
        Ok(())
    }

    // =========================================================================
    // Table Operations - Delegate to DdlExecutor
    // =========================================================================
    
    async fn create_table(
        &self,
        table_id: &TableId,
        table_type: TableType,
        schema_json: &str,
    ) -> Result<(), RaftError> {
        log::info!("ProviderMetaApplier: Creating table {} (type: {})", table_id.full_name(), table_type);
        
        let table_def: TableDefinition = serde_json::from_str(schema_json)
            .map_err(|e| RaftError::Internal(format!("Failed to deserialize table schema: {}", e)))?;
        
        self.executor.ddl()
            .create_table(table_id, table_type, &table_def)
            .await
            .map_err(|e: ApplierError| RaftError::Internal(e.to_string()))?;
        
        Ok(())
    }

    async fn alter_table(
        &self,
        table_id: &TableId,
        schema_json: &str,
    ) -> Result<(), RaftError> {
        log::info!("ProviderMetaApplier: Altering table {}", table_id.full_name());
        
        let table_def: TableDefinition = serde_json::from_str(schema_json)
            .map_err(|e| RaftError::Internal(format!("Failed to deserialize table schema: {}", e)))?;
        
        // For Raft replication, we don't have the old version, so pass 0
        self.executor.ddl()
            .alter_table(table_id, &table_def, 0)
            .await
            .map_err(|e: ApplierError| RaftError::Internal(e.to_string()))?;
        
        Ok(())
    }

    async fn drop_table(&self, table_id: &TableId) -> Result<(), RaftError> {
        log::info!("ProviderMetaApplier: Dropping table {}", table_id.full_name());
        
        self.executor.ddl()
            .drop_table(table_id)
            .await
            .map_err(|e: ApplierError| RaftError::Internal(e.to_string()))?;
        
        Ok(())
    }

    // =========================================================================
    // Storage Operations - Delegate to StorageExecutor
    // =========================================================================
    
    async fn register_storage(
        &self,
        storage_id: &StorageId,
        config_json: &str,
    ) -> Result<(), RaftError> {
        log::info!("ProviderMetaApplier: Registering storage {}", storage_id);
        
        let storage: Storage = serde_json::from_str(config_json)
            .map_err(|e| RaftError::Internal(format!("Failed to deserialize storage config: {}", e)))?;
        
        self.executor.storage()
            .create_storage(&storage)
            .await
            .map_err(|e: ApplierError| RaftError::Internal(e.to_string()))?;
        
        Ok(())
    }

    async fn unregister_storage(&self, storage_id: &StorageId) -> Result<(), RaftError> {
        log::info!("ProviderMetaApplier: Unregistering storage {}", storage_id);
        
        self.executor.storage()
            .drop_storage(storage_id)
            .await
            .map_err(|e: ApplierError| RaftError::Internal(e.to_string()))?;
        
        Ok(())
    }

    // =========================================================================
    // User Operations - Delegate to UserExecutor
    // =========================================================================
    
    async fn create_user(&self, user: &User) -> Result<(), RaftError> {
        log::info!("ProviderMetaApplier: Creating user {:?} ({})", user.id, user.username);
        
        self.executor.user()
            .create_user(user)
            .await
            .map_err(|e: ApplierError| RaftError::Internal(e.to_string()))?;
        
        Ok(())
    }

    async fn update_user(&self, user: &User) -> Result<(), RaftError> {
        log::debug!("ProviderMetaApplier: Updating user {:?}", user.id);
        
        self.executor.user()
            .update_user(user)
            .await
            .map_err(|e: ApplierError| RaftError::Internal(e.to_string()))?;
        
        Ok(())
    }

    async fn delete_user(&self, user_id: &UserId, _deleted_at: i64) -> Result<(), RaftError> {
        log::debug!("ProviderMetaApplier: Deleting user {:?}", user_id);
        
        self.executor.user()
            .delete_user(user_id)
            .await
            .map_err(|e: ApplierError| RaftError::Internal(e.to_string()))?;
        
        Ok(())
    }

    async fn record_login(&self, user_id: &UserId, logged_in_at: i64) -> Result<(), RaftError> {
        log::debug!("ProviderMetaApplier: Recording login for {:?}", user_id);
        
        self.executor.user()
            .record_login(user_id, logged_in_at)
            .await
            .map_err(|e: ApplierError| RaftError::Internal(e.to_string()))?;
        
        Ok(())
    }

    async fn set_user_locked(
        &self,
        user_id: &UserId,
        locked_until: Option<i64>,
        _updated_at: i64,
    ) -> Result<(), RaftError> {
        log::debug!("ProviderMetaApplier: Setting user {:?} locked until {:?}", user_id, locked_until);
        
        self.executor.user()
            .set_user_locked(user_id, locked_until)
            .await
            .map_err(|e: ApplierError| RaftError::Internal(e.to_string()))?;
        
        Ok(())
    }

    // =========================================================================
    // Job Operations - Stay local (no SQL handlers for jobs)
    // =========================================================================
    
    async fn create_job(
        &self,
        job_id: &JobId,
        job_type: JobType,
        namespace_id: Option<&NamespaceId>,
        table_name: Option<&TableName>,
        config_json: Option<&str>,
        created_at: i64,
    ) -> Result<(), RaftError> {
        log::info!("ProviderMetaApplier: Creating job {} (type: {:?})", job_id, job_type);
        
        // Build parameters JSON
        let mut params = serde_json::Map::new();
        if let Some(ns_id) = namespace_id {
            params.insert("namespace_id".to_string(), serde_json::Value::String(ns_id.as_str().to_string()));
        }
        if let Some(tbl_name) = table_name {
            params.insert("table_name".to_string(), serde_json::Value::String(tbl_name.as_str().to_string()));
        }
        if let Some(cfg) = config_json {
            if let Ok(cfg_val) = serde_json::from_str::<serde_json::Value>(cfg) {
                if let serde_json::Value::Object(obj) = cfg_val {
                    for (k, v) in obj {
                        params.insert(k, v);
                    }
                }
            }
        }
        let parameters = if params.is_empty() { None } else { Some(serde_json::to_string(&params).unwrap_or_default()) };
        
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
            created_at,
            updated_at: created_at,
            started_at: None,
            finished_at: None,
            memory_used: None,
            cpu_used: None,
        };
        
        self.app_context.system_tables()
            .jobs()
            .create_job(job)
            .map_err(|e| RaftError::Internal(format!("Failed to create job: {}", e)))?;
        
        Ok(())
    }

    async fn claim_job(
        &self,
        job_id: &JobId,
        node_id: NodeId,
        claimed_at: i64,
    ) -> Result<(), RaftError> {
        log::info!("ProviderMetaApplier: Claiming job {} by node {}", job_id, node_id);
        
        if let Some(mut job) = self.app_context.system_tables().jobs().get_job(job_id)
            .map_err(|e| RaftError::Internal(format!("Failed to get job: {}", e)))?
        {
            job.node_id = node_id;
            job.started_at = Some(claimed_at);
            job.status = JobStatus::Running;
            job.updated_at = claimed_at;
            
            self.app_context.system_tables()
                .jobs()
                .update_job(job)
                .map_err(|e| RaftError::Internal(format!("Failed to claim job: {}", e)))?;
        }
        
        Ok(())
    }

    async fn update_job_status(
        &self,
        job_id: &JobId,
        status: JobStatus,
        updated_at: i64,
    ) -> Result<(), RaftError> {
        log::debug!("ProviderMetaApplier: Updating job {} status to {:?}", job_id, status);
        
        if let Some(mut job) = self.app_context.system_tables().jobs().get_job(job_id)
            .map_err(|e| RaftError::Internal(format!("Failed to get job: {}", e)))?
        {
            job.status = status;
            job.updated_at = updated_at;
            
            self.app_context.system_tables()
                .jobs()
                .update_job(job)
                .map_err(|e| RaftError::Internal(format!("Failed to update job status: {}", e)))?;
        }
        
        Ok(())
    }

    async fn complete_job(
        &self,
        job_id: &JobId,
        result_json: Option<&str>,
        completed_at: i64,
    ) -> Result<(), RaftError> {
        log::info!("ProviderMetaApplier: Completing job {}", job_id);
        
        if let Some(mut job) = self.app_context.system_tables().jobs().get_job(job_id)
            .map_err(|e| RaftError::Internal(format!("Failed to get job: {}", e)))?
        {
            job.status = JobStatus::Completed;
            job.message = result_json.map(|s| s.to_string());
            job.finished_at = Some(completed_at);
            job.updated_at = completed_at;
            
            self.app_context.system_tables()
                .jobs()
                .update_job(job)
                .map_err(|e| RaftError::Internal(format!("Failed to complete job: {}", e)))?;
        }
        
        Ok(())
    }

    async fn fail_job(
        &self,
        job_id: &JobId,
        error_message: &str,
        failed_at: i64,
    ) -> Result<(), RaftError> {
        log::warn!("ProviderMetaApplier: Failing job {}: {}", job_id, error_message);
        
        if let Some(mut job) = self.app_context.system_tables().jobs().get_job(job_id)
            .map_err(|e| RaftError::Internal(format!("Failed to get job: {}", e)))?
        {
            job.status = JobStatus::Failed;
            job.message = Some(error_message.to_string());
            job.finished_at = Some(failed_at);
            job.updated_at = failed_at;
            
            self.app_context.system_tables()
                .jobs()
                .update_job(job)
                .map_err(|e| RaftError::Internal(format!("Failed to fail job: {}", e)))?;
        }
        
        Ok(())
    }

    async fn release_job(
        &self,
        job_id: &JobId,
        _reason: &str,
        released_at: i64,
    ) -> Result<(), RaftError> {
        log::info!("ProviderMetaApplier: Releasing job {}", job_id);
        
        if let Some(mut job) = self.app_context.system_tables().jobs().get_job(job_id)
            .map_err(|e| RaftError::Internal(format!("Failed to get job: {}", e)))?
        {
            job.status = JobStatus::New;
            job.node_id = NodeId::default();
            job.started_at = None;
            job.updated_at = released_at;
            
            self.app_context.system_tables()
                .jobs()
                .update_job(job)
                .map_err(|e| RaftError::Internal(format!("Failed to release job: {}", e)))?;
        }
        
        Ok(())
    }

    async fn cancel_job(
        &self,
        job_id: &JobId,
        reason: &str,
        cancelled_at: i64,
    ) -> Result<(), RaftError> {
        log::info!("ProviderMetaApplier: Cancelling job {}: {}", job_id, reason);
        
        if let Some(mut job) = self.app_context.system_tables().jobs().get_job(job_id)
            .map_err(|e| RaftError::Internal(format!("Failed to get job: {}", e)))?
        {
            job.status = JobStatus::Cancelled;
            job.finished_at = Some(cancelled_at);
            job.message = Some(reason.to_string());
            job.updated_at = cancelled_at;
            
            self.app_context.system_tables()
                .jobs()
                .update_job(job)
                .map_err(|e| RaftError::Internal(format!("Failed to cancel job: {}", e)))?;
        }
        
        Ok(())
    }

    async fn create_schedule(
        &self,
        schedule_id: &str,
        job_type: JobType,
        _cron_expression: &str,
        _config_json: Option<&str>,
        _created_at: i64,
    ) -> Result<(), RaftError> {
        log::info!("ProviderMetaApplier: Creating schedule {} (type: {:?})", schedule_id, job_type);
        Ok(())
    }

    async fn delete_schedule(&self, schedule_id: &str) -> Result<(), RaftError> {
        log::info!("ProviderMetaApplier: Deleting schedule {}", schedule_id);
        Ok(())
    }
}
