//! Username secondary index for users table
//!
//! This module provides a secondary index to lookup users by username.
//! Maps UserName -> UserId for efficient username-based queries.

use crate::error::KalamDbError;
use crate::stores::SystemTableStore;
use kalamdb_commons::models::UserName;
use kalamdb_commons::system::User;
use kalamdb_commons::UserId;
use kalamdb_store::EntityStoreV2;
use kalamdb_store::StorageBackend;
use std::sync::Arc;

/// Secondary index for username -> user_id lookups
pub type UsernameIndex = SystemTableStore<UserName, UserId>;

/// Helper function to create a new username secondary index
///
/// # Arguments
/// * `backend` - Storage backend (RocksDB or mock)
///
/// # Returns
/// A new SystemTableStore instance configured for the username index
pub fn new_username_index(backend: Arc<dyn StorageBackend>) -> UsernameIndex {
    SystemTableStore::new(backend, "system_users_username_idx")
}

/// Extension methods for UsernameIndex
pub trait UsernameIndexExt {
    /// Index a user (add username -> user_id mapping)
    fn index_user(&self, user: &User) -> Result<(), KalamDbError>;

    /// Remove a user from the index
    fn remove_user(&self, username: &str) -> Result<(), KalamDbError>;

    /// Lookup a user by username
    fn lookup(&self, username: &str) -> Result<Option<UserId>, KalamDbError>;
}

impl UsernameIndexExt for UsernameIndex {
    /// Index a user (add username -> user_id mapping)
    ///
    /// # Arguments
    /// * `user` - The user to index
    ///
    /// # Returns
    /// Result indicating success or failure
    fn index_user(&self, user: &User) -> Result<(), KalamDbError> {
        let username = user.username.to_lowercase();
        self.put(&username, &user.id)?;
        Ok(())
    }

    /// Remove a user from the index
    ///
    /// # Arguments
    /// * `username` - The username to remove
    ///
    /// # Returns
    /// Result indicating success or failure
    fn remove_user(&self, username: &str) -> Result<(), KalamDbError> {
        let username = UserName::new(username).to_lowercase();
        self.delete(&username)?;
        Ok(())
    }

    /// Lookup a user by username
    ///
    /// # Arguments
    /// * `username` - The username to lookup
    ///
    /// # Returns
    /// Option<UserId> if found, None otherwise
    fn lookup(&self, username: &str) -> Result<Option<UserId>, KalamDbError> {
        let username = UserName::new(username).to_lowercase();
        Ok(self.get(&username)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::{AuthType, Role, StorageId, StorageMode};
    use kalamdb_store::InMemoryBackend;

    fn create_test_index() -> UsernameIndex {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        new_username_index(backend)
    }

    fn create_test_user(id: &str, username: &str) -> User {
        User {
            id: UserId::new(id),
            username: UserName::new(username),
            password_hash: "hashed_password".to_string(),
            role: Role::User,
            email: Some(format!("{}@example.com", username)),
            auth_type: AuthType::Password,
            auth_data: None,
            storage_mode: StorageMode::Table,
            storage_id: Some(StorageId::new("local")),
            created_at: 1000,
            updated_at: 1000,
            last_seen: None,
            deleted_at: None,
        }
    }

    #[test]
    fn test_create_index() {
        let index = create_test_index();
        assert_eq!(index.partition(), "system_users_username_idx");
    }

    #[test]
    fn test_index_and_lookup_user() {
        let index = create_test_index();
        let user = create_test_user("user1", "alice");

        // Index the user
        index.index_user(&user).unwrap();

        // Lookup by username
        let user_id = index.lookup("alice").unwrap();
        assert!(user_id.is_some());
        assert_eq!(user_id.unwrap(), UserId::new("user1"));
    }

    #[test]
    fn test_case_insensitive_lookup() {
        let index = create_test_index();
        let user = create_test_user("user1", "Alice");

        // Index the user (username converted to lowercase internally)
        index.index_user(&user).unwrap();

        // Lookup with different case
        let user_id1 = index.lookup("alice").unwrap();
        let user_id2 = index.lookup("ALICE").unwrap();
        let user_id3 = index.lookup("Alice").unwrap();

        assert!(user_id1.is_some());
        assert_eq!(user_id1, user_id2);
        assert_eq!(user_id2, user_id3);
    }

    #[test]
    fn test_remove_user_from_index() {
        let index = create_test_index();
        let user = create_test_user("user1", "alice");

        // Index then remove
        index.index_user(&user).unwrap();
        index.remove_user("alice").unwrap();

        // Verify removed
        let user_id = index.lookup("alice").unwrap();
        assert!(user_id.is_none());
    }

    #[test]
    fn test_lookup_nonexistent_user() {
        let index = create_test_index();

        // Lookup user that doesn't exist
        let user_id = index.lookup("nonexistent").unwrap();
        assert!(user_id.is_none());
    }

    #[test]
    fn test_multiple_users() {
        let index = create_test_index();

        // Index multiple users
        for i in 1..=3 {
            let user = create_test_user(&format!("user{}", i), &format!("user{}", i));
            index.index_user(&user).unwrap();
        }

        // Lookup each user
        for i in 1..=3 {
            let user_id = index.lookup(&format!("user{}", i)).unwrap();
            assert!(user_id.is_some());
            assert_eq!(user_id.unwrap(), UserId::new(&format!("user{}", i)));
        }
    }
}
