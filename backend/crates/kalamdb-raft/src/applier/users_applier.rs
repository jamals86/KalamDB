//! UsersApplier trait for persisting user metadata
//!
//! This trait is called by UsersStateMachine after Raft consensus commits
//! to persist user metadata into system tables on each node.

use async_trait::async_trait;
use kalamdb_commons::models::UserId;
use kalamdb_commons::types::User;

use crate::RaftError;

/// Applier callback for user metadata operations
///
/// This trait is called by UsersStateMachine after Raft consensus commits
/// a command. All nodes (leader and followers) call this, ensuring that
/// all replicas persist the same state to their local storage.
///
/// # Implementation
///
/// The implementation lives in kalamdb-core and uses the users provider
/// to persist changes to the user accounts and authentication data.
#[async_trait]
pub trait UsersApplier: Send + Sync {
    /// Create a new user in persistent storage
    async fn create_user(&self, user: &User) -> Result<(), RaftError>;
    
    /// Update an existing user in persistent storage
    async fn update_user(&self, user: &User) -> Result<(), RaftError>;
    
    /// Soft-delete a user from persistent storage
    async fn delete_user(&self, user_id: &UserId, deleted_at: i64) -> Result<(), RaftError>;
    
    /// Record a successful login
    async fn record_login(&self, user_id: &UserId, logged_in_at: i64) -> Result<(), RaftError>;
    
    /// Lock or unlock a user account
    async fn set_locked(
        &self,
        user_id: &UserId,
        locked_until: Option<i64>,
        updated_at: i64,
    ) -> Result<(), RaftError>;
}

/// No-op applier for testing or standalone scenarios
///
/// Does nothing - used when persistence is handled elsewhere.
pub struct NoOpUsersApplier;

#[async_trait]
impl UsersApplier for NoOpUsersApplier {
    async fn create_user(&self, _user: &User) -> Result<(), RaftError> {
        Ok(())
    }

    async fn update_user(&self, _user: &User) -> Result<(), RaftError> {
        Ok(())
    }

    async fn delete_user(&self, _user_id: &UserId, _deleted_at: i64) -> Result<(), RaftError> {
        Ok(())
    }

    async fn record_login(&self, _user_id: &UserId, _logged_in_at: i64) -> Result<(), RaftError> {
        Ok(())
    }

    async fn set_locked(
        &self,
        _user_id: &UserId,
        _locked_until: Option<i64>,
        _updated_at: i64,
    ) -> Result<(), RaftError> {
        Ok(())
    }
}
