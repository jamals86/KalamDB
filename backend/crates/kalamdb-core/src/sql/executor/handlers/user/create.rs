//! Typed handler for CREATE USER statement

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use kalamdb_sql::ddl::CreateUserStatement;
use std::sync::Arc;
use kalamdb_commons::{UserId, AuthType};
use kalamdb_commons::types::User;

/// Handler for CREATE USER
pub struct CreateUserHandler {
    app_context: Arc<AppContext>,
}

impl CreateUserHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<CreateUserStatement> for CreateUserHandler {
    async fn execute(
        &self,
        statement: CreateUserStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        let users = self.app_context.system_tables().users();

        // Duplicate check (provider enforces via username index but we do early check for clearer error)
        if users.get_user_by_username(&statement.username)?.is_some() {
            return Err(KalamDbError::AlreadyExists(format!(
                "User '{}' already exists",
                statement.username
            )));
        }

        // Hash password if auth_type = Password
        let password_hash = match statement.auth_type {
            AuthType::Password => {
                let raw = statement.password.clone().ok_or_else(|| KalamDbError::InvalidOperation("Password required for WITH PASSWORD".to_string()))?;
                // Enforce password complexity if enabled in config
                if self.app_context.config().auth.enforce_password_complexity {
                    validate_password_complexity(&raw)?;
                }
                bcrypt::hash(raw, bcrypt::DEFAULT_COST).map_err(|e| KalamDbError::Other(format!("Password hash error: {}", e)))?
            }
            _ => "".to_string(),
        };

        let now = chrono::Utc::now().timestamp_millis();
        let user = User {
            id: UserId::new(format!("u_{}", uuid::Uuid::new_v4().simple())),
            username: statement.username.clone().into(),
            password_hash,
            role: statement.role,
            email: statement.email.clone(),
            auth_type: statement.auth_type,
            auth_data: None,
            storage_mode: kalamdb_commons::StorageMode::Table,
            storage_id: None,
            created_at: now,
            updated_at: now,
            last_seen: None,
            deleted_at: None,
        };

        users.create_user(user)?;

        Ok(ExecutionResult::Success { message: format!("User '{}' created", statement.username) })
    }

    async fn check_authorization(
        &self,
        _statement: &CreateUserStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        if !context.is_admin() {
            return Err(KalamDbError::Unauthorized(
                "CREATE USER requires DBA or System role".to_string(),
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
