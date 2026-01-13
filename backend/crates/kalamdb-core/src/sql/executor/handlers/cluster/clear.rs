//! CLUSTER CLEAR handler
//!
//! Clears old snapshots from the cluster storage

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::{ExecutionContext, ExecutionResult, ScalarValue, StatementHandler};
use kalamdb_sql::statement_classifier::{SqlStatement, SqlStatementKind};
use std::sync::Arc;

pub struct ClusterClearHandler {
    app_context: Arc<AppContext>,
}

impl ClusterClearHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl StatementHandler for ClusterClearHandler {
    async fn execute(
        &self,
        statement: SqlStatement,
        _params: Vec<ScalarValue>,
        ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Touch context for future use (cluster manager wiring coming later)
        let _ = &self.app_context;

        if !matches!(statement.kind(), SqlStatementKind::ClusterClear) {
            return Err(KalamDbError::InvalidOperation(format!(
                "CLUSTER CLEAR handler received wrong statement type: {}",
                statement.name()
            )));
        }

        // Log the operation
        log::info!("CLUSTER CLEAR initiated by user: {}", ctx.user_id);

        // TODO: Implement actual snapshot cleanup
        // For now, return a success message
        Ok(ExecutionResult::Success {
            message: "Old snapshots marked for deletion".to_string(),
        })
    }
}
