//! Typed DDL handler for DROP TABLE statements

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_sql::ddl::DropTableStatement;
use std::sync::Arc;

/// Typed handler for DROP TABLE statements
pub struct DropTableHandler {
    app_context: Arc<AppContext>,
}

impl DropTableHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<DropTableStatement> for DropTableHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        _statement: DropTableStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        Err(KalamDbError::InvalidOperation(
            "DROP TABLE not yet implemented in typed handler".to_string(),
        ))
    }

    async fn check_authorization(
        &self,
        _statement: &DropTableStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // TODO: Check table ownership for USER tables, DBA for SHARED/STREAM
        if !context.is_admin() {
            return Err(KalamDbError::Unauthorized(
                "Insufficient privileges to drop table".to_string(),
            ));
        }
        Ok(())
    }
}
