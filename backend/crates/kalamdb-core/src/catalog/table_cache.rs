//! In-memory cache for table metadata
//!
//! This module provides an in-memory cache for table metadata loaded from RocksDB via kalamdb-sql.
//! The cache is indexed by namespace and table name for fast lookups.
//!
//! **Architecture Update (T031a)**: Cache now loads from system_tables and system_namespaces CFs
//! via kalamdb-sql instead of JSON configuration files.
//!
//! **Phase 9 Extension (T184-T190)**: Added storage path template caching with partial resolution.
//! - Caches partially-resolved templates with static placeholders substituted ({namespace}, {tableName})
//! - Dynamic placeholders remain unevaluated ({userId}, {shard}) for per-request substitution
//! - Integrates with StorageRegistry for template resolution

use crate::catalog::TableMetadata;
use crate::error::KalamDbError;
use crate::storage::StorageRegistry;
use kalamdb_commons::models::{NamespaceId, StorageId, TableName};
use kalamdb_commons::schemas::TableType;
use kalamdb_sql::KalamSql;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Key for table cache (namespace, table_name)
type TableKey = (NamespaceId, TableName);

/// In-memory cache for table metadata
///
/// **Phase 9 Extension**: Now includes storage path template caching for dynamic path resolution.
#[derive(Clone)]
pub struct TableCache {
    /// Table metadata indexed by (namespace, table_name)
    tables: Arc<RwLock<HashMap<TableKey, TableMetadata>>>,
    
    /// Partially-resolved storage path templates indexed by (namespace, table_name)
    /// Templates have {namespace}/{tableName} substituted but keep {userId}/{shard} for per-request resolution
    storage_path_templates: Arc<RwLock<HashMap<TableKey, String>>>,
    
    /// Storage registry for resolving templates from system.storages
    storage_registry: Option<Arc<StorageRegistry>>,
}

impl std::fmt::Debug for TableCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TableCache")
            .field("tables", &self.tables)
            .field("storage_path_templates", &self.storage_path_templates)
            .field("storage_registry", &"<registry>")
            .finish()
    }
}

impl TableCache {
    /// Create a new empty table cache
    pub fn new() -> Self {
        Self {
            tables: Arc::new(RwLock::new(HashMap::new())),
            storage_path_templates: Arc::new(RwLock::new(HashMap::new())),
            storage_registry: None,
        }
    }

    /// Set the storage registry for dynamic path resolution (builder pattern)
    ///
    /// **T186**: Enables path template resolution via StorageRegistry
    ///
    /// # Arguments
    /// * `registry` - StorageRegistry instance for querying system.storages
    ///
    /// # Returns
    /// Self for method chaining
    ///
    /// # Example
    /// ```no_run
    /// # use kalamdb_core::catalog::TableCache;
    /// # use kalamdb_core::storage::StorageRegistry;
    /// # use std::sync::Arc;
    /// # fn example(registry: Arc<StorageRegistry>) {
    /// let cache = TableCache::new()
    ///     .with_storage_registry(registry);
    /// # }
    /// ```
    pub fn with_storage_registry(mut self, registry: Arc<StorageRegistry>) -> Self {
        self.storage_registry = Some(registry);
        self
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

    /// Get storage path template for a table (T187)
    ///
    /// Returns partially-resolved path template with static placeholders substituted
    /// but dynamic placeholders ({userId}, {shard}) remaining for per-request substitution.
    ///
    /// **Cache-first lookup**: Checks cache before resolving via StorageRegistry
    ///
    /// # Arguments
    /// * `namespace` - Namespace ID
    /// * `table_name` - Table name
    ///
    /// # Returns
    /// * `Ok(String)` - Partial path template (e.g., "/data/storage/my_ns/messages/{userId}/")
    /// * `Err(KalamDbError)` - If table not found or resolution fails
    ///
    /// # Example
    /// ```no_run
    /// # use kalamdb_core::catalog::TableCache;
    /// # use kalamdb_commons::models::{NamespaceId, TableName};
    /// # fn example(cache: &TableCache) -> Result<(), Box<dyn std::error::Error>> {
    /// let template = cache.get_storage_path(
    ///     &NamespaceId::new("my_ns"),
    ///     &TableName::new("messages")
    /// )?;
    /// // Returns: "/data/storage/my_ns/messages/{userId}/"
    /// // Caller substitutes {userId} per-request
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_storage_path(
        &self,
        namespace: &NamespaceId,
        table_name: &TableName,
    ) -> Result<String, KalamDbError> {
        let key = (namespace.clone(), table_name.clone());

        // 1. Check cache first
        {
            let templates = self.storage_path_templates.read().unwrap();
            if let Some(template) = templates.get(&key) {
                return Ok(template.clone());
            }
        }

        // 2. Cache miss - load table metadata
        let table = self.get(namespace, table_name).ok_or_else(|| {
            // Enhanced error message for debugging (Phase 9)
            let tables_lock = self.tables.read().unwrap();
            let cached_tables: Vec<String> = tables_lock
                .keys()
                .map(|(ns, tbl)| format!("{}.{}", ns.as_str(), tbl.as_str()))
                .collect();
            
            let debug_info = if cached_tables.is_empty() {
                "Cache is empty (no tables loaded)".to_string()
            } else if cached_tables.len() <= 10 {
                format!("Tables in cache: [{}]", cached_tables.join(", "))
            } else {
                format!(
                    "Cache contains {} tables. First 10: [{}]",
                    cached_tables.len(),
                    cached_tables.iter().take(10).cloned().collect::<Vec<_>>().join(", ")
                )
            };
            
            KalamDbError::Other(format!(
                "Table not found in cache: {}.{}. {}",
                namespace.as_str(),
                table_name.as_str(),
                debug_info
            ))
        })?;

        // 3. Resolve partial template via StorageRegistry
        let partial_template = self.resolve_partial_template(&table)?;

        // 4. Cache the partial template
        {
            let mut templates = self.storage_path_templates.write().unwrap();
            templates.insert(key, partial_template.clone());
        }

        Ok(partial_template)
    }

    /// Resolve partial storage path template (T188)
    ///
    /// Substitutes STATIC placeholders only: {namespace}, {tableName}
    /// Leaves DYNAMIC placeholders: {userId}, {shard}
    ///
    /// This is a private helper called by get_storage_path() on cache miss.
    ///
    /// # Arguments
    /// * `table` - Table metadata containing storage_id and table_type
    ///
    /// # Returns
    /// * `Ok(String)` - Partial template path
    /// * `Err(KalamDbError)` - If storage config not found or resolution fails
    fn resolve_partial_template(&self, table: &TableMetadata) -> Result<String, KalamDbError> {
        // Get storage registry (required for resolution)
        let registry = self.storage_registry.as_ref().ok_or_else(|| {
            KalamDbError::Other(
                "StorageRegistry not set on TableCache - call with_storage_registry()".to_string(),
            )
        })?;

        // Extract storage_id from table metadata
    let storage_id = table.storage_id.clone().unwrap_or_else(|| StorageId::new("local"));
    
    // Query storage config from system.storages
    let storage_config = registry
        .get_storage_config(storage_id.as_str())?
        .ok_or_else(|| {
            KalamDbError::Other(format!("Storage config not found: {}", storage_id))
        })?;

    // Select template based on table type
        let template = match table.table_type {
            TableType::User => &storage_config.user_tables_template,
            TableType::Shared | TableType::System => &storage_config.shared_tables_template,
            TableType::Stream => {
                // Stream tables don't use Parquet storage
                return Ok(String::new());
            }
        };

        // Substitute STATIC placeholders only
        let partial_path = template
            .replace("{namespace}", table.namespace.as_str())
            .replace("{tableName}", table.table_name.as_str());

        // Build full path: <base_directory>/<partial_template>/
        let full_path = if storage_config.base_directory.is_empty() {
            format!("/{}/", partial_path.trim_matches('/'))
        } else {
            format!(
                "{}/{}/",
                storage_config.base_directory.trim_end_matches('/'),
                partial_path.trim_matches('/')
            )
        };

        Ok(full_path)
    }

    /// Invalidate cached storage path templates (T189)
    ///
    /// Call this when a table schema changes (ALTER TABLE) or storage config changes
    /// to ensure the cache doesn't serve stale paths.
    ///
    /// # Example
    /// ```no_run
    /// # use kalamdb_core::catalog::TableCache;
    /// # fn example(cache: &TableCache) {
    /// // After ALTER TABLE or storage config change
    /// cache.invalidate_storage_paths();
    /// # }
    /// ```
    pub fn invalidate_storage_paths(&self) {
        let mut templates = self.storage_path_templates.write().unwrap();
        templates.clear();
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
    use kalamdb_commons::models::StorageId;

    fn create_test_table(namespace: &str, table_name: &str) -> TableMetadata {
        TableMetadata {
            table_name: TableName::new(table_name),
            table_type: TableType::User,
            namespace: NamespaceId::new(namespace),
            created_at: Utc::now(),
            storage_id: Some(StorageId::new("local")),
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

    // T190: Tests for storage path resolution

    #[test]
    fn test_get_storage_path_without_registry() {
        let cache = TableCache::new();
        let table = create_test_table("ns1", "table1");
        cache.insert(table);

        // Should fail without storage registry
        let result = cache.get_storage_path(&NamespaceId::new("ns1"), &TableName::new("table1"));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("StorageRegistry not set"));
    }

    #[test]
    fn test_invalidate_storage_paths() {
        let cache = TableCache::new();

        // Manually insert a cached template
        {
            let mut templates = cache.storage_path_templates.write().unwrap();
            templates.insert(
                (NamespaceId::new("ns1"), TableName::new("table1")),
                "/data/storage/ns1/table1/{userId}/".to_string(),
            );
        }

        // Verify cache has 1 entry
        {
            let templates = cache.storage_path_templates.read().unwrap();
            assert_eq!(templates.len(), 1);
        }

        // Invalidate
        cache.invalidate_storage_paths();

        // Verify cache is empty
        {
            let templates = cache.storage_path_templates.read().unwrap();
            assert_eq!(templates.len(), 0);
        }
    }

    #[test]
    fn test_path_template_cache_basic() {
        // This test verifies the cache structure works
        // Full integration test with StorageRegistry requires database setup
        let cache = TableCache::new();

        // Manually simulate cached template
        let key = (NamespaceId::new("test_ns"), TableName::new("test_table"));
        let template = "/data/storage/test_ns/test_table/{userId}/".to_string();

        {
            let mut templates = cache.storage_path_templates.write().unwrap();
            templates.insert(key.clone(), template.clone());
        }

        // Verify retrieval
        {
            let templates = cache.storage_path_templates.read().unwrap();
            let retrieved = templates.get(&key);
            assert_eq!(retrieved, Some(&template));
        }
    }
}
