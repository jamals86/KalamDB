//! User Executor - CREATE/ALTER/DROP USER operations
//!
//! This is the SINGLE place where user mutations happen.

use std::sync::Arc;

use kalamdb_commons::models::UserId;
use kalamdb_commons::types::User;

use crate::app_context::AppContext;
use crate::applier::ApplierError;

/// Executor for user management operations
pub struct UserExecutor {
    app_context: Arc<AppContext>,
}

impl UserExecutor {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
    
    /// Execute CREATE USER
    pub async fn create_user(&self, user: &User) -> Result<String, ApplierError> {
        log::debug!("CommandExecutorImpl: Creating user {}", user.id);
        
        self.app_context
            .system_tables()
            .users()
            .create_user(user.clone())
            .map_err(|e| ApplierError::Execution(format!("Failed to create user: {}", e)))?;
        
        Ok(format!("User {} created successfully", user.id))
    }
    
    /// Execute ALTER USER (update)
    pub async fn update_user(&self, user: &User) -> Result<String, ApplierError> {
        log::debug!("CommandExecutorImpl: Updating user {}", user.id);
        
        self.app_context
            .system_tables()
            .users()
            .update_user(user.clone())
            .map_err(|e| ApplierError::Execution(format!("Failed to update user: {}", e)))?;
        
        Ok(format!("User {} updated successfully", user.id))
    }
    
    /// Execute DROP USER (soft delete)
    /// 
    /// Note: The deleted_at timestamp is set internally by delete_user
    pub async fn delete_user(&self, user_id: &UserId) -> Result<String, ApplierError> {
        log::debug!("CommandExecutorImpl: Deleting user {}", user_id);
        
        self.app_context
            .system_tables()
            .users()
            .delete_user(user_id)
            .map_err(|e| ApplierError::Execution(format!("Failed to delete user: {}", e)))?;
        
        Ok(format!("User {} deleted successfully", user_id))
    }
    
    /// Record user login timestamp
    pub async fn record_login(&self, user_id: &UserId, logged_in_at: i64) -> Result<String, ApplierError> {
        log::debug!("CommandExecutorImpl: Recording login for user {}", user_id);
        
        if let Some(mut user) = self.app_context
            .system_tables()
            .users()
            .get_user_by_id(user_id)
            .map_err(|e| ApplierError::Execution(format!("Failed to get user: {}", e)))?
        {
            user.last_login_at = Some(logged_in_at);
            self.app_context
                .system_tables()
                .users()
                .update_user(user)
                .map_err(|e| ApplierError::Execution(format!("Failed to update user login: {}", e)))?;
            
            return Ok(format!("Login recorded for user {}", user_id));
        }
        
        Ok(format!("User {} not found for login recording", user_id))
    }
    
    /// Set user lock status
    pub async fn set_user_locked(
        &self,
        user_id: &UserId,
        locked_until: Option<i64>,
    ) -> Result<String, ApplierError> {
        log::debug!("CommandExecutorImpl: Setting user {} locked until {:?}", user_id, locked_until);
        
        if let Some(mut user) = self.app_context
            .system_tables()
            .users()
            .get_user_by_id(user_id)
            .map_err(|e| ApplierError::Execution(format!("Failed to get user: {}", e)))?
        {
            user.locked_until = locked_until;
            self.app_context
                .system_tables()
                .users()
                .update_user(user)
                .map_err(|e| ApplierError::Execution(format!("Failed to update user lock: {}", e)))?;
            
            if let Some(until) = locked_until {
                return Ok(format!("User {} locked until {}", user_id, until));
            } else {
                return Ok(format!("User {} unlocked", user_id));
            }
        }
        
        Ok(format!("User {} not found for lock update", user_id))
    }
}
