//! Typed DDL handler for DROP STORAGE statements

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_sql::ddl::DropStorageStatement;
use std::sync::Arc;

/// Typed handler for DROP STORAGE statements
pub struct DropStorageHandler {
    app_context: Arc<AppContext>,
}

impl DropStorageHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<DropStorageStatement> for DropStorageHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        _statement: DropStorageStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        Err(KalamDbError::InvalidOperation(
            "DROP STORAGE not yet implemented".to_string(),
        ))
    }

    async fn check_authorization(
        &self,
        _statement: &DropStorageStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        if !context.is_admin() {
            return Err(KalamDbError::Unauthorized(
                "Insufficient privileges to drop storage. DBA or System role required."
                    .to_string(),
            ));
        }
        Ok(())
    }
}
