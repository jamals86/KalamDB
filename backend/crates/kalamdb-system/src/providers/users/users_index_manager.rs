//! Index manager for users table
//!
//! This module coordinates all secondary indexes for the users table:
//! - Username index (unique): username -> user_id
//! - Role index (non-unique): role -> [user_ids]
//! - Deleted_at index (non-unique): deletion_date -> [user_ids]

use super::users_deleted_at_index::DeletedAtIndex;
use super::users_role_index::RoleIndex;
use super::users_username_index::{UsernameIndex, UsernameIndexExt};
use crate::error::SystemError;
use chrono::{DateTime, Utc};
use kalamdb_commons::system::User;
use kalamdb_commons::{Role, UserId};
use kalamdb_store::StorageBackend;
use std::sync::Arc;

/// Manages all secondary indexes for the users table
pub struct UserIndexManager {
    username_idx: UsernameIndex,
    role_idx: RoleIndex,
    deleted_at_idx: DeletedAtIndex,
}

impl UserIndexManager {
    /// Create a new UserIndexManager
    ///
    /// # Arguments
    /// * `backend` - Storage backend (RocksDB or mock)
    ///
    /// # Returns
    /// A new UserIndexManager with all indexes initialized
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self {
            username_idx: super::users_username_index::new_username_index(backend.clone()),
            role_idx: RoleIndex::new(backend.clone()),
            deleted_at_idx: DeletedAtIndex::new(backend),
        }
    }

    /// Index a new user (add to all applicable indexes)
    ///
    /// # Arguments
    /// * `user` - The user to index
    ///
    /// # Returns
    /// Result indicating success or failure
    pub fn index_user(&self, user: &User) -> Result<(), SystemError> {
        // Username index (always indexed)
        self.username_idx.index_user(user)?;

        // Role index (always indexed)
        self.role_idx.index_user(user)?;

        // Deleted_at index (only if deleted)
        if user.deleted_at.is_some() {
            self.deleted_at_idx.index_user(user)?;
        }

        Ok(())
    }

    /// Update user indexes (handle username, role, or deletion status changes)
    ///
    /// # Arguments
    /// * `old_user` - The old user state (None if new user)
    /// * `new_user` - The new user state
    ///
    /// # Returns
    /// Result indicating success or failure
    pub fn update_user(
        &self,
        old_user: Option<&User>,
        new_user: &User,
    ) -> Result<(), SystemError> {
        // Handle username changes
        if let Some(old) = old_user {
            if old.username != new_user.username {
                self.username_idx.remove_user(old.username.as_str())?;
                self.username_idx.index_user(new_user)?;
            }
        } else {
            // New user
            self.username_idx.index_user(new_user)?;
        }

        // Handle role changes
        let old_role = old_user.map(|u| &u.role);
        self.role_idx
            .update_user_role(&new_user.id, old_role, &new_user.role)?;

        // Handle deletion status changes
        let old_deleted_at = old_user
            .and_then(|u| u.deleted_at)
            .and_then(DateTime::<Utc>::from_timestamp_millis);
        let new_deleted_at = new_user
            .deleted_at
            .and_then(DateTime::<Utc>::from_timestamp_millis);

        if old_deleted_at.as_ref() != new_deleted_at.as_ref() {
            self.deleted_at_idx.update_user_deletion(
                &new_user.id,
                old_deleted_at.as_ref(),
                new_deleted_at.as_ref(),
            )?;
        }

        Ok(())
    }

    /// Remove user from all indexes
    ///
    /// # Arguments
    /// * `user` - The user to remove
    ///
    /// # Returns
    /// Result indicating success or failure
    pub fn remove_user(&self, user: &User) -> Result<(), SystemError> {
        // Remove from username index
        self.username_idx.remove_user(user.username.as_str())?;

        // Remove from role index
        self.role_idx.remove_user(&user.id, &user.role)?;

        // Remove from deleted_at index (if present)
        if let Some(deleted_at_ms) = user.deleted_at {
            if let Some(deleted_dt) = DateTime::<Utc>::from_timestamp_millis(deleted_at_ms) {
                self.deleted_at_idx.remove_user(&user.id, &deleted_dt)?;
            }
        }

        Ok(())
    }

    /// Lookup user ID by username
    ///
    /// # Arguments
    /// * `username` - The username to lookup
    ///
    /// # Returns
    /// Option<UserId> if found, None otherwise
    pub fn get_by_username(&self, username: &str) -> Result<Option<UserId>, SystemError> {
        self.username_idx.lookup(username)
    }

    /// Lookup all user IDs by role
    ///
    /// # Arguments
    /// * `role` - The role to lookup
    ///
    /// # Returns
    /// Vec<UserId> of users with that role
    pub fn get_by_role(&self, role: &Role) -> Result<Vec<UserId>, SystemError> {
        self.role_idx.lookup(role)
    }

    /// Lookup all deleted user IDs by deletion date
    ///
    /// # Arguments
    /// * `date` - The deletion date
    ///
    /// # Returns
    /// Vec<UserId> of users deleted on that date
    pub fn get_by_deletion_date(
        &self,
        date: &chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<UserId>, SystemError> {
        self.deleted_at_idx.lookup_on_date(date)
    }

    /// Lookup all users deleted before or on a specific date
    ///
    /// # Arguments
    /// * `before_date` - The cutoff date
    ///
    /// # Returns
    /// Vec<UserId> of users deleted before or on that date
    pub fn get_deleted_before(
        &self,
        before_date: &chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<UserId>, SystemError> {
        self.deleted_at_idx.lookup_before(before_date)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use kalamdb_commons::{AuthType, StorageId, StorageMode, UserName};
    use kalamdb_store::test_utils::InMemoryBackend;

    fn create_test_manager() -> UserIndexManager {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        UserIndexManager::new(backend)
    }

    fn create_test_user(
        id: &str,
        username: &str,
        role: Role,
        deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> User {
        let now_ms = Utc::now().timestamp_millis();
        let deleted_at_ms = deleted_at.map(|dt| dt.timestamp_millis());

        User {
            id: UserId::new(id),
            username: UserName::new(username),
            password_hash: "hashed_password".to_string(),
            role,
            email: Some(format!("{}@example.com", username)),
            auth_type: AuthType::Password,
            auth_data: None,
            created_at: now_ms,
            updated_at: now_ms,
            last_seen: None,
            deleted_at: deleted_at_ms,
            storage_mode: StorageMode::Table,
            storage_id: Some(StorageId::local()),
        }
    }

    #[test]
    fn test_index_new_user() {
        let manager = create_test_manager();
        let user = create_test_user("u1", "alice", Role::User, None);

        manager.index_user(&user).unwrap();

        // Verify username index
        let result = manager.get_by_username("alice").unwrap();
        assert_eq!(result, Some(UserId::new("u1")));

        // Verify role index
        let users = manager.get_by_role(&Role::User).unwrap();
        assert_eq!(users.len(), 1);
    }

    #[test]
    fn test_index_deleted_user() {
        let manager = create_test_manager();
        let deleted_at = Utc.with_ymd_and_hms(2025, 10, 29, 12, 0, 0).unwrap();
        let user = create_test_user("u1", "alice", Role::User, Some(deleted_at));

        manager.index_user(&user).unwrap();

        // Verify all three indexes
        assert_eq!(
            manager.get_by_username("alice").unwrap(),
            Some(UserId::new("u1"))
        );
        assert_eq!(manager.get_by_role(&Role::User).unwrap().len(), 1);
        assert_eq!(manager.get_by_deletion_date(&deleted_at).unwrap().len(), 1);
    }

    #[test]
    fn test_update_username() {
        let manager = create_test_manager();
        let old_user = create_test_user("u1", "alice", Role::User, None);
        let new_user = create_test_user("u1", "alice_updated", Role::User, None);

        manager.index_user(&old_user).unwrap();
        manager.update_user(Some(&old_user), &new_user).unwrap();

        // Old username should not exist
        assert_eq!(manager.get_by_username("alice").unwrap(), None);

        // New username should exist
        assert_eq!(
            manager.get_by_username("alice_updated").unwrap(),
            Some(UserId::new("u1"))
        );
    }

    #[test]
    fn test_update_role() {
        let manager = create_test_manager();
        let old_user = create_test_user("u1", "alice", Role::User, None);
        let new_user = create_test_user("u1", "alice", Role::Dba, None);

        manager.index_user(&old_user).unwrap();
        manager.update_user(Some(&old_user), &new_user).unwrap();

        // Should not be in User role
        assert_eq!(manager.get_by_role(&Role::User).unwrap().len(), 0);

        // Should be in Dba role
        let dbas = manager.get_by_role(&Role::Dba).unwrap();
        assert_eq!(dbas.len(), 1);
        assert_eq!(dbas[0].as_str(), "u1");
    }

    #[test]
    fn test_soft_delete() {
        let manager = create_test_manager();
        let active_user = create_test_user("u1", "alice", Role::User, None);
        let deleted_at = Utc.with_ymd_and_hms(2025, 10, 29, 12, 0, 0).unwrap();
        let deleted_user = create_test_user("u1", "alice", Role::User, Some(deleted_at));

        manager.index_user(&active_user).unwrap();
        manager
            .update_user(Some(&active_user), &deleted_user)
            .unwrap();

        // Should appear in deleted_at index
        let deleted = manager.get_by_deletion_date(&deleted_at).unwrap();
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0].as_str(), "u1");
    }

    #[test]
    fn test_restore_deleted_user() {
        let manager = create_test_manager();
        let deleted_at = Utc.with_ymd_and_hms(2025, 10, 29, 12, 0, 0).unwrap();
        let deleted_user = create_test_user("u1", "alice", Role::User, Some(deleted_at));
        let restored_user = create_test_user("u1", "alice", Role::User, None);

        manager.index_user(&deleted_user).unwrap();
        assert_eq!(manager.get_by_deletion_date(&deleted_at).unwrap().len(), 1);

        manager
            .update_user(Some(&deleted_user), &restored_user)
            .unwrap();

        // Should no longer appear in deleted_at index
        assert_eq!(manager.get_by_deletion_date(&deleted_at).unwrap().len(), 0);
    }

    #[test]
    fn test_remove_user() {
        let manager = create_test_manager();
        let user = create_test_user("u1", "alice", Role::User, None);

        manager.index_user(&user).unwrap();
        assert_eq!(
            manager.get_by_username("alice").unwrap(),
            Some(UserId::new("u1"))
        );

        manager.remove_user(&user).unwrap();

        // Should not exist in any index
        assert_eq!(manager.get_by_username("alice").unwrap(), None);
        assert_eq!(manager.get_by_role(&Role::User).unwrap().len(), 0);
    }

    #[test]
    fn test_multiple_users_different_roles() {
        let manager = create_test_manager();
        let user1 = create_test_user("u1", "alice", Role::User, None);
        let user2 = create_test_user("u2", "bob", Role::Dba, None);
        let user3 = create_test_user("u3", "charlie", Role::Service, None);

        manager.index_user(&user1).unwrap();
        manager.index_user(&user2).unwrap();
        manager.index_user(&user3).unwrap();

        assert_eq!(manager.get_by_role(&Role::User).unwrap().len(), 1);
        assert_eq!(manager.get_by_role(&Role::Dba).unwrap().len(), 1);
        assert_eq!(manager.get_by_role(&Role::Service).unwrap().len(), 1);
    }
}
