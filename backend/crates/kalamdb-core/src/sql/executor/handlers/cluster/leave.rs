//! CLUSTER LEAVE handler
//!
//! Removes the current node from the cluster (NOT YET IMPLEMENTED)

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::{ExecutionContext, ExecutionResult, ScalarValue, StatementHandler};
use kalamdb_sql::statement_classifier::{SqlStatement, SqlStatementKind};
use std::sync::Arc;

pub struct ClusterLeaveHandler {
    #[allow(dead_code)]
    app_context: Arc<AppContext>,
}

impl ClusterLeaveHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl StatementHandler for ClusterLeaveHandler {
    async fn execute(
        &self,
        statement: SqlStatement,
        _params: Vec<ScalarValue>,
        ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        if !matches!(statement.kind(), SqlStatementKind::ClusterLeave) {
            return Err(KalamDbError::InvalidOperation(format!(
                "CLUSTER LEAVE handler received wrong statement type: {}",
                statement.name()
            )));
        }

        log::warn!(
            "CLUSTER LEAVE attempted by user: {} - NOT IMPLEMENTED",
            ctx.user_id
        );

        // Return warning message - not yet implemented
        Ok(ExecutionResult::Success {
            message: 
                "⚠️  WARNING: CLUSTER LEAVE is not yet implemented!\n\n\
                 This command will eventually allow this node to gracefully leave the cluster,\n\
                 transferring leadership and data to other nodes before departing.\n\n\
                 For now, to remove a node from the cluster:\n\
                 1. Stop the server process\n\
                 2. Update the cluster configuration on other nodes\n\
                 3. Wait for the cluster to detect the node is gone\n\n\
                 See the documentation for cluster management procedures."
                    .to_string(),
        })
    }
}
