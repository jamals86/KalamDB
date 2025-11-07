//! Typed handler for DROP USER statement

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_sql::ddl::DropUserStatement;
use std::sync::Arc;

/// Handler for DROP USER
pub struct DropUserHandler {
    app_context: Arc<AppContext>,
}

impl DropUserHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<DropUserStatement> for DropUserHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        _statement: DropUserStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        Err(KalamDbError::InvalidOperation(
            "DROP USER not yet implemented in typed handler".to_string(),
        ))
    }

    async fn check_authorization(
        &self,
        _statement: &DropUserStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        if !context.is_admin() {
            return Err(KalamDbError::Unauthorized(
                "DROP USER requires DBA or System role".to_string(),
            ));
        }
        Ok(())
    }
}
