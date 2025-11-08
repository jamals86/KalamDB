//! Typed handler for DROP USER statement

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_sql::ddl::DropUserStatement;
use std::sync::Arc;
// No direct UserId usage, removing unused import

/// Handler for DROP USER
pub struct DropUserHandler {
    app_context: Arc<AppContext>,
}

impl DropUserHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<DropUserStatement> for DropUserHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        statement: DropUserStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        let users = self.app_context.system_tables().users();
        let existing = users.get_user_by_username(&statement.username)?;
        if existing.is_none() {
            if statement.if_exists {
                return Ok(ExecutionResult::Success { message: format!("User '{}' does not exist (skipped)", statement.username) });
            }
            return Err(KalamDbError::NotFound(format!("User '{}' not found", statement.username)));
        }
        let user = existing.unwrap();
        users.delete_user(&user.id)?;
        Ok(ExecutionResult::Success { message: format!("User '{}' dropped (soft delete)", statement.username) })
    }

    async fn check_authorization(
        &self,
        _statement: &DropUserStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        if !context.is_admin() {
            return Err(KalamDbError::Unauthorized(
                "DROP USER requires DBA or System role".to_string(),
            ));
        }
        Ok(())
    }
}
