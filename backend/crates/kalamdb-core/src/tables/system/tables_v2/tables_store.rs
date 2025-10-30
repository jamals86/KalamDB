//! Tables table store implementation
//!
//! This module provides a SystemTableStore<String, SystemTable> wrapper for the system.tables table.

use crate::stores::SystemTableStore;
use kalamdb_commons::system::SystemTable;
use kalamdb_store::{CrossUserTableStore, EntityStoreV2, StorageBackend};
use std::sync::Arc;

/// Type alias for the tables table store
pub type TablesStore = SystemTableStore<String, SystemTable>; //TODO: Need to use TableId?

/// Helper function to create a new tables table store
///
/// # Arguments
/// * `backend` - Storage backend (RocksDB or mock)
///
/// # Returns
/// A new SystemTableStore instance configured for the tables table
pub fn new_tables_store(backend: Arc<dyn StorageBackend>) -> TablesStore {
    SystemTableStore::new(backend, "system_tables") //TODO: user the enum partition name
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::{NamespaceId, Role, StorageId, TableAccess, TableName, TableType};
    use kalamdb_store::test_utils::InMemoryBackend;

    fn create_test_store() -> TablesStore {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        new_tables_store(backend)
    }

    fn create_test_table(table_id: &str, table_name: &str) -> SystemTable {
        SystemTable {
            table_id: kalamdb_commons::TableId::new(
                NamespaceId::new("default"),
                TableName::new(table_name)
            ),
            table_name: TableName::new(table_name),
            namespace: NamespaceId::new("default"),
            table_type: TableType::User,
            created_at: 1000,
            storage_location: "/data".to_string(),
            storage_id: Some(StorageId::new("local")),
            use_user_storage: false,
            flush_policy: "{}".to_string(),
            schema_version: 1,
            deleted_retention_hours: 24,
            access_level: None,
        }
    }

    #[test]
    fn test_create_store() {
        let store = create_test_store();
        assert_eq!(store.partition(), "system_tables");
    }

    #[test]
    fn test_put_and_get_table() {
        let store = create_test_store();
        let table_id = "default:conversations".to_string();
        let table = create_test_table("table1", "conversations");

        // Put table
        store.put(&table_id, &table).unwrap();

        // Get table
        let retrieved = store.get(&table_id).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.table_id.as_str(), "default:conversations");
        assert_eq!(retrieved.table_name.as_str(), "conversations");
    }

    #[test]
    fn test_delete_table() {
        let store = create_test_store();
        let table_id = "table1".to_string();
        let table = create_test_table("table1", "conversations");

        // Put then delete
        store.put(&table_id, &table).unwrap();
        store.delete(&table_id).unwrap();

        // Verify deleted
        let retrieved = store.get(&table_id).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_scan_all_tables() {
        let store = create_test_store();

        // Insert multiple tables
        for i in 1..=3 {
            let table_id = format!("table{}", i);
            let table = create_test_table(&table_id, &format!("table{}", i));
            store.put(&table_id, &table).unwrap();
        }

        // Scan all
        let tables = store.scan_all().unwrap();
        assert_eq!(tables.len(), 3);
    }

    #[test]
    fn test_admin_only_access() {
        let store = create_test_store();

        // System tables return None for table_access (admin-only)
        assert!(store.table_access().is_none());

        // Only Service, Dba, System roles can read
        assert!(!store.can_read(&Role::User));
        assert!(store.can_read(&Role::Service));
        assert!(store.can_read(&Role::Dba));
        assert!(store.can_read(&Role::System));
    }
}
