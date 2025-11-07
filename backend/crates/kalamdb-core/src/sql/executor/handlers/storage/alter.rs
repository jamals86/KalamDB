//! Typed DDL handler for ALTER STORAGE statements

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_sql::ddl::AlterStorageStatement;
use std::sync::Arc;

/// Typed handler for ALTER STORAGE statements
pub struct AlterStorageHandler {
    app_context: Arc<AppContext>,
}

impl AlterStorageHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<AlterStorageStatement> for AlterStorageHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        _statement: AlterStorageStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        Err(KalamDbError::InvalidOperation(
            "ALTER STORAGE not yet implemented".to_string(),
        ))
    }

    async fn check_authorization(
        &self,
        _statement: &AlterStorageStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        if !context.is_admin() {
            return Err(KalamDbError::Unauthorized(
                "Insufficient privileges to alter storage. DBA or System role required."
                    .to_string(),
            ));
        }
        Ok(())
    }
}
