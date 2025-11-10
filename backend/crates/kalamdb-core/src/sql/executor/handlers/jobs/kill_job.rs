//! Typed handler for KILL JOB statement

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use kalamdb_commons::JobId;
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
        statement: JobCommand,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        let job_manager = self.app_context.job_manager();
        match statement {
            JobCommand::Kill { job_id } => {
                let job_id_typed = JobId::new(job_id.clone());
                job_manager.cancel_job(&job_id_typed).await?;
                Ok(ExecutionResult::JobKilled { job_id, status: "cancelled".to_string() })
            }
        }
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
