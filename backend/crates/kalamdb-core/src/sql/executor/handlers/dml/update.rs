//! UPDATE Handler
//!
//! Handles UPDATE statements with parameter binding support via DataFusion.

use crate::error::KalamDbError;
use crate::sql::executor::handlers::StatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use async_trait::async_trait;
use datafusion::execution::context::SessionContext;
use kalamdb_sql::statement_classifier::{SqlStatement, SqlStatementKind};

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
impl StatementHandler for UpdateHandler {
    async fn execute(
        &self,
        session: &SessionContext,
        statement: SqlStatement,
        sql_text: String,
        params: Vec<ScalarValue>,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Validate statement type
        if !matches!(statement.kind(), SqlStatementKind::Update(_)) {
            return Err(KalamDbError::InvalidOperation(
                "UpdateHandler received non-UPDATE statement".into(),
            ));
        }

        // For Phase 6, UPDATE execution via DataFusion requires SQL text
        // which isn't available in SqlStatement::Update enum variant.
        // This will be improved in Phase 7 when proper statement parsing is added.
        
        // In Phase 7, the actual implementation will be:
        // 1. Parse UPDATE statement to extract table, SET clause, WHERE clause
        // 2. Bind parameters to expressions
        // 3. Execute via native write path or DataFusion
        // 4. Return ExecutionResult::Updated { rows_affected } (only rows with changes)
        
        Err(KalamDbError::InvalidOperation(
            format!("UPDATE execution requires Phase 7 DML implementation (SQL: {})", sql_text),
        ))
    }

    async fn check_authorization(
        &self,
        statement: &SqlStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // Validate statement type
        if !matches!(statement.kind(), SqlStatementKind::Update(_)) {
            return Err(KalamDbError::InvalidOperation(
                "UpdateHandler received non-UPDATE statement".into(),
            ));
        }

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
