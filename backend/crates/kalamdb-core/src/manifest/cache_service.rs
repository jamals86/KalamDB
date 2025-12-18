//! Manifest cache service with RocksDB persistence and in-memory hot cache (Phase 4 - US6).
//!
//! Provides fast manifest access with two-tier caching:
//! 1. Hot cache (DashMap) for sub-millisecond lookups
//! 2. Persistent cache (RocksDB) for crash recovery

use dashmap::DashMap;
use kalamdb_commons::{
    config::ManifestCacheSettings,
    types::{Manifest, ManifestCacheEntry, SyncState},
    NamespaceId, TableId, TableName, UserId,
};
use kalamdb_store::{entity_store::EntityStore, StorageBackend, StorageError};
use kalamdb_system::providers::manifest::{new_manifest_store, ManifestCacheKey, ManifestStore};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

#[derive(Debug)]
struct HotManifestEntry {
    entry: Arc<ManifestCacheEntry>,
    last_accessed_ts: AtomicI64,
}

impl HotManifestEntry {
    fn new(entry: Arc<ManifestCacheEntry>, last_accessed_ts: i64) -> Self {
        Self {
            entry,
            last_accessed_ts: AtomicI64::new(last_accessed_ts),
        }
    }

    fn touch_at(&self, ts: i64) {
        self.last_accessed_ts.store(ts, Ordering::Relaxed);
    }

    fn last_accessed_ts(&self) -> i64 {
        self.last_accessed_ts.load(Ordering::Relaxed)
    }
}

/// Manifest cache service with hot cache + RocksDB persistence.
///
/// Architecture:
/// - Hot cache: DashMap<String, HotManifestEntry> for fast reads
///   - `HotManifestEntry.entry`: Arc<ManifestCacheEntry>
///   - `HotManifestEntry.last_accessed_ts`: AtomicI64 (in-memory only, not persisted)
/// - Persistent store: RocksDB manifest_cache column family
/// - TTL enforcement: Background eviction job + freshness validation
pub struct ManifestCacheService {
    /// RocksDB-backed persistent store
    store: ManifestStore,

    /// In-memory hot cache for fast lookups
    hot_cache: DashMap<String, HotManifestEntry>,

    /// Configuration settings
    config: ManifestCacheSettings,
}

impl ManifestCacheService {
    /// Create a new manifest cache service
    pub fn new(backend: Arc<dyn StorageBackend>, config: ManifestCacheSettings) -> Self {
        Self {
            store: new_manifest_store(backend),
            hot_cache: DashMap::new(),
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
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<Option<Arc<ManifestCacheEntry>>, StorageError> {
        let cache_key = ManifestCacheKey::from(self.make_cache_key(table_id, user_id));

        // 1. Check hot cache
        if let Some(entry) = self.hot_cache.get(cache_key.as_str()) {
            entry.value().touch_at(chrono::Utc::now().timestamp());
            return Ok(Some(Arc::clone(&entry.value().entry)));
        }

        // 2. Check RocksDB CF
        if let Some(entry) = EntityStore::get(&self.store, &cache_key)? {
            let entry_arc = Arc::new(entry);
            self.insert_into_hot_cache(
                cache_key.as_str().to_string(),
                Arc::clone(&entry_arc),
                chrono::Utc::now().timestamp(),
            );
            return Ok(Some(entry_arc));
        }

        // 3. Not cached
        Ok(None)
    }

    /// Update manifest cache after successful flush (writes manifest to storage, marks entry in sync).
    pub fn update_after_flush(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
        manifest: &Manifest,
        etag: Option<String>,
        source_path: String,
    ) -> Result<(), StorageError> {
        self.upsert_cache_entry(
            table_id,
            user_id,
            manifest,
            etag,
            source_path,
            SyncState::InSync,
        )
    }

    /// Stage manifest metadata in the cache before the first flush writes manifest.json to disk.
    ///
    /// Uses `PendingWrite` state since the manifest hasn't been written to storage yet.
    pub fn stage_before_flush(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
        manifest: &Manifest,
        source_path: String,
    ) -> Result<(), StorageError> {
        self.upsert_cache_entry(
            table_id,
            user_id,
            manifest,
            None,
            source_path,
            SyncState::PendingWrite,
        )
    }

    /// Mark a cache entry as stale (e.g., after validation failure or corruption detection).
    ///
    /// Updates the sync_state to Stale in both hot cache and RocksDB.
    pub fn mark_as_stale(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<(), StorageError> {
        let cache_key_str = self.make_cache_key(table_id, user_id);
        let cache_key = ManifestCacheKey::from(cache_key_str.clone());

        // Update in RocksDB
        if let Some(mut entry) = EntityStore::get(&self.store, &cache_key)? {
            entry.mark_stale();
            EntityStore::put(&self.store, &cache_key, &entry)?;

            // Update hot cache if present
            if let Some(mut hot_entry) = self.hot_cache.get_mut(&cache_key_str) {
                // Replace with updated entry
                let last_accessed = hot_entry.last_accessed_ts();
                *hot_entry = HotManifestEntry::new(Arc::new(entry), last_accessed);
            }
        }

        Ok(())
    }

    /// Mark a cache entry as having an error state.
    ///
    /// Updates the sync_state to Error in both hot cache and RocksDB.
    pub fn mark_as_error(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<(), StorageError> {
        let cache_key_str = self.make_cache_key(table_id, user_id);
        let cache_key = ManifestCacheKey::from(cache_key_str.clone());

        // Update in RocksDB
        if let Some(mut entry) = EntityStore::get(&self.store, &cache_key)? {
            entry.mark_error();
            EntityStore::put(&self.store, &cache_key, &entry)?;

            // Update hot cache if present
            if let Some(mut hot_entry) = self.hot_cache.get_mut(&cache_key_str) {
                let last_accessed = hot_entry.last_accessed_ts();
                *hot_entry = HotManifestEntry::new(Arc::new(entry), last_accessed);
            }
        }

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
            Ok(!entry.value().entry.is_stale(self.config.ttl_seconds, now))
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
        let table_id = TableId::new(namespace.clone(), table.clone());
        let cache_key_str = self.make_cache_key(&table_id, user_id);
        let cache_key = ManifestCacheKey::from(cache_key_str.clone());
        self.hot_cache.remove(&cache_key_str);
        EntityStore::delete(&self.store, &cache_key)
    }

    /// Evict least-recently-used entry from hot cache
    fn evict_lru(&self) {
        let mut oldest_key: Option<String> = None;
        let mut oldest_timestamp = i64::MAX;

        // Find entry with oldest last_accessed timestamp
        for entry in self.hot_cache.iter() {
            let ts = entry.value().last_accessed_ts();
            if ts < oldest_timestamp {
                oldest_timestamp = ts;
                oldest_key = Some(entry.key().clone());
            }
        }

        // Remove oldest entry from hot cache
        // Note: We do NOT remove from RocksDB (L2 cache)
        if let Some(key) = oldest_key {
            self.hot_cache.remove(&key);
        }
    }

    /// Get all cache entries (for SHOW MANIFEST CACHE).
    pub fn get_all(&self) -> Result<Vec<(String, ManifestCacheEntry)>, StorageError> {
        let entries = EntityStore::scan_all(&self.store, None, None, None)?;
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
        let all_entries = EntityStore::scan_all(&self.store, None, None, None)?;
        Ok(all_entries.len())
    }

    /// Clear all cache entries (for testing/maintenance).
    pub fn clear(&self) -> Result<(), StorageError> {
        self.hot_cache.clear();
        let keys = EntityStore::scan_all(&self.store, None, None, None)?;
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
        let now = chrono::Utc::now().timestamp();
        let entries = EntityStore::scan_all(&self.store, None, None, None)?;
        for (key_bytes, entry) in entries {
            if let Ok(key_str) = String::from_utf8(key_bytes) {
                // Skip stale entries to avoid loading expired manifests into RAM
                if entry.is_stale(self.config.ttl_seconds, now) {
                    continue;
                }

                let last_refreshed = entry.last_refreshed;
                self.insert_into_hot_cache(
                    key_str,
                    Arc::new(entry),
                    last_refreshed,
                );
            }
        }
        Ok(())
    }

    // Helper methods

    fn make_cache_key(&self, table_id: &TableId, user_id: Option<&UserId>) -> String {
        let scope = user_id.map(|u| u.as_str()).unwrap_or("shared");
        format!(
            "{}:{}:{}",
            table_id.namespace_id().as_str(),
            table_id.table_name().as_str(),
            scope
        )
    }

    fn upsert_cache_entry(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
        manifest: &Manifest,
        etag: Option<String>,
        source_path: String,
        sync_state: SyncState,
    ) -> Result<(), StorageError> {
        let cache_key_str = self.make_cache_key(table_id, user_id);
        let cache_key = ManifestCacheKey::from(cache_key_str.clone());
        let now = chrono::Utc::now().timestamp();

        // Store the Manifest object directly (no longer serializing to JSON string)
        let entry = ManifestCacheEntry::new(manifest.clone(), etag, now, source_path, sync_state);

        EntityStore::put(&self.store, &cache_key, &entry)?;

        self.insert_into_hot_cache(cache_key_str, Arc::new(entry), now);

        Ok(())
    }

    fn insert_into_hot_cache(
        &self,
        cache_key: String,
        entry: Arc<ManifestCacheEntry>,
        last_accessed_ts: i64,
    ) {
        self.evict_to_capacity();
        self.hot_cache
            .insert(cache_key, HotManifestEntry::new(entry, last_accessed_ts));
    }

    fn evict_to_capacity(&self) {
        if self.config.max_entries == 0 {
            return;
        }

        while self.hot_cache.len() >= self.config.max_entries {
            let before = self.hot_cache.len();
            self.evict_lru();
            if self.hot_cache.len() == before {
                break;
            }
        }
    }

    /// Get last accessed timestamp for a key (used by eviction job).
    pub fn get_last_accessed(&self, cache_key: &str) -> Option<i64> {
        self.hot_cache
            .get(cache_key)
            .map(|v| v.value().last_accessed_ts())
    }

    /// Check if a cache key is currently in the hot cache (RAM).
    ///
    /// This is used by system.manifest table to populate the `in_memory` column.
    pub fn is_in_hot_cache(&self, cache_key: &str) -> bool {
        self.hot_cache.contains_key(cache_key)
    }

    /// Get cache configuration.
    pub fn config(&self) -> &ManifestCacheSettings {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::TableId;
    use kalamdb_store::entity_store::EntityStore;
    use kalamdb_store::test_utils::InMemoryBackend;
    use kalamdb_system::providers::manifest::ManifestCacheKey;

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

    fn create_test_manifest() -> Manifest {
        let table_id = TableId::new(NamespaceId::new("test"), TableName::new("table"));
        Manifest::new(table_id, Some(UserId::from("u_123")))
    }

    fn insert_entry(
        service: &ManifestCacheService,
        table_id: &TableId,
        entry: ManifestCacheEntry,
    ) {
        let key = ManifestCacheKey::from(service.make_cache_key(table_id, None));
        EntityStore::put(&service.store, &key, &entry).unwrap();
    }

    #[test]
    fn test_get_or_load_miss() {
        let service = create_test_service();
        let namespace = NamespaceId::new("ns1");
        let table = TableName::new("tbl1");
        let table_id = TableId::new(namespace.clone(), table.clone());

        let result = service
            .get_or_load(&table_id, Some(&UserId::from("u_123")))
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_update_after_flush() {
        let service = create_test_service();
        let namespace = NamespaceId::new("ns1");
        let table = TableName::new("tbl1");
        let table_id = TableId::new(namespace.clone(), table.clone());
        let manifest = create_test_manifest();

        service
            .update_after_flush(
                &table_id,
                Some(&UserId::from("u_123")),
                &manifest,
                Some("etag123".to_string()),
                "s3://bucket/path/manifest.json".to_string(),
            )
            .unwrap();

        // Verify cached
        let cached = service
            .get_or_load(&table_id, Some(&UserId::from("u_123")))
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
        let table_id = TableId::new(namespace.clone(), table.clone());
        let manifest = create_test_manifest();

        // Prime cache
        service
            .update_after_flush(
                &table_id,
                Some(&UserId::from("u_123")),
                &manifest,
                None,
                "path".to_string(),
            )
            .unwrap();

        // Second read should hit hot cache
        let result = service
            .get_or_load(&table_id, Some(&UserId::from("u_123")))
            .unwrap();
        assert!(result.is_some());

        // Verify last_accessed updated
        let cache_key = service.make_cache_key(&table_id, Some(&UserId::from("u_123")));
        assert!(service.get_last_accessed(&cache_key).is_some());
    }

    #[test]
    fn test_invalidate() {
        let service = create_test_service();
        let namespace = NamespaceId::new("ns1");
        let table = TableName::new("tbl1");
        let table_id = TableId::new(namespace.clone(), table.clone());
        let manifest = create_test_manifest();

        // Add entry
        service
            .update_after_flush(
                &table_id,
                Some(&UserId::from("u_123")),
                &manifest,
                None,
                "path".to_string(),
            )
            .unwrap();

        // Verify cached
        assert!(service
            .get_or_load(&table_id, Some(&UserId::from("u_123")))
            .unwrap()
            .is_some());

        // Invalidate
        service
            .invalidate(&namespace, &table, Some(&UserId::from("u_123")))
            .unwrap();

        // Verify removed
        assert!(service
            .get_or_load(&table_id, Some(&UserId::from("u_123")))
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_validate_freshness() {
        let service = create_test_service();
        let namespace = NamespaceId::new("ns1");
        let table = TableName::new("tbl1");
        let table_id = TableId::new(namespace.clone(), table.clone());
        let manifest = create_test_manifest();

        // Add fresh entry
        service
            .update_after_flush(
                &table_id,
                Some(&UserId::from("u_123")),
                &manifest,
                None,
                "path".to_string(),
            )
            .unwrap();

        let cache_key = service.make_cache_key(&table_id, Some(&UserId::from("u_123")));

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
        let table_id = TableId::new(namespace.clone(), table.clone());
        let manifest = create_test_manifest();

        // Add entry
        service1
            .update_after_flush(
                &table_id,
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
            .get_or_load(&table_id, Some(&UserId::from("u_123")))
            .unwrap();
        assert!(cached.is_some());
    }

    #[test]
    fn test_clear() {
        let service = create_test_service();
        let namespace = NamespaceId::new("ns1");
        let table = TableName::new("tbl1");
        let table_id = TableId::new(namespace.clone(), table.clone());
        let manifest = create_test_manifest();

        service
            .update_after_flush(
                &table_id,
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

    #[test]
    fn test_get_or_load_respects_capacity_on_rocksdb_load() {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let mut config = ManifestCacheSettings::default();
        config.max_entries = 1;

        let service = ManifestCacheService::new(Arc::clone(&backend), config.clone());
        let table1 = TableId::new(NamespaceId::new("ns1"), TableName::new("t1"));
        let table2 = TableId::new(NamespaceId::new("ns1"), TableName::new("t2"));

        let mut manifest1 = create_test_manifest();
        manifest1.table_id = table1.clone();
        let mut manifest2 = create_test_manifest();
        manifest2.table_id = table2.clone();

        service
            .update_after_flush(&table1, Some(&UserId::from("u_123")), &manifest1, None, "p1".to_string())
            .unwrap();
        service
            .update_after_flush(&table2, Some(&UserId::from("u_123")), &manifest2, None, "p2".to_string())
            .unwrap();

        // New instance to force loading from RocksDB
        let service_reader = ManifestCacheService::new(backend, config);
        let key1 = service_reader.make_cache_key(&table1, Some(&UserId::from("u_123")));
        let key2 = service_reader.make_cache_key(&table2, Some(&UserId::from("u_123")));

        service_reader
            .get_or_load(&table1, Some(&UserId::from("u_123")))
            .unwrap();
        assert_eq!(service_reader.hot_cache.len(), 1);
        assert!(service_reader.hot_cache.contains_key(&key1));

        service_reader
            .get_or_load(&table2, Some(&UserId::from("u_123")))
            .unwrap();
        assert_eq!(service_reader.hot_cache.len(), 1);
        assert!(service_reader.hot_cache.contains_key(&key2));
    }

    #[test]
    fn test_restore_from_rocksdb_skips_stale_and_limits_capacity() {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let mut config = ManifestCacheSettings::default();
        config.ttl_seconds = 5;
        config.max_entries = 1;

        let service = ManifestCacheService::new(Arc::clone(&backend), config.clone());
        let table1 = TableId::new(NamespaceId::new("ns1"), TableName::new("fresh"));
        let table2 = TableId::new(NamespaceId::new("ns1"), TableName::new("stale"));
        let now = chrono::Utc::now().timestamp();

        let fresh_manifest = Manifest::new(table1.clone(), None);
        let stale_manifest = Manifest::new(table2.clone(), None);

        let fresh_entry =
            ManifestCacheEntry::new(fresh_manifest, None, now, "p1".to_string(), SyncState::InSync);
        let stale_entry = ManifestCacheEntry::new(
            stale_manifest,
            None,
            now - 10,
            "p2".to_string(),
            SyncState::InSync,
        );

        insert_entry(&service, &table1, fresh_entry);
        insert_entry(&service, &table2, stale_entry);

        let restored = ManifestCacheService::new(backend, config);
        restored.restore_from_rocksdb().unwrap();

        assert_eq!(restored.hot_cache.len(), 1);
        let fresh_key = restored.make_cache_key(&table1, None);
        let stale_key = restored.make_cache_key(&table2, None);
        assert!(restored.hot_cache.contains_key(&fresh_key));
        assert!(!restored.hot_cache.contains_key(&stale_key));
    }
}
