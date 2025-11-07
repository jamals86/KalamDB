//! Typed DDL handler for SHOW STORAGES statements

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_sql::ddl::ShowStoragesStatement;
use std::sync::Arc;

/// Typed handler for SHOW STORAGES statements
pub struct ShowStoragesHandler {
    app_context: Arc<AppContext>,
}

impl ShowStoragesHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<ShowStoragesStatement> for ShowStoragesHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        _statement: ShowStoragesStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        Err(KalamDbError::InvalidOperation(
            "SHOW STORAGES not yet implemented".to_string(),
        ))
    }

    async fn check_authorization(
        &self,
        _statement: &ShowStoragesStatement,
        _context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // SHOW STORAGES allowed for all authenticated users
        Ok(())
    }
}
