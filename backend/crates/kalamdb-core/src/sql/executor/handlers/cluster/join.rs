//! CLUSTER JOIN handler
//!
//! Joins the current node to an existing cluster (NOT YET IMPLEMENTED)

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::{
    ExecutionContext, ExecutionResult, ScalarValue, StatementHandler,
};
use kalamdb_sql::statement_classifier::{SqlStatement, SqlStatementKind};
use std::sync::Arc;

pub struct ClusterJoinHandler {
    #[allow(dead_code)]
    app_context: Arc<AppContext>,
}

impl ClusterJoinHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl StatementHandler for ClusterJoinHandler {
    async fn execute(
        &self,
        statement: SqlStatement,
        _params: Vec<ScalarValue>,
        ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        let addr = match statement.kind() {
            SqlStatementKind::ClusterJoin(addr) => addr.clone(),
            _ => {
                return Err(KalamDbError::InvalidOperation(format!(
                    "CLUSTER JOIN handler received wrong statement type: {}",
                    statement.name()
                )));
            },
        };

        log::warn!(
            "CLUSTER JOIN attempted by user: {} to address: {} - NOT IMPLEMENTED",
            ctx.user_id(),
            addr
        );

        // Return warning message - not yet implemented
        Ok(ExecutionResult::Success {
            message: format!(
                "⚠️  WARNING: CLUSTER JOIN is not yet implemented!\n\n\
                 This command will eventually allow this node to join an existing cluster\n\
                 at address: {}\n\n\
                 For now, cluster membership must be configured at server startup.\n\
                 See the documentation for cluster configuration options.",
                addr
            ),
        })
    }
}
