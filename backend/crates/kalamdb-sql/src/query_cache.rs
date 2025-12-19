//! Query result cache for system table queries
//!
//! Caches results of frequently-accessed system table queries to reduce RocksDB reads.
//! Invalidated automatically on mutations to system tables.
//!
//! **Performance**: Uses moka cache with TinyLFU eviction for optimal hit rate,
//! Arc<[u8]> for zero-copy results, and automatic per-entry TTL expiration.

use moka::sync::Cache;
use moka::Expiry;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Cache key for query results
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum QueryCacheKey {
    /// scan_all_tables() result
    AllTables,
    /// scan_all_namespaces() result
    AllNamespaces,
    /// scan_all_live_queries() result
    AllLiveQueries,
    /// scan_all_storages() result
    AllStorages,
    /// scan_all_jobs() result
    AllJobs,
    /// get_table(table_id) result
    Table(String),
    /// get_namespace(namespace_id) result
    Namespace(String),
}

/// Cached query result (value only, TTL managed by moka)
#[derive(Debug, Clone)]
struct CachedResult {
    value: Arc<[u8]>, // Zero-copy shared result
}

impl CachedResult {
    fn new(value: Vec<u8>) -> Self {
        Self {
            value: value.into(), // Vec<u8> → Arc<[u8]>
        }
    }
}

/// TTL configuration for different query types
#[derive(Debug, Clone)]
pub struct QueryCacheTtlConfig {
    pub tables: Duration,
    pub namespaces: Duration,
    pub live_queries: Duration,
    pub storages: Duration,
    pub jobs: Duration,
    pub single_entity: Duration,
}

impl Default for QueryCacheTtlConfig {
    fn default() -> Self {
        Self {
            tables: Duration::from_secs(60),         // 60s for tables list
            namespaces: Duration::from_secs(60),     // 60s for namespaces list
            live_queries: Duration::from_secs(10),   // 10s for live queries (more dynamic)
            storages: Duration::from_secs(300),      // 5min for storages (rarely change)
            jobs: Duration::from_secs(30),           // 30s for jobs list
            single_entity: Duration::from_secs(120), // 2min for individual entities
        }
    }
}

/// Custom expiry policy for per-key TTL based on query type
struct QueryCacheExpiry {
    ttl_config: QueryCacheTtlConfig,
}

impl Expiry<QueryCacheKey, CachedResult> for QueryCacheExpiry {
    fn expire_after_create(
        &self,
        key: &QueryCacheKey,
        _value: &CachedResult,
        _current_time: Instant,
    ) -> Option<Duration> {
        Some(self.get_ttl(key))
    }

    fn expire_after_read(
        &self,
        _key: &QueryCacheKey,
        _value: &CachedResult,
        _current_time: Instant,
        _current_duration: Option<Duration>,
        _last_modified_at: Instant,
    ) -> Option<Duration> {
        // Don't extend TTL on read (TTL is fixed from creation)
        None
    }

    fn expire_after_update(
        &self,
        key: &QueryCacheKey,
        _value: &CachedResult,
        _current_time: Instant,
        _current_duration: Option<Duration>,
    ) -> Option<Duration> {
        // Reset TTL on update
        Some(self.get_ttl(key))
    }
}

impl QueryCacheExpiry {
    fn get_ttl(&self, key: &QueryCacheKey) -> Duration {
        match key {
            QueryCacheKey::AllTables => self.ttl_config.tables,
            QueryCacheKey::AllNamespaces => self.ttl_config.namespaces,
            QueryCacheKey::AllLiveQueries => self.ttl_config.live_queries,
            QueryCacheKey::AllStorages => self.ttl_config.storages,
            QueryCacheKey::AllJobs => self.ttl_config.jobs,
            QueryCacheKey::Table(_) | QueryCacheKey::Namespace(_) => self.ttl_config.single_entity,
        }
    }
}

/// Query result cache for system tables
///
/// Thread-safe cache with per-entry TTL expiration, TinyLFU eviction, and invalidation support.
/// Uses moka cache for high-performance concurrent access.
///
/// **Performance**:
/// - Lock-free reads: Multiple threads can read simultaneously without contention
/// - Zero-copy results: Arc<[u8]> allows sharing without cloning
/// - TinyLFU eviction: Automatically evicts entries using optimal LRU+LFU admission
/// - Per-entry TTL: Different TTLs for different query types
pub struct QueryCache {
    // Moka cache with per-entry expiration
    cache: Cache<QueryCacheKey, CachedResult>,
    // TTL configuration for get_ttl method
    ttl_config: QueryCacheTtlConfig,
}

impl QueryCache {
    /// Default maximum number of cached entries
    pub const DEFAULT_MAX_ENTRIES: u64 = 10_000;

    /// Create a new query cache with default TTL configuration and max entries
    pub fn new() -> Self {
        Self::with_config(QueryCacheTtlConfig::default())
    }

    /// Create a new query cache with custom TTL configuration
    pub fn with_config(ttl_config: QueryCacheTtlConfig) -> Self {
        Self::with_config_and_max_entries(ttl_config, Self::DEFAULT_MAX_ENTRIES)
    }

    /// Create a new query cache with custom TTL and max entries
    pub fn with_config_and_max_entries(
        ttl_config: QueryCacheTtlConfig,
        max_entries: u64,
    ) -> Self {
        let expiry = QueryCacheExpiry {
            ttl_config: ttl_config.clone(),
        };

        let cache = Cache::builder()
            .max_capacity(max_entries)
            .expire_after(expiry)
            .build();

        Self {
            cache,
            ttl_config,
        }
    }

    /// Get TTL for a specific query key (useful for debugging/stats)
    pub fn get_ttl(&self, key: &QueryCacheKey) -> Duration {
        match key {
            QueryCacheKey::AllTables => self.ttl_config.tables,
            QueryCacheKey::AllNamespaces => self.ttl_config.namespaces,
            QueryCacheKey::AllLiveQueries => self.ttl_config.live_queries,
            QueryCacheKey::AllStorages => self.ttl_config.storages,
            QueryCacheKey::AllJobs => self.ttl_config.jobs,
            QueryCacheKey::Table(_) | QueryCacheKey::Namespace(_) => self.ttl_config.single_entity,
        }
    }

    /// Get cached result
    ///
    /// Returns None if not in cache (moka handles expiration automatically).
    pub fn get<T: bincode::Decode<()>>(&self, key: &QueryCacheKey) -> Option<T> {
        if let Some(entry) = self.cache.get(key) {
            // Deserialize from bytes using bincode v2
            let config = bincode::config::standard();
            if let Ok((value, _)) = bincode::decode_from_slice(&entry.value, config) {
                return Some(value);
            }
        }
        None
    }

    /// Put result into cache (moka handles eviction automatically)
    pub fn put<T: bincode::Encode>(&self, key: QueryCacheKey, value: T) {
        // Serialize to bytes using bincode v2
        let config = bincode::config::standard();
        if let Ok(bytes) = bincode::encode_to_vec(&value, config) {
            self.cache.insert(key, CachedResult::new(bytes));
        }
    }

    /// Invalidate all tables-related queries
    pub fn invalidate_tables(&self) {
        self.cache.invalidate(&QueryCacheKey::AllTables);
        // Also remove individual table entries by iterating
        // Note: moka iterator returns Arc-wrapped keys
        let keys_to_remove: Vec<_> = self.cache.iter()
            .filter(|(k, _)| matches!(&**k, QueryCacheKey::Table(_)))
            .map(|(k, _)| (*k).clone())
            .collect();
        for key in keys_to_remove {
            self.cache.invalidate(&key);
        }
    }

    /// Invalidate all namespaces-related queries
    pub fn invalidate_namespaces(&self) {
        self.cache.invalidate(&QueryCacheKey::AllNamespaces);
        // Also remove individual namespace entries
        let keys_to_remove: Vec<_> = self.cache.iter()
            .filter(|(k, _)| matches!(&**k, QueryCacheKey::Namespace(_)))
            .map(|(k, _)| (*k).clone())
            .collect();
        for key in keys_to_remove {
            self.cache.invalidate(&key);
        }
    }

    /// Invalidate all live queries-related queries
    pub fn invalidate_live_queries(&self) {
        self.cache.invalidate(&QueryCacheKey::AllLiveQueries);
    }

    /// Invalidate all storages-related queries
    pub fn invalidate_storages(&self) {
        self.cache.invalidate(&QueryCacheKey::AllStorages);
    }

    /// Invalidate all jobs-related queries
    pub fn invalidate_jobs(&self) {
        self.cache.invalidate(&QueryCacheKey::AllJobs);
    }

    /// Invalidate a specific cached result
    pub fn invalidate(&self, key: &QueryCacheKey) {
        self.cache.invalidate(key);
    }

    /// Clear all cached results
    pub fn clear(&self) {
        self.cache.invalidate_all();
    }

    /// Remove expired entries (garbage collection)
    /// With moka, this triggers pending cleanup tasks
    pub fn evict_expired(&self) {
        self.cache.run_pending_tasks();
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        // Sync pending tasks for accurate count
        self.cache.run_pending_tasks();
        let total = self.cache.entry_count() as usize;

        // With moka, expired entries are automatically evicted
        // so all entries in cache are active
        CacheStats {
            total_entries: total,
            expired_entries: 0,
            active_entries: total,
        }
    }
}

impl Default for QueryCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub expired_entries: usize,
    pub active_entries: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
    struct TestData {
        id: String,
        value: i32,
    }

    #[test]
    fn test_cache_put_and_get() {
        let cache = QueryCache::new();
        let data = vec![
            TestData {
                id: "1".to_string(),
                value: 100,
            },
            TestData {
                id: "2".to_string(),
                value: 200,
            },
        ];

        cache.put(QueryCacheKey::AllTables, data.clone());

        let retrieved: Option<Vec<TestData>> = cache.get(&QueryCacheKey::AllTables);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), data);
    }

    #[test]
    fn test_cache_miss() {
        let cache = QueryCache::new();
        let retrieved: Option<Vec<TestData>> = cache.get(&QueryCacheKey::AllTables);
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_invalidate_tables() {
        let cache = QueryCache::new();
        let data = vec![TestData {
            id: "1".to_string(),
            value: 100,
        }];

        cache.put(QueryCacheKey::AllTables, data.clone());
        cache.put(QueryCacheKey::Table("users".to_string()), data.clone());
        cache.put(QueryCacheKey::AllNamespaces, data.clone());

        cache.invalidate_tables();

        let all_tables: Option<Vec<TestData>> = cache.get(&QueryCacheKey::AllTables);
        let single_table: Option<Vec<TestData>> =
            cache.get(&QueryCacheKey::Table("users".to_string()));
        let namespaces: Option<Vec<TestData>> = cache.get(&QueryCacheKey::AllNamespaces);

        assert!(all_tables.is_none());
        assert!(single_table.is_none());
        assert!(namespaces.is_some()); // Namespaces should still be cached
    }

    #[test]
    fn test_invalidate_namespaces() {
        let cache = QueryCache::new();
        let data = vec![TestData {
            id: "1".to_string(),
            value: 100,
        }];

        cache.put(QueryCacheKey::AllNamespaces, data.clone());
        cache.put(QueryCacheKey::Namespace("app1".to_string()), data.clone());
        cache.put(QueryCacheKey::AllTables, data.clone());

        cache.invalidate_namespaces();

        let all_namespaces: Option<Vec<TestData>> = cache.get(&QueryCacheKey::AllNamespaces);
        let single_namespace: Option<Vec<TestData>> =
            cache.get(&QueryCacheKey::Namespace("app1".to_string()));
        let tables: Option<Vec<TestData>> = cache.get(&QueryCacheKey::AllTables);

        assert!(all_namespaces.is_none());
        assert!(single_namespace.is_none());
        assert!(tables.is_some()); // Tables should still be cached
    }

    #[test]
    fn test_clear() {
        let cache = QueryCache::new();
        let data = vec![TestData {
            id: "1".to_string(),
            value: 100,
        }];

        cache.put(QueryCacheKey::AllTables, data.clone());
        cache.put(QueryCacheKey::AllNamespaces, data.clone());

        cache.clear();

        let tables: Option<Vec<TestData>> = cache.get(&QueryCacheKey::AllTables);
        let namespaces: Option<Vec<TestData>> = cache.get(&QueryCacheKey::AllNamespaces);

        assert!(tables.is_none());
        assert!(namespaces.is_none());
    }

    #[test]
    fn test_ttl_expiration() {
        let config = QueryCacheTtlConfig {
            tables: Duration::from_millis(50),
            namespaces: Duration::from_secs(60),
            live_queries: Duration::from_secs(10),
            storages: Duration::from_secs(300),
            jobs: Duration::from_secs(30),
            single_entity: Duration::from_secs(120),
        };

        let cache = QueryCache::with_config(config);
        let data = vec![TestData {
            id: "1".to_string(),
            value: 100,
        }];

        cache.put(QueryCacheKey::AllTables, data.clone());
        cache.put(QueryCacheKey::AllNamespaces, data.clone());

        // Wait for tables to expire
        std::thread::sleep(Duration::from_millis(100));

        let tables: Option<Vec<TestData>> = cache.get(&QueryCacheKey::AllTables);
        let namespaces: Option<Vec<TestData>> = cache.get(&QueryCacheKey::AllNamespaces);

        assert!(tables.is_none()); // Expired
        assert!(namespaces.is_some()); // Still valid
    }

    #[test]
    fn test_cache_stats() {
        let cache = QueryCache::new();
        let data = vec![TestData {
            id: "1".to_string(),
            value: 100,
        }];

        cache.put(QueryCacheKey::AllTables, data.clone());
        cache.put(QueryCacheKey::AllNamespaces, data.clone());

        let stats = cache.stats();
        assert_eq!(stats.total_entries, 2);
        assert_eq!(stats.active_entries, 2);
        assert_eq!(stats.expired_entries, 0);
    }

    #[test]
    fn test_evict_expired() {
        let config = QueryCacheTtlConfig {
            tables: Duration::from_millis(50),
            namespaces: Duration::from_secs(60),
            live_queries: Duration::from_secs(10),
            storages: Duration::from_secs(300),
            jobs: Duration::from_secs(30),
            single_entity: Duration::from_secs(120),
        };

        let cache = QueryCache::with_config(config);
        let data = vec![TestData {
            id: "1".to_string(),
            value: 100,
        }];

        cache.put(QueryCacheKey::AllTables, data.clone());
        cache.put(QueryCacheKey::AllNamespaces, data);

        // Wait for tables to expire (50ms TTL + buffer)
        std::thread::sleep(Duration::from_millis(100));

        cache.evict_expired();

        // Verify tables entry expired (can't be retrieved)
        let tables: Option<Vec<TestData>> = cache.get(&QueryCacheKey::AllTables);
        assert!(tables.is_none(), "Tables should have expired");

        // Namespaces should still be accessible
        let namespaces: Option<Vec<TestData>> = cache.get(&QueryCacheKey::AllNamespaces);
        assert!(namespaces.is_some(), "Namespaces should still be valid");
    }

    #[test]
    fn test_different_ttls_for_different_keys() {
        let config = QueryCacheTtlConfig {
            tables: Duration::from_secs(60),
            namespaces: Duration::from_secs(120),
            live_queries: Duration::from_secs(10),
            storages: Duration::from_secs(300),
            jobs: Duration::from_secs(30),
            single_entity: Duration::from_secs(90),
        };

        let cache = QueryCache::with_config(config);

        assert_eq!(
            cache.get_ttl(&QueryCacheKey::AllTables),
            Duration::from_secs(60)
        );
        assert_eq!(
            cache.get_ttl(&QueryCacheKey::AllNamespaces),
            Duration::from_secs(120)
        );
        assert_eq!(
            cache.get_ttl(&QueryCacheKey::AllLiveQueries),
            Duration::from_secs(10)
        );
        assert_eq!(
            cache.get_ttl(&QueryCacheKey::AllStorages),
            Duration::from_secs(300)
        );
        assert_eq!(
            cache.get_ttl(&QueryCacheKey::AllJobs),
            Duration::from_secs(30)
        );
        assert_eq!(
            cache.get_ttl(&QueryCacheKey::Table("users".to_string())),
            Duration::from_secs(90)
        );
        assert_eq!(
            cache.get_ttl(&QueryCacheKey::Namespace("app1".to_string())),
            Duration::from_secs(90)
        );
    }

    #[test]
    fn test_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let cache = Arc::new(QueryCache::new());
        let data = vec![TestData {
            id: "1".to_string(),
            value: 100,
        }];

        // Write from one thread
        let cache_clone = Arc::clone(&cache);
        let data_clone = data.clone();
        let writer = thread::spawn(move || {
            for i in 0..100 {
                let key = QueryCacheKey::Table(format!("table_{}", i));
                cache_clone.put(key, data_clone.clone());
            }
        });

        // Read from multiple threads simultaneously
        let mut readers = vec![];
        for _ in 0..5 {
            let cache_clone = Arc::clone(&cache);
            let reader = thread::spawn(move || {
                for i in 0..100 {
                    let key = QueryCacheKey::Table(format!("table_{}", i));
                    let _: Option<Vec<TestData>> = cache_clone.get(&key);
                }
            });
            readers.push(reader);
        }

        // Wait for all threads
        writer.join().unwrap();
        for reader in readers {
            reader.join().unwrap();
        }

        // Verify cache has entries
        let stats = cache.stats();
        assert!(stats.total_entries > 0);
    }

    #[test]
    fn test_lru_eviction() {
        // Create cache with max 5 entries
        let cache = QueryCache::with_config_and_max_entries(QueryCacheTtlConfig::default(), 5);

        let data = vec![TestData {
            id: "1".to_string(),
            value: 100,
        }];

        // Insert 10 entries (should trigger eviction)
        for i in 0..10 {
            let key = QueryCacheKey::Table(format!("table_{}", i));
            cache.put(key, data.clone());
        }

        // Force pending tasks to process evictions
        cache.evict_expired();

        let stats = cache.stats();
        // Should have at most 5 entries due to capacity limit
        assert!(
            stats.total_entries <= 5,
            "Expected at most 5 entries, got {}",
            stats.total_entries
        );
    }
}
