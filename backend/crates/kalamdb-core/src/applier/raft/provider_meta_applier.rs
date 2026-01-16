//! ProviderMetaApplier - Raft state machine applier for metadata operations
//!
//! This applier implements the MetaApplier trait and delegates to the
//! CommandExecutorImpl for actual persistence. This ensures a SINGLE
//! code path for all mutations regardless of standalone/cluster mode.
//!
//! Used by the Raft state machine to apply replicated commands on followers.

use async_trait::async_trait;
use kalamdb_commons::models::schemas::TableDefinition;
use kalamdb_commons::models::{
    ConnectionId, JobId, JobType, LiveQueryId, NamespaceId, NodeId, StorageId, TableId, TableName,
    UserId,
};
use kalamdb_commons::schemas::TableType;
use kalamdb_commons::system::{Job, LiveQuery, Storage};
use kalamdb_commons::types::{LiveQueryStatus, User};
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
    ) -> Result<String, RaftError> {
        log::debug!("ProviderMetaApplier: Creating namespace {} by {:?}", namespace_id, created_by);
        
        let message = self.executor.namespace()
            .create_namespace(namespace_id)
            .await
            .map_err(|e: ApplierError| RaftError::Internal(e.to_string()))?;
        
        Ok(message)
    }

    async fn delete_namespace(&self, namespace_id: &NamespaceId) -> Result<String, RaftError> {
        log::debug!("ProviderMetaApplier: Deleting namespace {}", namespace_id);
        
        let message = self.executor.namespace()
            .drop_namespace(namespace_id)
            .await
            .map_err(|e: ApplierError| RaftError::Internal(e.to_string()))?;
        
        Ok(message)
    }

    // =========================================================================
    // Table Operations - Delegate to DdlExecutor
    // =========================================================================
    
    async fn create_table(
        &self,
        table_id: &TableId,
        table_type: TableType,
        schema_json: &str,
    ) -> Result<String, RaftError> {
        log::debug!("ProviderMetaApplier: Creating table {} (type: {})", table_id.full_name(), table_type);
        
        let table_def: TableDefinition = serde_json::from_str(schema_json)
            .map_err(|e| RaftError::Internal(format!("Failed to deserialize table schema: {}", e)))?;
        
        let message = self.executor.ddl()
            .create_table(table_id, table_type, &table_def)
            .await
            .map_err(|e: ApplierError| RaftError::Internal(e.to_string()))?;
        
        Ok(message)
    }

    async fn alter_table(
        &self,
        table_id: &TableId,
        schema_json: &str,
    ) -> Result<String, RaftError> {
        log::debug!("ProviderMetaApplier: Altering table {}", table_id.full_name());
        
        let table_def: TableDefinition = serde_json::from_str(schema_json)
            .map_err(|e| RaftError::Internal(format!("Failed to deserialize table schema: {}", e)))?;
        
        // For Raft replication, we don't have the old version, so pass 0
        self.executor.ddl()
            .alter_table(table_id, &table_def, 0)
            .await
            .map_err(|e: ApplierError| RaftError::Internal(e.to_string()))
    }

    async fn drop_table(&self, table_id: &TableId) -> Result<String, RaftError> {
        log::debug!("ProviderMetaApplier: Dropping table {}", table_id.full_name());
        
        self.executor.ddl()
            .drop_table(table_id)
            .await
            .map_err(|e: ApplierError| RaftError::Internal(e.to_string()))
    }

    // =========================================================================
    // Storage Operations - Delegate to StorageExecutor
    // =========================================================================
    
    async fn register_storage(
        &self,
        storage_id: &StorageId,
        config_json: &str,
    ) -> Result<String, RaftError> {
        log::info!("ProviderMetaApplier: Registering storage {}", storage_id);
        
        let storage: Storage = serde_json::from_str(config_json)
            .map_err(|e| RaftError::Internal(format!("Failed to deserialize storage config: {}", e)))?;
        
        self.executor.storage()
            .create_storage(&storage)
            .await
            .map_err(|e: ApplierError| RaftError::Internal(e.to_string()))
    }

    async fn unregister_storage(&self, storage_id: &StorageId) -> Result<String, RaftError> {
        log::info!("ProviderMetaApplier: Unregistering storage {}", storage_id);
        
        self.executor.storage()
            .drop_storage(storage_id)
            .await
            .map_err(|e: ApplierError| RaftError::Internal(e.to_string()))
    }

    // =========================================================================
    // User Operations - Delegate to UserExecutor
    // =========================================================================
    
    async fn create_user(&self, user: &User) -> Result<String, RaftError> {
        log::info!("ProviderMetaApplier: Creating user {:?} ({})", user.id, user.username);
        
        self.executor.user()
            .create_user(user)
            .await
            .map_err(|e: ApplierError| RaftError::Internal(e.to_string()))
    }

    async fn update_user(&self, user: &User) -> Result<String, RaftError> {
        log::debug!("ProviderMetaApplier: Updating user {:?}", user.id);
        
        self.executor.user()
            .update_user(user)
            .await
            .map_err(|e: ApplierError| RaftError::Internal(e.to_string()))
    }

    async fn delete_user(&self, user_id: &UserId, _deleted_at: i64) -> Result<String, RaftError> {
        log::debug!("ProviderMetaApplier: Deleting user {:?}", user_id);
        
        self.executor.user()
            .delete_user(user_id)
            .await
            .map_err(|e: ApplierError| RaftError::Internal(e.to_string()))
    }

    async fn record_login(&self, user_id: &UserId, logged_in_at: i64) -> Result<String, RaftError> {
        log::debug!("ProviderMetaApplier: Recording login for {:?}", user_id);
        
        self.executor.user()
            .record_login(user_id, logged_in_at)
            .await
            .map_err(|e: ApplierError| RaftError::Internal(e.to_string()))
    }

    async fn set_user_locked(
        &self,
        user_id: &UserId,
        locked_until: Option<i64>,
        _updated_at: i64,
    ) -> Result<String, RaftError> {
        log::debug!("ProviderMetaApplier: Setting user {:?} locked until {:?}", user_id, locked_until);
        
        self.executor.user()
            .set_user_locked(user_id, locked_until)
            .await
            .map_err(|e: ApplierError| RaftError::Internal(e.to_string()))
    }

    // =========================================================================
    // Job Operations - Stay local (no SQL handlers for jobs)
    // =========================================================================
    
    async fn create_job(
        &self,
        job_id: &JobId,
        job_type: JobType,
        status: JobStatus,
        parameters_json: Option<&str>,
        idempotency_key: Option<&str>,
        max_retries: u8,
        queue: Option<&str>,
        priority: Option<i32>,
        node_id: NodeId,
        created_at: i64,
    ) -> Result<String, RaftError> {
        log::debug!("ProviderMetaApplier: Creating job {} (type: {:?})", job_id, job_type);
        
        let job = Job {
            job_id: job_id.clone(),
            job_type: job_type.clone(),
            status,
            node_id,
            parameters: parameters_json.map(String::from),
            idempotency_key: idempotency_key.map(String::from),
            retry_count: 0,
            max_retries,
            message: None,
            exception_trace: None,
            queue: queue.map(String::from),
            priority,
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
            .map_err(|e| RaftError::Internal(format!("Failed to create job: {}", e)))
    }

    async fn claim_job(
        &self,
        job_id: &JobId,
        node_id: NodeId,
        claimed_at: i64,
    ) -> Result<String, RaftError> {
        log::info!("ProviderMetaApplier: Claiming job {} by node {}", job_id, node_id);
        
        if let Some(mut job) = self.app_context.system_tables().jobs().get_job(job_id)
            .map_err(|e| RaftError::Internal(format!("Failed to get job: {}", e)))?
        {
            job.node_id = node_id.clone();
            job.started_at = Some(claimed_at);
            job.status = JobStatus::Running;
            job.updated_at = claimed_at;
            
            self.app_context.system_tables()
                .jobs()
                .update_job(job)
                .map_err(|e| RaftError::Internal(format!("Failed to claim job: {}", e)))?;
            
            return Ok(format!("Job {} claimed by node {}", job_id, node_id));
        }
        
        Ok(format!("Job {} not found for claiming", job_id))
    }

    async fn update_job_status(
        &self,
        job_id: &JobId,
        status: JobStatus,
        updated_at: i64,
    ) -> Result<String, RaftError> {
        log::debug!("ProviderMetaApplier: Updating job {} status to {:?}", job_id, status);
        
        if let Some(mut job) = self.app_context.system_tables().jobs().get_job(job_id)
            .map_err(|e| RaftError::Internal(format!("Failed to get job: {}", e)))?
        {
            let old_status = job.status.clone();
            job.status = status.clone();
            job.updated_at = updated_at;
            
            self.app_context.system_tables()
                .jobs()
                .update_job(job)
                .map_err(|e| RaftError::Internal(format!("Failed to update job status: {}", e)))?;
            
            return Ok(format!("Job {} status updated from {:?} to {:?}", job_id, old_status, status));
        }
        
        Ok(format!("Job {} not found for status update", job_id))
    }

    async fn complete_job(
        &self,
        job_id: &JobId,
        result_json: Option<&str>,
        completed_at: i64,
    ) -> Result<String, RaftError> {
        log::info!("ProviderMetaApplier: Completing job {}", job_id);
        
        if let Some(mut job) = self.app_context.system_tables().jobs().get_job(job_id)
            .map_err(|e| RaftError::Internal(format!("Failed to get job: {}", e)))?
        {
            let job_type = job.job_type.clone();
            job.status = JobStatus::Completed;
            job.message = result_json.map(|s| s.to_string());
            job.finished_at = Some(completed_at);
            job.updated_at = completed_at;
            
            self.app_context.system_tables()
                .jobs()
                .update_job(job)
                .map_err(|e| RaftError::Internal(format!("Failed to complete job: {}", e)))?;
            
            return Ok(format!("Job {} ({:?}) completed successfully", job_id, job_type));
        }
        
        Ok(format!("Job {} not found for completion", job_id))
    }

    async fn fail_job(
        &self,
        job_id: &JobId,
        error_message: &str,
        failed_at: i64,
    ) -> Result<String, RaftError> {
        log::warn!("ProviderMetaApplier: Failing job {}: {}", job_id, error_message);
        
        if let Some(mut job) = self.app_context.system_tables().jobs().get_job(job_id)
            .map_err(|e| RaftError::Internal(format!("Failed to get job: {}", e)))?
        {
            let job_type = job.job_type.clone();
            job.status = JobStatus::Failed;
            job.message = Some(error_message.to_string());
            job.finished_at = Some(failed_at);
            job.updated_at = failed_at;
            
            self.app_context.system_tables()
                .jobs()
                .update_job(job)
                .map_err(|e| RaftError::Internal(format!("Failed to fail job: {}", e)))?;
            
            return Ok(format!("Job {} ({:?}) marked as failed: {}", job_id, job_type, error_message));
        }
        
        Ok(format!("Job {} not found for failure", job_id))
    }

    async fn release_job(
        &self,
        job_id: &JobId,
        reason: &str,
        released_at: i64,
    ) -> Result<String, RaftError> {
        log::info!("ProviderMetaApplier: Releasing job {}: {}", job_id, reason);
        
        if let Some(mut job) = self.app_context.system_tables().jobs().get_job(job_id)
            .map_err(|e| RaftError::Internal(format!("Failed to get job: {}", e)))?
        {
            let job_type = job.job_type.clone();
            job.status = JobStatus::New;
            job.node_id = NodeId::default();
            job.started_at = None;
            job.updated_at = released_at;
            
            self.app_context.system_tables()
                .jobs()
                .update_job(job)
                .map_err(|e| RaftError::Internal(format!("Failed to release job: {}", e)))?;
            
            return Ok(format!("Job {} ({:?}) released: {}", job_id, job_type, reason));
        }
        
        Ok(format!("Job {} not found for release", job_id))
    }

    async fn cancel_job(
        &self,
        job_id: &JobId,
        reason: &str,
        cancelled_at: i64,
    ) -> Result<String, RaftError> {
        log::info!("ProviderMetaApplier: Cancelling job {}: {}", job_id, reason);
        
        if let Some(mut job) = self.app_context.system_tables().jobs().get_job(job_id)
            .map_err(|e| RaftError::Internal(format!("Failed to get job: {}", e)))?
        {
            let job_type = job.job_type.clone();
            job.status = JobStatus::Cancelled;
            job.finished_at = Some(cancelled_at);
            job.message = Some(reason.to_string());
            job.updated_at = cancelled_at;
            
            self.app_context.system_tables()
                .jobs()
                .update_job(job)
                .map_err(|e| RaftError::Internal(format!("Failed to cancel job: {}", e)))?;
            
            return Ok(format!("Job {} ({:?}) cancelled: {}", job_id, job_type, reason));
        }
        
        Ok(format!("Job {} not found for cancellation", job_id))
    }

    async fn create_schedule(
        &self,
        schedule_id: &str,
        job_type: JobType,
        cron_expression: &str,
        _config_json: Option<&str>,
        _created_at: i64,
    ) -> Result<String, RaftError> {
        log::info!("ProviderMetaApplier: Creating schedule {} (type: {:?})", schedule_id, job_type);
        Ok(format!("Schedule {} ({:?}) created with cron '{}'", schedule_id, job_type, cron_expression))
    }

    async fn delete_schedule(&self, schedule_id: &str) -> Result<String, RaftError> {
        log::info!("ProviderMetaApplier: Deleting schedule {}", schedule_id);
        Ok(format!("Schedule {} deleted successfully", schedule_id))
    }

    // =========================================================================
    // Live Query Operations - Persist to system.live_queries
    // =========================================================================

    async fn create_live_query(
        &self,
        live_id: &LiveQueryId,
        connection_id: &ConnectionId,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        user_id: &UserId,
        query: &str,
        options_json: Option<&str>,
        node_id: NodeId,
        subscription_id: &str,
        created_at: i64,
    ) -> Result<String, RaftError> {
        log::info!("ProviderMetaApplier: Creating live query {} on node {}", live_id, node_id);
        
        let live_query = LiveQuery {
            live_id: live_id.clone(),
            connection_id: connection_id.to_string(),
            namespace_id: namespace_id.clone(),
            table_name: table_name.clone(),
            user_id: user_id.clone(),
            query: query.to_string(),
            options: options_json.map(|s| s.to_string()),
            created_at,
            last_update: created_at,
            changes: 0,
            node_id,
            subscription_id: subscription_id.to_string(),
            status: LiveQueryStatus::Active,
            last_ping_at: created_at,
        };
        
        self.app_context.system_tables()
            .live_queries()
            .create_live_query(live_query)
            .map_err(|e| RaftError::Internal(format!("Failed to create live query: {}", e)))?;
        
        Ok(format!("Live query {} created for table {}.{}", live_id, namespace_id, table_name))
    }

    async fn update_live_query(
        &self,
        live_id: &LiveQueryId,
        last_update: i64,
        changes: i64,
    ) -> Result<String, RaftError> {
        log::trace!("ProviderMetaApplier: Updating live query {} (changes={})", live_id, changes);
        
        if let Some(mut lq) = self.app_context.system_tables().live_queries().get_live_query_by_id(live_id)
            .map_err(|e| RaftError::Internal(format!("Failed to get live query: {}", e)))?
        {
            lq.last_update = last_update;
            lq.changes = changes;
            
            self.app_context.system_tables()
                .live_queries()
                .update_live_query(lq)
                .map_err(|e| RaftError::Internal(format!("Failed to update live query: {}", e)))?;
            
            return Ok(format!("Live query {} updated (changes={})", live_id, changes));
        }
        
        Ok(format!("Live query {} not found for update", live_id))
    }

    async fn delete_live_query(
        &self,
        live_id: &LiveQueryId,
        _deleted_at: i64,
    ) -> Result<String, RaftError> {
        log::info!("ProviderMetaApplier: Deleting live query {}", live_id);
        
        self.app_context.system_tables()
            .live_queries()
            .delete_live_query(live_id)
            .map_err(|e| RaftError::Internal(format!("Failed to delete live query: {}", e)))?;
        
        Ok(format!("Live query {} deleted successfully", live_id))
    }

    async fn delete_live_queries_by_connection(
        &self,
        connection_id: &ConnectionId,
        _deleted_at: i64,
    ) -> Result<String, RaftError> {
        log::debug!("ProviderMetaApplier: Deleting live queries for connection {}", connection_id);
        
        // Get all live queries for this connection and delete them
        let live_queries = self.app_context.system_tables()
            .live_queries()
            .list_live_queries()
            .map_err(|e| RaftError::Internal(format!("Failed to list live queries: {}", e)))?;
        
        let mut deleted_count = 0;
        for lq in live_queries {
            if lq.connection_id == connection_id.as_str() {
                self.app_context.system_tables()
                    .live_queries()
                    .delete_live_query(&lq.live_id)
                    .map_err(|e| RaftError::Internal(format!("Failed to delete live query {}: {}", lq.live_id, e)))?;
                deleted_count += 1;
            }
        }
        
        Ok(format!("Deleted {} live queries for connection {}", deleted_count, connection_id))
    }
}
