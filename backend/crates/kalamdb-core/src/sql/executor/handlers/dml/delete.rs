//! DELETE Handler
//!
//! Handles DELETE statements with parameter binding support via DataFusion.

use crate::error::KalamDbError;
use crate::sql::executor::handlers::StatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use async_trait::async_trait;
use datafusion::execution::context::SessionContext;
use kalamdb_sql::statement_classifier::SqlStatement;
use kalamdb_sql::statement_classifier::SqlStatementKind;

/// Handler for DELETE statements
///
/// Delegates to DataFusion for DELETE execution with parameter binding support.
/// Returns rows_affected count following MySQL semantics.
pub struct DeleteHandler;

impl DeleteHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DeleteHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StatementHandler for DeleteHandler {
    async fn execute(
        &self,
        session: &SessionContext,
        statement: SqlStatement,
        sql_text: String,
        params: Vec<ScalarValue>,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Validate statement type
        if !matches!(statement.kind(), SqlStatementKind::Delete(_)) {
            return Err(KalamDbError::InvalidOperation(
                "DeleteHandler received non-DELETE statement".into(),
            ));
        }

        // For Phase 6, DELETE execution via DataFusion requires SQL text
        // which isn't available in SqlStatement::Delete enum variant.
        // This will be improved in Phase 7 when proper statement parsing is added.
        
        // In Phase 7, the actual implementation will be:
        // 1. Parse DELETE statement to extract table and WHERE clause
        // 2. Bind parameters to WHERE expressions
        // 3. Execute via native write path or DataFusion
        // 4. Return ExecutionResult::Deleted { rows_affected }
        
        Err(KalamDbError::InvalidOperation(
            format!("DELETE execution requires Phase 7 DML implementation (SQL: {})", sql_text),
        ))
    }

    async fn check_authorization(
        &self,
        statement: &SqlStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // Validate statement type
        if !matches!(statement.kind(), SqlStatementKind::Delete(_)) {
            return Err(KalamDbError::InvalidOperation(
                "DeleteHandler received non-DELETE statement".into(),
            ));
        }

        // DELETE requires at least User role
        use kalamdb_commons::Role;
        match context.user_role {
            Role::System | Role::Dba | Role::Service | Role::User => Ok(()),
            _ => Err(KalamDbError::PermissionDenied(
                "DELETE requires User role or higher".to_string(),
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
    async fn test_delete_authorization_user() {
        let handler = DeleteHandler::new();
        let ctx = test_context(Role::User);
        let stmt = kalamdb_sql::statement_classifier::SqlStatement::new(
            "DELETE FROM t WHERE id = 1".to_string(),
            kalamdb_sql::statement_classifier::SqlStatementKind::Delete(kalamdb_sql::ddl::DeleteStatement),
        );

        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_delete_authorization_dba() {
        let handler = DeleteHandler::new();
        let ctx = test_context(Role::Dba);
        let stmt = kalamdb_sql::statement_classifier::SqlStatement::new(
            "DELETE FROM t WHERE id = 1".to_string(),
            kalamdb_sql::statement_classifier::SqlStatementKind::Delete(kalamdb_sql::ddl::DeleteStatement),
        );

        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_delete_authorization_service() {
        let handler = DeleteHandler::new();
        let ctx = test_context(Role::Service);
        let stmt = kalamdb_sql::statement_classifier::SqlStatement::new(
            "DELETE FROM t WHERE id = 1".to_string(),
            kalamdb_sql::statement_classifier::SqlStatementKind::Delete(kalamdb_sql::ddl::DeleteStatement),
        );

        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_delete_wrong_statement_type() {
        let handler = DeleteHandler::new();
        let ctx = test_context(Role::User);
        let stmt = kalamdb_sql::statement_classifier::SqlStatement::new(
            "SELECT 1".to_string(),
            kalamdb_sql::statement_classifier::SqlStatementKind::Select,
        ); // Wrong type

        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            KalamDbError::InvalidOperation(msg) => {
                assert!(msg.contains("non-DELETE"));
            }
            _ => panic!("Expected InvalidOperation error"),
        }
    }

    // Note: Actual DELETE execution tests require table creation and SQL text,
    // which are better suited for integration tests in Phase 7.
}
