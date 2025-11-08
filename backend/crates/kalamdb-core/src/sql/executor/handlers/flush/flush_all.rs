//! Typed handler for FLUSH ALL TABLES statement

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_sql::ddl::FlushAllTablesStatement;
use std::sync::Arc;

/// Handler for FLUSH ALL TABLES
pub struct FlushAllTablesHandler {
    app_context: Arc<AppContext>,
}

impl FlushAllTablesHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<FlushAllTablesStatement> for FlushAllTablesHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        _statement: FlushAllTablesStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        Err(KalamDbError::InvalidOperation(
            "FLUSH ALL TABLES not yet implemented in typed handler".to_string(),
        ))
    }

    async fn check_authorization(
        &self,
        _statement: &FlushAllTablesStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        if !context.is_admin() {
            return Err(KalamDbError::Unauthorized(
                "FLUSH ALL TABLES requires DBA or System role".to_string(),
            ));
        }
        Ok(())
    }
}
