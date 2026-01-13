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
use kalamdb_commons::models::{
    ConnectionId, JobId, JobStatus, JobType, LiveQueryId, NamespaceId, NodeId, StorageId, TableId,
    TableName, UserId,
};
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
    ) -> Result<String, RaftError>;
    
    /// Delete a namespace from persistent storage
    async fn delete_namespace(&self, namespace_id: &NamespaceId) -> Result<String, RaftError>;

    // =========================================================================
    // Table Operations
    // =========================================================================
    
    /// Create a table in persistent storage
    ///
    /// # Arguments
    /// * `table_id` - TableId containing namespace and table name
    /// * `table_type` - Type of table (User, Shared, Stream)
    /// * `schema_json` - JSON-serialized TableDefinition
    ///
    /// # Returns
    /// Success message to be returned to the client
    async fn create_table(
        &self,
        table_id: &TableId,
        table_type: TableType,
        schema_json: &str,
    ) -> Result<String, RaftError>;
    
    /// Alter a table in persistent storage
    async fn alter_table(
        &self,
        table_id: &TableId,
        schema_json: &str,
    ) -> Result<String, RaftError>;
    
    /// Drop a table from persistent storage
    async fn drop_table(&self, table_id: &TableId) -> Result<String, RaftError>;

    // =========================================================================
    // Storage Operations
    // =========================================================================
    
    /// Register storage configuration
    async fn register_storage(
        &self,
        storage_id: &StorageId,
        config_json: &str,
    ) -> Result<String, RaftError>;
    
    /// Unregister storage configuration
    async fn unregister_storage(&self, storage_id: &StorageId) -> Result<String, RaftError>;

    // =========================================================================
    // User Operations
    // =========================================================================
    
    /// Create a new user in persistent storage
    async fn create_user(&self, user: &User) -> Result<String, RaftError>;
    
    /// Update an existing user in persistent storage
    async fn update_user(&self, user: &User) -> Result<String, RaftError>;
    
    /// Soft-delete a user from persistent storage
    async fn delete_user(&self, user_id: &UserId, deleted_at: i64) -> Result<String, RaftError>;
    
    /// Record a successful login
    async fn record_login(&self, user_id: &UserId, logged_in_at: i64) -> Result<String, RaftError>;
    
    /// Lock or unlock a user account
    async fn set_user_locked(
        &self,
        user_id: &UserId,
        locked_until: Option<i64>,
        updated_at: i64,
    ) -> Result<String, RaftError>;

    // =========================================================================
    // Job Operations
    // =========================================================================
    
    /// Create a new job in persistent storage
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
    ) -> Result<String, RaftError>;
    
    /// Claim a job for execution
    async fn claim_job(
        &self,
        job_id: &JobId,
        node_id: NodeId,
        claimed_at: i64,
    ) -> Result<String, RaftError>;
    
    /// Update job status
    async fn update_job_status(
        &self,
        job_id: &JobId,
        status: JobStatus,
        updated_at: i64,
    ) -> Result<String, RaftError>;
    
    /// Complete a job successfully
    async fn complete_job(
        &self,
        job_id: &JobId,
        result_json: Option<&str>,
        completed_at: i64,
    ) -> Result<String, RaftError>;
    
    /// Fail a job
    async fn fail_job(
        &self,
        job_id: &JobId,
        error_message: &str,
        failed_at: i64,
    ) -> Result<String, RaftError>;
    
    /// Release a claimed job
    async fn release_job(
        &self,
        job_id: &JobId,
        reason: &str,
        released_at: i64,
    ) -> Result<String, RaftError>;
    
    /// Cancel a job
    async fn cancel_job(
        &self,
        job_id: &JobId,
        reason: &str,
        cancelled_at: i64,
    ) -> Result<String, RaftError>;
    
    /// Create a schedule
    async fn create_schedule(
        &self,
        schedule_id: &str,
        job_type: JobType,
        cron_expression: &str,
        config_json: Option<&str>,
        created_at: i64,
    ) -> Result<String, RaftError>;
    
    /// Delete a schedule
    async fn delete_schedule(&self, schedule_id: &str) -> Result<String, RaftError>;

    // =========================================================================
    // Live Query Operations
    // =========================================================================

    /// Create a live query subscription (replicated across cluster)
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
    ) -> Result<String, RaftError>;

    /// Update a live query (last_update, changes count)
    async fn update_live_query(
        &self,
        live_id: &LiveQueryId,
        last_update: i64,
        changes: i64,
    ) -> Result<String, RaftError>;

    /// Delete a live query subscription
    async fn delete_live_query(
        &self,
        live_id: &LiveQueryId,
        deleted_at: i64,
    ) -> Result<String, RaftError>;

    /// Delete all live queries for a connection
    async fn delete_live_queries_by_connection(
        &self,
        connection_id: &ConnectionId,
        deleted_at: i64,
    ) -> Result<String, RaftError>;
}

/// No-op applier for testing or standalone scenarios
///
/// Does nothing - used when persistence is handled elsewhere or for testing.
pub struct NoOpMetaApplier;

#[async_trait]
impl MetaApplier for NoOpMetaApplier {
    // Namespace operations
    async fn create_namespace(&self, _: &NamespaceId, _: Option<&UserId>) -> Result<String, RaftError> {
        Ok(String::new())
    }
    async fn delete_namespace(&self, _: &NamespaceId) -> Result<String, RaftError> {
        Ok(String::new())
    }

    // Table operations
    async fn create_table(&self, _: &TableId, _: TableType, _: &str) -> Result<String, RaftError> {
        Ok(String::new())
    }
    async fn alter_table(&self, _: &TableId, _: &str) -> Result<String, RaftError> {
        Ok(String::new())
    }
    async fn drop_table(&self, _: &TableId) -> Result<String, RaftError> {
        Ok(String::new())
    }

    // Storage operations
    async fn register_storage(&self, _: &StorageId, _: &str) -> Result<String, RaftError> {
        Ok(String::new())
    }
    async fn unregister_storage(&self, _: &StorageId) -> Result<String, RaftError> {
        Ok(String::new())
    }

    // User operations
    async fn create_user(&self, _: &User) -> Result<String, RaftError> {
        Ok(String::new())
    }
    async fn update_user(&self, _: &User) -> Result<String, RaftError> {
        Ok(String::new())
    }
    async fn delete_user(&self, _: &UserId, _: i64) -> Result<String, RaftError> {
        Ok(String::new())
    }
    async fn record_login(&self, _: &UserId, _: i64) -> Result<String, RaftError> {
        Ok(String::new())
    }
    async fn set_user_locked(&self, _: &UserId, _: Option<i64>, _: i64) -> Result<String, RaftError> {
        Ok(String::new())
    }

    // Job operations
    async fn create_job(
        &self,
        _: &JobId,
        _: JobType,
        _: JobStatus,
        _: Option<&str>,
        _: Option<&str>,
        _: u8,
        _: Option<&str>,
        _: Option<i32>,
        _: NodeId,
        _: i64,
    ) -> Result<String, RaftError> {
        Ok(String::new())
    }
    async fn claim_job(&self, _: &JobId, _: NodeId, _: i64) -> Result<String, RaftError> {
        Ok(String::new())
    }
    async fn update_job_status(&self, _: &JobId, _: JobStatus, _: i64) -> Result<String, RaftError> {
        Ok(String::new())
    }
    async fn complete_job(&self, _: &JobId, _: Option<&str>, _: i64) -> Result<String, RaftError> {
        Ok(String::new())
    }
    async fn fail_job(&self, _: &JobId, _: &str, _: i64) -> Result<String, RaftError> {
        Ok(String::new())
    }
    async fn release_job(&self, _: &JobId, _: &str, _: i64) -> Result<String, RaftError> {
        Ok(String::new())
    }
    async fn cancel_job(&self, _: &JobId, _: &str, _: i64) -> Result<String, RaftError> {
        Ok(String::new())
    }
    async fn create_schedule(&self, _: &str, _: JobType, _: &str, _: Option<&str>, _: i64) -> Result<String, RaftError> {
        Ok(String::new())
    }
    async fn delete_schedule(&self, _: &str) -> Result<String, RaftError> {
        Ok(String::new())
    }

    // Live query operations
    async fn create_live_query(
        &self,
        _: &LiveQueryId,
        _: &ConnectionId,
        _: &NamespaceId,
        _: &TableName,
        _: &UserId,
        _: &str,
        _: Option<&str>,
        _: NodeId,
        _: &str,
        _: i64,
    ) -> Result<String, RaftError> {
        Ok(String::new())
    }
    async fn update_live_query(&self, _: &LiveQueryId, _: i64, _: i64) -> Result<String, RaftError> {
        Ok(String::new())
    }
    async fn delete_live_query(&self, _: &LiveQueryId, _: i64) -> Result<String, RaftError> {
        Ok(String::new())
    }
    async fn delete_live_queries_by_connection(&self, _: &ConnectionId, _: i64) -> Result<String, RaftError> {
        Ok(String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::models::{AuthType, NamespaceId, StorageMode, TableName, UserName};
    use kalamdb_commons::Role;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Mock applier that tracks all operations
    struct MockMetaApplier {
        namespace_ops: Arc<AtomicUsize>,
        table_ops: Arc<AtomicUsize>,
        user_ops: Arc<AtomicUsize>,
        job_ops: Arc<AtomicUsize>,
        storage_ops: Arc<AtomicUsize>,
    }

    impl MockMetaApplier {
        fn new() -> Self {
            Self {
                namespace_ops: Arc::new(AtomicUsize::new(0)),
                table_ops: Arc::new(AtomicUsize::new(0)),
                user_ops: Arc::new(AtomicUsize::new(0)),
                job_ops: Arc::new(AtomicUsize::new(0)),
                storage_ops: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn get_counts(&self) -> (usize, usize, usize, usize, usize) {
            (
                self.namespace_ops.load(Ordering::SeqCst),
                self.table_ops.load(Ordering::SeqCst),
                self.user_ops.load(Ordering::SeqCst),
                self.job_ops.load(Ordering::SeqCst),
                self.storage_ops.load(Ordering::SeqCst),
            )
        }
    }

    #[async_trait]
    impl MetaApplier for MockMetaApplier {
        async fn create_namespace(&self, _: &NamespaceId, _: Option<&UserId>) -> Result<String, RaftError> {
            self.namespace_ops.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }
        async fn delete_namespace(&self, _: &NamespaceId) -> Result<String, RaftError> {
            self.namespace_ops.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }
        async fn create_table(&self, _: &TableId, _: TableType, _: &str) -> Result<String, RaftError> {
            self.table_ops.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }
        async fn alter_table(&self, _: &TableId, _: &str) -> Result<String, RaftError> {
            self.table_ops.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }
        async fn drop_table(&self, _: &TableId) -> Result<String, RaftError> {
            self.table_ops.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }
        async fn register_storage(&self, _: &StorageId, _: &str) -> Result<String, RaftError> {
            self.storage_ops.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }
        async fn unregister_storage(&self, _: &StorageId) -> Result<String, RaftError> {
            self.storage_ops.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }
        async fn create_user(&self, _: &User) -> Result<String, RaftError> {
            self.user_ops.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }
        async fn update_user(&self, _: &User) -> Result<String, RaftError> {
            self.user_ops.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }
        async fn delete_user(&self, _: &UserId, _: i64) -> Result<String, RaftError> {
            self.user_ops.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }
        async fn record_login(&self, _: &UserId, _: i64) -> Result<String, RaftError> {
            self.user_ops.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }
        async fn set_user_locked(&self, _: &UserId, _: Option<i64>, _: i64) -> Result<String, RaftError> {
            self.user_ops.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }
        async fn create_job(&self, _: &JobId, _: JobType, _: JobStatus, _: Option<&str>, _: Option<&str>, _: u8, _: Option<&str>, _: Option<i32>, _: NodeId, _: i64) -> Result<String, RaftError> {
            self.job_ops.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }
        async fn claim_job(&self, _: &JobId, _: NodeId, _: i64) -> Result<String, RaftError> {
            self.job_ops.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }
        async fn update_job_status(&self, _: &JobId, _: JobStatus, _: i64) -> Result<String, RaftError> {
            self.job_ops.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }
        async fn complete_job(&self, _: &JobId, _: Option<&str>, _: i64) -> Result<String, RaftError> {
            self.job_ops.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }
        async fn fail_job(&self, _: &JobId, _: &str, _: i64) -> Result<String, RaftError> {
            self.job_ops.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }
        async fn release_job(&self, _: &JobId, _: &str, _: i64) -> Result<String, RaftError> {
            self.job_ops.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }
        async fn cancel_job(&self, _: &JobId, _: &str, _: i64) -> Result<String, RaftError> {
            self.job_ops.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }
        async fn create_schedule(&self, _: &str, _: JobType, _: &str, _: Option<&str>, _: i64) -> Result<String, RaftError> {
            self.job_ops.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }
        async fn delete_schedule(&self, _: &str) -> Result<String, RaftError> {
            self.job_ops.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }
        // Live query operations
        async fn create_live_query(&self, _: &LiveQueryId, _: &ConnectionId, _: &NamespaceId, _: &TableName, _: &UserId, _: &str, _: Option<&str>, _: NodeId, _: &str, _: i64) -> Result<String, RaftError> {
            Ok(String::new())
        }
        async fn update_live_query(&self, _: &LiveQueryId, _: i64, _: i64) -> Result<String, RaftError> {
            Ok(String::new())
        }
        async fn delete_live_query(&self, _: &LiveQueryId, _: i64) -> Result<String, RaftError> {
            Ok(String::new())
        }
        async fn delete_live_queries_by_connection(&self, _: &ConnectionId, _: i64) -> Result<String, RaftError> {
            Ok(String::new())
        }
    }

    #[tokio::test]
    async fn test_namespace_operations() {
        let applier = MockMetaApplier::new();
        let ns_id = NamespaceId::from("test_namespace");
        let user_id = UserId::from("user_123");

        applier.create_namespace(&ns_id, Some(&user_id)).await.unwrap();
        applier.delete_namespace(&ns_id).await.unwrap();

        assert_eq!(applier.get_counts(), (2, 0, 0, 0, 0));
    }

    #[tokio::test]
    async fn test_table_operations() {
        let applier = MockMetaApplier::new();
        let table_id = TableId::new(NamespaceId::from("ns"), TableName::from("table"));
        let schema_json = r#"{"columns":[]}"#;

        applier.create_table(&table_id, TableType::User, schema_json).await.unwrap();
        applier.alter_table(&table_id, schema_json).await.unwrap();
        applier.drop_table(&table_id).await.unwrap();

        assert_eq!(applier.get_counts(), (0, 3, 0, 0, 0));
    }

    #[tokio::test]
    async fn test_user_operations() {
        let applier = MockMetaApplier::new();
        let user = User {
            id: UserId::from("user_123"),
            username: UserName::new("testuser"),
            password_hash: "$2b$12$hash".to_string(),
            role: Role::User,
            email: None,
            auth_type: AuthType::Password,
            auth_data: None,
            storage_mode: StorageMode::Table,
            storage_id: None,
            failed_login_attempts: 0,
            locked_until: None,
            last_login_at: None,
            created_at: 1000,
            updated_at: 1000,
            last_seen: None,
            deleted_at: None,
        };

        applier.create_user(&user).await.unwrap();
        applier.update_user(&user).await.unwrap();
        applier.record_login(&user.id, 2000).await.unwrap();
        applier.set_user_locked(&user.id, Some(3000), 2000).await.unwrap();
        applier.delete_user(&user.id, 4000).await.unwrap();

        assert_eq!(applier.get_counts(), (0, 0, 5, 0, 0));
    }

    #[tokio::test]
    async fn test_job_operations() {
        let applier = MockMetaApplier::new();
        let job_id = JobId::from("FL-12345");

        applier.create_job(&job_id, JobType::Flush, JobStatus::Queued, None, None, 3, None, None, NodeId::from(1), 1000).await.unwrap();
        applier.claim_job(&job_id, NodeId::from(1), 1100).await.unwrap();
        applier.update_job_status(&job_id, JobStatus::Running, 1200).await.unwrap();
        applier.complete_job(&job_id, Some("{}"), 1300).await.unwrap();

        assert_eq!(applier.get_counts(), (0, 0, 0, 4, 0));
    }

    #[tokio::test]
    async fn test_storage_operations() {
        let applier = MockMetaApplier::new();
        let storage_id = StorageId::from("local");
        let config = r#"{"type":"local"}"#;

        applier.register_storage(&storage_id, config).await.unwrap();
        applier.unregister_storage(&storage_id).await.unwrap();

        assert_eq!(applier.get_counts(), (0, 0, 0, 0, 2));
    }

    #[tokio::test]
    async fn test_noop_applier_all_operations() {
        let applier = NoOpMetaApplier;
        let ns_id = NamespaceId::from("test");
        let table_id = TableId::new(ns_id.clone(), TableName::from("table"));
        let user_id = UserId::from("user");
        let user = User {
            id: user_id.clone(),
            username: UserName::new("test"),
            password_hash: "$2b$12$hash".to_string(),
            role: Role::User,
            email: None,
            auth_type: AuthType::Password,
            auth_data: None,
            storage_mode: StorageMode::Table,
            storage_id: None,
            failed_login_attempts: 0,
            locked_until: None,
            last_login_at: None,
            created_at: 1000,
            updated_at: 1000,
            last_seen: None,
            deleted_at: None,
        };

        // All operations should succeed and do nothing
        assert!(applier.create_namespace(&ns_id, Some(&user_id)).await.is_ok());
        assert!(applier.create_table(&table_id, TableType::User, "{}").await.is_ok());
        assert!(applier.create_user(&user).await.is_ok());
        assert!(applier.create_job(&JobId::from("J-1"), JobType::Flush, JobStatus::Queued, None, None, 3, None, None, NodeId::from(1), 1000).await.is_ok());
        assert!(applier.register_storage(&StorageId::from("s1"), "{}").await.is_ok());
    }

    #[tokio::test]
    async fn test_job_lifecycle() {
        let applier = MockMetaApplier::new();
        let job_id = JobId::from("FL-lifecycle");

        // Create -> Claim -> Update -> Complete
        applier.create_job(&job_id, JobType::Flush, JobStatus::Queued, None, None, 3, None, None, NodeId::from(1), 1000).await.unwrap();
        applier.claim_job(&job_id, NodeId::from(1), 1100).await.unwrap();
        applier.update_job_status(&job_id, JobStatus::Running, 1200).await.unwrap();
        applier.complete_job(&job_id, None, 1300).await.unwrap();

        assert_eq!(applier.job_ops.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn test_job_failure_path() {
        let applier = MockMetaApplier::new();
        let job_id = JobId::from("FL-fail");

        applier.create_job(&job_id, JobType::Flush, JobStatus::Queued, None, None, 3, None, None, NodeId::from(1), 1000).await.unwrap();
        applier.claim_job(&job_id, NodeId::from(1), 1100).await.unwrap();
        applier.fail_job(&job_id, "Timeout", 1200).await.unwrap();

        assert_eq!(applier.job_ops.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_mixed_operations() {
        let applier = MockMetaApplier::new();
        let ns_id = NamespaceId::from("mixed");
        let table_id = TableId::new(ns_id.clone(), TableName::from("t1"));
        let user_id = UserId::from("u1");
        let user = User {
            id: user_id.clone(),
            username: UserName::new("test"),
            password_hash: "$2b$12$hash".to_string(),
            role: Role::User,
            email: None,
            auth_type: AuthType::Password,
            auth_data: None,
            storage_mode: StorageMode::Table,
            storage_id: None,
            failed_login_attempts: 0,
            locked_until: None,
            last_login_at: None,
            created_at: 1000,
            updated_at: 1000,
            last_seen: None,
            deleted_at: None,
        };

        // Mix of different operation types
        applier.create_namespace(&ns_id, Some(&user_id)).await.unwrap();
        applier.create_table(&table_id, TableType::User, "{}").await.unwrap();
        applier.create_user(&user).await.unwrap();
        applier.create_job(&JobId::from("J1"), JobType::Cleanup, JobStatus::Queued, None, None, 3, None, None, NodeId::from(1), 1000).await.unwrap();

        assert_eq!(applier.get_counts(), (1, 1, 1, 1, 0));
    }
}
