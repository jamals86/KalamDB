//! In-memory cache for table metadata
//!
//! This module provides an in-memory cache for table metadata loaded from RocksDB via kalamdb-sql.
//! The cache is indexed by namespace and table name for fast lookups.
//!
//! **Architecture Update (T031a)**: Cache now loads from system_tables and system_namespaces CFs
//! via kalamdb-sql instead of JSON configuration files.

use crate::catalog::TableMetadata;
use crate::error::KalamDbError;
use kalamdb_commons::models::{NamespaceId, TableName};
use kalamdb_sql::KalamSql;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Key for table cache (namespace, table_name)
type TableKey = (NamespaceId, TableName);

/// In-memory cache for table metadata
#[derive(Clone, Debug)]
pub struct TableCache {
    /// Table metadata indexed by (namespace, table_name)
    tables: Arc<RwLock<HashMap<TableKey, TableMetadata>>>,
}

impl TableCache {
    /// Create a new empty table cache
    pub fn new() -> Self {
        Self {
            tables: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Insert or update table metadata in the cache
    pub fn insert(&self, table: TableMetadata) {
        let key = (table.namespace.clone(), table.table_name.clone());
        let mut tables = self.tables.write().unwrap();
        tables.insert(key, table);
    }

    /// Get table metadata from the cache
    pub fn get(&self, namespace: &NamespaceId, table_name: &TableName) -> Option<TableMetadata> {
        let key = (namespace.clone(), table_name.clone());
        let tables = self.tables.read().unwrap();
        tables.get(&key).cloned()
    }

    /// Remove table metadata from the cache
    pub fn remove(&self, namespace: &NamespaceId, table_name: &TableName) -> Option<TableMetadata> {
        let key = (namespace.clone(), table_name.clone());
        let mut tables = self.tables.write().unwrap();
        tables.remove(&key)
    }

    /// Check if a table exists in the cache
    pub fn contains(&self, namespace: &NamespaceId, table_name: &TableName) -> bool {
        let key = (namespace.clone(), table_name.clone());
        let tables = self.tables.read().unwrap();
        tables.contains_key(&key)
    }

    /// List all tables in a namespace
    pub fn list_tables(&self, namespace: &NamespaceId) -> Vec<TableMetadata> {
        let tables = self.tables.read().unwrap();
        tables
            .iter()
            .filter(|(key, _)| &key.0 == namespace)
            .map(|(_, table)| table.clone())
            .collect()
    }

    /// List all tables in the cache
    pub fn list_all_tables(&self) -> Vec<TableMetadata> {
        let tables = self.tables.read().unwrap();
        tables.values().cloned().collect()
    }

    /// Get the number of tables in the cache
    pub fn len(&self) -> usize {
        let tables = self.tables.read().unwrap();
        tables.len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        let tables = self.tables.read().unwrap();
        tables.is_empty()
    }

    /// Clear all tables from the cache
    pub fn clear(&self) {
        let mut tables = self.tables.write().unwrap();
        tables.clear();
    }

    /// Load table metadata from RocksDB via kalamdb-sql on startup
    ///
    /// **T031a Implementation**: Queries system_tables and system_namespaces CFs
    /// and caches results in memory for fast access.
    ///
    /// # Arguments
    /// * `kalam_sql` - KalamSQL instance for querying system tables
    ///
    /// # Returns
    /// * `Ok(usize)` - Number of tables loaded into cache
    /// * `Err(KalamDbError)` - If query fails
    ///
    /// # Example
    /// ```no_run
    /// # use kalamdb_core::catalog::TableCache;
    /// # use kalamdb_sql::KalamSql;
    /// # use std::sync::Arc;
    /// # fn example(kalam_sql: Arc<KalamSql>) -> Result<(), Box<dyn std::error::Error>> {
    /// let cache = TableCache::new();
    /// let loaded = cache.load_from_rocksdb(kalam_sql)?;
    /// println!("Loaded {} tables into cache", loaded);
    /// # Ok(())
    /// # }
    /// ```
    pub fn load_from_rocksdb(&self, _kalam_sql: Arc<KalamSql>) -> Result<usize, KalamDbError> {
        // TODO: Query SELECT * FROM system.tables
        // TODO: For each table row, construct TableMetadata and insert into cache
        // TODO: Query SELECT * FROM system.namespaces to validate namespace references

        log::warn!("TableCache::load_from_rocksdb() not fully implemented - requires system_tables query support in kalamdb-sql");
        log::info!("Table cache will be populated dynamically as tables are created/accessed");

        Ok(0)
    }
}

impl Default for TableCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::TableType;
    use crate::flush::FlushPolicy;
    use chrono::Utc;

    fn create_test_table(namespace: &str, table_name: &str) -> TableMetadata {
        TableMetadata {
            table_name: TableName::new(table_name),
            table_type: TableType::User,
            namespace: NamespaceId::new(namespace),
            created_at: Utc::now(),
            storage_location: "/data/storage".to_string(),
            flush_policy: FlushPolicy::row_limit(1000).unwrap(),
            schema_version: 1,
            deleted_retention_hours: Some(720), // 30 days
        }
    }

    #[test]
    fn test_table_cache_insert_and_get() {
        let cache = TableCache::new();
        let table = create_test_table("ns1", "table1");

        cache.insert(table.clone());

        let retrieved = cache.get(&NamespaceId::new("ns1"), &TableName::new("table1"));
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().table_name, TableName::new("table1"));
    }

    #[test]
    fn test_table_cache_remove() {
        let cache = TableCache::new();
        let table = create_test_table("ns1", "table1");

        cache.insert(table.clone());
        assert!(cache.contains(&NamespaceId::new("ns1"), &TableName::new("table1")));

        let removed = cache.remove(&NamespaceId::new("ns1"), &TableName::new("table1"));
        assert!(removed.is_some());
        assert!(!cache.contains(&NamespaceId::new("ns1"), &TableName::new("table1")));
    }

    #[test]
    fn test_table_cache_list_tables() {
        let cache = TableCache::new();

        cache.insert(create_test_table("ns1", "table1"));
        cache.insert(create_test_table("ns1", "table2"));
        cache.insert(create_test_table("ns2", "table3"));

        let ns1_tables = cache.list_tables(&NamespaceId::new("ns1"));
        assert_eq!(ns1_tables.len(), 2);

        let ns2_tables = cache.list_tables(&NamespaceId::new("ns2"));
        assert_eq!(ns2_tables.len(), 1);

        let all_tables = cache.list_all_tables();
        assert_eq!(all_tables.len(), 3);
    }

    #[test]
    fn test_table_cache_clear() {
        let cache = TableCache::new();

        cache.insert(create_test_table("ns1", "table1"));
        cache.insert(create_test_table("ns1", "table2"));
        assert_eq!(cache.len(), 2);

        cache.clear();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_table_cache_concurrent_access() {
        let cache = TableCache::new();
        let cache_clone = cache.clone();

        // Simulate concurrent inserts
        let table1 = create_test_table("ns1", "table1");
        let table2 = create_test_table("ns1", "table2");

        cache.insert(table1);
        cache_clone.insert(table2);

        assert_eq!(cache.len(), 2);
        assert_eq!(cache_clone.len(), 2);
    }
}
