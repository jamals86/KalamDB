//! Unified ManifestService for batch file metadata tracking.
//!
//! Provides manifest.json management with two-tier caching:
//! 1. Hot cache (moka) for sub-millisecond lookups with automatic TTI-based eviction
//! 2. Persistent cache (RocksDB) for crash recovery
//! 3. Cold storage (object_store) for manifest.json files
//!
//! Key type: (TableId, Option<UserId>) for type-safe cache access.

use crate::schema_registry::PathResolver;
use crate::schema_registry::SchemaRegistry;
use crate::storage::storage_registry::StorageRegistry;
use kalamdb_commons::config::ManifestCacheSettings;
use kalamdb_commons::models::types::{Manifest, ManifestCacheEntry, SegmentMetadata, SyncState};
use kalamdb_commons::models::StorageId;
use kalamdb_commons::{NamespaceId, TableId, TableName, UserId};
use kalamdb_store::entity_store::EntityStore;
use kalamdb_store::{StorageBackend, StorageError};
use kalamdb_system::providers::manifest::{new_manifest_store, ManifestCacheKey, ManifestStore};
use log::{debug, info, warn};
use moka::sync::Cache;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Cache key type for moka cache: (TableId, Option<UserId>)
pub type ManifestCacheKeyTuple = (TableId, Option<UserId>);

/// Unified ManifestService with hot cache + RocksDB persistence + cold storage.
///
/// Architecture:
/// - Hot cache: moka::sync::Cache<(TableId, Option<UserId>), Arc<ManifestCacheEntry>> for fast reads
/// - Persistent store: RocksDB manifest_cache column family for crash recovery
/// - Cold store: manifest.json files in object_store (S3/local filesystem)
pub struct ManifestService {
    /// Base storage path (fallback for building paths)
    _base_path: String,

    /// RocksDB-backed persistent store
    store: ManifestStore,

    /// In-memory hot cache for fast lookups (moka with automatic eviction)
    /// Key: (TableId, Option<UserId>)
    hot_cache: Cache<ManifestCacheKeyTuple, Arc<ManifestCacheEntry>>,

    /// Configuration settings
    config: ManifestCacheSettings,

    /// Optional registries for path/object store resolution.
    ///
    /// In production these are injected via `new_with_registries()` to avoid any
    /// global `AppContext::get()` usage. In tests we allow a fallback.
    schema_registry: Option<Arc<SchemaRegistry>>,
    storage_registry: Option<Arc<StorageRegistry>>,
}

/// Minimum weight for any cache entry (shared tables)
const MIN_ENTRY_WEIGHT: u32 = 1;

impl ManifestService {
    /// Create a new ManifestService with tiered eviction strategy.
    ///
    /// The hot cache uses a weigher to prioritize shared tables over user tables:
    /// - Shared tables (user_id = None): weight = 1
    /// - User tables (user_id = Some): weight = config.user_table_weight_factor (default 10)
    ///
    /// This means when memory pressure occurs, user table manifests are evicted
    /// approximately N times faster than shared table manifests (N = user_table_weight_factor).
    pub fn new(
        storage_backend: Arc<dyn StorageBackend>,
        base_path: String,
        config: ManifestCacheSettings,
    ) -> Self {
        // Build moka cache with TTI, max capacity, and tiered weigher
        let tti_secs = config.ttl_seconds() as u64;

        // Capture the weight factor from config for use in closure
        let user_weight = config.user_table_weight_factor.max(1); // At least 1

        // Weigher for tiered eviction: shared tables stay longer than user tables
        // Weight is based on entry type, not actual memory size
        let weigher = move |key: &ManifestCacheKeyTuple, _entry: &Arc<ManifestCacheEntry>| -> u32 {
            match &key.1 {
                None => MIN_ENTRY_WEIGHT, // Shared table - low weight, stays longer
                Some(_) => user_weight,   // User table - high weight, evicted sooner
            }
        };

        // Calculate weighted capacity: if max_entries=1000 and user_weight=10, we want room for
        // ~1000 shared tables OR ~100 user tables (or mix)
        let weighted_capacity = (config.max_entries as u64) * (user_weight as u64);

        let hot_cache = Cache::builder()
            .max_capacity(weighted_capacity)
            .weigher(weigher)
            .time_to_idle(Duration::from_secs(tti_secs))
            .build();

        Self {
            _base_path: base_path,
            store: new_manifest_store(storage_backend),
            hot_cache,
            config,
            schema_registry: None,
            storage_registry: None,
        }
    }

    /// Create a new ManifestService with access to SchemaRegistry + StorageRegistry.
    ///
    /// This is the preferred constructor for production wiring.
    pub fn new_with_registries(
        storage_backend: Arc<dyn StorageBackend>,
        base_path: String,
        config: ManifestCacheSettings,
        schema_registry: Arc<SchemaRegistry>,
        storage_registry: Arc<StorageRegistry>,
    ) -> Self {
        let mut service = Self::new(storage_backend, base_path, config);
        service.schema_registry = Some(schema_registry);
        service.storage_registry = Some(storage_registry);
        service
    }

    fn registries(&self) -> Result<(Arc<SchemaRegistry>, Arc<StorageRegistry>), StorageError> {
        if let (Some(schema_registry), Some(storage_registry)) =
            (self.schema_registry.as_ref(), self.storage_registry.as_ref())
        {
            return Ok((Arc::clone(schema_registry), Arc::clone(storage_registry)));
        }

        #[cfg(test)]
        {
            let app_ctx = crate::app_context::AppContext::get();
            return Ok((app_ctx.schema_registry(), app_ctx.storage_registry()));
        }

        #[cfg(not(test))]
        {
            Err(StorageError::Other(
                "ManifestService missing SchemaRegistry/StorageRegistry; use new_with_registries()"
                    .to_string(),
            ))
        }
    }

    // ========== Hot Cache Operations (formerly ManifestCacheService) ==========

    /// Get or load a manifest cache entry.
    ///
    /// Flow:
    /// 1. Check hot cache → return immediately
    /// 2. Check RocksDB CF → load to hot cache, return
    /// 3. Return None (caller should load from storage backend)
    ///
    /// Moka automatically updates last_accessed on cache hit.
    pub fn get_or_load(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<Option<Arc<ManifestCacheEntry>>, StorageError> {
        let cache_key = (table_id.clone(), user_id.cloned());
        let rocksdb_key = ManifestCacheKey::from(self.make_cache_key_string(table_id, user_id));

        // 1. Check hot cache (moka automatically updates TTI on access)
        if let Some(entry) = self.hot_cache.get(&cache_key) {
            return Ok(Some(entry));
        }

        // 2. Check RocksDB CF
        if let Some(entry) = EntityStore::get(&self.store, &rocksdb_key)? {
            let entry_arc = Arc::new(entry);
            self.hot_cache.insert(cache_key, Arc::clone(&entry_arc));
            return Ok(Some(entry_arc));
        }

        // 3. Not cached
        Ok(None)
    }

    /// Update manifest cache after successful flush.
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

    /// Mark a cache entry as stale.
    pub fn mark_as_stale(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<(), StorageError> {
        let cache_key = (table_id.clone(), user_id.cloned());
        let rocksdb_key = ManifestCacheKey::from(self.make_cache_key_string(table_id, user_id));

        if let Some(mut entry) = EntityStore::get(&self.store, &rocksdb_key)? {
            entry.mark_stale();
            EntityStore::put(&self.store, &rocksdb_key, &entry)?;
            self.hot_cache.insert(cache_key, Arc::new(entry));
        }

        Ok(())
    }

    /// Mark a cache entry as having an error state.
    pub fn mark_as_error(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<(), StorageError> {
        let cache_key = (table_id.clone(), user_id.cloned());
        let rocksdb_key = ManifestCacheKey::from(self.make_cache_key_string(table_id, user_id));

        if let Some(mut entry) = EntityStore::get(&self.store, &rocksdb_key)? {
            entry.mark_error();
            EntityStore::put(&self.store, &rocksdb_key, &entry)?;
            self.hot_cache.insert(cache_key, Arc::new(entry));
        }

        Ok(())
    }

    /// Mark a cache entry as syncing (flush in progress).
    pub fn mark_syncing(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<(), StorageError> {
        let cache_key = (table_id.clone(), user_id.cloned());
        let rocksdb_key = ManifestCacheKey::from(self.make_cache_key_string(table_id, user_id));

        if let Some(mut entry) = EntityStore::get(&self.store, &rocksdb_key)? {
            entry.mark_syncing();
            EntityStore::put(&self.store, &rocksdb_key, &entry)?;
            self.hot_cache.insert(cache_key, Arc::new(entry));
        }

        Ok(())
    }

    /// Validate freshness of cached entry based on TTL.
    pub fn validate_freshness(&self, table_id: &TableId, user_id: Option<&UserId>) -> Result<bool, StorageError> {
        let cache_key = (table_id.clone(), user_id.cloned());
        let rocksdb_key = ManifestCacheKey::from(self.make_cache_key_string(table_id, user_id));

        if let Some(entry) = self.hot_cache.get(&cache_key) {
            let now = chrono::Utc::now().timestamp();
            Ok(!entry.is_stale(self.config.ttl_seconds(), now))
        } else if let Some(entry) = EntityStore::get(&self.store, &rocksdb_key)? {
            let now = chrono::Utc::now().timestamp();
            Ok(!entry.is_stale(self.config.ttl_seconds(), now))
        } else {
            Ok(false)
        }
    }

    /// Invalidate (delete) a cache entry.
    pub fn invalidate(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<(), StorageError> {
        let cache_key = (table_id.clone(), user_id.cloned());
        let rocksdb_key = ManifestCacheKey::from(self.make_cache_key_string(table_id, user_id));

        self.hot_cache.invalidate(&cache_key);
        EntityStore::delete(&self.store, &rocksdb_key)
    }

    /// Invalidate all cache entries for a table (all users + shared).
    pub fn invalidate_table(&self, table_id: &TableId) -> Result<usize, StorageError> {
        let key_prefix = format!(
            "{}:",
            table_id // TableId Display: "namespace:table"
        );

        let mut invalidated = 0;
        let all_entries = EntityStore::scan_all(&self.store, None, None, None)?;

        for (key_bytes, _entry) in all_entries {
            let key_str = match String::from_utf8(key_bytes.clone()) {
                Ok(s) => s,
                Err(_) => continue,
            };

            if key_str.starts_with(&key_prefix) {
                // Parse key to get user_id for hot cache invalidation
                if let Some(user_id) = self.parse_user_id_from_key(&key_str) {
                    let cache_key = (table_id.clone(), user_id);
                    self.hot_cache.invalidate(&cache_key);
                }

                let rocksdb_key = ManifestCacheKey::from(key_str);
                EntityStore::delete(&self.store, &rocksdb_key)?;
                invalidated += 1;
            }
        }

        info!(
            "Invalidated {} manifest cache entries for table {}",
            invalidated, table_id
        );

        Ok(invalidated)
    }

    /// Get all cache entries (for SHOW MANIFEST CACHE).
    pub fn get_all(&self) -> Result<Vec<(String, ManifestCacheEntry)>, StorageError> {
        let entries = EntityStore::scan_all(&self.store, None, None, None)?;
        let string_entries = entries
            .into_iter()
            .filter_map(|(key_bytes, entry)| String::from_utf8(key_bytes).ok().map(|k| (k, entry)))
            .collect();
        Ok(string_entries)
    }

    /// Get total count of cached entries
    pub fn count(&self) -> Result<usize, StorageError> {
        let all_entries = EntityStore::scan_all(&self.store, None, None, None)?;
        Ok(all_entries.len())
    }

    /// Get cache statistics including weighted counts.
    ///
    /// Returns (shared_count, user_count, total_weight) where:
    /// - shared_count: number of shared table manifests (weight=1 each)
    /// - user_count: number of user table manifests (weight=user_table_weight_factor each)
    /// - total_weight: sum of all weights (used for capacity calculation)
    pub fn cache_stats(&self) -> (usize, usize, u64) {
        let mut shared_count = 0usize;
        let mut user_count = 0usize;

        for (key, _) in &self.hot_cache {
            match key.1 {
                None => shared_count += 1,
                Some(_) => user_count += 1,
            }
        }

        let user_weight = self.config.user_table_weight_factor.max(1) as u64;
        let total_weight =
            (shared_count as u64 * MIN_ENTRY_WEIGHT as u64) + (user_count as u64 * user_weight);

        (shared_count, user_count, total_weight)
    }

    /// Get the configured maximum weighted capacity.
    pub fn max_weighted_capacity(&self) -> u64 {
        let user_weight = self.config.user_table_weight_factor.max(1) as u64;
        (self.config.max_entries as u64) * user_weight
    }

    /// Clear all cache entries.
    pub fn clear(&self) -> Result<(), StorageError> {
        self.hot_cache.invalidate_all();
        let keys = EntityStore::scan_all(&self.store, None, None, None)?;
        for (key_bytes, _) in keys {
            let key = ManifestCacheKey::from(String::from_utf8_lossy(&key_bytes).to_string());
            EntityStore::delete(&self.store, &key)?;
        }
        Ok(())
    }

    // /// Restore hot cache from RocksDB (for testing/debugging only).
    // ///
    // /// NOTE: Not used at startup - manifests are loaded lazily via get_or_load()
    // /// which checks hot cache → RocksDB on-demand. This avoids loading manifests
    // /// that may never be accessed.
    // #[allow(dead_code)]
    // pub fn restore_from_rocksdb(&self) -> Result<(), StorageError> {
    //     let now = chrono::Utc::now().timestamp();
    //     let entries = EntityStore::scan_all(&self.store, None, None, None)?;

    //     for (key_bytes, entry) in entries {
    //         if let Ok(key_str) = String::from_utf8(key_bytes) {
    //             if entry.is_stale(self.config.ttl_seconds(), now) {
    //                 continue;
    //             }

    //             // Parse the key to construct the tuple key
    //             if let Some((table_id, user_id)) = self.parse_key_string(&key_str) {
    //                 self.hot_cache.insert((table_id, user_id), Arc::new(entry));
    //             }
    //         }
    //     }
    //     Ok(())
    // }

    /// Check if a cache key is currently in the hot cache (RAM).
    pub fn is_in_hot_cache(&self, table_id: &TableId, user_id: Option<&UserId>) -> bool {
        let cache_key = (table_id.clone(), user_id.cloned());
        self.hot_cache.contains_key(&cache_key)
    }

    /// Check if a cache key string is in hot cache (for system.manifest table compatibility).
    pub fn is_in_hot_cache_by_string(&self, cache_key_str: &str) -> bool {
        if let Some((table_id, user_id)) = self.parse_key_string(cache_key_str) {
            self.hot_cache.contains_key(&(table_id, user_id))
        } else {
            false
        }
    }

    /// Get the number of entries in the hot cache.
    pub fn hot_cache_len(&self) -> usize {
        self.hot_cache.run_pending_tasks();
        self.hot_cache.entry_count() as usize
    }

    /// Evict stale manifest entries from RocksDB.
    pub fn evict_stale_entries(&self, ttl_seconds: i64) -> Result<usize, StorageError> {
        let now = chrono::Utc::now().timestamp();
        let cutoff = now - ttl_seconds;
        let mut evicted_count = 0;

        let all_entries = EntityStore::scan_all(&self.store, None, None, None)?;

        for (key_bytes, entry) in all_entries {
            let key_str = match String::from_utf8(key_bytes.clone()) {
                Ok(s) => s,
                Err(_) => continue,
            };

            if entry.last_refreshed < cutoff {
                if let Some((table_id, user_id)) = self.parse_key_string(&key_str) {
                    self.hot_cache.invalidate(&(table_id, user_id));
                }

                let rocksdb_key = ManifestCacheKey::from(key_str);
                EntityStore::delete(&self.store, &rocksdb_key)?;
                evicted_count += 1;
            }
        }

        info!(
            "Manifest eviction: removed {} stale entries (ttl_seconds={}, cutoff={})",
            evicted_count, ttl_seconds, cutoff
        );

        Ok(evicted_count)
    }

    /// Get cache configuration.
    pub fn config(&self) -> &ManifestCacheSettings {
        &self.config
    }

    // ========== Cold Storage Operations (formerly ManifestService) ==========

    /// Create an in-memory manifest for a table scope.
    pub fn create_manifest(
        &self,
        table_id: &TableId,
        _table_type: kalamdb_commons::models::schemas::TableType,
        user_id: Option<&UserId>,
    ) -> Manifest {
        Manifest::new(table_id.clone(), user_id.cloned())
    }

    /// Ensure a manifest exists (checking cache, then disk, otherwise creating in-memory).
    pub fn ensure_manifest_initialized(
        &self,
        table_id: &TableId,
        table_type: kalamdb_commons::models::schemas::TableType,
        user_id: Option<&UserId>,
    ) -> Result<Manifest, StorageError> {
        // 1. Check Hot Store (Cache)
        if let Some(entry) = self.get_or_load(table_id, user_id)? {
            return Ok(entry.manifest.clone());
        }

        // 2. Check Cold Store (via object_store)
        match self.read_manifest(table_id, user_id) {
            Ok(manifest) => {
                // Stage it in cache
                let storage_path = self.get_storage_path(table_id, user_id)?;
                let manifest_path = format!("{}/manifest.json", storage_path);
                self.stage_before_flush(table_id, user_id, &manifest, manifest_path)?;
                return Ok(manifest);
            }
            Err(_) => {
                // Manifest doesn't exist or can't be read, create new one
            }
        }

        // 3. Create New (In-Memory only)
        let manifest = self.create_manifest(table_id, table_type, user_id);
        Ok(manifest)
    }

    /// Update manifest: append segment to cache.
    pub fn update_manifest(
        &self,
        table_id: &TableId,
        table_type: kalamdb_commons::models::schemas::TableType,
        user_id: Option<&UserId>,
        segment: SegmentMetadata,
    ) -> Result<Manifest, StorageError> {
        // Ensure manifest is loaded/initialized
        let mut manifest = self.ensure_manifest_initialized(table_id, table_type, user_id)?;

        // Add segment
        manifest.add_segment(segment);

        // Update cache entry
        let storage_path = self.get_storage_path(table_id, user_id)?;
        let manifest_path = format!("{}/manifest.json", storage_path);
        self.upsert_cache_entry(
            table_id,
            user_id,
            &manifest,
            None,
            manifest_path,
            SyncState::PendingWrite,
        )?;

        Ok(manifest)
    }

    /// Flush manifest: Write to Cold Store (storage via object_store).
    pub fn flush_manifest(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<(), StorageError> {
        let cache_key = (table_id.clone(), user_id.cloned());

        if let Some(entry) = self.hot_cache.get(&cache_key) {
            let (store, storage, _) = self.get_storage_context(table_id, user_id)?;
            self.write_manifest_via_store(store, &storage, table_id, user_id, &entry.manifest)?;
            debug!(
                "Flushed manifest for {} (ver: {})",
                table_id,
                entry.manifest.version
            );
        } else {
            warn!(
                "Attempted to flush manifest for {} but it was not in cache",
                table_id
            );
        }
        Ok(())
    }

    /// Read manifest.json from storage.
    pub fn read_manifest(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<Manifest, StorageError> {
        let (store, storage, manifest_path) = self.get_storage_context(table_id, user_id)?;

        let json_str = kalamdb_filestore::read_manifest_json(store, &storage, &manifest_path)
            .map_err(|e| StorageError::IoError(format!("Failed to read manifest: {}", e)))?;

        serde_json::from_str(&json_str).map_err(|e| {
            StorageError::SerializationError(format!("Failed to parse manifest JSON: {}", e))
        })
    }

    /// Rebuild manifest from Parquet footers.
    pub fn rebuild_manifest(
        &self,
        table_id: &TableId,
        _table_type: kalamdb_commons::models::schemas::TableType,
        user_id: Option<&UserId>,
    ) -> Result<Manifest, StorageError> {
        let (store, storage, _) = self.get_storage_context(table_id, user_id)?;
        let table_dir = self.get_storage_path(table_id, user_id)?;
        let mut manifest = Manifest::new(table_id.clone(), user_id.cloned());

        let files = kalamdb_filestore::list_files_sync(Arc::clone(&store), &storage, &table_dir)
            .map_err(|e| StorageError::IoError(format!("Failed to list files: {}", e)))?;

        let mut batch_files: Vec<String> = files
            .into_iter()
            .filter(|f| f.ends_with(".parquet") && f.contains("batch-"))
            .collect();
        batch_files.sort();

        for batch_path in batch_files {
            if let Some(segment) =
                self.extract_segment_metadata_via_store(Arc::clone(&store), &storage, &batch_path)?
            {
                manifest.add_segment(segment);
            }
        }

        // Update cache and write to storage
        let storage_path = self.get_storage_path(table_id, user_id)?;
        let manifest_path = format!("{}/manifest.json", storage_path);
        self.upsert_cache_entry(
            table_id,
            user_id,
            &manifest,
            None,
            manifest_path,
            SyncState::InSync,
        )?;
        self.write_manifest_via_store(Arc::clone(&store), &storage, table_id, user_id, &manifest)?;

        Ok(manifest)
    }

    /// Validate manifest consistency.
    pub fn validate_manifest(&self, _manifest: &Manifest) -> Result<(), StorageError> {
        // Basic validation - can be expanded
        Ok(())
    }

    /// Public helper for consumers that need the resolved manifest.json path.
    pub fn manifest_path(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<String, StorageError> {
        let storage_path = self.get_storage_path(table_id, user_id)?;
        Ok(format!("{}/manifest.json", storage_path))
    }

    // ========== Private Helper Methods ==========

    fn make_cache_key_string(&self, table_id: &TableId, user_id: Option<&UserId>) -> String {
        let scope = user_id.map(|u| u.as_str()).unwrap_or("shared");
        format!(
            "{}:{}",
            table_id, // TableId Display: "namespace:table"
            scope
        )
    }

    fn parse_key_string(&self, key_str: &str) -> Option<(TableId, Option<UserId>)> {
        let parts: Vec<&str> = key_str.split(':').collect();
        if parts.len() == 3 {
            let namespace = NamespaceId::new(parts[0]);
            let table = TableName::new(parts[1]);
            let user_id = if parts[2] == "shared" {
                None
            } else {
                Some(UserId::from(parts[2]))
            };
            Some((TableId::new(namespace, table), user_id))
        } else {
            None
        }
    }

    fn parse_user_id_from_key(&self, key_str: &str) -> Option<Option<UserId>> {
        let parts: Vec<&str> = key_str.split(':').collect();
        if parts.len() == 3 {
            let user_id = if parts[2] == "shared" {
                None
            } else {
                Some(UserId::from(parts[2]))
            };
            Some(user_id)
        } else {
            None
        }
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
        let cache_key = (table_id.clone(), user_id.cloned());
        let rocksdb_key = ManifestCacheKey::from(self.make_cache_key_string(table_id, user_id));
        let now = chrono::Utc::now().timestamp();

        let entry = ManifestCacheEntry::new(manifest.clone(), etag, now, source_path, sync_state);

        EntityStore::put(&self.store, &rocksdb_key, &entry)?;
        self.hot_cache.insert(cache_key, Arc::new(entry));

        Ok(())
    }

    fn get_storage_context(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<
        (
            Arc<dyn object_store::ObjectStore>,
            kalamdb_commons::system::Storage,
            String,
        ),
        StorageError,
    > {
        let (schema_registry, storage_registry) = self.registries()?;

        let cached = schema_registry
            .get(table_id)
            .ok_or_else(|| StorageError::Other(format!("Table not found: {}", table_id)))?;

        let storage_id = cached.storage_id.clone().unwrap_or_else(StorageId::local);
        let storage_arc = storage_registry
            .get_storage(&storage_id)
            .map_err(|e| StorageError::IoError(e.to_string()))?
            .ok_or_else(|| {
                StorageError::Other(format!("Storage {} not found", storage_id.as_str()))
            })?;
        let storage = (*storage_arc).clone();

        let store = storage_registry
            .get_object_store(&storage_id)
            .map_err(|e| StorageError::IoError(e.to_string()))?;

        let relative = PathResolver::get_relative_storage_path(&cached, user_id, None)
            .map_err(|e| StorageError::IoError(e.to_string()))?;

        let base_dir = {
            let trimmed = storage.base_directory.trim();
            if trimmed.is_empty() {
                storage_registry.default_storage_path().to_string()
            } else {
                trimmed.to_string()
            }
        };

        let storage_path = if base_dir.ends_with('/') {
            format!("{}{}", base_dir, relative.trim_start_matches('/'))
        } else {
            format!("{}/{}", base_dir, relative.trim_start_matches('/'))
        };
        let manifest_path = format!("{}/manifest.json", storage_path);

        Ok((store, storage, manifest_path))
    }

    fn get_storage_path(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<String, StorageError> {
        let (schema_registry, storage_registry) = self.registries()?;
        let cached = schema_registry
            .get(table_id)
            .ok_or_else(|| StorageError::Other(format!("Table not found: {}", table_id)))?;

        let storage_id = cached.storage_id.clone().unwrap_or_else(StorageId::local);
        let storage_arc = storage_registry
            .get_storage(&storage_id)
            .map_err(|e| StorageError::IoError(e.to_string()))?
            .ok_or_else(|| {
                StorageError::Other(format!("Storage {} not found", storage_id.as_str()))
            })?;
        let storage = (*storage_arc).clone();

        let relative = PathResolver::get_relative_storage_path(&cached, user_id, None)
            .map_err(|e| StorageError::IoError(e.to_string()))?;

        let base_dir = {
            let trimmed = storage.base_directory.trim();
            if trimmed.is_empty() {
                storage_registry.default_storage_path().to_string()
            } else {
                trimmed.to_string()
            }
        };

        Ok(if base_dir.ends_with('/') {
            format!("{}{}", base_dir, relative.trim_start_matches('/'))
        } else {
            format!("{}/{}", base_dir, relative.trim_start_matches('/'))
        })
    }

    fn write_manifest_via_store(
        &self,
        store: Arc<dyn object_store::ObjectStore>,
        storage: &kalamdb_commons::system::Storage,
        table_id: &TableId,
        user_id: Option<&UserId>,
        manifest: &Manifest,
    ) -> Result<(), StorageError> {
        let storage_path = self.get_storage_path(table_id, user_id)?;
        let manifest_path = format!("{}/manifest.json", storage_path);

        let json_str = serde_json::to_string_pretty(manifest).map_err(|e| {
            StorageError::SerializationError(format!("Failed to serialize manifest: {}", e))
        })?;

        kalamdb_filestore::write_manifest_json(store, storage, &manifest_path, &json_str)
            .map_err(|e| StorageError::IoError(format!("Failed to write manifest: {}", e)))
    }

    fn extract_segment_metadata_via_store(
        &self,
        store: Arc<dyn object_store::ObjectStore>,
        storage: &kalamdb_commons::system::Storage,
        parquet_path: &str,
    ) -> Result<Option<SegmentMetadata>, StorageError> {
        let file_name = parquet_path
            .rsplit('/')
            .next()
            .unwrap_or(parquet_path)
            .to_string();

        let id = file_name.clone();

        let size_bytes = kalamdb_filestore::head_file_sync(store, storage, parquet_path)
            .map(|m| m.size_bytes as u64)
            .unwrap_or(0);

        Ok(Some(SegmentMetadata::new(
            id,
            file_name,
            HashMap::new(),
            0,
            0,
            0,
            size_bytes,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::models::schemas::TableType;
    use kalamdb_store::test_utils::InMemoryBackend;

    fn create_test_service() -> ManifestService {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let config = ManifestCacheSettings {
            eviction_interval_seconds: 300,
            max_entries: 1000,
            eviction_ttl_days: 7,
            user_table_weight_factor: 10,
        };
        ManifestService::new(backend, "/tmp/test".to_string(), config)
    }

    fn create_test_manifest(table_id: &TableId, user_id: Option<&UserId>) -> Manifest {
        Manifest::new(table_id.clone(), user_id.cloned())
    }

    fn build_table_id(ns: &str, tbl: &str) -> TableId {
        TableId::new(NamespaceId::new(ns), TableName::new(tbl))
    }

    #[test]
    fn test_create_manifest() {
        let service = create_test_service();
        let table_id = build_table_id("ns1", "products");

        let manifest = service.create_manifest(&table_id, TableType::Shared, None);

        assert_eq!(manifest.table_id, table_id);
        assert_eq!(manifest.user_id, None);
        assert_eq!(manifest.segments.len(), 0);
    }

    #[test]
    fn test_get_or_load_miss() {
        let service = create_test_service();
        let table_id = build_table_id("ns1", "tbl1");

        let result = service
            .get_or_load(&table_id, Some(&UserId::from("u_123")))
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_update_after_flush() {
        let service = create_test_service();
        let table_id = build_table_id("ns1", "tbl1");
        let manifest = create_test_manifest(&table_id, Some(&UserId::from("u_123")));

        service
            .update_after_flush(
                &table_id,
                Some(&UserId::from("u_123")),
                &manifest,
                Some("etag123".to_string()),
                "s3://bucket/path/manifest.json".to_string(),
            )
            .unwrap();

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
        let table_id = build_table_id("ns1", "tbl1");
        let manifest = create_test_manifest(&table_id, Some(&UserId::from("u_123")));

        service
            .update_after_flush(
                &table_id,
                Some(&UserId::from("u_123")),
                &manifest,
                None,
                "path".to_string(),
            )
            .unwrap();

        let result = service
            .get_or_load(&table_id, Some(&UserId::from("u_123")))
            .unwrap();
        assert!(result.is_some());

        assert!(service.is_in_hot_cache(&table_id, Some(&UserId::from("u_123"))));
    }

    #[test]
    fn test_invalidate() {
        let service = create_test_service();
        let namespace = NamespaceId::new("ns1");
        let table = TableName::new("tbl1");
        let table_id = TableId::new(namespace.clone(), table.clone());
        let manifest = create_test_manifest(&table_id, Some(&UserId::from("u_123")));

        service
            .update_after_flush(
                &table_id,
                Some(&UserId::from("u_123")),
                &manifest,
                None,
                "path".to_string(),
            )
            .unwrap();

        assert!(service
            .get_or_load(&table_id, Some(&UserId::from("u_123")))
            .unwrap()
            .is_some());

        service
            .invalidate(&table_id, Some(&UserId::from("u_123")))
            .unwrap();

        assert!(service
            .get_or_load(&table_id, Some(&UserId::from("u_123")))
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_mark_syncing_updates_state() {
        let service = create_test_service();
        let table_id = build_table_id("ns1", "tbl1");
        let manifest = create_test_manifest(&table_id, Some(&UserId::from("u_123")));

        service
            .update_after_flush(
                &table_id,
                Some(&UserId::from("u_123")),
                &manifest,
                None,
                "path".to_string(),
            )
            .unwrap();

        let cached = service
            .get_or_load(&table_id, Some(&UserId::from("u_123")))
            .unwrap()
            .unwrap();
        assert_eq!(cached.sync_state, SyncState::InSync);

        service
            .mark_syncing(&table_id, Some(&UserId::from("u_123")))
            .unwrap();

        let cached_after = service
            .get_or_load(&table_id, Some(&UserId::from("u_123")))
            .unwrap()
            .unwrap();
        assert_eq!(cached_after.sync_state, SyncState::Syncing);
    }

    #[test]
    fn test_clear() {
        let service = create_test_service();
        let table_id = build_table_id("ns1", "tbl1");
        let manifest = create_test_manifest(&table_id, Some(&UserId::from("u_123")));

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

    // #[test]
    // fn test_restore_from_rocksdb() {
    //     let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
    //     let config = ManifestCacheSettings::default();

    //     let service1 = ManifestService::new(Arc::clone(&backend), "/tmp".to_string(), config.clone());
    //     let table_id = build_table_id("ns1", "tbl1");
    //     let manifest = create_test_manifest(&table_id, Some(&UserId::from("u_123")));

    //     service1
    //         .update_after_flush(
    //             &table_id,
    //             Some(&UserId::from("u_123")),
    //             &manifest,
    //             None,
    //             "path".to_string(),
    //         )
    //         .unwrap();

    //     // Create new service (simulating restart)
    //     let service2 = ManifestService::new(backend, "/tmp".to_string(), config);
    //     service2.restore_from_rocksdb().unwrap();

    //     let cached = service2
    //         .get_or_load(&table_id, Some(&UserId::from("u_123")))
    //         .unwrap();
    //     assert!(cached.is_some());
    // }

    #[test]
    fn test_cache_key_parsing() {
        let service = create_test_service();
        let table_id = build_table_id("myns", "mytable");

        // Test shared table key
        let key_str = service.make_cache_key_string(&table_id, None);
        assert_eq!(key_str, "myns:mytable:shared");

        let parsed = service.parse_key_string(&key_str);
        assert!(parsed.is_some());
        let (parsed_table_id, parsed_user_id) = parsed.unwrap();
        assert_eq!(parsed_table_id, table_id);
        assert_eq!(parsed_user_id, None);

        // Test user table key
        let user_id = UserId::from("u_test");
        let key_str = service.make_cache_key_string(&table_id, Some(&user_id));
        assert_eq!(key_str, "myns:mytable:u_test");

        let parsed = service.parse_key_string(&key_str);
        assert!(parsed.is_some());
        let (parsed_table_id, parsed_user_id) = parsed.unwrap();
        assert_eq!(parsed_table_id, table_id);
        assert_eq!(parsed_user_id, Some(user_id));
    }
}
