//! Shared table store implementation using EntityStore pattern
//!
//! This module provides a SystemTableStore-based implementation for cross-user shared tables.

use crate::stores::SystemTableStore;
use kalamdb_commons::models::{NamespaceId, TableName};
use kalamdb_commons::TableAccess;
use kalamdb_store::{EntityStore, StorageBackend};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Shared table row ID (simple string)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SharedTableRowId(String);

impl SharedTableRowId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<[u8]> for SharedTableRowId {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

/// Shared table row data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SharedTableRow {
    pub row_id: String,
    pub fields: serde_json::Value,
    pub _updated: String,
    pub _deleted: bool,
    pub access_level: TableAccess,
}

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
    SystemTableStore::new(backend, partition_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_store::{EntityStoreV2, test_utils::InMemoryBackend};

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
        let key = SharedTableRowId::new("row1");
        let row = SharedTableRow {
            row_id: "row1".to_string(),
            fields: serde_json::json!({"name": "Public Data"}),
            _updated: "2025-01-01T00:00:00Z".to_string(),
            _deleted: false,
            access_level: TableAccess::Public,
        };

        // Put and get
        store.put(&key, &row).unwrap();
        let retrieved = store.get(&key).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), row);
    }

    #[test]
    fn test_shared_table_store_delete() {
        let store = create_test_store();
        let key = SharedTableRowId::new("row1");
        let row = SharedTableRow {
            row_id: "row1".to_string(),
            fields: serde_json::json!({"data": "test"}),
            _updated: "2025-01-01T00:00:00Z".to_string(),
            _deleted: false,
            access_level: TableAccess::Private,
        };

        // Put, delete, verify
        store.put(&key, &row).unwrap();
        store.delete(&key).unwrap();
        assert!(store.get(&key).unwrap().is_none());
    }

    #[test]
    fn test_shared_table_store_scan_all() {
        let store = create_test_store();

        // Insert multiple rows with different access levels
        for i in 1..=5 {
            let key = SharedTableRowId::new(format!("row{}", i));
            let access = if i % 2 == 0 {
                TableAccess::Public
            } else {
                TableAccess::Private
            };
            let row = SharedTableRow {
                row_id: format!("row{}", i),
                fields: serde_json::json!({"id": i}),
                _updated: "2025-01-01T00:00:00Z".to_string(),
                _deleted: false,
                access_level: access,
            };
            store.put(&key, &row).unwrap();
        }

        // Scan all
        let all_rows = store.scan_all().unwrap();
        assert_eq!(all_rows.len(), 5);

        // Filter by access level
        let public_rows: Vec<_> = all_rows
            .iter()
            .filter(|(_, row)| row.access_level == TableAccess::Public)
            .collect();
        assert_eq!(public_rows.len(), 2);
    }
}
