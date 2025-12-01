//! Typed DDL handler for SHOW STORAGES statements

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
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
        _statement: ShowStoragesStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        let storages_provider = self.app_context.system_tables().storages();

        // Query all storages via the table provider (returns RecordBatch)
        let batches = storages_provider.scan_all_storages()?;

        // Return as query result
        let row_count = batches.num_rows();
        Ok(ExecutionResult::Rows {
            batches: vec![batches],
            row_count,
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::create_test_session;
    use datafusion::prelude::SessionContext;
    use kalamdb_commons::models::UserId;
    use kalamdb_commons::Role;

    fn create_test_context() -> ExecutionContext {
        ExecutionContext::new(UserId::new("test_user"), Role::User, create_test_session())
    }

    #[tokio::test]
    async fn test_show_storages_authorization() {
        let app_ctx = AppContext::get();
        let handler = ShowStoragesHandler::new(app_ctx);
        let stmt = ShowStoragesStatement {};

        // All users can show storages
        let ctx = create_test_context();
        let result = handler.check_authorization(&stmt, &ctx).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_show_storages_success() {
        let app_ctx = AppContext::get();
        let handler = ShowStoragesHandler::new(app_ctx);
        let stmt = ShowStoragesStatement {};
        let ctx = create_test_context();

        let result = handler.execute(stmt, vec![], &ctx).await;

        // Should return batches
        assert!(result.is_ok());
        if let Ok(ExecutionResult::Rows { batches, .. }) = result {
            assert!(!batches.is_empty());
        }
    }
}
