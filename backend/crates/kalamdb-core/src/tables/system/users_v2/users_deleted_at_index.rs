//! Deleted_at secondary index for users table
//!
//! This module provides a secondary index to lookup deleted users by deletion timestamp.
//! Maps deletion_date (YYYY-MM-DD) -> Vec<UserId> for efficient cleanup job queries.

use crate::error::KalamDbError;
use chrono::{DateTime, Utc};
use kalamdb_commons::system::User;
use kalamdb_commons::UserId;
use kalamdb_store::{Partition, StorageBackend};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Storage for deleted_at index: deletion_date -> Vec<UserId>
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletedAtIndexEntry {
    pub user_ids: Vec<String>,
}

/// Deleted_at index implementation
pub struct DeletedAtIndex {
    backend: Arc<dyn StorageBackend>,
    partition: Partition,
}

impl DeletedAtIndex {
    /// Create a new deleted_at index
    ///
    /// # Arguments
    /// * `backend` - Storage backend (RocksDB or mock)
    ///
    /// # Returns
    /// A new DeletedAtIndex instance
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self {
            backend,
            partition: Partition::new("system_users_deleted_at_idx"),
        }
    }

    /// Return the partition name (for tests/diagnostics)
    pub fn partition(&self) -> &str {
        self.partition.name()
    }

    /// Index a deleted user
    ///
    /// # Arguments
    /// * `user` - The user to index (must have deleted_at set)
    ///
    /// # Returns
    /// Result indicating success or failure
    pub fn index_user(&self, user: &User) -> Result<(), KalamDbError> {
        if let Some(deleted_at_ms) = user.deleted_at {
            if let Some(deleted_at_dt) = DateTime::<Utc>::from_timestamp_millis(deleted_at_ms) {
                self.add_user_entry(&deleted_at_dt, &user.id)?;
            }
        }
        Ok(())
    }

    /// Remove a user from the deleted_at index
    ///
    /// # Arguments
    /// * `user_id` - The user ID to remove
    /// * `deleted_at` - The deletion timestamp
    ///
    /// # Returns
    /// Result indicating success or failure
    pub fn remove_user(
        &self,
        user_id: &UserId,
        deleted_at: &DateTime<Utc>,
    ) -> Result<(), KalamDbError> {
        let date_key = datetime_to_key(deleted_at);
        let user_id_str = user_id.as_str().to_string();

        if let Some(mut entry) = self.get_entry(&date_key)? {
            entry.user_ids.retain(|id| id != &user_id_str);

            if entry.user_ids.is_empty() {
                // Delete empty entry
                self.delete_entry(&date_key)?;
            } else {
                // Save updated entry
                self.put_entry(&date_key, &entry)?;
            }
        }

        Ok(())
    }

    /// Update user deletion status
    ///
    /// # Arguments
    /// * `user_id` - The user ID
    /// * `old_deleted_at` - The old deletion timestamp (None if being deleted now)
    /// * `new_deleted_at` - The new deletion timestamp (None if being restored)
    ///
    /// # Returns
    /// Result indicating success or failure
    pub fn update_user_deletion(
        &self,
        user_id: &UserId,
        old_deleted_at: Option<&DateTime<Utc>>,
        new_deleted_at: Option<&DateTime<Utc>>,
    ) -> Result<(), KalamDbError> {
        // Remove from old index if present
        if let Some(old_dt) = old_deleted_at {
            self.remove_user(user_id, old_dt)?;
        }

        // Add to new index if deleted
        if let Some(new_dt) = new_deleted_at {
            self.add_user_entry(new_dt, user_id)?;
        }

        Ok(())
    }

    /// Lookup all users deleted on or before a specific date
    ///
    /// # Arguments
    /// * `before_date` - Find users deleted on or before this date
    ///
    /// # Returns
    /// Vec<UserId> of deleted users
    pub fn lookup_before(&self, before_date: &DateTime<Utc>) -> Result<Vec<UserId>, KalamDbError> {
        let end_key = datetime_to_key(before_date);
        let mut user_ids = Vec::new();

        // Scan all entries up to and including the end_key
        // In a real implementation, this would use a range scan
        // For now, we'll need to implement this when we add range scan support
        // This is a simplified version that gets the exact date
        if let Some(entry) = self.get_entry(&end_key)? {
            for id_str in entry.user_ids {
                user_ids.push(UserId::new(&id_str));
            }
        }

        Ok(user_ids)
    }

    /// Lookup all users deleted on a specific date
    ///
    /// # Arguments
    /// * `date` - The deletion date
    ///
    /// # Returns
    /// Vec<UserId> of users deleted on that date
    pub fn lookup_on_date(&self, date: &DateTime<Utc>) -> Result<Vec<UserId>, KalamDbError> {
        let date_key = datetime_to_key(date);

        match self.get_entry(&date_key)? {
            Some(entry) => Ok(entry
                .user_ids
                .into_iter()
                .map(|id| UserId::new(&id))
                .collect()),
            None => Ok(Vec::new()),
        }
    }

    fn add_user_entry(&self, date: &DateTime<Utc>, user_id: &UserId) -> Result<(), KalamDbError> {
        let date_key = datetime_to_key(date);
        let user_id_str = user_id.as_str().to_string();

        let mut entry = self.get_entry(&date_key)?.unwrap_or(DeletedAtIndexEntry {
            user_ids: Vec::new(),
        });

        if !entry.user_ids.contains(&user_id_str) {
            entry.user_ids.push(user_id_str);
            self.put_entry(&date_key, &entry)?;
        }

        Ok(())
    }

    // Private helper methods
    fn get_entry(&self, key: &[u8]) -> Result<Option<DeletedAtIndexEntry>, KalamDbError> {
        match self.backend.get(&self.partition, key)? {
            Some(bytes) => {
                let entry: DeletedAtIndexEntry = serde_json::from_slice(&bytes)
                    .map_err(|e| KalamDbError::SerializationError(e.to_string()))?;
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }

    fn put_entry(&self, key: &[u8], entry: &DeletedAtIndexEntry) -> Result<(), KalamDbError> {
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

/// Convert DateTime to storage key (YYYY-MM-DD format)
fn datetime_to_key(dt: &DateTime<Utc>) -> Vec<u8> {
    dt.format("%Y-%m-%d").to_string().into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use kalamdb_commons::models::UserName;
    use kalamdb_commons::{AuthType, Role, StorageId, StorageMode};
    use kalamdb_store::InMemoryBackend;

    fn create_test_index() -> DeletedAtIndex {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        DeletedAtIndex::new(backend)
    }

    fn create_test_user(id: &str, username: &str, deleted_at: Option<DateTime<Utc>>) -> User {
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
            created_at: Utc::now().timestamp_millis(),
            updated_at: Utc::now().timestamp_millis(),
            last_seen: None,
            deleted_at: deleted_at.map(|dt| dt.timestamp_millis()),
        }
    }

    #[test]
    fn test_index_deleted_user() {
        let index = create_test_index();
        let deleted_at = Utc.with_ymd_and_hms(2025, 10, 29, 12, 0, 0).unwrap();
        let user = create_test_user("u1", "alice", Some(deleted_at));

        index.index_user(&user).unwrap();

        let result = index.lookup_on_date(&deleted_at).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].as_str(), "u1");
    }

    #[test]
    fn test_index_active_user_ignored() {
        let index = create_test_index();
        let user = create_test_user("u1", "alice", None); // Not deleted

        index.index_user(&user).unwrap();

        // Should not appear in any date lookup
        let today = Utc::now();
        let result = index.lookup_on_date(&today).unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_index_multiple_users_same_date() {
        let index = create_test_index();
        let deleted_at = Utc.with_ymd_and_hms(2025, 10, 29, 12, 0, 0).unwrap();
        let user1 = create_test_user("u1", "alice", Some(deleted_at));
        let user2 = create_test_user("u2", "bob", Some(deleted_at));

        index.index_user(&user1).unwrap();
        index.index_user(&user2).unwrap();

        let result = index.lookup_on_date(&deleted_at).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.iter().any(|id| id.as_str() == "u1"));
        assert!(result.iter().any(|id| id.as_str() == "u2"));
    }

    #[test]
    fn test_index_multiple_dates() {
        let index = create_test_index();
        let date1 = Utc.with_ymd_and_hms(2025, 10, 28, 12, 0, 0).unwrap();
        let date2 = Utc.with_ymd_and_hms(2025, 10, 29, 12, 0, 0).unwrap();

        let user1 = create_test_user("u1", "alice", Some(date1));
        let user2 = create_test_user("u2", "bob", Some(date2));

        index.index_user(&user1).unwrap();
        index.index_user(&user2).unwrap();

        let result1 = index.lookup_on_date(&date1).unwrap();
        let result2 = index.lookup_on_date(&date2).unwrap();

        assert_eq!(result1.len(), 1);
        assert_eq!(result2.len(), 1);
        assert_eq!(result1[0].as_str(), "u1");
        assert_eq!(result2[0].as_str(), "u2");
    }

    #[test]
    fn test_remove_user() {
        let index = create_test_index();
        let deleted_at = Utc.with_ymd_and_hms(2025, 10, 29, 12, 0, 0).unwrap();
        let user = create_test_user("u1", "alice", Some(deleted_at));

        index.index_user(&user).unwrap();
        assert_eq!(index.lookup_on_date(&deleted_at).unwrap().len(), 1);

        index.remove_user(&user.id, &deleted_at).unwrap();
        assert_eq!(index.lookup_on_date(&deleted_at).unwrap().len(), 0);
    }

    #[test]
    fn test_update_user_deletion_soft_delete() {
        let index = create_test_index();
        let user_id = UserId::new("u1");
        let deleted_at = Utc.with_ymd_and_hms(2025, 10, 29, 12, 0, 0).unwrap();

        // Soft delete user
        index
            .update_user_deletion(&user_id, None, Some(&deleted_at))
            .unwrap();

        let result = index.lookup_on_date(&deleted_at).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].as_str(), "u1");
    }

    #[test]
    fn test_update_user_deletion_restore() {
        let index = create_test_index();
        let user_id = UserId::new("u1");
        let deleted_at = Utc.with_ymd_and_hms(2025, 10, 29, 12, 0, 0).unwrap();

        // Soft delete user
        index
            .update_user_deletion(&user_id, None, Some(&deleted_at))
            .unwrap();
        assert_eq!(index.lookup_on_date(&deleted_at).unwrap().len(), 1);

        // Restore user (set deleted_at to None)
        index
            .update_user_deletion(&user_id, Some(&deleted_at), None)
            .unwrap();
        assert_eq!(index.lookup_on_date(&deleted_at).unwrap().len(), 0);
    }

    #[test]
    fn test_idempotent_indexing() {
        let index = create_test_index();
        let deleted_at = Utc.with_ymd_and_hms(2025, 10, 29, 12, 0, 0).unwrap();
        let user = create_test_user("u1", "alice", Some(deleted_at));

        index.index_user(&user).unwrap();
        index.index_user(&user).unwrap(); // Index again

        let result = index.lookup_on_date(&deleted_at).unwrap();
        assert_eq!(result.len(), 1); // Should still be 1, not 2
    }

    #[test]
    fn test_same_day_different_times() {
        let index = create_test_index();
        // Both deleted on same day but different times
        let dt1 = Utc.with_ymd_and_hms(2025, 10, 29, 10, 0, 0).unwrap();
        let dt2 = Utc.with_ymd_and_hms(2025, 10, 29, 16, 0, 0).unwrap();

        let user1 = create_test_user("u1", "alice", Some(dt1));
        let user2 = create_test_user("u2", "bob", Some(dt2));

        index.index_user(&user1).unwrap();
        index.index_user(&user2).unwrap();

        // Both should appear under the same date
        let result = index.lookup_on_date(&dt1).unwrap();
        assert_eq!(result.len(), 2);
    }
}
