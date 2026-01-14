//! CLUSTER LIST handler
//!
//! Lists all nodes in the cluster using the system.cluster table

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::{ExecutionContext, ExecutionResult, ScalarValue, StatementHandler};
use kalamdb_sql::statement_classifier::{SqlStatement, SqlStatementKind};
use std::sync::Arc;

pub struct ClusterListHandler {
    app_context: Arc<AppContext>,
}

impl ClusterListHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl StatementHandler for ClusterListHandler {
    async fn execute(
        &self,
        statement: SqlStatement,
        _params: Vec<ScalarValue>,
        ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Touch context for future use (cluster manager wiring coming later)
        let _ = &self.app_context;

        if !matches!(statement.kind(), SqlStatementKind::ClusterList) {
            return Err(KalamDbError::InvalidOperation(format!(
                "CLUSTER LIST handler received wrong statement type: {}",
                statement.name()
            )));
        }

        // CLUSTER LIST is just a convenience alias for SELECT from system.cluster
        // Delegate to the actual query execution by returning Ok with instruction
        // to treat it as a SELECT query
        
        log::info!("CLUSTER LIST queried by user: {}", ctx.user_id);

        // For now, return an empty result set with a placeholder message
        // In a real implementation, this would query system.cluster table
        Ok(ExecutionResult::Success {
            message: "Cluster listing not yet implemented".to_string(),
        })
    }
}
