//! Typed handler for KILL LIVE QUERY statement

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use kalamdb_sql::ddl::KillLiveQueryStatement;
use std::sync::Arc;

/// Handler for KILL LIVE QUERY
pub struct KillLiveQueryHandler {
    app_context: Arc<AppContext>,
}

impl KillLiveQueryHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<KillLiveQueryStatement> for KillLiveQueryHandler {
    async fn execute(
        &self,
        statement: KillLiveQueryStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        let manager = self.app_context.live_query_manager();
        manager.unregister_subscription(&statement.live_id).await?;
        Ok(ExecutionResult::Success {
            message: format!("Live query killed: {}", statement.live_id),
        })
    }

    async fn check_authorization(
        &self,
        _statement: &KillLiveQueryStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        if !context.is_admin() {
            return Err(KalamDbError::Unauthorized(
                "KILL LIVE QUERY requires DBA or System role".to_string(),
            ));
        }
        Ok(())
    }
}
