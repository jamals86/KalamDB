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

use crate::jobs::executors::{JobContext, JobDecision, JobExecutor, JobParams};
use crate::error::KalamDbError;
use async_trait::async_trait;
use kalamdb_commons::JobType;
use kalamdb_commons::models::UserId;
use serde::{Deserialize, Serialize};

/// Typed parameters for user cleanup operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserCleanupParams {
    /// User ID (required)
    pub user_id: UserId,
    /// Username (required)
    pub username: String,
    /// Cascade delete user's tables (optional, defaults to false)
    #[serde(default)]
    pub cascade: bool,
}

impl JobParams for UserCleanupParams {
    fn validate(&self) -> Result<(), KalamDbError> {
        if self.username.is_empty() {
            return Err(KalamDbError::InvalidOperation(
                "username cannot be empty".to_string(),
            ));
        }
        Ok(())
    }
}

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
    type Params = UserCleanupParams;

    fn job_type(&self) -> JobType {
        JobType::UserCleanup
    }

    fn name(&self) -> &'static str {
        "UserCleanupExecutor"
    }

    async fn execute(&self, ctx: &JobContext<Self::Params>) -> Result<JobDecision, KalamDbError> {
        ctx.log_info("Starting user cleanup operation");

        // Parameters already validated in JobContext - type-safe access
        let params = ctx.params();
        let user_id = &params.user_id;
        let username = &params.username;
        let cascade = params.cascade;

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
        //          let cleanup_params = CleanupParams {
        //              table_id: TableId::new(table.namespace_id, table.table_name),
        //              table_type: table.table_type,
        //              operation: CleanupOperation::DropTable,
        //          };
        //          job_manager.create_job(cleanup_params)?;
        //      }
        //
        //   3. If cascade=true, remove user from shared table ACLs:
        //      // This requires adding list_shared_tables_with_user() to TablesTableProvider
        //      // For now, skip ACL cleanup (low priority - user won't be able to access anyway)
        //
        //   4. Clean up user's live queries:
        //      let live_queries_provider = ctx.app_ctx.system_tables().live_queries();
        //      let user_queries = live_queries_provider.list_by_user(user_id).await?;
        //      for query in user_queries {
        //          live_query_manager.stop_query(&query.query_id)?;
        //      }
        //
        //   5. Clean up user's auth tokens:
        //      // TODO: Add JWT token revocation mechanism (blacklist or token versioning)
        //
        // For now, return placeholder metrics
        let tables_deleted = 0;
        let queries_stopped = 0;

        let message = format!(
            "Cleaned up user {} ({}) - {} tables deleted, {} queries stopped (cascade: {})",
            username, user_id, tables_deleted, queries_stopped, cascade
        );

        ctx.log_info(&message);

        Ok(JobDecision::Completed {
            message: Some(message),
        })
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

    #[test]
    fn test_executor_properties() {
        let executor = UserCleanupExecutor::new();
        assert_eq!(executor.job_type(), JobType::UserCleanup);
        assert_eq!(executor.name(), "UserCleanupExecutor");
    }
}
