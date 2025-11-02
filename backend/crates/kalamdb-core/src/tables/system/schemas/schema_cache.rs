//! High-performance schema cache using DashMap for lock-free concurrent access.
//!
//! This module provides an LRU-eviction cache for TableDefinition objects,
//! achieving >99% hit rate for schema lookups with sub-100μs latency.
//!
//! ## Architecture
//!
//! - **DashMap**: Lock-free concurrent HashMap for O(1) reads/writes
//! - **LRU Eviction**: When cache exceeds max_size, evict least recently used
//! - **No Locks**: Multiple threads can read simultaneously without contention
//!
//! ## Performance Goals
//!
//! - Cache hit: <100μs (in-memory lookup)
//! - Cache miss: <5ms (RocksDB read + cache insert)
//! - Hit rate: >99% for typical workloads
//!
//! ## Usage
//!
//! ```rust,ignore
//! use kalamdb_core::tables::system::schemas::SchemaCache;
//!
//! let cache = SchemaCache::new(1000); // Max 1000 cached schemas
//!
//! // Check cache first
//! if let Some(schema) = cache.get(&table_id) {
//!     // Cache hit - use schema
//! } else {
//!     // Cache miss - read from EntityStore
//!     let schema = store.get(&table_id)?.unwrap();
//!     cache.insert(table_id, schema);
//! }
//!
//! // Invalidate on ALTER TABLE
//! cache.invalidate(&table_id);
//! ```

use dashmap::DashMap;
use kalamdb_commons::models::TableId;
use kalamdb_commons::schemas::TableDefinition;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// High-performance schema cache with LRU eviction.
///
/// This cache uses DashMap for lock-free concurrent access and maintains
/// LRU ordering for eviction when the cache exceeds max_size.
///
/// ## Thread Safety
/// - Multiple threads can read simultaneously (lock-free)
/// - Writes use fine-grained locking per entry
/// - No global locks that block all operations
///
/// ## Metrics
/// - `hit_count`: Number of cache hits
/// - `miss_count`: Number of cache misses
/// - `eviction_count`: Number of LRU evictions
pub struct SchemaCache {
    /// Main cache storage (TableId → TableDefinition)
    cache: DashMap<TableId, Arc<TableDefinition>>,

    /// Access timestamps for LRU eviction (TableId → access_time)
    access_times: DashMap<TableId, u64>,

    /// Monotonic access counter for LRU ordering
    access_counter: AtomicUsize,

    /// Maximum number of entries before eviction
    max_size: usize,

    /// Cache hit count (for metrics)
    hit_count: AtomicUsize,

    /// Cache miss count (for metrics)
    miss_count: AtomicUsize,

    /// Eviction count (for metrics)
    eviction_count: AtomicUsize,
}

impl SchemaCache {
    /// Creates a new SchemaCache with the given maximum size.
    ///
    /// # Arguments
    /// * `max_size` - Maximum number of schemas to cache before LRU eviction
    ///
    /// # Returns
    /// A new SchemaCache instance
    ///
    /// # Example
    /// ```rust,ignore
    /// let cache = SchemaCache::new(1000); // Cache up to 1000 schemas
    /// ```
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: DashMap::new(),
            access_times: DashMap::new(),
            access_counter: AtomicUsize::new(0),
            max_size,
            hit_count: AtomicUsize::new(0),
            miss_count: AtomicUsize::new(0),
            eviction_count: AtomicUsize::new(0),
        }
    }

    /// Retrieves a schema from the cache.
    ///
    /// Updates access time for LRU ordering on cache hit.
    ///
    /// # Arguments
    /// * `table_id` - Table identifier
    ///
    /// # Returns
    /// - `Some(Arc<TableDefinition>)` if found in cache
    /// - `None` if not cached
    ///
    /// # Example
    /// ```rust,ignore
    /// if let Some(schema) = cache.get(&table_id) {
    ///     println!("Cache hit! Schema version: {}", schema.schema_version());
    /// }
    /// ```
    pub fn get(&self, table_id: &TableId) -> Option<Arc<TableDefinition>> {
        if let Some(entry) = self.cache.get(table_id) {
            // Update access time for LRU
            let access_time = self.access_counter.fetch_add(1, Ordering::Relaxed);
            self.access_times
                .insert(table_id.clone(), access_time as u64);

            // Increment hit counter
            self.hit_count.fetch_add(1, Ordering::Relaxed);

            Some(entry.value().clone())
        } else {
            // Increment miss counter
            self.miss_count.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// Inserts a schema into the cache.
    ///
    /// Performs LRU eviction if cache is at max_size.
    ///
    /// # Arguments
    /// * `table_id` - Table identifier
    /// * `schema` - TableDefinition to cache
    ///
    /// # Example
    /// ```rust,ignore
    /// let schema = store.get(&table_id)?.unwrap();
    /// cache.insert(table_id, schema);
    /// ```
    pub fn insert(&self, table_id: TableId, schema: TableDefinition) {
        // Check if eviction is needed
        if self.cache.len() >= self.max_size {
            self.evict_lru();
        }

        // Insert schema and update access time
        let access_time = self.access_counter.fetch_add(1, Ordering::Relaxed);
        self.cache.insert(table_id.clone(), Arc::new(schema));
        self.access_times.insert(table_id, access_time as u64);
    }

    /// Invalidates a schema in the cache.
    ///
    /// Call this when a schema is modified (ALTER TABLE) to ensure
    /// the cache doesn't serve stale data.
    ///
    /// # Arguments
    /// * `table_id` - Table identifier to invalidate
    ///
    /// # Example
    /// ```rust,ignore
    /// // After ALTER TABLE
    /// cache.invalidate(&table_id);
    /// ```
    pub fn invalidate(&self, table_id: &TableId) {
        self.cache.remove(table_id);
        self.access_times.remove(table_id);
    }

    /// Clears all entries from the cache.
    ///
    /// Useful for testing or when resetting the database.
    pub fn clear(&self) {
        self.cache.clear();
        self.access_times.clear();
        self.access_counter.store(0, Ordering::Relaxed);
    }

    /// Returns the current cache size.
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Returns true if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Returns cache hit rate as a percentage (0.0 to 1.0).
    ///
    /// # Returns
    /// - Hit rate: hits / (hits + misses)
    /// - Returns 0.0 if no accesses yet
    pub fn hit_rate(&self) -> f64 {
        let hits = self.hit_count.load(Ordering::Relaxed);
        let misses = self.miss_count.load(Ordering::Relaxed);
        let total = hits + misses;

        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    /// Returns cache statistics for monitoring.
    ///
    /// # Returns
    /// Tuple of (hit_count, miss_count, eviction_count, cache_size)
    pub fn stats(&self) -> (usize, usize, usize, usize) {
        (
            self.hit_count.load(Ordering::Relaxed),
            self.miss_count.load(Ordering::Relaxed),
            self.eviction_count.load(Ordering::Relaxed),
            self.len(),
        )
    }

    /// Evicts the least recently used entry from the cache.
    ///
    /// This is called automatically when the cache exceeds max_size.
    fn evict_lru(&self) {
        // Find entry with smallest access_time
        let oldest = self
            .access_times
            .iter()
            .min_by_key(|entry| *entry.value())
            .map(|entry| entry.key().clone());

        if let Some(table_id) = oldest {
            self.cache.remove(&table_id);
            self.access_times.remove(&table_id);
            self.eviction_count.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::schemas::{ColumnDefinition, TableDefinition, TableOptions, TableType};
    use kalamdb_commons::types::KalamDataType;
    use kalamdb_commons::{NamespaceId, TableName};

    fn create_test_schema(table_name: &str) -> TableDefinition {
        let columns = vec![
            ColumnDefinition::new(
                "id",
                1,
                KalamDataType::Text,
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
                true,
                false,
                false,
                kalamdb_commons::schemas::ColumnDefault::None,
                None,
            ),
        ];
        TableDefinition::new(
            NamespaceId::new("default"),
            TableName::new(table_name),
            TableType::User,
            columns,
            TableOptions::user(),
            None,
        )
        .expect("Failed to create table definition")
    }

    #[test]
    fn test_cache_basic_operations() {
        let cache = SchemaCache::new(10);

        let table_id = TableId::from_strings("default", "users");
        let schema = create_test_schema("users");

        // Cache miss
        assert!(cache.get(&table_id).is_none());
        assert_eq!(cache.len(), 0);

        // Insert
        cache.insert(table_id.clone(), schema);
        assert_eq!(cache.len(), 1);

        // Cache hit
        let cached = cache.get(&table_id);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().table_type, TableType::User);

        // Invalidate
        cache.invalidate(&table_id);
        assert_eq!(cache.len(), 0);
        assert!(cache.get(&table_id).is_none());
    }

    #[test]
    fn test_cache_lru_eviction() {
        let cache = SchemaCache::new(3); // Max 3 entries

        let table1 = TableId::from_strings("default", "table1");
        let table2 = TableId::from_strings("default", "table2");
        let table3 = TableId::from_strings("default", "table3");
        let table4 = TableId::from_strings("default", "table4");

        // Fill cache to capacity
        cache.insert(table1.clone(), create_test_schema("table1"));
        cache.insert(table2.clone(), create_test_schema("table2"));
        cache.insert(table3.clone(), create_test_schema("table3"));
        assert_eq!(cache.len(), 3);

        // Access table2 and table3 (making table1 LRU)
        cache.get(&table2);
        cache.get(&table3);

        // Insert table4 - should evict table1 (LRU)
        cache.insert(table4.clone(), create_test_schema("table4"));

        assert_eq!(cache.len(), 3);
        assert!(cache.get(&table1).is_none(), "table1 should be evicted");
        assert!(
            cache.get(&table2).is_some(),
            "table2 should still be cached"
        );
        assert!(
            cache.get(&table3).is_some(),
            "table3 should still be cached"
        );
        assert!(cache.get(&table4).is_some(), "table4 should be cached");
    }

    #[test]
    fn test_cache_metrics() {
        let cache = SchemaCache::new(10);

        let table1 = TableId::from_strings("default", "table1");
        let schema = create_test_schema("table1");

        // Initial state
        assert_eq!(cache.hit_rate(), 0.0);

        // Cache miss
        cache.get(&table1);
        assert_eq!(cache.hit_rate(), 0.0); // 0 hits / 1 total

        // Insert and hit
        cache.insert(table1.clone(), schema);
        cache.get(&table1); // Hit
        cache.get(&table1); // Hit

        // Hit rate: 2 hits / 3 total = 0.666...
        let hit_rate = cache.hit_rate();
        assert!(hit_rate > 0.6 && hit_rate < 0.7, "Hit rate should be ~66%");

        // Check stats
        let (hits, misses, evictions, size) = cache.stats();
        assert_eq!(hits, 2);
        assert_eq!(misses, 1);
        assert_eq!(evictions, 0);
        assert_eq!(size, 1);
    }

    #[test]
    fn test_cache_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let cache = Arc::new(SchemaCache::new(100));
        let table_id = TableId::from_strings("default", "users");
        let schema = create_test_schema("users");

        cache.insert(table_id.clone(), schema);

        // Spawn 10 threads to read concurrently
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let cache_clone = Arc::clone(&cache);
                let table_id_clone = table_id.clone();

                thread::spawn(move || {
                    for _ in 0..100 {
                        let result = cache_clone.get(&table_id_clone);
                        assert!(result.is_some());
                    }
                })
            })
            .collect();

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // All 1000 accesses should be hits
        let (hits, misses, _, _) = cache.stats();
        assert_eq!(hits, 1000);
        assert_eq!(misses, 0);
        assert_eq!(cache.hit_rate(), 1.0);
    }

    #[test]
    fn test_cache_clear() {
        let cache = SchemaCache::new(10);

        let table1 = TableId::from_strings("default", "table1");
        let table2 = TableId::from_strings("default", "table2");

        cache.insert(table1.clone(), create_test_schema("table1"));
        cache.insert(table2.clone(), create_test_schema("table2"));

        assert_eq!(cache.len(), 2);

        cache.clear();

        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
        assert!(cache.get(&table1).is_none());
        assert!(cache.get(&table2).is_none());
    }
}
