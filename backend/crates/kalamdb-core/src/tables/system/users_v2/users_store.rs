//! Users table store implementation
//!
//! This module provides a SystemTableStore<UserId, User> wrapper for the system.users table.

use crate::stores::SystemTableStore;
use kalamdb_commons::system::User;
use kalamdb_commons::UserId;
use kalamdb_store::StorageBackend;
use std::sync::Arc;

/// Type alias for the users table store
pub type UsersStore = SystemTableStore<UserId, User>;

/// Helper function to create a new users table store
///
/// # Arguments
/// * `backend` - Storage backend (RocksDB or mock)
///
/// # Returns
/// A new SystemTableStore instance configured for the users table
pub fn new_users_store(backend: Arc<dyn StorageBackend>) -> UsersStore {
    SystemTableStore::new(backend, "system_users") //TODO: user the enum partition name
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::{AuthType, Role, StorageId, StorageMode};
    use kalamdb_store::InMemoryBackend;

    fn create_test_store() -> UsersStore {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        new_users_store(backend)
    }

    fn create_test_user(id_str: &str, username: &str) -> User {
        User {
            id: UserId::new(id_str),
            username: username.to_string(),
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
    fn test_create_store() {
        let store = create_test_store();
        assert_eq!(store.partition(), "system_users");
    }

    #[test]
    fn test_put_and_get_user() {
        let store = create_test_store();
        let user_id = UserId::new("user1");
        let user = create_test_user("user1", "alice");

        // Put user
        store.put(&user_id, &user).unwrap();

        // Get user
        let retrieved = store.get(&user_id).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, user_id);
        assert_eq!(retrieved.username, "alice");
        assert_eq!(retrieved.email, Some("alice@example.com".to_string()));
    }

    #[test]
    fn test_delete_user() {
        let store = create_test_store();
        let user_id = UserId::new("user1");
        let user = create_test_user("user1", "alice");

        // Put then delete
        store.put(&user_id, &user).unwrap();
        store.delete(&user_id).unwrap();

        // Verify deleted
        let retrieved = store.get(&user_id).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_scan_all_users() {
        let store = create_test_store();

        // Insert multiple users
        for i in 1..=3 {
            let user_id = UserId::new(&format!("user{}", i));
            let user = create_test_user(&format!("user{}", i), &format!("user{}", i));
            store.put(&user_id, &user).unwrap();
        }

        // Scan all
        let users = store.scan_all().unwrap();
        assert_eq!(users.len(), 3);
    }

    #[test]
    fn test_admin_only_access() {
        let store = create_test_store();

        // System tables return None for table_access (admin-only)
        assert!(store.table_access().is_none());

        // Only Service, Dba, System roles can read
        assert!(!store.can_read(Role::User));
        assert!(store.can_read(Role::Service));
        assert!(store.can_read(Role::Dba));
        assert!(store.can_read(Role::System));
    }
}
