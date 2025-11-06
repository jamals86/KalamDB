//! Query Handler (placeholder)

use crate::error::KalamDbError;
use crate::sql::executor::handlers::StatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use async_trait::async_trait;
use datafusion::execution::context::SessionContext;
use kalamdb_sql::statement_classifier::SqlStatement;

/// Handler for query operations. Currently a stub that keeps the crate compiling.
pub struct QueryHandler {
    #[allow(dead_code)]
    app_context: std::sync::Arc<crate::app_context::AppContext>,
}

impl QueryHandler {
    /// Create a new query handler.
    pub fn new(app_context: std::sync::Arc<crate::app_context::AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait]
impl StatementHandler for QueryHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        _statement: SqlStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        Err(KalamDbError::InvalidOperation(
            "Query handler not yet implemented".to_string(),
        ))
    }

    async fn check_authorization(
        &self,
        _statement: &SqlStatement,
        _context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        Ok(())
    }
}
