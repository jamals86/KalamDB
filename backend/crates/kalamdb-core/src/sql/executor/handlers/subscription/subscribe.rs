//! Typed handler for SUBSCRIBE statement

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_sql::ddl::SubscribeStatement;
use std::sync::Arc;

/// Handler for SUBSCRIBE TO (Live Query)
pub struct SubscribeHandler {
    app_context: Arc<AppContext>,
}

impl SubscribeHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<SubscribeStatement> for SubscribeHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        _statement: SubscribeStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        Err(KalamDbError::InvalidOperation(
            "SUBSCRIBE not yet implemented in typed handler".to_string(),
        ))
    }

    async fn check_authorization(
        &self,
        _statement: &SubscribeStatement,
        _context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // SUBSCRIBE allowed for all authenticated users (table-specific auth done in execution)
        Ok(())
    }
}
