//! Typed DDL handler for SHOW NAMESPACES statements

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_sql::ddl::ShowNamespacesStatement;
use std::sync::Arc;

/// Typed handler for SHOW NAMESPACES statements
pub struct ShowNamespacesHandler {
    app_context: Arc<AppContext>,
}

impl ShowNamespacesHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<ShowNamespacesStatement> for ShowNamespacesHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        _statement: ShowNamespacesStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        Err(KalamDbError::InvalidOperation(
            "SHOW NAMESPACES not yet implemented".to_string(),
        ))
    }

    async fn check_authorization(
        &self,
        _statement: &ShowNamespacesStatement,
        _context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // SHOW NAMESPACES allowed for all authenticated users
        Ok(())
    }
}
