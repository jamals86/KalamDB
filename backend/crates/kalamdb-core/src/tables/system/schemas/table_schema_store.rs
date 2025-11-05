//! EntityStore implementation for TableDefinition persistence.
//!
//! This module provides type-safe storage for table schemas using the
//! SystemTableStore pattern from Phase 14.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use kalamdb_core::tables::system::schemas::TableSchemaStore;
//! use kalamdb_commons::models::TableId;
//! use kalamdb_commons::schemas::TableDefinition;
//!
//! let store = TableSchemaStore::new(backend);
//!
//! // Store a schema
//! let table_id = TableId::from_strings("default", "users");
//! let schema = TableDefinition { /* ... */ };
//! store.put(&table_id, &schema)?;
//!
//! // Retrieve a schema
//! if let Some(schema) = store.get(&table_id)? {
//!     println!("Schema version: {}", schema.schema_version());
//! }
//!
//! // Scan all schemas in a namespace
//! for (table_id, schema) in store.scan_namespace(&namespace_id)? {
//!     println!("Table: {}", table_id);
//! }
//! ```

use crate::tables::system::system_table_store::SystemTableStore;
use kalamdb_commons::models::{NamespaceId, TableId};
use kalamdb_commons::schemas::TableDefinition;
use kalamdb_store::entity_store::EntityStore;
use kalamdb_store::storage_trait::{Result, StorageBackend};
use std::sync::Arc;

/// EntityStore for TableDefinition with namespace-scoped scanning.
///
/// This is the single source of truth for all table schemas in KalamDB.
/// It wraps SystemTableStore<TableId, TableDefinition> and adds
/// namespace-scoped operations.
///
/// ## Partition Name
/// - `"system_table_schemas"` - All table schemas across all namespaces
///
/// ## Key Format
/// - TableId encodes both namespace and table name: "{namespace_id}:{table_name}"
///
/// ## Value Format
/// - TableDefinition is serialized using JSON (via EntityStore default)
pub struct TableSchemaStore {
    /// Underlying storage implementation
    inner: SystemTableStore<TableId, TableDefinition>,
}

impl TableSchemaStore {
    /// Creates a new TableSchemaStore with the given backend.
    ///
    /// # Arguments
    /// * `backend` - Storage backend (RocksDB, in-memory, etc.)
    ///
    /// # Returns
    /// A new TableSchemaStore instance
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self {
            inner: SystemTableStore::new(backend, "system_table_schemas"),
        }
    }

    /// Scans all table schemas in a specific namespace.
    ///
    /// Uses prefix scanning on TableId storage keys to find all tables
    /// in the given namespace.
    ///
    /// # Arguments
    /// * `namespace_id` - Namespace to scan
    ///
    /// # Returns
    /// Iterator over (TableId, TableDefinition) pairs in this namespace
    ///
    /// # Example
    /// ```rust,ignore
    /// let namespace_id = NamespaceId::new("default");
    /// for (table_id, schema) in store.scan_namespace(&namespace_id)? {
    ///     println!("Table: {} (version {})",
    ///              table_id.table_name(),
    ///              schema.schema_version);
    /// }
    /// ```
    pub fn scan_namespace(
        &self,
        namespace_id: &NamespaceId,
    ) -> Result<Vec<(TableId, TableDefinition)>> {
        use kalamdb_store::storage_trait::Partition;

        // Construct prefix: "{namespace_id}:"
        let prefix = format!("{}:", namespace_id.as_str());
        let prefix_bytes = prefix.as_bytes();

        // Use backend's scan method with prefix
        let partition = Partition::new(self.partition());
        let iter = self.backend().scan(&partition, Some(prefix_bytes), None)?;

        // Parse TableId from key bytes
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

    /// Retrieves all table schemas across all namespaces.
    ///
    /// This is useful for admin operations like SHOW TABLES without
    /// namespace filtering.
    ///
    /// # Returns
    /// All (TableId, TableDefinition) pairs in the database
    pub fn get_all(&self) -> Result<Vec<(TableId, TableDefinition)>> {
        use kalamdb_store::storage_trait::Partition;

        // Use backend's scan method directly
        let partition = Partition::new(self.partition());
        let iter = self.backend().scan(&partition, None, None)?;

        // Parse TableId from key bytes
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

/// Implement EntityStore trait to expose standard CRUD operations
impl EntityStore<TableId, TableDefinition> for TableSchemaStore {
    fn backend(&self) -> &Arc<dyn StorageBackend> {
        self.inner.backend()
    }

    fn partition(&self) -> &str {
        self.inner.partition()
    }

    // Override put to use as_storage_key() for proper composite key handling
    fn put(&self, key: &TableId, entity: &TableDefinition) -> Result<()> {
        use kalamdb_store::storage_trait::Partition;

        let partition = Partition::new(self.partition());
        let storage_key = key.as_storage_key();
        let value = self.serialize(entity)?;
        self.backend().put(&partition, &storage_key, &value)
    }

    // Override get to use as_storage_key() for proper composite key handling
    fn get(&self, key: &TableId) -> Result<Option<TableDefinition>> {
        use kalamdb_store::storage_trait::Partition;

        let partition = Partition::new(self.partition());
        let storage_key = key.as_storage_key();
        match self.backend().get(&partition, &storage_key)? {
            Some(bytes) => Ok(Some(self.deserialize(&bytes)?)),
            None => Ok(None),
        }
    }

    // Override delete to use as_storage_key() for proper composite key handling
    fn delete(&self, key: &TableId) -> Result<()> {
        use kalamdb_store::storage_trait::Partition;

        let partition = Partition::new(self.partition());
        let storage_key = key.as_storage_key();
        self.backend().delete(&partition, &storage_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::models::{NamespaceId, TableName};
    use kalamdb_commons::schemas::{ColumnDefinition, TableDefinition, TableOptions, TableType};
    use kalamdb_commons::types::KalamDataType;
    use kalamdb_store::test_utils::InMemoryBackend;

    fn create_test_table(namespace: &str, table: &str) -> (TableId, TableDefinition) {
        let table_id = TableId::from_strings(namespace, table);
        let columns = vec![ColumnDefinition::new(
            "id",
            1,
            KalamDataType::Text,
            false,
            true,
            false,
            kalamdb_commons::schemas::ColumnDefault::None,
            None,
        )];

        let table_def = TableDefinition::new(
            NamespaceId::new(namespace),
            TableName::new(table),
            TableType::User,
            columns,
            TableOptions::user(),
            None,
        )
        .expect("Failed to create table definition");

        (table_id, table_def)
    }

    #[test]
    fn test_table_schema_store_crud() {
        let backend = Arc::new(InMemoryBackend::new());
        let store = TableSchemaStore::new(backend);

        let (table_id, table_def) = create_test_table("default", "users");

        // Put schema
        store
            .put(&table_id, &table_def)
            .expect("Failed to put schema");

        // Get schema
        let retrieved = store.get(&table_id).expect("Failed to get schema");
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.table_type, TableType::User);
        assert_eq!(retrieved.columns.len(), 1);

        // Delete schema
        store.delete(&table_id).expect("Failed to delete schema");
        let deleted = store.get(&table_id).expect("Failed to get after delete");
        assert!(deleted.is_none());
    }

    #[test]
    fn test_scan_namespace() {
        let backend = Arc::new(InMemoryBackend::new());
        let store = TableSchemaStore::new(backend);

        // Create schemas in different namespaces
        let default_ns = NamespaceId::new("default");
        let test_ns = NamespaceId::new("test");

        let (table1_id, table1_def) = create_test_table("default", "users");
        let (table2_id, table2_def) = create_test_table("default", "posts");
        let (table3_id, table3_def) = create_test_table("test", "logs");

        store.put(&table1_id, &table1_def).unwrap();
        store.put(&table2_id, &table2_def).unwrap();
        store.put(&table3_id, &table3_def).unwrap();

        // Scan default namespace
        let default_tables = store
            .scan_namespace(&default_ns)
            .expect("Failed to scan namespace");
        assert_eq!(default_tables.len(), 2);

        // Scan test namespace
        let test_tables = store
            .scan_namespace(&test_ns)
            .expect("Failed to scan namespace");
        assert_eq!(test_tables.len(), 1);

        // Get all tables
        let all_tables = store.get_all().expect("Failed to get all tables");
        assert_eq!(all_tables.len(), 3);
    }
}
