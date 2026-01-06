//! Implementation of UsersApplier for provider persistence
//!
//! This module provides the concrete implementation of `kalamdb_raft::UsersApplier`
//! that persists user metadata to the system.users provider.

use std::sync::Arc;

use async_trait::async_trait;
use kalamdb_commons::models::UserId;
use kalamdb_commons::types::User;
use kalamdb_raft::{RaftError, UsersApplier};
use kalamdb_system::{SystemError, SystemTablesRegistry};

pub struct ProviderUsersApplier {
    system_tables: Arc<SystemTablesRegistry>,
}

impl ProviderUsersApplier {
    pub fn new(system_tables: Arc<SystemTablesRegistry>) -> Self {
        Self { system_tables }
    }
}

#[async_trait]
impl UsersApplier for ProviderUsersApplier {
    async fn create_user(&self, user: &User) -> Result<(), RaftError> {
        self.system_tables
            .users()
            .create_user(user.clone())
            .map_err(|e| RaftError::provider(format!("Failed to create user: {}", e)))
    }

    async fn update_user(&self, user: &User) -> Result<(), RaftError> {
        let users = self.system_tables.users();
        match users.update_user(user.clone()) {
            Ok(()) => Ok(()),
            Err(SystemError::NotFound(_)) => users
                .create_user(user.clone())
                .map_err(|e| RaftError::provider(format!("Failed to upsert user: {}", e))),
            Err(e) => Err(RaftError::provider(format!("Failed to update user: {}", e))),
        }
    }

    async fn delete_user(&self, user_id: &UserId, deleted_at: i64) -> Result<(), RaftError> {
        let users = self.system_tables.users();
        if let Some(mut user) = users
            .get_user_by_id(user_id)
            .map_err(|e| RaftError::provider(format!("Failed to lookup user: {}", e)))?
        {
            user.deleted_at = Some(deleted_at);
            user.updated_at = deleted_at;
            users
                .update_user(user)
                .map_err(|e| RaftError::provider(format!("Failed to delete user: {}", e)))?;
        }
        Ok(())
    }

    async fn record_login(&self, user_id: &UserId, logged_in_at: i64) -> Result<(), RaftError> {
        let users = self.system_tables.users();
        if let Some(mut user) = users
            .get_user_by_id(user_id)
            .map_err(|e| RaftError::provider(format!("Failed to lookup user: {}", e)))?
        {
            user.last_login_at = Some(logged_in_at);
            user.updated_at = logged_in_at;
            users
                .update_user(user)
                .map_err(|e| RaftError::provider(format!("Failed to record login: {}", e)))?;
        }
        Ok(())
    }

    async fn set_locked(
        &self,
        user_id: &UserId,
        locked_until: Option<i64>,
        updated_at: i64,
    ) -> Result<(), RaftError> {
        let users = self.system_tables.users();
        if let Some(mut user) = users
            .get_user_by_id(user_id)
            .map_err(|e| RaftError::provider(format!("Failed to lookup user: {}", e)))?
        {
            user.locked_until = locked_until;
            user.updated_at = updated_at;
            users
                .update_user(user)
                .map_err(|e| RaftError::provider(format!("Failed to set lock: {}", e)))?;
        }
        Ok(())
    }
}
