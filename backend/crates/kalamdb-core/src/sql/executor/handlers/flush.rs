//! Flush Handler
//!
//! Handles FLUSH TABLE operations for user and shared tables.
//! **Phase 7 Task T068**: Implement flush operations handler.

use crate::error::KalamDbError;
use crate::sql::executor::handlers::{
    ExecutionContext, ExecutionResult, ParamValue, StatementHandler,
};
use async_trait::async_trait;
use datafusion::execution::context::SessionContext;
use kalamdb_sql::statement_classifier::SqlStatement;

/// Handler for FLUSH TABLE operations (placeholder)
pub struct FlushHandler {
    #[allow(dead_code)]
    app_context: std::sync::Arc<crate::app_context::AppContext>,
}

impl FlushHandler {
    /// Create a new flush handler
    pub fn new(app_context: std::sync::Arc<crate::app_context::AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait]
impl StatementHandler for FlushHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        _statement: SqlStatement,
        _params: Vec<ParamValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        Err(KalamDbError::InvalidOperation(
            "Flush handler not yet implemented".to_string(),
        ))
    }

    async fn check_authorization(
        &self,
        _statement: &SqlStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // FLUSH TABLE requires Dba or System role
        use kalamdb_commons::Role;
            if !matches!(context.user_role(), Role::Dba | Role::System) {
            return Err(KalamDbError::Unauthorized(
                "FLUSH TABLE requires Dba or System role".to_string(),
            ));
        }
        Ok(())
    }
}
