//! Unified schema cache for table metadata and schemas
//!
//! **Phase 10: Cache Consolidation** - Replaces dual-cache architecture (TableCache + SchemaCache)
//! with single unified cache to eliminate ~50% memory waste and synchronization complexity.
//!
//! **Architecture**:
//! - Single DashMap<TableId, Arc<CachedTableData>> for all table metadata + schemas
//! - LRU eviction policy (configurable max_size)
//! - Lock-free concurrent access via DashMap
//! - Arc-based cloning for cheap shared access
//!
//! **Key Benefits**:
//! - ~50% memory reduction (eliminates duplicate caching)
//! - Single source of truth (eliminates sync bugs)
//! - Simpler API (one cache instead of two)
//! - Better performance (single lookup instead of potentially two)

use crate::error::KalamDbError;
use crate::flush::FlushPolicy;
use crate::storage::StorageRegistry;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use kalamdb_commons::models::schemas::TableDefinition;
use kalamdb_commons::models::{NamespaceId, StorageId, TableId, TableName, UserId};
use kalamdb_commons::schemas::TableType;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Cached table data containing all metadata and schema information
///
/// This struct consolidates data previously split between TableCache (path resolution)
/// and SchemaCache (schema queries) to eliminate duplication.
///
/// **Performance Note**: `last_accessed` timestamp is stored separately in SchemaCache
/// to avoid cloning this entire struct on every cache access.
#[derive(Debug, Clone)]
pub struct CachedTableData {
    /// Composite table identifier (contains namespace + table_name)
    pub table_id: TableId,

    /// Type of table (User, Shared, System, Stream)
    pub table_type: TableType,

    /// When the table was created
    pub created_at: DateTime<Utc>,

    /// Reference to storage configuration in system.storages
    pub storage_id: Option<StorageId>,

    /// When to flush buffered data to Parquet
    pub flush_policy: FlushPolicy,

    /// Partially-resolved storage path template
    /// Static placeholders substituted ({namespace}, {tableName}), dynamic ones remain ({userId}, {shard})
    pub storage_path_template: String,

    /// Current schema version number
    pub schema_version: u32,

    /// How long to keep deleted rows (in hours)
    pub deleted_retention_hours: Option<u32>,

    /// Full schema definition with all columns
    pub schema: Arc<TableDefinition>,
}

impl CachedTableData {
    /// Create new cached table data
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        table_id: TableId,
        table_type: TableType,
        created_at: DateTime<Utc>,
        storage_id: Option<StorageId>,
        flush_policy: FlushPolicy,
        storage_path_template: String,
        schema_version: u32,
        deleted_retention_hours: Option<u32>,
        schema: Arc<TableDefinition>,
    ) -> Self {
        Self {
            table_id,
            table_type,
            created_at,
            storage_id,
            flush_policy,
            storage_path_template,
            schema_version,
            deleted_retention_hours,
            schema,
        }
    }
}

/// Unified schema cache for table metadata and schemas
///
/// Replaces dual-cache architecture with single DashMap for all table data.
///
/// **Performance Optimization**: LRU timestamps are stored separately to avoid
/// cloning large CachedTableData structs on every access.
pub struct SchemaCache {
    /// Cached table data indexed by TableId
    cache: DashMap<TableId, Arc<CachedTableData>>,

    /// LRU timestamps indexed by TableId (separate to avoid cloning CachedTableData)
    lru_timestamps: DashMap<TableId, AtomicU64>,

    /// Maximum number of entries before LRU eviction
    max_size: usize,

    /// Storage registry for resolving path templates
    storage_registry: Option<Arc<StorageRegistry>>,

    /// Cache hit count (for metrics)
    hits: AtomicU64,

    /// Cache miss count (for metrics)
    misses: AtomicU64,
}

impl SchemaCache {
    /// Create a new schema cache with specified maximum size
    ///
    /// # Arguments
    /// * `max_size` - Maximum number of table entries before LRU eviction (0 = unlimited)
    /// * `storage_registry` - Optional storage registry for path template resolution
    ///
    /// # Example
    /// ```no_run
    /// use kalamdb_core::catalog::SchemaCache;
    /// use kalamdb_core::storage::StorageRegistry;
    /// use std::sync::Arc;
    ///
    /// let registry = Arc::new(StorageRegistry::new());
    /// let cache = SchemaCache::new(10000, Some(registry));
    /// ```
    pub fn new(max_size: usize, storage_registry: Option<Arc<StorageRegistry>>) -> Self {
        Self {
            cache: DashMap::new(),
            lru_timestamps: DashMap::new(),
            max_size,
            storage_registry,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// Get current Unix timestamp in milliseconds
    fn current_timestamp() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    /// Get cached table data by TableId
    ///
    /// Updates LRU timestamp on cache hit.
    ///
    /// # Arguments
    /// * `table_id` - Composite table identifier
    ///
    /// # Returns
    /// `Some(Arc<CachedTableData>)` if found, `None` otherwise
    ///
    /// # Example
    /// ```no_run
    /// # use kalamdb_core::catalog::SchemaCache;
    /// # use kalamdb_commons::models::{TableId, NamespaceId, TableName};
    /// # let cache = SchemaCache::new(1000, None);
    /// let table_id = TableId::new(
    ///     NamespaceId::new("my_namespace"),
    ///     TableName::new("my_table")
    /// );
    /// if let Some(data) = cache.get(&table_id) {
    ///     println!("Found table: {:?}", data.schema);
    /// }
    /// ```
    pub fn get(&self, table_id: &TableId) -> Option<Arc<CachedTableData>> {
        if let Some(entry) = self.cache.get(table_id) {
            self.hits.fetch_add(1, Ordering::Relaxed);
            
            // Update LRU timestamp in separate map (avoids cloning CachedTableData)
            if let Some(timestamp) = self.lru_timestamps.get(table_id) {
                timestamp.store(Self::current_timestamp(), Ordering::Relaxed);
            }
            
            // Return Arc clone (cheap - just increments reference count)
            Some(Arc::clone(entry.value()))
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// Get cached table data by namespace and table name
    ///
    /// Convenience method that creates TableId internally.
    ///
    /// # Arguments
    /// * `namespace` - Namespace identifier
    /// * `table_name` - Table name
    ///
    /// # Returns
    /// `Some(Arc<CachedTableData>)` if found, `None` otherwise
    ///
    /// # Example
    /// ```no_run
    /// # use kalamdb_core::catalog::SchemaCache;
    /// # use kalamdb_commons::models::{NamespaceId, TableName};
    /// # let cache = SchemaCache::new(1000, None);
    /// if let Some(data) = cache.get_by_name(
    ///     &NamespaceId::new("my_namespace"),
    ///     &TableName::new("my_table")
    /// ) {
    ///     println!("Found table type: {:?}", data.table_type);
    /// }
    /// ```
    pub fn get_by_name(
        &self,
        namespace: &NamespaceId,
        table_name: &TableName,
    ) -> Option<Arc<CachedTableData>> {
        let table_id = TableId::new(namespace.clone(), table_name.clone());
        self.get(&table_id)
    }

    /// Insert or update cached table data
    ///
    /// Performs LRU eviction if max_size is exceeded.
    ///
    /// # Arguments
    /// * `table_id` - Composite table identifier
    /// * `data` - Table data to cache
    ///
    /// # Example
    /// ```no_run
    /// # use kalamdb_core::catalog::{SchemaCache, CachedTableData};
    /// # use kalamdb_commons::models::{TableId, NamespaceId, TableName};
    /// # use kalamdb_commons::schemas::{TableType, TableDefinition};
    /// # use kalamdb_core::flush::FlushPolicy;
    /// # use chrono::Utc;
    /// # use std::sync::Arc;
    /// # let cache = SchemaCache::new(1000, None);
    /// # let table_id = TableId::new(NamespaceId::new("ns"), TableName::new("tbl"));
    /// # let schema = Arc::new(TableDefinition::default());
    /// let data = CachedTableData::new(
    ///     table_id.clone(),
    ///     TableType::User,
    ///     Utc::now(),
    ///     None,
    ///     FlushPolicy::default(),
    ///     "/data/{namespace}/{tableName}/".to_string(),
    ///     1,
    ///     None,
    ///     schema,
    /// );
    /// cache.insert(table_id, Arc::new(data));
    /// ```
    pub fn insert(&self, table_id: TableId, data: Arc<CachedTableData>) {
        // Check if we need to evict before inserting
        if self.max_size > 0 && self.cache.len() >= self.max_size {
            self.evict_lru();
        }

        // Insert data and initialize LRU timestamp
        self.lru_timestamps.insert(table_id.clone(), AtomicU64::new(Self::current_timestamp()));
        self.cache.insert(table_id, data);
    }

    /// Invalidate (remove) cached table data
    ///
    /// Used when table is dropped or altered.
    ///
    /// # Arguments
    /// * `table_id` - Composite table identifier
    ///
    /// # Example
    /// ```no_run
    /// # use kalamdb_core::catalog::SchemaCache;
    /// # use kalamdb_commons::models::{TableId, NamespaceId, TableName};
    /// # let cache = SchemaCache::new(1000, None);
    /// let table_id = TableId::new(
    ///     NamespaceId::new("my_namespace"),
    ///     TableName::new("my_table")
    /// );
    /// cache.invalidate(&table_id);
    /// ```
    pub fn invalidate(&self, table_id: &TableId) {
        self.cache.remove(table_id);
        self.lru_timestamps.remove(table_id);
    }

    /// Evict least-recently-used entry from cache
    ///
    /// Scans LRU timestamps to find oldest entry.
    /// This is O(n) but only called when max_size is exceeded.
    fn evict_lru(&self) {
        let mut oldest_key: Option<TableId> = None;
        let mut oldest_timestamp = u64::MAX;

        // Find entry with oldest last_accessed timestamp
        for entry in self.lru_timestamps.iter() {
            let timestamp = entry.value().load(Ordering::Relaxed);
            if timestamp < oldest_timestamp {
                oldest_timestamp = timestamp;
                oldest_key = Some(entry.key().clone());
            }
        }

        // Remove oldest entry from both maps
        if let Some(key) = oldest_key {
            self.cache.remove(&key);
            self.lru_timestamps.remove(&key);
        }
    }

    /// Resolve storage path with dynamic placeholders substituted
    ///
    /// Takes partially-resolved template from CachedTableData and substitutes
    /// dynamic placeholders ({userId}, {shard}) for final path.
    ///
    /// # Arguments
    /// * `table_id` - Composite table identifier
    /// * `user_id` - User ID for {userId} placeholder (optional)
    /// * `shard` - Shard number for {shard} placeholder (optional)
    ///
    /// # Returns
    /// `Ok(String)` with fully-resolved path, or `Err(KalamDbError)` if table not found
    ///
    /// # Example
    /// ```no_run
    /// # use kalamdb_core::catalog::SchemaCache;
    /// # use kalamdb_commons::models::{TableId, NamespaceId, TableName, UserId};
    /// # let cache = SchemaCache::new(1000, None);
    /// # let table_id = TableId::new(NamespaceId::new("ns"), TableName::new("tbl"));
    /// let path = cache.get_storage_path(
    ///     &table_id,
    ///     Some(&UserId::new("alice")),
    ///     Some(0)
    /// );
    /// // Returns: "/data/ns/tbl/alice/shard_0/"
    /// ```
    pub fn get_storage_path(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
        shard: Option<u32>,
    ) -> Result<String, KalamDbError> {
        let data = self.get(table_id).ok_or_else(|| {
            KalamDbError::TableNotFound(format!(
                "Table not found in cache: {}:{}",
                table_id.namespace_id().as_str(),
                table_id.table_name().as_str()
            ))
        })?;

        let mut path = data.storage_path_template.clone();

        // Substitute {userId} placeholder
        if let Some(uid) = user_id {
            path = path.replace("{userId}", uid.as_str());
        }

        // Substitute {shard} placeholder
        if let Some(shard_num) = shard {
            path = path.replace("{shard}", &format!("shard_{}", shard_num));
        }

        Ok(path)
    }

    /// Get cache hit rate (for metrics)
    ///
    /// # Returns
    /// Hit rate as percentage (0.0 to 1.0)
    pub fn hit_rate(&self) -> f64 {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total = hits + misses;

        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    /// Get cache statistics
    ///
    /// # Returns
    /// Tuple of (size, hits, misses, hit_rate)
    pub fn stats(&self) -> (usize, u64, u64, f64) {
        let size = self.cache.len();
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let hit_rate = self.hit_rate();

        (size, hits, misses, hit_rate)
    }

    /// Clear all cached data
    ///
    /// Useful for testing or manual cache invalidation.
    pub fn clear(&self) {
        self.cache.clear();
        self.lru_timestamps.clear();
        self.hits.store(0, Ordering::Relaxed);
        self.misses.store(0, Ordering::Relaxed);
    }

    /// Get number of cached entries
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

impl Default for SchemaCache {
    fn default() -> Self {
        Self::new(10000, None) // Default max size: 10,000 tables
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::models::schemas::{ColumnDefinition, TableDefinition};
    use kalamdb_commons::models::types::KalamDataType;
    use std::thread;
    use std::time::Duration;

    fn create_test_schema() -> Arc<TableDefinition> {
        use kalamdb_commons::schemas::{ColumnDefault, TableOptions};
        
        Arc::new(
            TableDefinition::new(
                NamespaceId::new("test_ns"),
                TableName::new("test_table"),
                TableType::User,
                vec![ColumnDefinition::new(
                    "id".to_string(),
                    1, // ordinal_position
                    KalamDataType::Int,
                    false, // nullable
                    false, // is_auto_increment
                    false, // is_unique
                    ColumnDefault::None,
                    None, // comment
                )],
                TableOptions::user(), // Default user table options
                None, // table_comment
            )
            .expect("Failed to create test schema"),
        )
    }

    fn create_test_data(table_id: TableId) -> Arc<CachedTableData> {
        // Create partially-resolved template with {namespace} and {tableName} substituted
        let storage_path_template = format!(
            "/data/{}/{}/{{userId}}/{{shard}}/",
            table_id.namespace_id().as_str(),
            table_id.table_name().as_str()
        );
        
        Arc::new(CachedTableData::new(
            table_id,
            TableType::User,
            Utc::now(),
            Some(StorageId::new("local")),
            FlushPolicy::default(),
            storage_path_template,
            1,
            Some(24),
            create_test_schema(),
        ))
    }

    #[test]
    fn test_insert_and_get() {
        let cache = SchemaCache::new(1000, None);
        let table_id = TableId::new(NamespaceId::new("ns1"), TableName::new("table1"));
        let data = create_test_data(table_id.clone());

        cache.insert(table_id.clone(), data.clone());

        let retrieved = cache.get(&table_id).expect("Should find table");
        assert_eq!(retrieved.table_id, table_id);
        assert_eq!(retrieved.table_type, TableType::User);
        assert_eq!(retrieved.schema_version, 1);
    }

    #[test]
    fn test_get_by_name() {
        let cache = SchemaCache::new(1000, None);
        let namespace = NamespaceId::new("ns1");
        let table_name = TableName::new("table1");
        let table_id = TableId::new(namespace.clone(), table_name.clone());
        let data = create_test_data(table_id.clone());

        cache.insert(table_id, data);

        let retrieved = cache
            .get_by_name(&namespace, &table_name)
            .expect("Should find table");
        assert_eq!(retrieved.table_type, TableType::User);
    }

    #[test]
    fn test_lru_eviction() {
        let cache = SchemaCache::new(3, None); // Small cache for testing

        // Insert 3 tables
        let table1 = TableId::new(NamespaceId::new("ns1"), TableName::new("table1"));
        let table2 = TableId::new(NamespaceId::new("ns1"), TableName::new("table2"));
        let table3 = TableId::new(NamespaceId::new("ns1"), TableName::new("table3"));

        cache.insert(table1.clone(), create_test_data(table1.clone()));
        thread::sleep(Duration::from_millis(10));

        cache.insert(table2.clone(), create_test_data(table2.clone()));
        thread::sleep(Duration::from_millis(10));

        cache.insert(table3.clone(), create_test_data(table3.clone()));

        assert_eq!(cache.len(), 3);

        // Access table1 to update its LRU timestamp
        cache.get(&table1);

        // Insert 4th table - should evict table2 (oldest untouched)
        let table4 = TableId::new(NamespaceId::new("ns1"), TableName::new("table4"));
        cache.insert(table4.clone(), create_test_data(table4.clone()));

        assert_eq!(cache.len(), 3);
        assert!(cache.get(&table1).is_some());
        assert!(cache.get(&table2).is_none()); // Evicted
        assert!(cache.get(&table3).is_some());
        assert!(cache.get(&table4).is_some());
    }

    #[test]
    fn test_invalidate() {
        let cache = SchemaCache::new(1000, None);
        let table_id = TableId::new(NamespaceId::new("ns1"), TableName::new("table1"));
        let data = create_test_data(table_id.clone());

        cache.insert(table_id.clone(), data);
        assert!(cache.get(&table_id).is_some());

        cache.invalidate(&table_id);
        assert!(cache.get(&table_id).is_none());
    }

    #[test]
    fn test_storage_path_resolution() {
        let cache = SchemaCache::new(1000, None);
        let table_id = TableId::new(NamespaceId::new("my_ns"), TableName::new("messages"));
        let data = create_test_data(table_id.clone());

        cache.insert(table_id.clone(), data);

        let path = cache
            .get_storage_path(&table_id, Some(&UserId::new("alice")), Some(0))
            .expect("Should resolve path");

        assert_eq!(path, "/data/my_ns/messages/alice/shard_0/");
    }

    #[test]
    fn test_concurrent_access() {
        let cache = Arc::new(SchemaCache::new(1000, None));
        let table_id = TableId::new(NamespaceId::new("ns1"), TableName::new("table1"));
        let data = create_test_data(table_id.clone());

        cache.insert(table_id.clone(), data);

        let mut handles = vec![];

        // Spawn 10 threads to read concurrently
        for _ in 0..10 {
            let cache = Arc::clone(&cache);
            let table_id = table_id.clone();

            let handle = thread::spawn(move || {
                for _ in 0..100 {
                    let result = cache.get(&table_id);
                    assert!(result.is_some());
                }
            });

            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().expect("Thread panicked");
        }
    }

    #[test]
    fn test_metrics() {
        let cache = SchemaCache::new(1000, None);
        let table_id = TableId::new(NamespaceId::new("ns1"), TableName::new("table1"));
        let data = create_test_data(table_id.clone());

        cache.insert(table_id.clone(), data);

        // Generate hits and misses
        cache.get(&table_id); // Hit
        cache.get(&table_id); // Hit
        cache.get(&TableId::new(
            NamespaceId::new("ns1"),
            TableName::new("nonexistent"),
        )); // Miss

        let (size, hits, misses, hit_rate) = cache.stats();
        assert_eq!(size, 1);
        assert_eq!(hits, 2);
        assert_eq!(misses, 1);
        assert!((hit_rate - 0.666).abs() < 0.01); // ~66.7%
    }

    #[test]
    fn test_clear() {
        let cache = SchemaCache::new(1000, None);
        let table_id = TableId::new(NamespaceId::new("ns1"), TableName::new("table1"));
        let data = create_test_data(table_id.clone());

        cache.insert(table_id, data);
        assert_eq!(cache.len(), 1);

        cache.clear();
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.stats().1, 0); // hits reset
        assert_eq!(cache.stats().2, 0); // misses reset
    }
}
