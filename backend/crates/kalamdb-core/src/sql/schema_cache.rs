//! Schema cache for DataFusion session management
//!
//! Caches Arrow schemas to avoid repeated deserialization from RocksDB.
//! Invalidated on ALTER TABLE operations.

use crate::catalog::{NamespaceId, TableName};
use crate::error::KalamDbError;
use crate::schema::arrow_schema::ArrowSchemaWithOptions;
use arrow::datatypes::SchemaRef;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Cache key for schema lookup
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SchemaCacheKey {
    pub namespace_id: Option<NamespaceId>,
    pub table_name: TableName,
    pub version: i32,
}

impl SchemaCacheKey {
    pub fn new(namespace_id: Option<NamespaceId>, table_name: TableName, version: i32) -> Self {
        Self {
            namespace_id,
            table_name,
            version,
        }
    }
}

/// Cached schema entry with TTL
#[derive(Debug, Clone)]
struct CachedSchema {
    schema: SchemaRef,
    cached_at: Instant,
}

impl CachedSchema {
    fn new(schema: SchemaRef) -> Self {
        Self {
            schema,
            cached_at: Instant::now(),
        }
    }

    fn is_expired(&self, ttl: Duration) -> bool {
        self.cached_at.elapsed() > ttl
    }
}

/// Schema cache for Arrow schemas
///
/// Thread-safe cache with TTL expiration and invalidation support.
pub struct SchemaCache {
    cache: Arc<RwLock<HashMap<SchemaCacheKey, CachedSchema>>>,
    ttl: Duration,
}

impl SchemaCache {
    /// Create a new schema cache with default TTL (5 minutes)
    pub fn new() -> Self {
        Self::with_ttl(Duration::from_secs(300))
    }

    /// Create a new schema cache with custom TTL
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            ttl,
        }
    }

    /// Get schema from cache
    ///
    /// Returns None if not in cache or expired.
    pub fn get(&self, key: &SchemaCacheKey) -> Option<SchemaRef> {
        let cache = self.cache.read().unwrap();
        if let Some(entry) = cache.get(key) {
            if !entry.is_expired(self.ttl) {
                return Some(entry.schema.clone());
            }
        }
        None
    }

    /// Put schema into cache
    pub fn put(&self, key: SchemaCacheKey, schema: SchemaRef) {
        let mut cache = self.cache.write().unwrap();
        cache.insert(key, CachedSchema::new(schema));
    }

    /// Invalidate schema for a specific table (all versions)
    ///
    /// Called after ALTER TABLE operations.
    pub fn invalidate_table(&self, namespace_id: &Option<NamespaceId>, table_name: &TableName) {
        let mut cache = self.cache.write().unwrap();
        cache.retain(|k, _| !(k.namespace_id == *namespace_id && k.table_name == *table_name));
    }

    /// Invalidate a specific schema version
    pub fn invalidate(&self, key: &SchemaCacheKey) {
        let mut cache = self.cache.write().unwrap();
        cache.remove(key);
    }

    /// Clear all cached schemas
    pub fn clear(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.clear();
    }

    /// Remove expired entries (garbage collection)
    pub fn evict_expired(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.retain(|_, entry| !entry.is_expired(self.ttl));
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let cache = self.cache.read().unwrap();
        let total = cache.len();
        let expired = cache
            .values()
            .filter(|entry| entry.is_expired(self.ttl))
            .count();
        CacheStats {
            total_entries: total,
            expired_entries: expired,
            active_entries: total - expired,
        }
    }
}

impl Default for SchemaCache {
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

/// Helper to deserialize and cache schemas
pub fn get_or_load_schema<F>(
    cache: &SchemaCache,
    key: SchemaCacheKey,
    loader: F,
) -> Result<SchemaRef, KalamDbError>
where
    F: FnOnce() -> Result<String, KalamDbError>,
{
    // Try cache first
    if let Some(schema) = cache.get(&key) {
        return Ok(schema);
    }

    // Load from source
    let schema_json = loader()?;
    let arrow_schema = ArrowSchemaWithOptions::from_json_string(&schema_json)
        .map_err(|e| KalamDbError::InvalidSql(format!("Failed to parse schema JSON: {}", e)))?;

    let schema = arrow_schema.schema.clone();

    // Cache and return
    cache.put(key, schema.clone());
    Ok(schema)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::datatypes::{DataType, Field, Schema};

    fn create_test_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int32, false),
            Field::new("name", DataType::Utf8, true),
        ]))
    }

    #[test]
    fn test_cache_put_and_get() {
        let cache = SchemaCache::new();
        let key = SchemaCacheKey::new(None, TableName::new("users"), 1);
        let schema = create_test_schema();

        cache.put(key.clone(), schema.clone());

        let retrieved = cache.get(&key);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().fields(), schema.fields());
    }

    #[test]
    fn test_cache_miss() {
        let cache = SchemaCache::new();
        let key = SchemaCacheKey::new(None, TableName::new("users"), 1);

        let retrieved = cache.get(&key);
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_invalidate_table() {
        let cache = SchemaCache::new();
        let key1 = SchemaCacheKey::new(None, TableName::new("users"), 1);
        let key2 = SchemaCacheKey::new(None, TableName::new("users"), 2);
        let key3 = SchemaCacheKey::new(None, TableName::new("posts"), 1);

        let schema = create_test_schema();
        cache.put(key1.clone(), schema.clone());
        cache.put(key2.clone(), schema.clone());
        cache.put(key3.clone(), schema.clone());

        // Invalidate all versions of "users" table
        cache.invalidate_table(&None, &TableName::new("users"));

        assert!(cache.get(&key1).is_none());
        assert!(cache.get(&key2).is_none());
        assert!(cache.get(&key3).is_some()); // Different table, should remain
    }

    #[test]
    fn test_invalidate_specific_version() {
        let cache = SchemaCache::new();
        let key1 = SchemaCacheKey::new(None, TableName::new("users"), 1);
        let key2 = SchemaCacheKey::new(None, TableName::new("users"), 2);

        let schema = create_test_schema();
        cache.put(key1.clone(), schema.clone());
        cache.put(key2.clone(), schema.clone());

        cache.invalidate(&key1);

        assert!(cache.get(&key1).is_none());
        assert!(cache.get(&key2).is_some()); // Different version, should remain
    }

    #[test]
    fn test_clear() {
        let cache = SchemaCache::new();
        let key = SchemaCacheKey::new(None, TableName::new("users"), 1);
        let schema = create_test_schema();

        cache.put(key.clone(), schema);
        assert!(cache.get(&key).is_some());

        cache.clear();
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_ttl_expiration() {
        let cache = SchemaCache::with_ttl(Duration::from_millis(50));
        let key = SchemaCacheKey::new(None, TableName::new("users"), 1);
        let schema = create_test_schema();

        cache.put(key.clone(), schema);
        assert!(cache.get(&key).is_some());

        // Wait for expiration
        std::thread::sleep(Duration::from_millis(100));

        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_cache_stats() {
        let cache = SchemaCache::new();
        let key1 = SchemaCacheKey::new(None, TableName::new("users"), 1);
        let key2 = SchemaCacheKey::new(None, TableName::new("posts"), 1);
        let schema = create_test_schema();

        cache.put(key1, schema.clone());
        cache.put(key2, schema);

        let stats = cache.stats();
        assert_eq!(stats.total_entries, 2);
        assert_eq!(stats.active_entries, 2);
        assert_eq!(stats.expired_entries, 0);
    }

    #[test]
    fn test_evict_expired() {
        let cache = SchemaCache::with_ttl(Duration::from_millis(50));
        let key = SchemaCacheKey::new(None, TableName::new("users"), 1);
        let schema = create_test_schema();

        cache.put(key.clone(), schema);
        assert_eq!(cache.stats().total_entries, 1);

        // Wait for expiration
        std::thread::sleep(Duration::from_millis(100));

        cache.evict_expired();
        assert_eq!(cache.stats().total_entries, 0);
    }

    #[test]
    fn test_namespace_id_in_key() {
        let cache = SchemaCache::new();
        let key1 = SchemaCacheKey::new(Some(NamespaceId::new("app1")), TableName::new("users"), 1);
        let key2 = SchemaCacheKey::new(Some(NamespaceId::new("app2")), TableName::new("users"), 1);
        let schema = create_test_schema();

        cache.put(key1.clone(), schema.clone());
        cache.put(key2.clone(), schema);

        // Invalidate only app1's users table
        cache.invalidate_table(&Some(NamespaceId::new("app1")), &TableName::new("users"));

        assert!(cache.get(&key1).is_none());
        assert!(cache.get(&key2).is_some()); // Different namespace, should remain
    }
}
