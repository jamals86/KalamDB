//! Manifest cache service with RocksDB persistence and in-memory hot cache (Phase 4 - US6).
//!
//! Provides fast manifest access with two-tier caching:
//! 1. Hot cache (DashMap) for sub-millisecond lookups
//! 2. Persistent cache (RocksDB) for crash recovery

use dashmap::DashMap;
use kalamdb_commons::{
    config::ManifestCacheSettings,
    types::{ManifestCacheEntry, ManifestFile, SyncState},
    NamespaceId, TableName, UserId,
};
use kalamdb_store::{entity_store::EntityStore, StorageBackend, StorageError};
use kalamdb_system::providers::manifest::{new_manifest_store, ManifestCacheKey, ManifestStore};
use std::sync::Arc;

/// Manifest cache service with hot cache + RocksDB persistence.
///
/// Architecture:
/// - Hot cache: DashMap<String, Arc<ManifestCacheEntry>> for fast reads
/// - last_accessed: DashMap<String, i64> (in-memory only, not persisted)
/// - Persistent store: RocksDB manifest_cache column family
/// - TTL enforcement: Background eviction job + freshness validation
pub struct ManifestCacheService {
    /// RocksDB-backed persistent store
    store: ManifestStore,

    /// In-memory hot cache for fast lookups
    hot_cache: DashMap<String, Arc<ManifestCacheEntry>>,

    /// Last access timestamps (in-memory only)
    last_accessed: DashMap<String, i64>,

    /// Configuration settings
    config: ManifestCacheSettings,
}

impl ManifestCacheService {
    /// Create a new manifest cache service
    pub fn new(backend: Arc<dyn StorageBackend>, config: ManifestCacheSettings) -> Self {
        Self {
            store: new_manifest_store(backend),
            hot_cache: DashMap::new(),
            last_accessed: DashMap::new(),
            config,
        }
    }

    /// Get or load a manifest cache entry.
    ///
    /// Flow:
    /// 1. Check hot cache → return immediately
    /// 2. Check RocksDB CF → load to hot cache, return
    /// 3. Return None (caller should load from storage backend)
    ///
    /// Updates last_accessed timestamp on cache hit.
    pub fn get_or_load(
        &self,
        namespace: &NamespaceId,
        table: &TableName,
        user_id: Option<&UserId>,
    ) -> Result<Option<Arc<ManifestCacheEntry>>, StorageError> {
        let cache_key = ManifestCacheKey::from(self.make_cache_key(namespace, table, user_id));

        // 1. Check hot cache
        if let Some(entry) = self.hot_cache.get(cache_key.as_str()) {
            self.update_last_accessed(cache_key.as_str());
            return Ok(Some(Arc::clone(entry.value())));
        }

        // 2. Check RocksDB CF
        if let Some(entry) = EntityStore::get(&self.store, &cache_key)? {
            let entry_arc = Arc::new(entry);
            self.hot_cache
                .insert(cache_key.as_str().to_string(), Arc::clone(&entry_arc));
            self.update_last_accessed(cache_key.as_str());
            return Ok(Some(entry_arc));
        }

        // 3. Not cached
        Ok(None)
    }

    /// Update manifest cache after successful flush.
    ///
    /// Atomically writes to:
    /// - RocksDB manifest_cache CF
    /// - Hot cache (DashMap)
    ///
    /// Caller must ensure manifest.json has already been written to storage backend.
    pub fn update_after_flush(
        &self,
        namespace: &NamespaceId,
        table: &TableName,
        user_id: Option<&UserId>,
        manifest: &ManifestFile,
        etag: Option<String>,
        source_path: String,
    ) -> Result<(), StorageError> {
        let cache_key_str = self.make_cache_key(namespace, table, user_id);
        let cache_key = ManifestCacheKey::from(cache_key_str.clone());
        let now = chrono::Utc::now().timestamp();

        let manifest_json = manifest.to_json().map_err(|e| {
            StorageError::SerializationError(format!("Failed to serialize ManifestFile: {}", e))
        })?;

        let entry =
            ManifestCacheEntry::new(manifest_json, etag, now, source_path, SyncState::InSync);

        // Write to RocksDB CF
        EntityStore::put(&self.store, &cache_key, &entry)?;

        // Update hot cache
        self.hot_cache
            .insert(cache_key_str.clone(), Arc::new(entry));
        self.update_last_accessed(&cache_key_str);

        Ok(())
    }

    /// Validate freshness of cached entry based on TTL.
    ///
    /// Returns:
    /// - Ok(true): Entry is fresh
    /// - Ok(false): Entry is stale (needs refresh)
    pub fn validate_freshness(&self, cache_key: &str) -> Result<bool, StorageError> {
        if let Some(entry) = self.hot_cache.get(cache_key) {
            let now = chrono::Utc::now().timestamp();
            Ok(!entry.is_stale(self.config.ttl_seconds, now))
        } else if let Some(entry) =
            EntityStore::get(&self.store, &ManifestCacheKey::from(cache_key))?
        {
            let now = chrono::Utc::now().timestamp();
            Ok(!entry.is_stale(self.config.ttl_seconds, now))
        } else {
            Ok(false) // Not cached = not fresh
        }
    }

    /// Invalidate (delete) a cache entry.
    ///
    /// Removes from both hot cache and RocksDB CF.
    pub fn invalidate(
        &self,
        namespace: &NamespaceId,
        table: &TableName,
        user_id: Option<&UserId>,
    ) -> Result<(), StorageError> {
        let cache_key_str = self.make_cache_key(namespace, table, user_id);
        let cache_key = ManifestCacheKey::from(cache_key_str.clone());
        self.hot_cache.remove(&cache_key_str);
        self.last_accessed.remove(&cache_key_str);
        EntityStore::delete(&self.store, &cache_key)
    }

    /// Get all cache entries (for SHOW MANIFEST CACHE).
    pub fn get_all(&self) -> Result<Vec<(String, ManifestCacheEntry)>, StorageError> {
        let entries = EntityStore::scan_all(&self.store)?;
        // Convert Vec<u8> keys to Strings
        let string_entries = entries
            .into_iter()
            .filter_map(|(key_bytes, entry)| String::from_utf8(key_bytes).ok().map(|k| (k, entry)))
            .collect();
        Ok(string_entries)
    }

    /// Get total count of cached entries (hot cache + RocksDB)
    pub fn count(&self) -> Result<usize, StorageError> {
        // Get all keys from RocksDB
        let all_entries = EntityStore::scan_all(&self.store)?;
        Ok(all_entries.len())
    }

    /// Clear all cache entries (for testing/maintenance).
    pub fn clear(&self) -> Result<(), StorageError> {
        self.hot_cache.clear();
        self.last_accessed.clear();
        let keys = EntityStore::scan_all(&self.store)?;
        for (key_bytes, _) in keys {
            let key = ManifestCacheKey::from(String::from_utf8_lossy(&key_bytes).to_string());
            EntityStore::delete(&self.store, &key)?;
        }
        Ok(())
    }

    /// Restore hot cache from RocksDB on server restart.
    ///
    /// Loads all entries from RocksDB CF into hot cache.
    /// Called during AppContext initialization.
    pub fn restore_from_rocksdb(&self) -> Result<(), StorageError> {
        let entries = EntityStore::scan_all(&self.store)?;
        for (key_bytes, entry) in entries {
            if let Ok(key_str) = String::from_utf8(key_bytes) {
                self.hot_cache.insert(key_str, Arc::new(entry));
            }
        }
        Ok(())
    }

    // Helper methods

    fn make_cache_key(
        &self,
        namespace: &NamespaceId,
        table: &TableName,
        user_id: Option<&UserId>,
    ) -> String {
        let scope = user_id.map(|u| u.as_str()).unwrap_or("shared");
        format!("{}:{}:{}", namespace.as_str(), table.as_str(), scope)
    }

    fn update_last_accessed(&self, cache_key: &str) {
        let now = chrono::Utc::now().timestamp();
        self.last_accessed.insert(cache_key.to_string(), now);
    }

    /// Get last accessed timestamp for a key (used by eviction job).
    pub fn get_last_accessed(&self, cache_key: &str) -> Option<i64> {
        self.last_accessed.get(cache_key).map(|v| *v)
    }

    /// Get cache configuration.
    pub fn config(&self) -> &ManifestCacheSettings {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_store::test_utils::InMemoryBackend;

    fn create_test_service() -> ManifestCacheService {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let config = ManifestCacheSettings {
            ttl_seconds: 3600,
            eviction_interval_seconds: 300,
            max_entries: 1000,
            last_accessed_memory_window: 3600,
        };
        ManifestCacheService::new(backend, config)
    }

    fn create_test_manifest() -> ManifestFile {
        ManifestFile {
            table_id: "test.table".to_string(),
            scope: "u_123".to_string(),
            version: 1,
            generated_at: chrono::Utc::now().timestamp(),
            max_batch: 0,
            batches: Vec::new(),
        }
    }

    #[test]
    fn test_get_or_load_miss() {
        let service = create_test_service();
        let namespace = NamespaceId::new("ns1");
        let table = TableName::new("tbl1");

        let result = service
            .get_or_load(&namespace, &table, Some(&UserId::from("u_123")))
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_update_after_flush() {
        let service = create_test_service();
        let namespace = NamespaceId::new("ns1");
        let table = TableName::new("tbl1");
        let manifest = create_test_manifest();

        service
            .update_after_flush(
                &namespace,
                &table,
                Some(&UserId::from("u_123")),
                &manifest,
                Some("etag123".to_string()),
                "s3://bucket/path/manifest.json".to_string(),
            )
            .unwrap();

        // Verify cached
        let cached = service
            .get_or_load(&namespace, &table, Some(&UserId::from("u_123")))
            .unwrap();
        assert!(cached.is_some());
        let entry = cached.unwrap();
        assert_eq!(entry.etag, Some("etag123".to_string()));
        assert_eq!(entry.sync_state, SyncState::InSync);
    }

    #[test]
    fn test_hot_cache_hit() {
        let service = create_test_service();
        let namespace = NamespaceId::new("ns1");
        let table = TableName::new("tbl1");
        let manifest = create_test_manifest();

        // Prime cache
        service
            .update_after_flush(
                &namespace,
                &table,
                Some(&UserId::from("u_123")),
                &manifest,
                None,
                "path".to_string(),
            )
            .unwrap();

        // Second read should hit hot cache
        let result = service
            .get_or_load(&namespace, &table, Some(&UserId::from("u_123")))
            .unwrap();
        assert!(result.is_some());

        // Verify last_accessed updated
        let cache_key = service.make_cache_key(&namespace, &table, Some(&UserId::from("u_123")));
        assert!(service.get_last_accessed(&cache_key).is_some());
    }

    #[test]
    fn test_invalidate() {
        let service = create_test_service();
        let namespace = NamespaceId::new("ns1");
        let table = TableName::new("tbl1");
        let manifest = create_test_manifest();

        // Add entry
        service
            .update_after_flush(
                &namespace,
                &table,
                Some(&UserId::from("u_123")),
                &manifest,
                None,
                "path".to_string(),
            )
            .unwrap();

        // Verify cached
        assert!(service
            .get_or_load(&namespace, &table, Some(&UserId::from("u_123")))
            .unwrap()
            .is_some());

        // Invalidate
        service
            .invalidate(&namespace, &table, Some(&UserId::from("u_123")))
            .unwrap();

        // Verify removed
        assert!(service
            .get_or_load(&namespace, &table, Some(&UserId::from("u_123")))
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_validate_freshness() {
        let service = create_test_service();
        let namespace = NamespaceId::new("ns1");
        let table = TableName::new("tbl1");
        let manifest = create_test_manifest();

        // Add fresh entry
        service
            .update_after_flush(
                &namespace,
                &table,
                Some(&UserId::from("u_123")),
                &manifest,
                None,
                "path".to_string(),
            )
            .unwrap();

        let cache_key = service.make_cache_key(&namespace, &table, Some(&UserId::from("u_123")));

        // Should be fresh
        assert!(service.validate_freshness(&cache_key).unwrap());
    }

    #[test]
    fn test_restore_from_rocksdb() {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let config = ManifestCacheSettings::default();

        let service1 = ManifestCacheService::new(Arc::clone(&backend), config.clone());
        let namespace = NamespaceId::new("ns1");
        let table = TableName::new("tbl1");
        let manifest = create_test_manifest();

        // Add entry
        service1
            .update_after_flush(
                &namespace,
                &table,
                Some(&UserId::from("u_123")),
                &manifest,
                None,
                "path".to_string(),
            )
            .unwrap();

        // Create new service (simulating restart)
        let service2 = ManifestCacheService::new(backend, config);
        service2.restore_from_rocksdb().unwrap();

        // Verify entry restored to hot cache
        let cached = service2
            .get_or_load(&namespace, &table, Some(&UserId::from("u_123")))
            .unwrap();
        assert!(cached.is_some());
    }

    #[test]
    fn test_clear() {
        let service = create_test_service();
        let namespace = NamespaceId::new("ns1");
        let table = TableName::new("tbl1");
        let manifest = create_test_manifest();

        service
            .update_after_flush(
                &namespace,
                &table,
                Some(&UserId::from("u_123")),
                &manifest,
                None,
                "path".to_string(),
            )
            .unwrap();

        assert_eq!(service.count().unwrap(), 1);

        service.clear().unwrap();
        assert_eq!(service.count().unwrap(), 0);
    }
}
