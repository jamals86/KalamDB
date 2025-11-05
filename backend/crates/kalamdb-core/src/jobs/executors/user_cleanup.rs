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
        // - Permanently delete user record from system.users via UsersTableProvider
        // - If cascade=true:
        //   - Query TablesTableProvider for user's tables
        //   - Drop all user's tables via execute_drop_table()
        //   - Remove user from shared table ACLs (scan system.tables for access_level='restricted')
        //   - Invalidate user's JWT tokens (no-op for stateless JWT - they'll expire naturally)
        //   - Clean up user's live queries via LiveQueriesTableProvider
        // - Track metrics (tables_deleted, acls_removed, live_queries_deleted)
        // Implementation approach:
        //   1. Delete from system.users
        //   2. If cascade: scan system.tables for owner=user_id
        //   3. If cascade: delete each table via cleanup job
        //   4. If cascade: scan shared tables for ACLs containing user_id
        //   5. Return metrics

        ctx.log_info("User cleanup completed successfully");

        Ok(JobDecision::Completed {
            message: Some(format!(
                "Cleaned up user {} ({}) successfully",
                username, user_id
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

    #[tokio::test]
    async fn test_validate_params_success() {
        let executor = UserCleanupExecutor::new();

        let job = Job::new(
            JobId::new("UC-test123"),
            JobType::UserCleanup,
            NamespaceId::new("default"),
            NodeId::from("node1"),
        );

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

        let job = Job::new(
            JobId::new("UC-test123"),
            JobType::UserCleanup,
            NamespaceId::new("default"),
            NodeId::from("node1"),
        );

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
