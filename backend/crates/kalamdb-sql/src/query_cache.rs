//! Query result cache for system table queries
//!
//! Caches results of frequently-accessed system table queries to reduce RocksDB reads.
//! Invalidated automatically on mutations to system tables.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
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
    /// scan_all_storage_locations() result
    AllStorageLocations,
    /// scan_all_jobs() result
    AllJobs,
    /// get_table(table_id) result
    Table(String),
    /// get_namespace(namespace_id) result
    Namespace(String),
}

/// Cached query result with TTL
#[derive(Debug, Clone)]
struct CachedResult<T> {
    value: T,
    cached_at: Instant,
}

impl<T> CachedResult<T> {
    fn new(value: T) -> Self {
        Self {
            value,
            cached_at: Instant::now(),
        }
    }

    fn is_expired(&self, ttl: Duration) -> bool {
        self.cached_at.elapsed() > ttl
    }
}

/// Query result cache for system tables
///
/// Thread-safe cache with TTL expiration and invalidation support.
/// Designed for caching expensive system table scans.
pub struct QueryCache {
    // Generic cache for serializable results
    cache: Arc<RwLock<HashMap<QueryCacheKey, CachedResult<Vec<u8>>>>>,
    // TTL configuration per query type
    ttl_config: QueryCacheTtlConfig,
}

/// TTL configuration for different query types
#[derive(Debug, Clone)]
pub struct QueryCacheTtlConfig {
    pub tables: Duration,
    pub namespaces: Duration,
    pub live_queries: Duration,
    pub storage_locations: Duration,
    pub jobs: Duration,
    pub single_entity: Duration,
}

impl Default for QueryCacheTtlConfig {
    fn default() -> Self {
        Self {
            tables: Duration::from_secs(60),           // 60s for tables list
            namespaces: Duration::from_secs(60),       // 60s for namespaces list
            live_queries: Duration::from_secs(10),     // 10s for live queries (more dynamic)
            storage_locations: Duration::from_secs(300), // 5min for storage locations (rarely change)
            jobs: Duration::from_secs(30),             // 30s for jobs list
            single_entity: Duration::from_secs(120),   // 2min for individual entities
        }
    }
}

impl QueryCache {
    /// Create a new query cache with default TTL configuration
    pub fn new() -> Self {
        Self::with_config(QueryCacheTtlConfig::default())
    }

    /// Create a new query cache with custom TTL configuration
    pub fn with_config(ttl_config: QueryCacheTtlConfig) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            ttl_config,
        }
    }

    /// Get TTL for a specific query key
    fn get_ttl(&self, key: &QueryCacheKey) -> Duration {
        match key {
            QueryCacheKey::AllTables => self.ttl_config.tables,
            QueryCacheKey::AllNamespaces => self.ttl_config.namespaces,
            QueryCacheKey::AllLiveQueries => self.ttl_config.live_queries,
            QueryCacheKey::AllStorageLocations => self.ttl_config.storage_locations,
            QueryCacheKey::AllJobs => self.ttl_config.jobs,
            QueryCacheKey::Table(_) | QueryCacheKey::Namespace(_) => {
                self.ttl_config.single_entity
            }
        }
    }

    /// Get cached result
    ///
    /// Returns None if not in cache or expired.
    pub fn get<T>(&self, key: &QueryCacheKey) -> Option<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let cache = self.cache.read().unwrap();
        if let Some(entry) = cache.get(key) {
            let ttl = self.get_ttl(key);
            if !entry.is_expired(ttl) {
                // Deserialize from bytes
                if let Ok(value) = bincode::deserialize(&entry.value) {
                    return Some(value);
                }
            }
        }
        None
    }

    /// Put result into cache
    pub fn put<T>(&self, key: QueryCacheKey, value: T)
    where
        T: Serialize,
    {
        // Serialize to bytes
        if let Ok(bytes) = bincode::serialize(&value) {
            let mut cache = self.cache.write().unwrap();
            cache.insert(key, CachedResult::new(bytes));
        }
    }

    /// Invalidate all tables-related queries
    pub fn invalidate_tables(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.remove(&QueryCacheKey::AllTables);
        // Also remove individual table entries
        cache.retain(|k, _| !matches!(k, QueryCacheKey::Table(_)));
    }

    /// Invalidate all namespaces-related queries
    pub fn invalidate_namespaces(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.remove(&QueryCacheKey::AllNamespaces);
        // Also remove individual namespace entries
        cache.retain(|k, _| !matches!(k, QueryCacheKey::Namespace(_)));
    }

    /// Invalidate all live queries-related queries
    pub fn invalidate_live_queries(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.remove(&QueryCacheKey::AllLiveQueries);
    }

    /// Invalidate all storage locations-related queries
    pub fn invalidate_storage_locations(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.remove(&QueryCacheKey::AllStorageLocations);
    }

    /// Invalidate all jobs-related queries
    pub fn invalidate_jobs(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.remove(&QueryCacheKey::AllJobs);
    }

    /// Invalidate a specific cached result
    pub fn invalidate(&self, key: &QueryCacheKey) {
        let mut cache = self.cache.write().unwrap();
        cache.remove(key);
    }

    /// Clear all cached results
    pub fn clear(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.clear();
    }

    /// Remove expired entries (garbage collection)
    pub fn evict_expired(&self) {
        let mut cache = self.cache.write().unwrap();
        let ttl_config = &self.ttl_config;
        cache.retain(|key, entry| {
            let ttl = match key {
                QueryCacheKey::AllTables => ttl_config.tables,
                QueryCacheKey::AllNamespaces => ttl_config.namespaces,
                QueryCacheKey::AllLiveQueries => ttl_config.live_queries,
                QueryCacheKey::AllStorageLocations => ttl_config.storage_locations,
                QueryCacheKey::AllJobs => ttl_config.jobs,
                QueryCacheKey::Table(_) | QueryCacheKey::Namespace(_) => {
                    ttl_config.single_entity
                }
            };
            !entry.is_expired(ttl)
        });
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let cache = self.cache.read().unwrap();
        let total = cache.len();
        
        let mut expired = 0;
        let ttl_config = &self.ttl_config;
        for (key, entry) in cache.iter() {
            let ttl = match key {
                QueryCacheKey::AllTables => ttl_config.tables,
                QueryCacheKey::AllNamespaces => ttl_config.namespaces,
                QueryCacheKey::AllLiveQueries => ttl_config.live_queries,
                QueryCacheKey::AllStorageLocations => ttl_config.storage_locations,
                QueryCacheKey::AllJobs => ttl_config.jobs,
                QueryCacheKey::Table(_) | QueryCacheKey::Namespace(_) => {
                    ttl_config.single_entity
                }
            };
            if entry.is_expired(ttl) {
                expired += 1;
            }
        }

        CacheStats {
            total_entries: total,
            expired_entries: expired,
            active_entries: total - expired,
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

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
        let single_table: Option<Vec<TestData>> = cache.get(&QueryCacheKey::Table("users".to_string()));
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
        let single_namespace: Option<Vec<TestData>> = cache.get(&QueryCacheKey::Namespace("app1".to_string()));
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
            storage_locations: Duration::from_secs(300),
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
            storage_locations: Duration::from_secs(300),
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

        // Wait for tables to expire
        std::thread::sleep(Duration::from_millis(100));

        cache.evict_expired();

        let stats = cache.stats();
        assert_eq!(stats.total_entries, 1); // Only namespaces should remain
    }

    #[test]
    fn test_different_ttls_for_different_keys() {
        let config = QueryCacheTtlConfig {
            tables: Duration::from_secs(60),
            namespaces: Duration::from_secs(120),
            live_queries: Duration::from_secs(10),
            storage_locations: Duration::from_secs(300),
            jobs: Duration::from_secs(30),
            single_entity: Duration::from_secs(90),
        };

        let cache = QueryCache::with_config(config);

        assert_eq!(cache.get_ttl(&QueryCacheKey::AllTables), Duration::from_secs(60));
        assert_eq!(cache.get_ttl(&QueryCacheKey::AllNamespaces), Duration::from_secs(120));
        assert_eq!(cache.get_ttl(&QueryCacheKey::AllLiveQueries), Duration::from_secs(10));
        assert_eq!(cache.get_ttl(&QueryCacheKey::AllStorageLocations), Duration::from_secs(300));
        assert_eq!(cache.get_ttl(&QueryCacheKey::AllJobs), Duration::from_secs(30));
        assert_eq!(cache.get_ttl(&QueryCacheKey::Table("users".to_string())), Duration::from_secs(90));
        assert_eq!(cache.get_ttl(&QueryCacheKey::Namespace("app1".to_string())), Duration::from_secs(90));
    }
}
