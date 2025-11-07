//! Typed DDL handler for SHOW TABLES statements

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_sql::ddl::ShowTablesStatement;
use std::sync::Arc;

/// Typed handler for SHOW TABLES statements
pub struct ShowTablesHandler {
    app_context: Arc<AppContext>,
}

impl ShowTablesHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<ShowTablesStatement> for ShowTablesHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        _statement: ShowTablesStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        Err(KalamDbError::InvalidOperation(
            "SHOW TABLES not yet implemented".to_string(),
        ))
    }

    async fn check_authorization(
        &self,
        _statement: &ShowTablesStatement,
        _context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // SHOW TABLES allowed for all authenticated users
        Ok(())
    }
}
