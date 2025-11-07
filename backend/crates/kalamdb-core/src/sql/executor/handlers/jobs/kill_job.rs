//! Typed handler for KILL JOB statement

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_sql::ddl::JobCommand;
use std::sync::Arc;

/// Handler for KILL JOB
pub struct KillJobHandler {
    app_context: Arc<AppContext>,
}

impl KillJobHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<JobCommand> for KillJobHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        _statement: JobCommand,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        Err(KalamDbError::InvalidOperation(
            "KILL JOB not yet implemented in typed handler".to_string(),
        ))
    }

    async fn check_authorization(
        &self,
        _statement: &JobCommand,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        if !context.is_admin() {
            return Err(KalamDbError::Unauthorized(
                "KILL JOB requires DBA or System role".to_string(),
            ));
        }
        Ok(())
    }
}
