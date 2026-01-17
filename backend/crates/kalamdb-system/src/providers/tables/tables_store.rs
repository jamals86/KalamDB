//! Tables table store implementation
//!
//! Phase 16: Consolidated store using TableVersionId keys.
//! Stores all table versions in a single partition with dual-key pattern:
//! - Latest pointer: "{namespace}:{table}<lat>" -> TableDefinition
//! - Versioned:      "{namespace}:{table}<ver>{version:08}" -> TableDefinition
//!
//! This allows:
//! - O(1) lookup of latest version
//! - Efficient range scans for version history
//! - Single storage partition for simplicity

use crate::system_table_store::SystemTableStore;
use crate::SystemTable;
use kalamdb_commons::models::{NamespaceId, TableId, TableVersionId};
use kalamdb_commons::schemas::TableDefinition;
use kalamdb_store::entity_store::EntityStore;
use kalamdb_store::storage_trait::Partition;
use kalamdb_store::StorageBackend;
use std::sync::Arc;

/// Type alias for the tables table store
/// Uses TableVersionId as key to support both latest and versioned lookups
pub type TablesStore = SystemTableStore<TableVersionId, TableDefinition>;

/// Helper function to create a new tables table store
///
/// # Arguments
/// * `backend` - Storage backend (RocksDB or mock)
///
/// # Returns
/// A new SystemTableStore instance configured for the tables table
pub fn new_tables_store(backend: Arc<dyn StorageBackend>) -> TablesStore {
    SystemTableStore::new(backend, SystemTable::Tables)
}

/// Helper methods for TablesStore specific operations
impl TablesStore {
    /// Get the latest version of a table definition
    pub fn get_latest(
        &self,
        table_id: &TableId,
    ) -> Result<Option<TableDefinition>, kalamdb_store::StorageError> {
        let latest_key = TableVersionId::latest(table_id.clone());
        self.get(&latest_key)
    }

    /// Get a specific version of a table definition
    pub fn get_version(
        &self,
        table_id: &TableId,
        version: u32,
    ) -> Result<Option<TableDefinition>, kalamdb_store::StorageError> {
        let version_key = TableVersionId::versioned(table_id.clone(), version);
        self.get(&version_key)
    }

    /// Store a new table definition version
    ///
    /// This stores both:
    /// 1. The versioned entry: `{tableId}<ver>{version:08}` -> TableDefinition
    /// 2. Updates the latest pointer: `{tableId}<lat>` -> TableDefinition
    ///
    /// # Arguments
    /// * `table_id` - The table identifier
    /// * `table_def` - The table definition (must have correct schema_version)
    pub fn put_version(
        &self,
        table_id: &TableId,
        table_def: &TableDefinition,
    ) -> Result<(), kalamdb_store::StorageError> {
        let version = table_def.schema_version;

        // Store the versioned entry
        let version_key = TableVersionId::versioned(table_id.clone(), version);
        self.put(&version_key, table_def)?;

        // Update the latest pointer
        let latest_key = TableVersionId::latest(table_id.clone());
        self.put(&latest_key, table_def)?;

        Ok(())
    }

    /// Delete all versions of a table (for DROP TABLE)
    ///
    /// Deletes both the latest pointer and all versioned entries.
    pub fn delete_all_versions(
        &self,
        table_id: &TableId,
    ) -> Result<usize, kalamdb_store::StorageError> {
        let mut deleted_count = 0;

        // Delete latest pointer
        let latest_key = TableVersionId::latest(table_id.clone());
        self.delete(&latest_key)?;
        deleted_count += 1;

        // Scan and delete all versioned entries
        let prefix = TableVersionId::version_scan_prefix(table_id);
        let partition = Partition::new(self.partition());
        let iter = self.backend().scan(&partition, Some(&prefix), None, None)?;

        for (key_bytes, _) in iter {
            if let Some(version_key) = TableVersionId::from_storage_key(&key_bytes) {
                self.delete(&version_key)?;
                deleted_count += 1;
            }
        }

        Ok(deleted_count)
    }

    /// List all versions of a table (for schema history queries)
    ///
    /// Returns versions in ascending order by version number.
    pub fn list_versions(
        &self,
        table_id: &TableId,
    ) -> Result<Vec<(u32, TableDefinition)>, kalamdb_store::StorageError> {
        let prefix = TableVersionId::version_scan_prefix(table_id);
        let partition = Partition::new(self.partition());
        let iter = self.backend().scan(&partition, Some(&prefix), None, None)?;

        let mut versions = Vec::new();
        for (key_bytes, value_bytes) in iter {
            if let Some(version_key) = TableVersionId::from_storage_key(&key_bytes) {
                if let Some(version) = version_key.version() {
                    if let Ok(table_def) = self.deserialize(&value_bytes) {
                        versions.push((version, table_def));
                    }
                }
            }
        }

        // Sort by version ascending
        versions.sort_by_key(|(v, _)| *v);

        Ok(versions)
    }

    /// Get the current version number for a table
    pub fn get_current_version(
        &self,
        table_id: &TableId,
    ) -> Result<Option<u32>, kalamdb_store::StorageError> {
        self.get_latest(table_id).map(|opt| opt.map(|def| def.schema_version))
    }

    /// Scan all tables (latest versions only) in a specific namespace
    pub fn scan_namespace(
        &self,
        namespace_id: &NamespaceId,
    ) -> Result<Vec<(TableId, TableDefinition)>, kalamdb_store::StorageError> {
        // Construct prefix: "{namespace_id}:"
        let prefix = format!("{}:", namespace_id.as_str());
        let prefix_bytes = prefix.as_bytes();

        let partition = Partition::new(self.partition());
        let iter = self.backend().scan(&partition, Some(prefix_bytes), None, None)?;

        let mut result = Vec::new();
        for (key_bytes, value_bytes) in iter {
            if let Some(version_key) = TableVersionId::from_storage_key(&key_bytes) {
                // Only include latest entries
                if version_key.is_latest() {
                    if let Ok(table_def) = self.deserialize(&value_bytes) {
                        result.push((version_key.table_id().clone(), table_def));
                    }
                }
            }
        }

        Ok(result)
    }

    /// Scan all table entries (both latest and versioned)
    ///
    /// Returns tuples of (TableVersionId, TableDefinition, is_latest)
    pub fn scan_all_with_versions(
        &self,
    ) -> Result<Vec<(TableVersionId, TableDefinition, bool)>, kalamdb_store::StorageError> {
        let partition = Partition::new(self.partition());
        let iter = self.backend().scan(&partition, None, None, None)?;

        let mut result = Vec::new();
        for (key_bytes, value_bytes) in iter {
            if let Some(version_key) = TableVersionId::from_storage_key(&key_bytes) {
                if let Ok(table_def) = self.deserialize(&value_bytes) {
                    let is_latest = version_key.is_latest();
                    result.push((version_key, table_def, is_latest));
                }
            }
        }

        Ok(result)
    }

    /// Scan all latest table definitions
    pub fn scan_all_latest(
        &self,
    ) -> Result<Vec<(TableId, TableDefinition)>, kalamdb_store::StorageError> {
        let entries = self.scan_all_with_versions()?;
        Ok(entries
            .into_iter()
            .filter_map(|(key, def, is_latest)| {
                if is_latest {
                    Some((key.table_id().clone(), def))
                } else {
                    None
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::datatypes::KalamDataType;
    use kalamdb_commons::schemas::{ColumnDefinition, TableDefinition, TableOptions, TableType};
    use kalamdb_commons::{NamespaceId, Role, TableId, TableName};
    use kalamdb_store::test_utils::InMemoryBackend;
    use kalamdb_store::CrossUserTableStore;

    fn create_test_store() -> TablesStore {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        new_tables_store(backend)
    }

    fn create_test_table(
        namespace: &str,
        table_name: &str,
        version: u32,
    ) -> (TableId, TableDefinition) {
        let namespace_id = NamespaceId::new(namespace);
        let table_name_id = TableName::new(table_name);
        let table_id = TableId::new(namespace_id.clone(), table_name_id.clone());

        let columns = vec![
            ColumnDefinition::new(
                1,
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
                2,
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

        let mut table_def = TableDefinition::new(
            namespace_id,
            table_name_id,
            TableType::User,
            columns,
            TableOptions::user(),
            None,
        )
        .expect("Failed to create table definition");
        table_def.schema_version = version;

        (table_id, table_def)
    }

    #[test]
    fn test_create_store() {
        let store = create_test_store();
        assert_eq!(
            store.partition(),
            SystemTable::Tables.column_family_name().expect("Tables is a table, not a view")
        );
    }

    #[test]
    fn test_put_and_get_version() {
        let store = create_test_store();
        let (table_id, table_def) = create_test_table("default", "conversations", 1);

        // Put version
        store.put_version(&table_id, &table_def).unwrap();

        // Get latest
        let retrieved = store.get_latest(&table_id).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.namespace_id.as_str(), "default");
        assert_eq!(retrieved.table_name.as_str(), "conversations");
        assert_eq!(retrieved.schema_version, 1);

        // Get specific version
        let specific = store.get_version(&table_id, 1).unwrap();
        assert!(specific.is_some());
    }

    #[test]
    fn test_multiple_versions() {
        let store = create_test_store();
        let (table_id, mut table_def) = create_test_table("default", "users", 1);

        // Store version 1
        store.put_version(&table_id, &table_def).unwrap();

        // Store version 2
        table_def.schema_version = 2;
        store.put_version(&table_id, &table_def).unwrap();

        // Store version 3
        table_def.schema_version = 3;
        store.put_version(&table_id, &table_def).unwrap();

        // Get latest should be version 3
        let latest = store.get_latest(&table_id).unwrap().unwrap();
        assert_eq!(latest.schema_version, 3);

        // List all versions
        let versions = store.list_versions(&table_id).unwrap();
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0].0, 1);
        assert_eq!(versions[1].0, 2);
        assert_eq!(versions[2].0, 3);
    }

    #[test]
    fn test_delete_all_versions() {
        let store = create_test_store();
        let (table_id, mut table_def) = create_test_table("default", "users", 1);

        // Store multiple versions
        store.put_version(&table_id, &table_def).unwrap();
        table_def.schema_version = 2;
        store.put_version(&table_id, &table_def).unwrap();

        // Delete all
        let deleted = store.delete_all_versions(&table_id).unwrap();
        assert_eq!(deleted, 3); // 2 versioned + 1 latest

        // Verify deleted
        assert!(store.get_latest(&table_id).unwrap().is_none());
        assert!(store.list_versions(&table_id).unwrap().is_empty());
    }

    #[test]
    fn test_scan_all_with_versions() {
        let store = create_test_store();

        // Insert multiple tables with versions
        let (table1_id, mut table1_def) = create_test_table("default", "users", 1);
        store.put_version(&table1_id, &table1_def).unwrap();
        table1_def.schema_version = 2;
        store.put_version(&table1_id, &table1_def).unwrap();

        let (table2_id, table2_def) = create_test_table("default", "posts", 1);
        store.put_version(&table2_id, &table2_def).unwrap();

        // Scan all with versions
        let all = store.scan_all_with_versions().unwrap();
        // 5 entries: users<lat>, users<ver>1, users<ver>2, posts<lat>, posts<ver>1
        assert_eq!(all.len(), 5);

        // Count latest entries
        let latest_count = all.iter().filter(|(_, _, is_latest)| *is_latest).count();
        assert_eq!(latest_count, 2);
    }

    #[test]
    fn test_scan_all_latest() {
        let store = create_test_store();

        // Insert multiple tables with versions
        let (table1_id, mut table1_def) = create_test_table("default", "users", 1);
        store.put_version(&table1_id, &table1_def).unwrap();
        table1_def.schema_version = 2;
        store.put_version(&table1_id, &table1_def).unwrap();

        let (table2_id, table2_def) = create_test_table("default", "posts", 1);
        store.put_version(&table2_id, &table2_def).unwrap();

        // Scan latest only
        let latest = store.scan_all_latest().unwrap();
        assert_eq!(latest.len(), 2);
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
        let (table1_id, table1_def) = create_test_table("default", "users", 1);
        let (table2_id, table2_def) = create_test_table("default", "posts", 1);
        let (table3_id, table3_def) = create_test_table("test", "logs", 1);

        store.put_version(&table1_id, &table1_def).unwrap();
        store.put_version(&table2_id, &table2_def).unwrap();
        store.put_version(&table3_id, &table3_def).unwrap();

        // Scan default namespace (latest only)
        let default_tables = store.scan_namespace(&NamespaceId::new("default")).unwrap();
        assert_eq!(default_tables.len(), 2);

        // Scan test namespace
        let test_tables = store.scan_namespace(&NamespaceId::new("test")).unwrap();
        assert_eq!(test_tables.len(), 1);
    }

    #[test]
    fn test_get_current_version() {
        let store = create_test_store();
        let (table_id, table_def) = create_test_table("default", "users", 1);

        // No version initially
        assert!(store.get_current_version(&table_id).unwrap().is_none());

        // After storing
        store.put_version(&table_id, &table_def).unwrap();
        assert_eq!(store.get_current_version(&table_id).unwrap(), Some(1));
    }
}
