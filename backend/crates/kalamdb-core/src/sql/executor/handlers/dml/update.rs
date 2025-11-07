//! UPDATE Handler
//!
//! Handles UPDATE statements with parameter binding support via DataFusion.

use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use async_trait::async_trait;
use datafusion::execution::context::SessionContext;
use kalamdb_sql::ddl::UpdateStatement;

/// Handler for UPDATE statements
///
/// Delegates to DataFusion for UPDATE execution with parameter binding support.
/// Returns rows_affected count (only counts rows with actual changes, not rows matched).
pub struct UpdateHandler;

impl UpdateHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for UpdateHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TypedStatementHandler<UpdateStatement> for UpdateHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        _statement: UpdateStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // TODO: Implement UPDATE handler with TypedStatementHandler pattern
        Err(KalamDbError::InvalidOperation(
            "UPDATE handler not yet implemented for TypedStatementHandler pattern".into(),
        ))
    }

    async fn check_authorization(
        &self,
        _statement: &UpdateStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // UPDATE requires at least User role
        use kalamdb_commons::Role;
        match context.user_role {
            Role::System | Role::Dba | Role::Service | Role::User => Ok(()),
            _ => Err(KalamDbError::PermissionDenied(
                "UPDATE requires User role or higher".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::models::UserId;
    use kalamdb_commons::Role;

    fn test_context(role: Role) -> ExecutionContext {
        ExecutionContext::new(UserId::from("test_user"), role)
    }

    #[tokio::test]
    async fn test_update_authorization_user() {
        let handler = UpdateHandler::new();
        let ctx = test_context(Role::User);
        let stmt = kalamdb_sql::statement_classifier::SqlStatement::new(
            "UPDATE t SET x = 1".to_string(),
            kalamdb_sql::statement_classifier::SqlStatementKind::Update(kalamdb_sql::ddl::UpdateStatement),
        );

        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_authorization_dba() {
        let handler = UpdateHandler::new();
        let ctx = test_context(Role::Dba);
        let stmt = kalamdb_sql::statement_classifier::SqlStatement::new(
            "UPDATE t SET x = 1".to_string(),
            kalamdb_sql::statement_classifier::SqlStatementKind::Update(kalamdb_sql::ddl::UpdateStatement),
        );

        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_authorization_service() {
        let handler = UpdateHandler::new();
        let ctx = test_context(Role::Service);
        let stmt = kalamdb_sql::statement_classifier::SqlStatement::new(
            "UPDATE t SET x = 1".to_string(),
            kalamdb_sql::statement_classifier::SqlStatementKind::Update(kalamdb_sql::ddl::UpdateStatement),
        );

        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_wrong_statement_type() {
        let handler = UpdateHandler::new();
        let ctx = test_context(Role::User);
        let stmt = kalamdb_sql::statement_classifier::SqlStatement::new(
            "SELECT 1".to_string(),
            kalamdb_sql::statement_classifier::SqlStatementKind::Select,
        ); // Wrong type

        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            KalamDbError::InvalidOperation(msg) => {
                assert!(msg.contains("non-UPDATE"));
            }
            _ => panic!("Expected InvalidOperation error"),
        }
    }

    // Note: Actual UPDATE execution tests require table creation and SQL text,
    // which are better suited for integration tests in Phase 7.
}
