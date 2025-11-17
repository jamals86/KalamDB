//! Typed handler for ALTER USER statement

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use kalamdb_sql::ddl::{AlterUserStatement, UserModification};
use std::sync::Arc;
// No direct Role/UserId usage here (Role changing handled via statement), remove unused imports

/// Handler for ALTER USER
pub struct AlterUserHandler {
    app_context: Arc<AppContext>,
    enforce_complexity: bool,
}

impl AlterUserHandler {
    pub fn new(app_context: Arc<AppContext>, enforce_complexity: bool) -> Self {
        Self { app_context, enforce_complexity }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<AlterUserStatement> for AlterUserHandler {
    async fn execute(
        &self,
        statement: AlterUserStatement,
        _params: Vec<ScalarValue>,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        let users = self.app_context.system_tables().users();
        let existing = users.get_user_by_username(&statement.username)?
            .ok_or_else(|| KalamDbError::NotFound(format!("User '{}' not found", statement.username)))?;

        let mut updated = existing.clone();

        match statement.modification {
            UserModification::SetPassword(new_pw) => {
                // Self-service allowed: user modifying own password
                let is_self = context.user_id.as_str() == updated.id.as_str();
                if !is_self && !context.is_admin() {
                    return Err(KalamDbError::Unauthorized("Only admins can change other users' passwords".to_string()));
                }
                // Enforce password complexity if enabled in config
                if self.enforce_complexity || self.app_context.config().auth.enforce_password_complexity {
                    validate_password_complexity(&new_pw)?;
                }
                updated.password_hash = bcrypt::hash(new_pw, bcrypt::DEFAULT_COST)
                    .map_err(|e| KalamDbError::Other(format!("Password hash error: {}", e)))?;
            }
            UserModification::SetRole(new_role) => {
                if !context.is_admin() {
                    return Err(KalamDbError::Unauthorized("Only admins can change roles".to_string()));
                }
                updated.role = new_role;
            }
            UserModification::SetEmail(new_email) => {
                let is_self = context.user_id.as_str() == updated.id.as_str();
                if !is_self && !context.is_admin() {
                    return Err(KalamDbError::Unauthorized("Only admins can update other users' emails".to_string()));
                }
                updated.email = Some(new_email);
            }
        }

        updated.updated_at = chrono::Utc::now().timestamp_millis();
        users.update_user(updated)?;

        Ok(ExecutionResult::Success { message: format!("User '{}' updated", statement.username) })
    }

    async fn check_authorization(
        &self,
        _statement: &AlterUserStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        if !context.is_admin() {
            return Err(KalamDbError::Unauthorized(
                "ALTER USER requires DBA or System role".to_string(),
            ));
        }
        Ok(())
    }
}

/// Validate password complexity according to policy
/// Requires at least one uppercase, one lowercase, one digit, and one special character
fn validate_password_complexity(pw: &str) -> Result<(), KalamDbError> {
    let has_upper = pw.chars().any(|c| c.is_ascii_uppercase());
    if !has_upper { return Err(KalamDbError::InvalidOperation("Password must include at least one uppercase letter".to_string())); }
    let has_lower = pw.chars().any(|c| c.is_ascii_lowercase());
    if !has_lower { return Err(KalamDbError::InvalidOperation("Password must include at least one lowercase letter".to_string())); }
    let has_digit = pw.chars().any(|c| c.is_ascii_digit());
    if !has_digit { return Err(KalamDbError::InvalidOperation("Password must include at least one digit".to_string())); }
    let has_special = pw.chars().any(|c| !c.is_ascii_alphanumeric());
    if !has_special { return Err(KalamDbError::InvalidOperation("Password must include at least one special character".to_string())); }
    Ok(())
}
