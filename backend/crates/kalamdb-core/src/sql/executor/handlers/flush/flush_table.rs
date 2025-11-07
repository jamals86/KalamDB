//! Typed handler for FLUSH TABLE statement

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_sql::ddl::FlushTableStatement;
use std::sync::Arc;

/// Handler for FLUSH TABLE
pub struct FlushTableHandler {
    app_context: Arc<AppContext>,
}

impl FlushTableHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<FlushTableStatement> for FlushTableHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        _statement: FlushTableStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        Err(KalamDbError::InvalidOperation(
            "FLUSH TABLE not yet implemented in typed handler".to_string(),
        ))
    }

    async fn check_authorization(
        &self,
        _statement: &FlushTableStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        if !context.is_admin() {
            return Err(KalamDbError::Unauthorized(
                "FLUSH TABLE requires DBA or System role".to_string(),
            ));
        }
        Ok(())
    }
}
