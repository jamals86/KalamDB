//! Typed DDL handler for ALTER NAMESPACE statements

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_sql::ddl::AlterNamespaceStatement;
use std::sync::Arc;

/// Typed handler for ALTER NAMESPACE statements
pub struct AlterNamespaceHandler {
    app_context: Arc<AppContext>,
}

impl AlterNamespaceHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<AlterNamespaceStatement> for AlterNamespaceHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        _statement: AlterNamespaceStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        Err(KalamDbError::InvalidOperation(
            "ALTER NAMESPACE not yet implemented".to_string(),
        ))
    }

    async fn check_authorization(
        &self,
        _statement: &AlterNamespaceStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        if !context.is_admin() {
            return Err(KalamDbError::Unauthorized(
                "Insufficient privileges to alter namespaces. DBA or System role required."
                    .to_string(),
            ));
        }
        Ok(())
    }
}
