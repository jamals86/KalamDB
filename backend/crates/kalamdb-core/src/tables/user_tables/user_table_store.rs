//! User table store implementation using EntityStore pattern
//!
//! This module provides a SystemTableStore-based implementation for user-scoped tables.

use crate::tables::system::system_table_store::SystemTableStore;
use kalamdb_commons::models::{NamespaceId, TableName, UserId};
use kalamdb_commons::StorageKey;
use kalamdb_store::StorageBackend;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Composite key for user table rows: user_id:row_id
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UserTableRowId {
    key: String, // Cached "user_id:row_id" format
}

impl UserTableRowId {
    /// Create a new user table row ID
    pub fn new(user_id: UserId, row_id: impl Into<String>) -> Self {
        let row_id_str: String = row_id.into();
        Self {
            key: format!("{}:{}", user_id.as_str(), row_id_str),
        }
    }

    /// Create from an existing key string (used internally)
    pub fn from_key_string(key: String) -> Self {
        Self { key }
    }

    /// Create from raw bytes
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let key = String::from_utf8_lossy(bytes).to_string();
        Self { key }
    }

    /// Get the full key string
    pub fn as_str(&self) -> &str {
        &self.key
    }

    /// Parse user_id from the key
    pub fn user_id(&self) -> UserId {
        let parts: Vec<&str> = self.key.splitn(2, ':').collect();
        UserId::new(parts[0])
    }

    /// Parse row_id from the key
    pub fn row_id(&self) -> &str {
        let parts: Vec<&str> = self.key.splitn(2, ':').collect();
        parts.get(1).unwrap_or(&"")
    }
}

impl AsRef<[u8]> for UserTableRowId {
    fn as_ref(&self) -> &[u8] {
        self.key.as_bytes()
    }
}

impl StorageKey for UserTableRowId {
    fn storage_key(&self) -> Vec<u8> {
        self.key.as_bytes().to_vec()
    }
}

/// User table row data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserTableRow {
    pub row_id: String,  //TODO: use UserTableRowId?
    pub user_id: String, //TODO: use UserId?
    pub fields: serde_json::Value,
    pub _updated: String,
    pub _deleted: bool,
}

/// Type alias for user table store (extends SystemTableStore)
///
/// Uses composite UserTableRowId keys (user_id:row_id) for user isolation.
pub type UserTableStore = SystemTableStore<UserTableRowId, UserTableRow>;

/// Helper function to create a new user table store
///
/// # Arguments
/// * `backend` - Storage backend (RocksDB or mock)
/// * `namespace_id` - Namespace identifier
/// * `table_name` - Table name
///
/// # Returns
/// A new SystemTableStore instance configured for the user table
pub fn new_user_table_store(
    backend: Arc<dyn StorageBackend>,
    namespace_id: &NamespaceId,
    table_name: &TableName,
) -> UserTableStore {
    //TODO: Use a template function inside: kalamdb_store::Partition::new_user_table_partition
    let partition_name = format!(
        "{}{}:{}",
        kalamdb_commons::constants::ColumnFamilyNames::USER_TABLE_PREFIX,
        namespace_id.as_str(),
        table_name.as_str()
    );

    // Ensure the partition exists in RocksDB
    let partition = kalamdb_store::Partition::new(partition_name.clone());
    let _ = backend.create_partition(&partition); // Ignore error if already exists

    SystemTableStore::new(backend, partition_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_store::{test_utils::InMemoryBackend, EntityStoreV2};

    fn create_test_store() -> UserTableStore {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        new_user_table_store(
            backend,
            &NamespaceId::new("test_ns"),
            &TableName::new("test_table"),
        )
    }

    fn create_test_row(user_id: &str, row_id: &str) -> UserTableRow {
        UserTableRow {
            row_id: row_id.to_string(),
            user_id: user_id.to_string(),
            fields: serde_json::json!({"name": "Alice"}),
            _updated: "2025-01-01T00:00:00Z".to_string(),
            _deleted: false,
        }
    }

    #[test]
    fn test_user_table_store_create() {
        let store = create_test_store();
        assert!(store.partition().contains("user_"));
    }

    #[test]
    fn test_user_table_store_put_get() {
        let store = create_test_store();
        let key = UserTableRowId::new(UserId::new("user1"), "row1");
        let row = create_test_row("user1", "row1");

        // Put and get
        store.put(&key, &row).unwrap();
        let retrieved = store.get(&key).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), row);
    }

    #[test]
    fn test_user_table_store_delete() {
        let store = create_test_store();
        let key = UserTableRowId::new(UserId::new("user1"), "row1");
        let row = create_test_row("user1", "row1");

        // Put, delete, verify
        store.put(&key, &row).unwrap();
        store.delete(&key).unwrap();
        let retrieved = store.get(&key).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_user_table_store_scan_all() {
        let store = create_test_store();

        // Insert multiple rows for different users
        for user_i in 1..=2 {
            for row_i in 1..=3 {
                let key = UserTableRowId::new(
                    UserId::new(&format!("user{}", user_i)),
                    format!("row{}", row_i),
                );
                let row = create_test_row(&format!("user{}", user_i), &format!("row{}", row_i));
                store.put(&key, &row).unwrap();
            }
        }

        // Scan all
        let all_rows = store.scan_all().unwrap();
        assert_eq!(all_rows.len(), 6); // 2 users * 3 rows
    }

    #[test]
    fn test_user_isolation() {
        let store = create_test_store();

        // User1's row
        let key1 = UserTableRowId::new(UserId::new("user1"), "row1");
        let row1 = create_test_row("user1", "row1");
        store.put(&key1, &row1).unwrap();

        // User2's row with same row_id
        let key2 = UserTableRowId::new(UserId::new("user2"), "row1");
        let row2 = create_test_row("user2", "row1");
        store.put(&key2, &row2).unwrap();

        // Both exist independently
        assert_eq!(store.get(&key1).unwrap().unwrap().user_id, "user1");
        assert_eq!(store.get(&key2).unwrap().unwrap().user_id, "user2");
    }
}
