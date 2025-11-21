//! Shared table store implementation using EntityStore pattern
//!
//! This module provides a SystemTableStore-based implementation for cross-user shared tables.
//!
//! **MVCC Architecture (Phase 12, User Story 5)**:
//! - SharedTableRowId: SeqId directly (from kalamdb_commons)
//! - SharedTableRow: Minimal structure with _seq, _deleted, fields (JSON)
//! - Storage key format: {_seq} (big-endian bytes)
//! - NO access_level field (cached in schema definition, not per-row)

use kalamdb_commons::ids::{SeqId, SharedTableRowId};
use kalamdb_commons::models::row::Row;
use kalamdb_commons::models::{NamespaceId, TableName};
use kalamdb_store::{entity_store::KSerializable, StorageBackend};
use kalamdb_system::system_table_store::SystemTableStore;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Shared table row data
///
/// **MVCC Architecture (Phase 12, User Story 5)**:
/// - Removed: row_id (redundant with _seq), _updated (timestamp embedded in _seq Snowflake ID), access_level (moved to schema definition)
/// - Kept: _seq (version identifier with embedded timestamp), _deleted (tombstone), fields (all shared table columns including PK)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SharedTableRow {
    pub _seq: SeqId,
    pub _deleted: bool,
    pub fields: Row, // All user-defined columns including PK
}

impl KSerializable for SharedTableRow {}

/// Type alias for shared table store (extends SystemTableStore)
pub type SharedTableStore = SystemTableStore<SharedTableRowId, SharedTableRow>;

/// Helper function to create a new shared table store
///
/// # Arguments
/// * `backend` - Storage backend (RocksDB or mock)
/// * `namespace_id` - Namespace identifier
/// * `table_name` - Table name
///
/// # Returns
/// A new SystemTableStore instance configured for the shared table
pub fn new_shared_table_store(
    backend: Arc<dyn StorageBackend>,
    namespace_id: &NamespaceId,
    table_name: &TableName,
) -> SharedTableStore {
    let partition_name = format!(
        "{}{}:{}",
        kalamdb_commons::constants::ColumnFamilyNames::SHARED_TABLE_PREFIX,
        namespace_id.as_str(),
        table_name.as_str()
    );
    // Ensure the partition exists in RocksDB (mirror user table behavior)
    let partition = kalamdb_store::Partition::new(partition_name.clone());
    let _ = backend.create_partition(&partition); // Ignore error if already exists

    SystemTableStore::new(backend, partition_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_store::{test_utils::InMemoryBackend, EntityStoreV2};

    fn create_test_store() -> SharedTableStore {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        new_shared_table_store(
            backend,
            &NamespaceId::new("test_ns"),
            &TableName::new("test_table"),
        )
    }

    #[test]
    fn test_shared_table_store_create() {
        let store = create_test_store();
        assert!(store.partition().contains("shared_"));
    }

    #[test]
    fn test_shared_table_store_put_get() {
        let store = create_test_store();
        let key = SeqId::new(100);
        let row = SharedTableRow {
            _seq: SeqId::new(100),
            fields: serde_json::from_value(serde_json::json!({"name": "Public Data", "id": 1}))
                .unwrap(),
            _deleted: false,
        };

        // Put and get
        store.put(&key, &row).unwrap();
        let retrieved = store.get(&key).unwrap().unwrap();
        assert_eq!(retrieved, row);
    }

    #[test]
    fn test_shared_table_store_delete() {
        let store = create_test_store();
        let key = SeqId::new(200);
        let row = SharedTableRow {
            _seq: SeqId::new(200),
            fields: serde_json::from_value(serde_json::json!({"data": "test", "id": 2})).unwrap(),
            _deleted: false,
        };

        // Put, delete, verify
        store.put(&key, &row).unwrap();
        store.delete(&key).unwrap();
        assert!(store.get(&key).unwrap().is_none());
    }

    #[test]
    fn test_shared_table_store_scan_all() {
        let store = create_test_store();

        // Insert multiple rows
        for i in 1..=5 {
            let key = SeqId::new(i as i64 * 100);
            let row = SharedTableRow {
                _seq: SeqId::new(i as i64 * 100),
                fields: serde_json::from_value(serde_json::json!({"id": i})).unwrap(),
                _deleted: false,
            };
            store.put(&key, &row).unwrap();
        }

        // Scan all
        let all_rows = store.scan_all(None, None, None).unwrap();
        assert_eq!(all_rows.len(), 5);
    }
}
