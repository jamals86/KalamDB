//! Role secondary index for users table
//!
//! This module provides a secondary index to lookup users by role.
//! Maps Role -> Vec<UserId> for efficient role-based queries (non-unique index).

use crate::error::KalamDbError;
use kalamdb_commons::models::Role;
use kalamdb_commons::system::User;
use kalamdb_commons::UserId;
use kalamdb_store::{Partition, StorageBackend};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Storage for role index: Role -> Vec<UserId>
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleIndexEntry {
    pub user_ids: Vec<String>,
}

/// Role index implementation
pub struct RoleIndex {
    backend: Arc<dyn StorageBackend>,
    partition: Partition,
}

impl RoleIndex {
    /// Create a new role index
    ///
    /// # Arguments
    /// * `backend` - Storage backend (RocksDB or mock)
    ///
    /// # Returns
    /// A new RoleIndex instance
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self {
            backend,
            partition: Partition::new("system_users_role_idx"),
        }
    }

    /// Return the partition name (for tests/diagnostics)
    pub fn partition(&self) -> &str {
        self.partition.name()
    }

    /// Add a user to the role index
    ///
    /// # Arguments
    /// * `user` - The user to index
    ///
    /// # Returns
    /// Result indicating success or failure
    pub fn index_user(&self, user: &User) -> Result<(), KalamDbError> {
        let role_key = role_to_key(&user.role);
        let user_id_str = user.id.as_str().to_string();

        // Get existing entry or create new
        let mut entry = self.get_entry(&role_key)?.unwrap_or(RoleIndexEntry {
            user_ids: Vec::new(),
        });

        // Add user_id if not already present
        if !entry.user_ids.contains(&user_id_str) {
            entry.user_ids.push(user_id_str);
        }

        // Save updated entry
        self.put_entry(&role_key, &entry)?;
        Ok(())
    }

    /// Remove a user from the role index
    ///
    /// # Arguments
    /// * `user_id` - The user ID to remove
    /// * `role` - The role to remove from
    ///
    /// # Returns
    /// Result indicating success or failure
    pub fn remove_user(&self, user_id: &UserId, role: &Role) -> Result<(), KalamDbError> {
        let role_key = role_to_key(role);
        let user_id_str = user_id.as_str().to_string();

        if let Some(mut entry) = self.get_entry(&role_key)? {
            entry.user_ids.retain(|id| id != &user_id_str);

            if entry.user_ids.is_empty() {
                // Delete empty entry
                self.delete_entry(&role_key)?;
            } else {
                // Save updated entry
                self.put_entry(&role_key, &entry)?;
            }
        }

        Ok(())
    }

    /// Update user role (remove from old role, add to new role)
    ///
    /// # Arguments
    /// * `user_id` - The user ID
    /// * `old_role` - The old role (None if new user)
    /// * `new_role` - The new role
    ///
    /// # Returns
    /// Result indicating success or failure
    pub fn update_user_role(
        &self,
        user_id: &UserId,
        old_role: Option<&Role>,
        new_role: &Role,
    ) -> Result<(), KalamDbError> {
        // Remove from old role if present
        if let Some(old) = old_role {
            if old != new_role {
                self.remove_user(user_id, old)?;
            }
        }

        let role_key = role_to_key(new_role);
        let user_id_str = user_id.as_str().to_string();

        let mut entry = self.get_entry(&role_key)?.unwrap_or(RoleIndexEntry {
            user_ids: Vec::new(),
        });

        if !entry.user_ids.contains(&user_id_str) {
            entry.user_ids.push(user_id_str);
            self.put_entry(&role_key, &entry)?;
        }
        Ok(())
    }

    /// Lookup all users with a specific role
    ///
    /// # Arguments
    /// * `role` - The role to lookup
    ///
    /// # Returns
    /// Vec<UserId> of users with that role
    pub fn lookup(&self, role: &Role) -> Result<Vec<UserId>, KalamDbError> {
        let role_key = role_to_key(role);

        match self.get_entry(&role_key)? {
            Some(entry) => Ok(entry
                .user_ids
                .into_iter()
                .map(|id| UserId::new(&id))
                .collect()),
            None => Ok(Vec::new()),
        }
    }

    // Private helper methods
    fn get_entry(&self, key: &[u8]) -> Result<Option<RoleIndexEntry>, KalamDbError> {
        match self.backend.get(&self.partition, key)? {
            Some(bytes) => {
                let entry: RoleIndexEntry = serde_json::from_slice(&bytes)
                    .map_err(|e| KalamDbError::SerializationError(e.to_string()))?;
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }

    fn put_entry(&self, key: &[u8], entry: &RoleIndexEntry) -> Result<(), KalamDbError> {
        let bytes = serde_json::to_vec(entry)
            .map_err(|e| KalamDbError::SerializationError(e.to_string()))?;
        self.backend.put(&self.partition, key, &bytes)?;
        Ok(())
    }

    fn delete_entry(&self, key: &[u8]) -> Result<(), KalamDbError> {
        self.backend.delete(&self.partition, key)?;
        Ok(())
    }
}

/// Convert Role to storage key
fn role_to_key(role: &Role) -> Vec<u8> {
    role.as_str().as_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::models::UserName;
    use kalamdb_commons::{AuthType, StorageId, StorageMode};
    use kalamdb_store::test_utils::InMemoryBackend;

    fn create_test_index() -> RoleIndex {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        RoleIndex::new(backend)
    }

    fn create_test_user(id: &str, username: &str, role: Role) -> User {
        User {
            id: UserId::new(id),
            username: UserName::new(username),
            password_hash: "hashed_password".to_string(),
            role,
            email: Some(format!("{}@example.com", username)),
            auth_type: AuthType::Password,
            auth_data: None,
            storage_mode: StorageMode::Table,
            storage_id: Some(StorageId::local()),
            created_at: chrono::Utc::now().timestamp_millis(),
            updated_at: chrono::Utc::now().timestamp_millis(),
            last_seen: None,
            deleted_at: None,
        }
    }

    #[test]
    fn test_index_single_user() {
        let index = create_test_index();
        let user = create_test_user("u1", "alice", Role::User);

        index.index_user(&user).unwrap();

        let result = index.lookup(&Role::User).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].as_str(), "u1");
    }

    #[test]
    fn test_index_multiple_users_same_role() {
        let index = create_test_index();
        let user1 = create_test_user("u1", "alice", Role::User);
        let user2 = create_test_user("u2", "bob", Role::User);

        index.index_user(&user1).unwrap();
        index.index_user(&user2).unwrap();

        let result = index.lookup(&Role::User).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.iter().any(|id| id.as_str() == "u1"));
        assert!(result.iter().any(|id| id.as_str() == "u2"));
    }

    #[test]
    fn test_index_multiple_roles() {
        let index = create_test_index();
        let user1 = create_test_user("u1", "alice", Role::User);
        let user2 = create_test_user("u2", "admin", Role::Dba);
        let user3 = create_test_user("u3", "service", Role::Service);

        index.index_user(&user1).unwrap();
        index.index_user(&user2).unwrap();
        index.index_user(&user3).unwrap();

        let users = index.lookup(&Role::User).unwrap();
        let dbas = index.lookup(&Role::Dba).unwrap();
        let services = index.lookup(&Role::Service).unwrap();

        assert_eq!(users.len(), 1);
        assert_eq!(dbas.len(), 1);
        assert_eq!(services.len(), 1);
    }

    #[test]
    fn test_remove_user() {
        let index = create_test_index();
        let user = create_test_user("u1", "alice", Role::User);

        index.index_user(&user).unwrap();
        assert_eq!(index.lookup(&Role::User).unwrap().len(), 1);

        index.remove_user(&user.id, &Role::User).unwrap();
        assert_eq!(index.lookup(&Role::User).unwrap().len(), 0);
    }

    #[test]
    fn test_update_user_role() {
        let index = create_test_index();
        let user_id = UserId::new("u1");

        // Add as User
        index.update_user_role(&user_id, None, &Role::User).unwrap();
        assert_eq!(index.lookup(&Role::User).unwrap().len(), 1);
        assert_eq!(index.lookup(&Role::Dba).unwrap().len(), 0);

        // Update to Dba
        index
            .update_user_role(&user_id, Some(&Role::User), &Role::Dba)
            .unwrap();
        assert_eq!(index.lookup(&Role::User).unwrap().len(), 0);
        assert_eq!(index.lookup(&Role::Dba).unwrap().len(), 1);
    }

    #[test]
    fn test_lookup_nonexistent_role() {
        let index = create_test_index();
        let result = index.lookup(&Role::System).unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_idempotent_indexing() {
        let index = create_test_index();
        let user = create_test_user("u1", "alice", Role::User);

        index.index_user(&user).unwrap();
        index.index_user(&user).unwrap(); // Index again

        let result = index.lookup(&Role::User).unwrap();
        assert_eq!(result.len(), 1); // Should still be 1, not 2
    }
}
