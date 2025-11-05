//! Table Registry Handler
//!
//! Handles REGISTER TABLE and UNREGISTER TABLE operations.
//! **Phase 7 Task T071**: Implement table registry handler.

use crate::error::KalamDbError;
use crate::sql::executor::handlers::{
    ExecutionContext, ExecutionResult, ParamValue, StatementHandler,
};
use async_trait::async_trait;
use datafusion::execution::context::SessionContext;
use kalamdb_sql::statement_classifier::SqlStatement;

/// Handler for table registry operations
pub struct TableRegistryHandler {
    app_context: std::sync::Arc<crate::app_context::AppContext>,
}

impl TableRegistryHandler {
    /// Create a new table registry handler
    pub fn new(app_context: std::sync::Arc<crate::app_context::AppContext>) -> Self {
        Self { app_context }
    }

    /// Execute REGISTER TABLE statement
    pub async fn execute_register_table(
        &self,
        session: &SessionContext,
        statement: SqlStatement,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // TODO: Implement REGISTER TABLE logic
        // 1. Extract table name, namespace, and provider details from statement
        // 2. Validate table doesn't already exist
        // 3. Create table metadata entry in system.tables
        // 4. Register table provider in DataFusion catalog
        // 5. Invalidate schema cache
        // 6. Log audit entry
        // 7. Return ExecutionResult
        //
        // **Use Case**: External tables, views, materialized views
        // **Authorization**: Only Dba and System roles
        
        Err(KalamDbError::InvalidOperation(
            "REGISTER TABLE not yet implemented in TableRegistryHandler".to_string(),
        ))
    }

    /// Execute UNREGISTER TABLE statement
    pub async fn execute_unregister_table(
        &self,
        session: &SessionContext,
        statement: SqlStatement,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // TODO: Implement UNREGISTER TABLE logic
        // 1. Extract table name and namespace from statement
        // 2. Validate table exists and is registered (not native USER/SHARED/STREAM)
        // 3. Soft delete table metadata in system.tables
        // 4. Unregister from DataFusion catalog
        // 5. Invalidate schema cache
        // 6. Log audit entry
        // 7. Return ExecutionResult
        //
        // **Authorization**: Only Dba and System roles
        // **Validation**: Cannot unregister system tables
        
        Err(KalamDbError::InvalidOperation(
            "UNREGISTER TABLE not yet implemented in TableRegistryHandler".to_string(),
        ))
    }
}

#[async_trait]
impl StatementHandler for TableRegistryHandler {
    async fn execute(
        &self,
        session: &SessionContext,
        statement: SqlStatement,
        params: Vec<ParamValue>,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        match statement {
            SqlStatement::RegisterTable { .. } => {
                self.execute_register_table(session, statement, context)
                    .await
            }
            SqlStatement::UnregisterTable { .. } => {
                self.execute_unregister_table(session, statement, context)
                    .await
            }
            _ => Err(KalamDbError::InvalidOperation(
                "Not a table registry statement".to_string(),
            )),
        }
    }

    async fn check_authorization(
        &self,
        statement: &SqlStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        use kalamdb_commons::Role;
        
        // Table registry operations require Dba or System role
            if !matches!(context.user_role(), Role::Dba | Role::System) {
            return Err(KalamDbError::Unauthorized(
                "Table registry operations require Dba or System role".to_string(),
            ));
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::{Role, UserId};

    #[tokio::test]
    async fn test_table_registry_handler_placeholder() {
        // Placeholder test to verify module compiles
        let app_ctx = crate::test_helpers::init_test_app_context();
        let handler = TableRegistryHandler::new(app_ctx);
        let ctx = ExecutionContext::new(UserId::from("test"), Role::Dba);

        // Should return "not yet implemented" error
        let result = handler
            .execute_register_table(&SessionContext::new(), SqlStatement::Unknown, &ctx)
            .await;
        
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_table_registry_authorization() {
        let app_ctx = crate::test_helpers::init_test_app_context();
        let handler = TableRegistryHandler::new(app_ctx);
        
        // User role should be unauthorized
        let ctx_user = ExecutionContext::new(UserId::from("test"), Role::User);
        let result = handler.check_authorization(&SqlStatement::RegisterTable { .. }, &ctx_user).await;
        assert!(result.is_err());
        
        // Dba role should be authorized
        let ctx_dba = ExecutionContext::new(UserId::from("test"), Role::Dba);
        let result = handler.check_authorization(&SqlStatement::RegisterTable { .. }, &ctx_dba).await;
        assert!(result.is_ok());
    }
}
