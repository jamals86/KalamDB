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
        let namespaces_provider = self.app_context.system_tables().namespaces();
        
        // Query all namespaces via the table provider (returns RecordBatch)
        let batches = namespaces_provider.scan_all_namespaces()?;
        
        // Return as query result
            let row_count = batches.num_rows();
            Ok(ExecutionResult::Rows {
                batches: vec![batches],
                row_count,
            })
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

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::Role;
    use kalamdb_commons::models::UserId;

    fn create_test_context() -> ExecutionContext {
        ExecutionContext::new(UserId::new("test_user"), Role::User)
    }

    #[tokio::test]
    async fn test_show_namespaces_authorization() {
        let app_ctx = AppContext::get();
        let handler = ShowNamespacesHandler::new(app_ctx);
        let stmt = ShowNamespacesStatement {};
        
        // All users can show namespaces
        let ctx = create_test_context();
        let result = handler.check_authorization(&stmt, &ctx).await;
        
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_show_namespaces_success() {
        let app_ctx = AppContext::get();
        let handler = ShowNamespacesHandler::new(app_ctx);
        let stmt = ShowNamespacesStatement {};
        let ctx = create_test_context();
        let session = SessionContext::new();

        let result = handler.execute(&session, stmt, vec![], &ctx).await;
        
        // Should return batches
        assert!(result.is_ok());
            if let Ok(ExecutionResult::Rows { batches, .. }) = result {
            assert!(!batches.is_empty());
        }
    }
}
