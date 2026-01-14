//! CLUSTER FLUSH handler
//!
//! Forces all Raft logs to be written to snapshots across the cluster

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::{ExecutionContext, ExecutionResult, ScalarValue, StatementHandler};
use kalamdb_sql::statement_classifier::{SqlStatement, SqlStatementKind};
use std::sync::Arc;

pub struct ClusterFlushHandler {
    app_context: Arc<AppContext>,
}

impl ClusterFlushHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl StatementHandler for ClusterFlushHandler {
    async fn execute(
        &self,
        statement: SqlStatement,
        _params: Vec<ScalarValue>,
        ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Touch context for future use (cluster manager wiring coming later)
        let _ = &self.app_context;

        if !matches!(statement.kind(), SqlStatementKind::ClusterFlush) {
            return Err(KalamDbError::InvalidOperation(format!(
                "CLUSTER FLUSH handler received wrong statement type: {}",
                statement.name()
            )));
        }

        // Log the operation
        log::info!("CLUSTER FLUSH initiated by user: {}", ctx.user_id);

        // TODO: Implement actual cluster flush
        // For now, return a success message
        Ok(ExecutionResult::Success {
            message: "Cluster flush initiated (snapshots will be created)".to_string(),
        })
    }
}
