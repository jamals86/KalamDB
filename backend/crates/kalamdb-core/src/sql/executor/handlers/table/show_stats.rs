//! Typed DDL handler for SHOW STATS statements

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_sql::ddl::ShowTableStatsStatement;
use std::sync::Arc;

/// Typed handler for SHOW STATS statements
pub struct ShowStatsHandler {
    app_context: Arc<AppContext>,
}

impl ShowStatsHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<ShowTableStatsStatement> for ShowStatsHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        _statement: ShowTableStatsStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        Err(KalamDbError::InvalidOperation(
            "SHOW STATS not yet implemented".to_string(),
        ))
    }

    async fn check_authorization(
        &self,
        _statement: &ShowTableStatsStatement,
        _context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // SHOW STATS allowed for all authenticated users
        Ok(())
    }
}
