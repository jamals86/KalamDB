//! Typed DDL handler for ALTER TABLE statements

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_sql::ddl::AlterTableStatement;
use std::sync::Arc;

/// Typed handler for ALTER TABLE statements
pub struct AlterTableHandler {
    app_context: Arc<AppContext>,
}

impl AlterTableHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<AlterTableStatement> for AlterTableHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        _statement: AlterTableStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        Err(KalamDbError::InvalidOperation(
            "ALTER TABLE not yet implemented in typed handler".to_string(),
        ))
    }

    async fn check_authorization(
        &self,
        _statement: &AlterTableStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // TODO: Check table ownership for USER tables, DBA for SHARED/STREAM
        if !context.is_admin() {
            // Simplified check - implement proper table ownership check
            return Err(KalamDbError::Unauthorized(
                "Insufficient privileges to alter table".to_string(),
            ));
        }
        Ok(())
    }
}
