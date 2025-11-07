//! Typed DDL handler for CREATE STORAGE statements

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_sql::ddl::CreateStorageStatement;
use std::sync::Arc;

/// Typed handler for CREATE STORAGE statements
pub struct CreateStorageHandler {
    app_context: Arc<AppContext>,
}

impl CreateStorageHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<CreateStorageStatement> for CreateStorageHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        _statement: CreateStorageStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        Err(KalamDbError::InvalidOperation(
            "CREATE STORAGE not yet implemented".to_string(),
        ))
    }

    async fn check_authorization(
        &self,
        _statement: &CreateStorageStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        if !context.is_admin() {
            return Err(KalamDbError::Unauthorized(
                "Insufficient privileges to create storage. DBA or System role required."
                    .to_string(),
            ));
        }
        Ok(())
    }
}
