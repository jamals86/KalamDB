//! User Management Handler
//!
//! Handles CREATE USER, ALTER USER, DROP USER operations.
//! **Phase 7 Task T070**: Implement user management handler.

use crate::error::KalamDbError;
use crate::sql::executor::handlers::{
    ExecutionContext, ExecutionResult, ParamValue, StatementHandler,
};
use async_trait::async_trait;
use datafusion::execution::context::SessionContext;
use kalamdb_sql::statement_classifier::SqlStatement;

/// Handler for user management operations
pub struct UserManagementHandler {
    app_context: std::sync::Arc<crate::app_context::AppContext>,
}

impl UserManagementHandler {
    /// Create a new user management handler
    pub fn new(app_context: std::sync::Arc<crate::app_context::AppContext>) -> Self {
        Self { app_context }
    }

    /// Execute CREATE USER statement
    pub async fn execute_create_user(
        &self,
        session: &SessionContext,
        statement: SqlStatement,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // TODO: Implement CREATE USER logic
        // 1. Extract username, password, role from statement
        // 2. Validate password complexity (if enforce_password_complexity enabled)
        // 3. Hash password using bcrypt
        // 4. Create user via UsersTableProvider
        // 5. Log audit entry
        // 6. Return ExecutionResult
        //
        // **Authorization**: Only Dba and System roles can create users
        // **Validation**: Username uniqueness, password complexity
        
        Err(KalamDbError::InvalidOperation(
            "CREATE USER not yet implemented in UserManagementHandler".to_string(),
        ))
    }

    /// Execute ALTER USER statement
    pub async fn execute_alter_user(
        &self,
        session: &SessionContext,
        statement: SqlStatement,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // TODO: Implement ALTER USER logic
        // 1. Extract username and changes from statement
        // 2. Validate changes (password complexity, role changes)
        // 3. Update user via UsersTableProvider
        // 4. Log audit entry
        // 5. Return ExecutionResult
        //
        // **Authorization**: Dba/System for role changes, users can change own password
        
        Err(KalamDbError::InvalidOperation(
            "ALTER USER not yet implemented in UserManagementHandler".to_string(),
        ))
    }

    /// Execute DROP USER statement
    pub async fn execute_drop_user(
        &self,
        session: &SessionContext,
        statement: SqlStatement,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // TODO: Implement DROP USER logic
        // 1. Extract username from statement
        // 2. Soft delete user via UsersTableProvider (set deleted_at)
        // 3. Log audit entry
        // 4. Return ExecutionResult
        //
        // **Authorization**: Only Dba and System roles can drop users
        // **Validation**: Cannot drop system user
        
        Err(KalamDbError::InvalidOperation(
            "DROP USER not yet implemented in UserManagementHandler".to_string(),
        ))
    }
}

#[async_trait]
impl StatementHandler for UserManagementHandler {
    async fn execute(
        &self,
        session: &SessionContext,
        statement: SqlStatement,
        params: Vec<ParamValue>,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        match statement {
            SqlStatement::CreateUser { .. } => {
                self.execute_create_user(session, statement, context)
                    .await
            }
            SqlStatement::AlterUser { .. } => {
                self.execute_alter_user(session, statement, context).await
            }
            SqlStatement::DropUser { .. } => {
                self.execute_drop_user(session, statement, context).await
            }
            _ => Err(KalamDbError::InvalidOperation(
                "Not a user management statement".to_string(),
            )),
        }
    }

    async fn check_authorization(
        &self,
        statement: &SqlStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        use kalamdb_commons::Role;
        
        // CREATE USER and DROP USER require Dba or System role
        match statement {
            SqlStatement::CreateUser { .. } | SqlStatement::DropUser { .. } => {
                if context.user_role() < &Role::Dba {
                    return Err(KalamDbError::Unauthorized(
                        "User management requires Dba or System role".to_string(),
                    ));
                }
            }
            SqlStatement::AlterUser { .. } => {
                // ALTER USER authorization is more complex:
                // - Users can change their own password
                // - Dba/System can change any user's role/password
                // This is handled in execute_alter_user
            }
            _ => {}
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::{Role, UserId};

    #[tokio::test]
    async fn test_user_management_handler_placeholder() {
        // Placeholder test to verify module compiles
        let app_ctx = crate::test_helpers::init_test_app_context();
        let handler = UserManagementHandler::new(app_ctx);
        let ctx = ExecutionContext::new(UserId::from("test"), Role::Dba);

        // Should return "not yet implemented" error
        let result = handler
            .execute_create_user(&SessionContext::new(), SqlStatement::Unknown, &ctx)
            .await;
        
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_user_management_authorization() {
        let app_ctx = crate::test_helpers::init_test_app_context();
        let handler = UserManagementHandler::new(app_ctx);
        
        // User role should be unauthorized for CREATE USER
        let ctx_user = ExecutionContext::new(UserId::from("test"), Role::User);
        let result = handler.check_authorization(&SqlStatement::CreateUser { .. }, &ctx_user).await;
        assert!(result.is_err());
        
        // Dba role should be authorized
        let ctx_dba = ExecutionContext::new(UserId::from("test"), Role::Dba);
        let result = handler.check_authorization(&SqlStatement::CreateUser { .. }, &ctx_dba).await;
        assert!(result.is_ok());
    }
}
