//! User table store implementation using EntityStore pattern
//!
//! This module provides a SystemTableStore-based implementation for user-scoped tables.
//!
//! **MVCC Architecture (Phase 12, User Story 5)**:
//! - UserTableRowId: Composite struct with user_id and _seq fields (from kalamdb_commons)
//! - UserTableRow: Minimal structure with user_id, _seq, _deleted, fields (JSON)
//! - Storage key format: {user_id}:{_seq} (big-endian bytes)

use kalamdb_commons::ids::{SeqId, UserTableRowId};
use kalamdb_commons::models::{NamespaceId, TableName, UserId};
use kalamdb_store::StorageBackend;
use kalamdb_system::system_table_store::SystemTableStore;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// User table row data
///
/// **MVCC Architecture (Phase 12, User Story 5)**:
/// - Removed: row_id (redundant with _seq), _updated (timestamp embedded in _seq Snowflake ID)
/// - Kept: user_id (row owner), _seq (version identifier with embedded timestamp), _deleted (tombstone), fields (all user columns including PK)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserTableRow {
    pub user_id: UserId,
    pub _seq: SeqId,
    pub _deleted: bool,
    pub fields: serde_json::Value, // All user-defined columns including PK
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
    namespace_id: &NamespaceId, //TODO: Use TableId instead of both namespace and table name
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

    fn create_test_row(user_id: &str, seq: i64) -> UserTableRow {
        UserTableRow {
            user_id: UserId::new(user_id),
            _seq: SeqId::new(seq),
            fields: serde_json::json!({"name": "Alice", "id": 1}),
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
        let key = UserTableRowId::new(UserId::new("user1"), SeqId::new(100));
        let row = create_test_row("user1", 100);

        // Put and get
        store.put(&key, &row).unwrap();
        let retrieved = store.get(&key).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), row);
    }

    #[test]
    fn test_user_table_store_delete() {
        let store = create_test_store();
        let key = UserTableRowId::new(UserId::new("user1"), SeqId::new(200));
        let row = create_test_row("user1", 200);

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
                    SeqId::new((user_i * 1000 + row_i) as i64),
                );
                let row =
                    create_test_row(&format!("user{}", user_i), (user_i * 1000 + row_i) as i64);
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
        let key1 = UserTableRowId::new(UserId::new("user1"), SeqId::new(100));
        let row1 = create_test_row("user1", 100);
        store.put(&key1, &row1).unwrap();

        // User2's row with same seq
        let key2 = UserTableRowId::new(UserId::new("user2"), SeqId::new(100));
        let row2 = create_test_row("user2", 100);
        store.put(&key2, &row2).unwrap();

        // Both exist independently
        assert_eq!(
            store.get(&key1).unwrap().unwrap().user_id,
            UserId::new("user1")
        );
        assert_eq!(
            store.get(&key2).unwrap().unwrap().user_id,
            UserId::new("user2")
        );
    }
}
