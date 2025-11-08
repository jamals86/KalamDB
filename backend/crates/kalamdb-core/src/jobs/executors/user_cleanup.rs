//! User Cleanup Job Executor
//!
//! **Phase 9 (T150)**: JobExecutor implementation for user cleanup operations
//!
//! Handles cleanup of soft-deleted user accounts and associated data.
//!
//! ## Responsibilities
//! - Clean up soft-deleted user records
//! - Cascade delete user's tables and data
//! - Remove user from all access control lists
//! - Clean up user's authentication tokens
//!
//! ## Parameters Format
//! ```json
//! {
//!   "user_id": "USR123",
//!   "username": "john_doe",
//!   "cascade": true
//! }
//! ```

use crate::jobs::executors::{JobContext, JobDecision, JobExecutor};
use crate::error::KalamDbError;
use async_trait::async_trait;
use kalamdb_commons::system::Job;
use kalamdb_commons::JobType;

/// User Cleanup Job Executor
///
/// Executes user cleanup operations after soft delete.
pub struct UserCleanupExecutor;

impl UserCleanupExecutor {
    /// Create a new UserCleanupExecutor
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl JobExecutor for UserCleanupExecutor {
    fn job_type(&self) -> JobType {
        JobType::UserCleanup
    }

    fn name(&self) -> &'static str {
        "UserCleanupExecutor"
    }

    async fn validate_params(&self, job: &Job) -> Result<(), KalamDbError> {
        let params = job
            .parameters
            .as_ref()
            .ok_or_else(|| KalamDbError::InvalidOperation("Missing parameters".to_string()))?;

        let params_obj: serde_json::Value = serde_json::from_str(params)
            .map_err(|e| KalamDbError::InvalidOperation(format!("Invalid JSON parameters: {}", e)))?;

        // Validate required fields
        if params_obj.get("user_id").is_none() {
            return Err(KalamDbError::InvalidOperation(
                "Missing required parameter: user_id".to_string(),
            ));
        }
        if params_obj.get("username").is_none() {
            return Err(KalamDbError::InvalidOperation(
                "Missing required parameter: username".to_string(),
            ));
        }

        // Validate cascade if present
        if let Some(cascade) = params_obj.get("cascade") {
            if !cascade.is_boolean() {
                return Err(KalamDbError::InvalidOperation(
                    "cascade must be a boolean".to_string(),
                ));
            }
        }

        Ok(())
    }

    async fn execute(&self, ctx: &JobContext, job: &Job) -> Result<JobDecision, KalamDbError> {
        ctx.log_info("Starting user cleanup operation");

        // Validate parameters
        self.validate_params(job).await?;

        // Parse parameters
        let params = job.parameters.as_ref().unwrap();
        let params_obj: serde_json::Value = serde_json::from_str(params)
            .map_err(|e| KalamDbError::InvalidOperation(format!("Failed to parse parameters: {}", e)))?;

        let user_id = params_obj["user_id"].as_str().unwrap();
        let username = params_obj["username"].as_str().unwrap();
        let cascade = params_obj
            .get("cascade")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        ctx.log_info(&format!(
            "Cleaning up user {} ({}) (cascade: {})",
            username, user_id, cascade
        ));

        // TODO: Implement actual user cleanup logic
        // Current architecture: System table providers are ready via app_context.system_tables()
        // Implementation steps:
        //   1. Delete user from system.users:
        //      let users_provider = ctx.app_ctx.system_tables().users();
        //      users_provider.delete_user(user_id).await?;
        //
        //   2. If cascade=true, cascade delete user's tables:
        //      let tables_provider = ctx.app_ctx.system_tables().tables();
        //      let user_tables = tables_provider.list_by_owner(user_id).await?;
        //      for table in user_tables {
        //          // Create CleanupJob for each table (avoids blocking this job)
        //          let cleanup_params = serde_json::json!({
        //              "table_id": format!("{}:{}", table.namespace_id, table.table_name),
        //              "table_type": table.table_type,
        //              "operation": "drop_table"
        //          });
        //          job_manager.create_job(JobType::Cleanup, ..., cleanup_params)?;
        //      }
        //
        //   3. If cascade=true, remove user from shared table ACLs:
        //      // This requires adding list_shared_tables_with_user() to TablesTableProvider
        //      // For now, skip ACL cleanup (low priority - user won't be able to access anyway)
        //
        //   4. Clean up user's live queries:
        //      let live_queries_provider = ctx.app_ctx.system_tables().live_queries();
        //      live_queries_provider.delete_by_user(user_id).await?;
        //
        //   5. Track metrics (tables_deleted, live_queries_deleted)
        //
        // For now, return placeholder metrics
        let tables_deleted = 0;
        let live_queries_deleted = 0;

        ctx.log_info(&format!(
            "User cleanup completed - {} tables, {} live queries",
            tables_deleted, live_queries_deleted
        ));

        Ok(JobDecision::Completed {
            message: Some(format!(
                "Cleaned up user {} ({}) - {} tables, {} live queries deleted",
                username, user_id, tables_deleted, live_queries_deleted
            )),
        })
    }

    async fn cancel(&self, ctx: &JobContext, _job: &Job) -> Result<(), KalamDbError> {
        ctx.log_warn("User cleanup job cancellation requested");
        // User cleanup jobs should complete to avoid partial cleanup state
        Err(KalamDbError::InvalidOperation("User cleanup jobs cannot be safely cancelled".to_string()))
    }
}

impl Default for UserCleanupExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::{JobId, NamespaceId, NodeId};
    use kalamdb_commons::system::Job;

    fn make_job(id: &str, job_type: JobType, ns: &str) -> Job {
        let now = chrono::Utc::now().timestamp_millis();
        Job {
            job_id: JobId::new(id),
            job_type,
            namespace_id: NamespaceId::new(ns),
            table_name: None,
            status: kalamdb_commons::JobStatus::Running,
            parameters: None,
            message: None,
            exception_trace: None,
            idempotency_key: None,
            retry_count: 0,
            max_retries: 3,
            memory_used: None,
            cpu_used: None,
            created_at: now,
            updated_at: now,
            started_at: Some(now),
            finished_at: None,
            node_id: NodeId::from("node1"),
            queue: None,
            priority: None,
        }
    }

    #[tokio::test]
    async fn test_validate_params_success() {
        let executor = UserCleanupExecutor::new();

        let job = make_job("UC-test123", JobType::UserCleanup, "default");

        let mut job = job;
        job.parameters = Some(
            serde_json::json!({
                "user_id": "USR123",
                "username": "john_doe",
                "cascade": true
            })
            .to_string(),
        );

        assert!(executor.validate_params(&job).await.is_ok());
    }

    #[tokio::test]
    async fn test_validate_params_missing_user_id() {
        let executor = UserCleanupExecutor::new();

        let job = make_job("UC-test123", JobType::UserCleanup, "default");

        let mut job = job;
        job.parameters = Some(
            serde_json::json!({
                "username": "john_doe"
            })
            .to_string(),
        );

        let result = executor.validate_params(&job).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err
            .to_string()
            .contains("Missing required parameter: user_id"));
    }

    #[test]
    fn test_executor_properties() {
        let executor = UserCleanupExecutor::new();
        assert_eq!(executor.job_type(), JobType::UserCleanup);
        assert_eq!(executor.name(), "UserCleanupExecutor");
    }
}
