//! Tables table store implementation
//!
//! This module provides a SystemTableStore<TableId, TableDefinition> wrapper for the system.tables table.

use crate::tables::system::system_table_store::SystemTableStore;
use kalamdb_commons::models::{TableId, NamespaceId};
use kalamdb_commons::schemas::TableDefinition;
use kalamdb_store::StorageBackend;
use std::sync::Arc;

/// Type alias for the tables table store
pub type TablesStore = SystemTableStore<TableId, TableDefinition>;

/// Helper function to create a new tables table store
///
/// # Arguments
/// * `backend` - Storage backend (RocksDB or mock)
///
/// # Returns
/// A new SystemTableStore instance configured for the tables table
pub fn new_tables_store(backend: Arc<dyn StorageBackend>) -> TablesStore {
    SystemTableStore::new(backend, "system_tables")
}

/// Helper methods for TablesStore specific operations
impl TablesStore {
    /// Scan all tables in a specific namespace
    pub fn scan_namespace(&self, namespace_id: &NamespaceId) -> Result<Vec<(TableId, TableDefinition)>, kalamdb_store::StorageError> {
        use kalamdb_store::storage_trait::Partition;
        use kalamdb_store::EntityStoreV2;

        // Construct prefix: "{namespace_id}:"
        let prefix = format!("{}:", namespace_id.as_str());
        let prefix_bytes = prefix.as_bytes();

        // Use backend's scan method with prefix
        let partition = Partition::new(self.partition());
        let iter = self.backend().scan(&partition, Some(prefix_bytes), None)?;

        // Parse TableId from key bytes and deserialize TableDefinition
        let mut result = Vec::new();
        for (key_bytes, value_bytes) in iter {
            if let Some(table_id) = TableId::from_storage_key(&key_bytes) {
                if let Ok(table_def) = self.deserialize(&value_bytes) {
                    result.push((table_id, table_def));
                }
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::{NamespaceId, Role, TableName, TableId};
    use kalamdb_commons::datatypes::KalamDataType;
    use kalamdb_commons::schemas::{ColumnDefinition, TableDefinition, TableOptions, TableType};
    use kalamdb_store::test_utils::InMemoryBackend;
    use kalamdb_store::CrossUserTableStore;
    use kalamdb_store::EntityStoreV2 as EntityStore;

    fn create_test_store() -> TablesStore {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        new_tables_store(backend)
    }

    fn create_test_table(namespace: &str, table_name: &str) -> (TableId, TableDefinition) {
        let namespace_id = NamespaceId::new(namespace);
        let table_name_id = TableName::new(table_name);
        let table_id = TableId::new(namespace_id.clone(), table_name_id.clone());
        
        let columns = vec![
            ColumnDefinition::new(
                "id",
                1,
                KalamDataType::Uuid,
                false,
                true,
                false,
                kalamdb_commons::schemas::ColumnDefault::None,
                None,
            ),
            ColumnDefinition::new(
                "name",
                2,
                KalamDataType::Text,
                false,
                false,
                false,
                kalamdb_commons::schemas::ColumnDefault::None,
                None,
            ),
        ];

        let table_def = TableDefinition::new(
            namespace_id,
            table_name_id,
            TableType::User,
            columns,
            TableOptions::user(),
            None,
        ).expect("Failed to create table definition");

        (table_id, table_def)
    }

    #[test]
    fn test_create_store() {
        let store = create_test_store();
        assert_eq!(store.partition(), "system_tables");
    }

    #[test]
    fn test_put_and_get_table() {
        let store = create_test_store();
        let (table_id, table_def) = create_test_table("default", "conversations");

        // Put table
        store.put(&table_id, &table_def).unwrap();

        // Get table
        let retrieved = store.get(&table_id).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.namespace_id.as_str(), "default");
        assert_eq!(retrieved.table_name.as_str(), "conversations");
    }

    #[test]
    fn test_delete_table() {
        let store = create_test_store();
        let (table_id, table_def) = create_test_table("default", "conversations");

        // Put then delete
        store.put(&table_id, &table_def).unwrap();
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
            let (table_id, table_def) = create_test_table("default", &format!("table{}", i));
            store.put(&table_id, &table_def).unwrap();
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

    #[test]
    fn test_scan_namespace() {
        let store = create_test_store();

        // Insert tables in different namespaces
        let (table1_id, table1_def) = create_test_table("default", "users");
        let (table2_id, table2_def) = create_test_table("default", "posts");
        let (table3_id, table3_def) = create_test_table("test", "logs");

        store.put(&table1_id, &table1_def).unwrap();
        store.put(&table2_id, &table2_def).unwrap();
        store.put(&table3_id, &table3_def).unwrap();

        // Scan default namespace
        let default_tables = store.scan_namespace(&NamespaceId::new("default")).unwrap();
        assert_eq!(default_tables.len(), 2);

        // Scan test namespace
        let test_tables = store.scan_namespace(&NamespaceId::new("test")).unwrap();
        assert_eq!(test_tables.len(), 1);
    }
}
