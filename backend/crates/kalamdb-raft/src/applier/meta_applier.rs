//! MetaApplier trait - unified applier for all metadata operations
//!
//! This trait combines SystemApplier, UsersApplier, and JobsApplier into a single
//! interface for the unified Meta Raft group. It is called by MetaStateMachine
//! after Raft consensus commits a command.
//!
//! # Implementation
//!
//! The implementation lives in kalamdb-core and delegates to the appropriate
//! providers (namespaces, tables, storages, users, jobs).

use async_trait::async_trait;
use kalamdb_commons::models::schemas::TableType;
use kalamdb_commons::models::{JobId, JobType, NamespaceId, NodeId, StorageId, TableId, TableName, UserId};
use kalamdb_commons::types::User;

use crate::RaftError;

/// Unified applier callback for all metadata operations
///
/// This trait is called by MetaStateMachine after Raft consensus commits
/// a command. All nodes (leader and followers) call this, ensuring that
/// all replicas persist the same state to their local storage.
///
/// # Design
///
/// By combining all metadata operations into a single applier, we ensure:
/// - Single point of integration with kalamdb-core providers
/// - Consistent error handling across all metadata types
/// - Simpler dependency injection during RaftManager setup
#[async_trait]
pub trait MetaApplier: Send + Sync {
    // =========================================================================
    // Namespace Operations
    // =========================================================================
    
    /// Create a namespace in persistent storage
    async fn create_namespace(
        &self,
        namespace_id: &NamespaceId,
        created_by: Option<&UserId>,
    ) -> Result<(), RaftError>;
    
    /// Delete a namespace from persistent storage
    async fn delete_namespace(&self, namespace_id: &NamespaceId) -> Result<(), RaftError>;

    // =========================================================================
    // Table Operations
    // =========================================================================
    
    /// Create a table in persistent storage
    ///
    /// # Arguments
    /// * `table_id` - TableId containing namespace and table name
    /// * `table_type` - Type of table (User, Shared, Stream)
    /// * `schema_json` - JSON-serialized TableDefinition
    async fn create_table(
        &self,
        table_id: &TableId,
        table_type: TableType,
        schema_json: &str,
    ) -> Result<(), RaftError>;
    
    /// Alter a table in persistent storage
    async fn alter_table(
        &self,
        table_id: &TableId,
        schema_json: &str,
    ) -> Result<(), RaftError>;
    
    /// Drop a table from persistent storage
    async fn drop_table(&self, table_id: &TableId) -> Result<(), RaftError>;

    // =========================================================================
    // Storage Operations
    // =========================================================================
    
    /// Register storage configuration
    async fn register_storage(
        &self,
        storage_id: &StorageId,
        config_json: &str,
    ) -> Result<(), RaftError>;
    
    /// Unregister storage configuration
    async fn unregister_storage(&self, storage_id: &StorageId) -> Result<(), RaftError>;

    // =========================================================================
    // User Operations
    // =========================================================================
    
    /// Create a new user in persistent storage
    async fn create_user(&self, user: &User) -> Result<(), RaftError>;
    
    /// Update an existing user in persistent storage
    async fn update_user(&self, user: &User) -> Result<(), RaftError>;
    
    /// Soft-delete a user from persistent storage
    async fn delete_user(&self, user_id: &UserId, deleted_at: i64) -> Result<(), RaftError>;
    
    /// Record a successful login
    async fn record_login(&self, user_id: &UserId, logged_in_at: i64) -> Result<(), RaftError>;
    
    /// Lock or unlock a user account
    async fn set_user_locked(
        &self,
        user_id: &UserId,
        locked_until: Option<i64>,
        updated_at: i64,
    ) -> Result<(), RaftError>;

    // =========================================================================
    // Job Operations
    // =========================================================================
    
    /// Create a new job in persistent storage
    async fn create_job(
        &self,
        job_id: &JobId,
        job_type: JobType,
        namespace_id: Option<&NamespaceId>,
        table_name: Option<&TableName>,
        config_json: Option<&str>,
        created_at: i64,
    ) -> Result<(), RaftError>;
    
    /// Claim a job for execution
    async fn claim_job(
        &self,
        job_id: &JobId,
        node_id: NodeId,
        claimed_at: i64,
    ) -> Result<(), RaftError>;
    
    /// Update job status
    async fn update_job_status(
        &self,
        job_id: &JobId,
        status: &str,
        updated_at: i64,
    ) -> Result<(), RaftError>;
    
    /// Complete a job successfully
    async fn complete_job(
        &self,
        job_id: &JobId,
        result_json: Option<&str>,
        completed_at: i64,
    ) -> Result<(), RaftError>;
    
    /// Fail a job
    async fn fail_job(
        &self,
        job_id: &JobId,
        error_message: &str,
        failed_at: i64,
    ) -> Result<(), RaftError>;
    
    /// Release a claimed job
    async fn release_job(
        &self,
        job_id: &JobId,
        reason: &str,
        released_at: i64,
    ) -> Result<(), RaftError>;
    
    /// Cancel a job
    async fn cancel_job(
        &self,
        job_id: &JobId,
        reason: &str,
        cancelled_at: i64,
    ) -> Result<(), RaftError>;
    
    /// Create a schedule
    async fn create_schedule(
        &self,
        schedule_id: &str,
        job_type: JobType,
        cron_expression: &str,
        config_json: Option<&str>,
        created_at: i64,
    ) -> Result<(), RaftError>;
    
    /// Delete a schedule
    async fn delete_schedule(&self, schedule_id: &str) -> Result<(), RaftError>;
}

/// No-op applier for testing or standalone scenarios
///
/// Does nothing - used when persistence is handled elsewhere or for testing.
pub struct NoOpMetaApplier;

#[async_trait]
impl MetaApplier for NoOpMetaApplier {
    // Namespace operations
    async fn create_namespace(&self, _: &NamespaceId, _: Option<&UserId>) -> Result<(), RaftError> {
        Ok(())
    }
    async fn delete_namespace(&self, _: &NamespaceId) -> Result<(), RaftError> {
        Ok(())
    }

    // Table operations
    async fn create_table(&self, _: &TableId, _: TableType, _: &str) -> Result<(), RaftError> {
        Ok(())
    }
    async fn alter_table(&self, _: &TableId, _: &str) -> Result<(), RaftError> {
        Ok(())
    }
    async fn drop_table(&self, _: &TableId) -> Result<(), RaftError> {
        Ok(())
    }

    // Storage operations
    async fn register_storage(&self, _: &StorageId, _: &str) -> Result<(), RaftError> {
        Ok(())
    }
    async fn unregister_storage(&self, _: &StorageId) -> Result<(), RaftError> {
        Ok(())
    }

    // User operations
    async fn create_user(&self, _: &User) -> Result<(), RaftError> {
        Ok(())
    }
    async fn update_user(&self, _: &User) -> Result<(), RaftError> {
        Ok(())
    }
    async fn delete_user(&self, _: &UserId, _: i64) -> Result<(), RaftError> {
        Ok(())
    }
    async fn record_login(&self, _: &UserId, _: i64) -> Result<(), RaftError> {
        Ok(())
    }
    async fn set_user_locked(&self, _: &UserId, _: Option<i64>, _: i64) -> Result<(), RaftError> {
        Ok(())
    }

    // Job operations
    async fn create_job(
        &self,
        _: &JobId,
        _: JobType,
        _: Option<&NamespaceId>,
        _: Option<&TableName>,
        _: Option<&str>,
        _: i64,
    ) -> Result<(), RaftError> {
        Ok(())
    }
    async fn claim_job(&self, _: &JobId, _: NodeId, _: i64) -> Result<(), RaftError> {
        Ok(())
    }
    async fn update_job_status(&self, _: &JobId, _: &str, _: i64) -> Result<(), RaftError> {
        Ok(())
    }
    async fn complete_job(&self, _: &JobId, _: Option<&str>, _: i64) -> Result<(), RaftError> {
        Ok(())
    }
    async fn fail_job(&self, _: &JobId, _: &str, _: i64) -> Result<(), RaftError> {
        Ok(())
    }
    async fn release_job(&self, _: &JobId, _: &str, _: i64) -> Result<(), RaftError> {
        Ok(())
    }
    async fn cancel_job(&self, _: &JobId, _: &str, _: i64) -> Result<(), RaftError> {
        Ok(())
    }
    async fn create_schedule(&self, _: &str, _: JobType, _: &str, _: Option<&str>, _: i64) -> Result<(), RaftError> {
        Ok(())
    }
    async fn delete_schedule(&self, _: &str) -> Result<(), RaftError> {
        Ok(())
    }
}
