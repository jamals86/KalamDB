//! Typed handler for CREATE USER statement

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_sql::ddl::CreateUserStatement;
use std::sync::Arc;

/// Handler for CREATE USER
pub struct CreateUserHandler {
    app_context: Arc<AppContext>,
}

impl CreateUserHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<CreateUserStatement> for CreateUserHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        _statement: CreateUserStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        Err(KalamDbError::InvalidOperation(
            "CREATE USER not yet implemented in typed handler".to_string(),
        ))
    }

    async fn check_authorization(
        &self,
        _statement: &CreateUserStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        if !context.is_admin() {
            return Err(KalamDbError::Unauthorized(
                "CREATE USER requires DBA or System role".to_string(),
            ));
        }
        Ok(())
    }
}
