//! User table store implementation using EntityStore pattern
//!
//! This module provides an EntityStore-based implementation for user-scoped tables.
//! Unlike system tables, user tables use EntityStore directly (not SystemTableStore)
//! because they are user data, not system metadata.
//!
//! **MVCC Architecture (Phase 12, User Story 5)**:
//! - UserTableRowId: Composite struct with user_id and _seq fields (from kalamdb_commons)
//! - UserTableRow: Minimal structure with user_id, _seq, _deleted, fields (JSON)
//! - Storage key format: {user_id}:{_seq} (big-endian bytes)
//!
//! **PK Index (Phase 14)**:
//! - IndexedEntityStore variant maintains a secondary index on the PK field
//! - Enables O(1) lookup of row by PK value instead of O(n) scan

use super::pk_index::create_user_table_pk_index;
use kalamdb_commons::ids::{SeqId, UserTableRowId};
use kalamdb_commons::models::row::Row;
use kalamdb_commons::models::{KTableRow, NamespaceId, TableName, UserId};
use kalamdb_store::entity_store::{EntityStore, KSerializable};
use kalamdb_store::{IndexedEntityStore, StorageBackend};
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
    pub _seq: SeqId,    //TODO: Rename this to seq without the _
    pub _deleted: bool, //TODO: Rename this to deleted without the _
    pub fields: Row,    // All user-defined columns including PK
}

impl KSerializable for UserTableRow {}

impl From<UserTableRow> for KTableRow {
    fn from(row: UserTableRow) -> Self {
        KTableRow {
            user_id: row.user_id,
            _seq: row._seq,
            _deleted: row._deleted,
            fields: row.fields,
        }
    }
}

/// Store for user tables (user data, not system metadata).
///
/// Uses composite UserTableRowId keys (user_id:_seq) for user isolation.
/// Unlike SystemTableStore, this is a direct EntityStore implementation
/// without admin-only access control.
#[derive(Clone)]
pub struct UserTableStore {
    backend: Arc<dyn StorageBackend>,
    partition: String,
}

impl UserTableStore {
    /// Create a new user table store
    ///
    /// # Arguments
    /// * `backend` - Storage backend (RocksDB or mock)
    /// * `partition` - Partition name (e.g., "user_default:users")
    pub fn new(backend: Arc<dyn StorageBackend>, partition: impl Into<String>) -> Self {
        Self {
            backend,
            partition: partition.into(),
        }
    }
}

/// Implement EntityStore trait for typed CRUD operations
impl EntityStore<UserTableRowId, UserTableRow> for UserTableStore {
    fn backend(&self) -> &Arc<dyn StorageBackend> {
        &self.backend
    }

    fn partition(&self) -> &str {
        &self.partition
    }
}

/// Helper function to create a new user table store
///
/// # Arguments
/// * `backend` - Storage backend (RocksDB or mock)
/// * `namespace_id` - Namespace identifier
/// * `table_name` - Table name
///
/// # Returns
/// A new UserTableStore instance configured for the user table
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

    UserTableStore::new(backend, partition_name)
}

/// Type alias for indexed user table store with PK index.
///
/// This store automatically maintains a secondary index on the primary key field,
/// enabling O(1) lookup of rows by PK value.
pub type UserTableIndexedStore = IndexedEntityStore<UserTableRowId, UserTableRow>;

/// Create a new indexed user table store with PK index.
///
/// # Arguments
/// * `backend` - Storage backend (RocksDB or mock)
/// * `namespace_id` - Namespace identifier
/// * `table_name` - Table name
/// * `pk_field_name` - Name of the primary key column
///
/// # Returns
/// A new IndexedEntityStore configured with PK index for efficient lookups
pub fn new_indexed_user_table_store(
    backend: Arc<dyn StorageBackend>,
    namespace_id: &NamespaceId,
    table_name: &TableName,
    pk_field_name: &str,
) -> UserTableIndexedStore {
    let partition_name = format!(
        "{}{}:{}",
        kalamdb_commons::constants::ColumnFamilyNames::USER_TABLE_PREFIX,
        namespace_id.as_str(),
        table_name.as_str()
    );

    // Ensure the partition exists in RocksDB
    let partition = kalamdb_store::Partition::new(partition_name.clone());
    let _ = backend.create_partition(&partition); // Ignore error if already exists

    // Create PK index
    let pk_index = create_user_table_pk_index(
        namespace_id.as_str(),
        table_name.as_str(),
        pk_field_name,
    );

    IndexedEntityStore::new(backend, partition_name, vec![pk_index])
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::scalar::ScalarValue;
    use kalamdb_store::test_utils::InMemoryBackend;
    use std::collections::BTreeMap;

    fn create_test_store() -> UserTableStore {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        new_user_table_store(
            backend,
            &NamespaceId::new("test_ns"),
            &TableName::new("test_table"),
        )
    }

    fn create_test_row(user_id: &str, seq: i64) -> UserTableRow {
        let mut values = BTreeMap::new();
        values.insert("name".to_string(), ScalarValue::Utf8(Some("Alice".to_string())));
        values.insert("id".to_string(), ScalarValue::Int64(Some(1)));
        UserTableRow {
            user_id: UserId::new(user_id),
            _seq: SeqId::new(seq),
            fields: Row::new(values),
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
        let all_rows = store.scan_all(None, None, None).unwrap();
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
