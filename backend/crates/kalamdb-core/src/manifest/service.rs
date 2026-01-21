//! Unified ManifestService for batch file metadata tracking.
//!
//! Provides manifest.json management with two-tier caching:
//! 1. Hot cache (moka) for sub-millisecond lookups with automatic TTI-based eviction
//! 2. Persistent cache (RocksDB) for crash recovery
//! 3. Cold storage (filestore) for manifest.json files
//!
//! Key type: (TableId, Option<UserId>) for type-safe cache access.

use crate::schema_registry::SchemaRegistry;
use kalamdb_commons::models::types::{Manifest, ManifestCacheEntry, SegmentMetadata, SyncState};
use kalamdb_commons::{TableId, UserId};
use kalamdb_configs::ManifestCacheSettings;
use kalamdb_filestore::StorageRegistry;
use kalamdb_store::entity_store::{EntityStore, KSerializable};
use kalamdb_store::{Partition, StorageBackend, StorageError};
use kalamdb_system::providers::manifest::ManifestCacheKey;
use kalamdb_system::providers::ManifestTableProvider;
use log::{debug, info, warn};
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;

const MAX_MANIFEST_SCAN_LIMIT: usize = 100000;

/// Unified ManifestService with hot cache + RocksDB persistence + cold storage.
///
/// Architecture:
/// - Persistent store: RocksDB manifest_cache column family for crash recovery
/// - Cold store: manifest.json files in filestore (S3/local filesystem)
pub struct ManifestService {
    /// Provider wrapping the store
    provider: Arc<ManifestTableProvider>,

    /// Configuration settings
    config: ManifestCacheSettings,

    /// Optional registries for path/object store resolution.
    ///
    /// In production these are injected via `new_with_registries()`
    schema_registry: Option<Arc<SchemaRegistry>>,
    storage_registry: Option<Arc<StorageRegistry>>,
}

impl ManifestService {
    /// Create a new ManifestService
    pub fn new(
        provider: Arc<ManifestTableProvider>,
        config: ManifestCacheSettings,
    ) -> Self {
        Self {
            provider,
            config,
            schema_registry: None,
            storage_registry: None,
        }
    }

    /// Create a ManifestService with injected registries (compat helper for tests).
    pub fn new_with_registries(
        backend: Arc<dyn StorageBackend>,
        _base_path: String,
        config: ManifestCacheSettings,
        schema_registry: Arc<SchemaRegistry>,
        storage_registry: Arc<StorageRegistry>,
    ) -> Self {
        let provider = Arc::new(ManifestTableProvider::new(backend));
        let mut service = Self::new(provider, config);
        service.set_schema_registry(schema_registry);
        service.set_storage_registry(storage_registry);
        service
    }

    /// Set SchemaRegistry (break circular dependency)
    pub fn set_schema_registry(&mut self, registry: Arc<SchemaRegistry>) {
        self.schema_registry = Some(registry);
    }

    /// Set StorageRegistry
    pub fn set_storage_registry(&mut self, registry: Arc<StorageRegistry>) {
        self.storage_registry = Some(registry);
    }
    
    // Internal helper to get registries (panics if not set in production flows)
    fn get_schema_registry(&self) -> &Arc<SchemaRegistry> {
         self.schema_registry.as_ref().expect("SchemaRegistry not initialized in ManifestService")
    }

    fn get_storage_registry(&self) -> &Arc<StorageRegistry> {
         self.storage_registry.as_ref().expect("StorageRegistry not initialized in ManifestService")
    }

    // ========== Cache Operations (now mostly passthrough to RocksDB) ==========

    /// Get or load a manifest cache entry.
    ///
    /// Flow:
    /// 1. Check RocksDB CF
    /// 2. Return Option<Arc<ManifestCacheEntry>>
    pub fn get_or_load(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<Option<Arc<ManifestCacheEntry>>, StorageError> {
        let rocksdb_key = ManifestCacheKey::from(self.make_cache_key_string(table_id, user_id));
        match EntityStore::get(self.provider.store(), &rocksdb_key) {
            Ok(Some(entry)) => Ok(Some(Arc::new(entry))),
            Ok(None) => Ok(None),
            Err(StorageError::SerializationError(err)) => {
                warn!(
                    "Manifest cache entry corrupted for key {}: {} (dropping)",
                    rocksdb_key.as_str(),
                    err
                );
                let _ = EntityStore::delete(self.provider.store(), &rocksdb_key);
                Ok(None)
            }
            Err(err) => Err(err),
        }
    }

    /// Count all cached manifest entries.
    pub fn count(&self) -> Result<usize, StorageError> {
        let partition = Partition::new(self.provider.store().partition());
        let iter = self
            .provider
            .store()
            .backend()
            .scan(&partition, None, None, Some(MAX_MANIFEST_SCAN_LIMIT))?;
        let mut count = 0usize;
        for _ in iter {
            count += 1;
        }
        Ok(count)
    }

    /// Return all cached entries with their storage keys.
    pub fn get_all(&self) -> Result<Vec<(String, ManifestCacheEntry)>, StorageError> {
        let partition = Partition::new(self.provider.store().partition());
        let iter = self
            .provider
            .store()
            .backend()
            .scan(&partition, None, None, Some(MAX_MANIFEST_SCAN_LIMIT))?;
        let mut results = Vec::new();
        for (key_bytes, value_bytes) in iter {
            match ManifestCacheEntry::decode(&value_bytes) {
                Ok(entry) => {
                    let key = String::from_utf8(key_bytes).unwrap_or_default();
                    results.push((key, entry));
                }
                Err(StorageError::SerializationError(err)) => {
                    warn!(
                        "Manifest cache entry corrupted for key {}: {} (dropping)",
                        String::from_utf8_lossy(&key_bytes),
                        err
                    );
                    let _ = self
                        .provider
                        .store()
                        .backend()
                        .delete(&partition, &key_bytes);
                }
                Err(err) => return Err(err),
            }
        }
        Ok(results)
    }

    /// Clear all cached entries.
    pub fn clear(&self) -> Result<(), StorageError> {
        let partition = Partition::new(self.provider.store().partition());
        let iter = self
            .provider
            .store()
            .backend()
            .scan(&partition, None, None, Some(MAX_MANIFEST_SCAN_LIMIT))?;
        let keys: Vec<Vec<u8>> = iter.map(|(key_bytes, _)| key_bytes).collect();

        for key_bytes in keys {
            self.provider
                .store()
                .backend()
                .delete(&partition, &key_bytes)?;
        }
        Ok(())
    }

    /// Return shared/user counts and total weighted capacity for monitoring.
    pub fn cache_stats(&self) -> Result<(usize, usize, usize), StorageError> {
        let mut shared_count = 0usize;
        let mut user_count = 0usize;
        let partition = Partition::new(self.provider.store().partition());
        let iter = self
            .provider
            .store()
            .backend()
            .scan(&partition, None, None, Some(MAX_MANIFEST_SCAN_LIMIT))?;
        for (key_bytes, value_bytes) in iter {
            match ManifestCacheEntry::decode(&value_bytes) {
                Ok(entry) => {
                    if entry.manifest.user_id.is_some() {
                        user_count += 1;
                    } else {
                        shared_count += 1;
                    }
                }
                Err(StorageError::SerializationError(err)) => {
                    warn!(
                        "Manifest cache entry corrupted for key {}: {} (dropping)",
                        String::from_utf8_lossy(&key_bytes),
                        err
                    );
                    let _ = self
                        .provider
                        .store()
                        .backend()
                        .delete(&partition, &key_bytes);
                }
                Err(err) => return Err(err),
            }
        }
        let weight_factor = self.config.user_table_weight_factor as usize;
        let total_weight = shared_count + (user_count * weight_factor);
        Ok((shared_count, user_count, total_weight))
    }

    /// Compute max weighted capacity based on configuration.
    pub fn max_weighted_capacity(&self) -> usize {
        self.config.max_entries * self.config.user_table_weight_factor as usize
    }

    /// Update manifest cache after successful flush.
    pub fn update_after_flush(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
        manifest: &Manifest,
        etag: Option<String>,
    ) -> Result<(), StorageError> {
        self.upsert_cache_entry(table_id, user_id, manifest, etag, SyncState::InSync)
    }

    /// Stage manifest metadata in the cache before the first flush writes manifest.json to disk.
    pub fn stage_before_flush(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
        manifest: &Manifest,
    ) -> Result<(), StorageError> {
        self.upsert_cache_entry(
            table_id,
            user_id,
            manifest,
            None,
            SyncState::PendingWrite,
        )
    }

    /// Mark a cache entry as stale.
    pub fn mark_as_stale(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<(), StorageError> {
        let rocksdb_key = ManifestCacheKey::from(self.make_cache_key_string(table_id, user_id));

        if let Some(mut entry) = EntityStore::get(self.provider.store(), &rocksdb_key)? {
            entry.mark_stale();
            EntityStore::put(self.provider.store(), &rocksdb_key, &entry)?;
        }

        Ok(())
    }

    /// Mark a cache entry as having an error state.
    pub fn mark_as_error(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<(), StorageError> {
        let rocksdb_key = ManifestCacheKey::from(self.make_cache_key_string(table_id, user_id));

        match EntityStore::get(self.provider.store(), &rocksdb_key) {
            Ok(Some(mut entry)) => {
                entry.mark_error();
                EntityStore::put(self.provider.store(), &rocksdb_key, &entry)?;
            }
            Ok(None) => {}
            Err(StorageError::SerializationError(err)) => {
                warn!(
                    "Manifest cache entry corrupted for key {}: {} (dropping)",
                    rocksdb_key.as_str(),
                    err
                );
                let _ = EntityStore::delete(self.provider.store(), &rocksdb_key);
            }
            Err(err) => return Err(err),
        }

        Ok(())
    }

    /// Mark a cache entry as syncing (flush in progress).
    pub fn mark_syncing(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<(), StorageError> {
        let rocksdb_key = ManifestCacheKey::from(self.make_cache_key_string(table_id, user_id));

        match EntityStore::get(self.provider.store(), &rocksdb_key) {
            Ok(Some(mut entry)) => {
                entry.mark_syncing();
                EntityStore::put(self.provider.store(), &rocksdb_key, &entry)?;
            }
            Ok(None) => {}
            Err(StorageError::SerializationError(err)) => {
                warn!(
                    "Manifest cache entry corrupted for key {}: {} (dropping)",
                    rocksdb_key.as_str(),
                    err
                );
                let _ = EntityStore::delete(self.provider.store(), &rocksdb_key);
            }
            Err(err) => return Err(err),
        }

        Ok(())
    }

    /// Mark a cache entry as having pending writes (hot data not yet flushed to cold storage).
    /// 
    /// This should be called after any write operation (INSERT, UPDATE, DELETE) to indicate
    /// that the RocksDB hot store has data that needs to be flushed to Parquet cold storage.
    /// The sync_state will transition from InSync to PendingWrite.
    pub fn mark_pending_write(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<(), StorageError> {
        let rocksdb_key = ManifestCacheKey::from(self.make_cache_key_string(table_id, user_id));

        match EntityStore::get(self.provider.store(), &rocksdb_key) {
            Ok(Some(mut entry)) => {
                entry.mark_pending_write();
                EntityStore::put(self.provider.store(), &rocksdb_key, &entry)?;
                debug!(
                    "Marked manifest entry as pending_write: table={}, user={:?}",
                    table_id,
                    user_id.map(|u| u.as_str())
                );
            }
            Ok(None) => {
                // If no cache entry exists yet, create one with PendingWrite state
                // This shouldn't happen in normal flow since ensure_manifest_ready is called first
                warn!(
                    "mark_pending_write called but no cache entry exists: table={}, user={:?}",
                    table_id,
                    user_id.map(|u| u.as_str())
                );
            }
            Err(StorageError::SerializationError(err)) => {
                warn!(
                    "Manifest cache entry corrupted for key {}: {} (dropping)",
                    rocksdb_key.as_str(),
                    err
                );
                let _ = EntityStore::delete(self.provider.store(), &rocksdb_key);
            }
            Err(err) => return Err(err),
        }

        Ok(())
    }

    /// Validate freshness of cached entry based on TTL.
    pub fn validate_freshness(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<bool, StorageError> {
        let rocksdb_key = ManifestCacheKey::from(self.make_cache_key_string(table_id, user_id));

        if let Some(entry) = EntityStore::get(self.provider.store(), &rocksdb_key)? {
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
        let rocksdb_key = ManifestCacheKey::from(self.make_cache_key_string(table_id, user_id));
        EntityStore::delete(self.provider.store(), &rocksdb_key)
    }

    /// Invalidate all cache entries for a table (all users + shared).
    pub fn invalidate_table(&self, table_id: &TableId) -> Result<usize, StorageError> {
        let key_prefix = format!(
            "{}:",
            table_id // TableId Display: "namespace:table"
        );

        let mut invalidated = 0;
        let partition = Partition::new(self.provider.store().partition());
        let iter = self
            .provider
            .store()
            .backend()
            .scan(
                &partition,
                Some(key_prefix.as_bytes()),
                None,
                Some(MAX_MANIFEST_SCAN_LIMIT),
            )?;
        let keys: Vec<Vec<u8>> = iter.map(|(key_bytes, _)| key_bytes).collect();

        for key_bytes in keys {
            self.provider
                .store()
                .backend()
                .delete(&partition, &key_bytes)?;
            invalidated += 1;
        }

        debug!("Invalidated {} manifest cache entries for table {}", invalidated, table_id);

        Ok(invalidated)
    }

    /// Check if a cache key is currently in the hot cache (RAM).
    /// With no hot cache, we check RocksDB existence.
    pub fn is_in_hot_cache(&self, table_id: &TableId, user_id: Option<&UserId>) -> bool {
         let rocksdb_key = ManifestCacheKey::from(self.make_cache_key_string(table_id, user_id));
         EntityStore::get(self.provider.store(), &rocksdb_key).unwrap_or(None).is_some()
    }

    /// Check if a cache key string is in hot cache (for system.manifest table compatibility).
    pub fn is_in_hot_cache_by_string(&self, cache_key_str: &str) -> bool {
        let rocksdb_key = ManifestCacheKey::from(cache_key_str);
        EntityStore::get(self.provider.store(), &rocksdb_key).unwrap_or(None).is_some()
    }

    /// Evict stale manifest entries from RocksDB.
    pub fn evict_stale_entries(&self, ttl_seconds: i64) -> Result<usize, StorageError> {
        let now = chrono::Utc::now().timestamp();
        let cutoff = now - ttl_seconds;
        let mut evicted_count = 0;
        let partition = Partition::new(self.provider.store().partition());
        let iter = self
            .provider
            .store()
            .backend()
            .scan(&partition, None, None, Some(MAX_MANIFEST_SCAN_LIMIT))?;
        let mut delete_keys = Vec::new();

        for (key_bytes, value_bytes) in iter {
            match ManifestCacheEntry::decode(&value_bytes) {
                Ok(entry) => {
                    if entry.last_refreshed < cutoff {
                        delete_keys.push(key_bytes);
                    }
                }
                Err(StorageError::SerializationError(err)) => {
                    warn!(
                        "Manifest cache entry corrupted for key {}: {} (dropping)",
                        String::from_utf8_lossy(&key_bytes),
                        err
                    );
                    delete_keys.push(key_bytes);
                }
                Err(err) => return Err(err),
            }
        }

        for key_bytes in delete_keys {
            self.provider
                .store()
                .backend()
                .delete(&partition, &key_bytes)?;
            evicted_count += 1;
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

        // 2. Check Cold Store (via filestore)
        match self.read_manifest(table_id, user_id) {
            Ok(manifest) => {
                self.stage_before_flush(table_id, user_id, &manifest)?;
                return Ok(manifest);
            },
            Err(_) => {
                // Manifest doesn't exist or can't be read, create new one
            },
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

        self.upsert_cache_entry(
            table_id,
            user_id,
            &manifest,
            None,
            SyncState::PendingWrite,
        )?;

        Ok(manifest)
    }

    /// Flush manifest: Write to Cold Store (storage via StorageCached).
    pub fn flush_manifest(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<(), StorageError> {
        if let Some(entry) = self.get_or_load(table_id, user_id)? {
            let schema_registry = self.get_schema_registry();
            let storage_registry = self.get_storage_registry();

            let cached = schema_registry
                .get(table_id)
                .ok_or_else(|| StorageError::Other(format!("Table not found: {}", table_id)))?;
            
            let storage_cached = cached.storage_cached(storage_registry)
                .map_err(|e| StorageError::Other(e.to_string()))?;
            
            // Serialize to Value for write_manifest_sync
            let json_value = serde_json::to_value(&entry.manifest).map_err(|e| {
                StorageError::SerializationError(format!("Failed to serialize manifest: {}", e))
            })?;
            
            storage_cached.write_manifest_sync(
                cached.table.table_type,
                table_id,
                user_id,
                &json_value
            ).map_err(|e| StorageError::IoError(e.to_string()))?;
            
            debug!("Flushed manifest for {} (ver: {})", table_id, entry.manifest.version);
        } else {
            warn!("Attempted to flush manifest for {} but it was not in cache", table_id);
        }
        Ok(())
    }

    /// Read manifest.json from storage.
    pub fn read_manifest(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<Manifest, StorageError> {
        let schema_registry = self.get_schema_registry();
        let storage_registry = self.get_storage_registry();

        let cached = schema_registry
            .get(table_id)
            .ok_or_else(|| StorageError::Other(format!("Table not found: {}", table_id)))?;
        
        let storage_cached = cached.storage_cached(storage_registry)
            .map_err(|e| StorageError::Other(e.to_string()))?;
        
        let manifest_value = storage_cached.read_manifest_sync(
            cached.table.table_type,
            table_id, 
            user_id
        ).map_err(|e| StorageError::IoError(e.to_string()))?
        .ok_or_else(|| StorageError::Other("Manifest not found".to_string()))?;
        
        serde_json::from_value(manifest_value).map_err(|e| {
            StorageError::SerializationError(format!("Failed to deserialize manifest: {}", e))
        })
    }

    /// Rebuild manifest from Parquet footers.
    pub fn rebuild_manifest(
        &self,
        table_id: &TableId,
        _table_type: kalamdb_commons::models::schemas::TableType,
        user_id: Option<&UserId>,
    ) -> Result<Manifest, StorageError> {
        let schema_registry = self.get_schema_registry();
        let storage_registry = self.get_storage_registry();

        let cached = schema_registry
            .get(table_id)
            .ok_or_else(|| StorageError::Other(format!("Table not found: {}", table_id)))?;
        
        let storage_cached = cached.storage_cached(storage_registry)
            .map_err(|e| StorageError::Other(e.to_string()))?;
        
        let mut manifest = Manifest::new(table_id.clone(), user_id.cloned());

        // List all parquet files using the optimized method
        let mut batch_files = storage_cached.list_parquet_files_sync(
            cached.table.table_type,
            table_id,
            user_id
        ).map_err(|e| StorageError::IoError(e.to_string()))?;

        // Filter only batch files (exclude compaction temp files etc)
        batch_files.retain(|f| f.contains("batch-"));
        batch_files.sort();
        
        // Batch fetch metadata if possible (TODO: Add bulk head to filestore)
        // For now, sequential head
        for file_name in &batch_files {
            let id = file_name.clone();
            
            // Get file size via head operation
            let file_info = storage_cached.head_sync(
                cached.table.table_type,
                table_id,
                user_id,
                file_name
            ).map_err(|e| StorageError::IoError(e.to_string()))?;
            
            let size_bytes = file_info.size as u64;
            
            // Create segment metadata (we don't parse full footer for rebuild, just size)
            let segment = SegmentMetadata::new(id, file_name.clone(), HashMap::new(), 0, 0, 0, size_bytes);
            manifest.add_segment(segment);
        }
        
        self.upsert_cache_entry(
            table_id,
            user_id,
            &manifest,
            None,
            SyncState::InSync,
        )?;
        
        // Write manifest to storage using direct helper (Task 102)
        let json_value = serde_json::to_value(&manifest).map_err(|e| {
            StorageError::SerializationError(format!("Failed to serialize manifest: {}", e))
        })?;
        
        storage_cached.write_manifest_sync(
             cached.table.table_type,
             table_id,
             user_id,
             &json_value
        ).map_err(|e| StorageError::IoError(e.to_string()))?;

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
        let schema_registry = self.get_schema_registry();
        let storage_registry = self.get_storage_registry();

        let cached = schema_registry
            .get(table_id)
            .ok_or_else(|| StorageError::Other(format!("Table not found: {}", table_id)))?;
        
        let storage_cached = cached.storage_cached(storage_registry)
            .map_err(|e| StorageError::Other(e.to_string()))?;
        
        let manifest_path_result = storage_cached.get_manifest_path(
            cached.table.table_type,
            table_id,
            user_id
        );
        
        Ok(manifest_path_result.full_path)
    }

    pub fn get_manifest_user_ids(
        &self,
        table_id: &TableId,
    ) -> Result<Vec<UserId>, StorageError> {
        let prefix = format!("{}:", table_id);
        let partition = Partition::new(self.provider.store().partition());
        let iter = self
            .provider
            .store()
            .backend()
            .scan(
                &partition,
                Some(prefix.as_bytes()),
                None,
                Some(MAX_MANIFEST_SCAN_LIMIT),
            )?;
        let mut user_ids = HashSet::new();

        for (key_bytes, _value_bytes) in iter {
            let key_str = match std::str::from_utf8(&key_bytes) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let mut parts = key_str.splitn(3, ':');
            let _namespace = parts.next();
            let _table = parts.next();
            let scope = parts.next();
            if let Some(scope) = scope {
                if scope != "shared" {
                    user_ids.insert(UserId::from(scope));
                }
            }
        }

        Ok(user_ids.into_iter().collect())
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

    fn upsert_cache_entry(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
        manifest: &Manifest,
        etag: Option<String>,
        sync_state: SyncState,
    ) -> Result<(), StorageError> {
        let rocksdb_key = ManifestCacheKey::from(self.make_cache_key_string(table_id, user_id));
        let now = chrono::Utc::now().timestamp();

        let entry = ManifestCacheEntry::new(manifest.clone(), etag, now, sync_state);

        EntityStore::put(self.provider.store(), &rocksdb_key, &entry)?;

        Ok(())
    }

    // Private helper methods removed - now using StorageCached operations directly
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::models::schemas::TableType;
    use kalamdb_commons::{NamespaceId, TableName};
    use kalamdb_store::StorageBackend;
    use kalamdb_store::test_utils::InMemoryBackend;

    fn create_test_service() -> ManifestService {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let provider = Arc::new(ManifestTableProvider::new(backend));
        let config = ManifestCacheSettings {
            eviction_interval_seconds: 300,
            max_entries: 1000,
            eviction_ttl_days: 7,
            user_table_weight_factor: 10,
        };
        ManifestService::new(provider, config)
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

        let result = service.get_or_load(&table_id, Some(&UserId::from("u_123"))).unwrap();
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
            )
            .unwrap();

        let cached = service.get_or_load(&table_id, Some(&UserId::from("u_123"))).unwrap();
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
            )
            .unwrap();

        let result = service.get_or_load(&table_id, Some(&UserId::from("u_123"))).unwrap();
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
            )
            .unwrap();

        assert!(service.get_or_load(&table_id, Some(&UserId::from("u_123"))).unwrap().is_some());

        service.invalidate(&table_id, Some(&UserId::from("u_123"))).unwrap();

        assert!(service.get_or_load(&table_id, Some(&UserId::from("u_123"))).unwrap().is_none());
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
            )
            .unwrap();

        let cached = service.get_or_load(&table_id, Some(&UserId::from("u_123"))).unwrap().unwrap();
        assert_eq!(cached.sync_state, SyncState::InSync);

        service.mark_syncing(&table_id, Some(&UserId::from("u_123"))).unwrap();

        let cached_after =
            service.get_or_load(&table_id, Some(&UserId::from("u_123"))).unwrap().unwrap();
        assert_eq!(cached_after.sync_state, SyncState::Syncing);
    }

    // #[test]
    // fn test_restore_from_rocksdb() {
    //     let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
    //     let config = ManifestCacheSettings::default();

    //     let service1 = ManifestService::new(Arc::clone(&backend), config.clone());
    //     let table_id = build_table_id("ns1", "tbl1");
    //     let manifest = create_test_manifest(&table_id, Some(&UserId::from("u_123")));

    //     service1
    //         .update_after_flush(
    //             &table_id,
    //             Some(&UserId::from("u_123")),
    //             &manifest,
    //             None,
    //         )
    //         .unwrap();

    //     // Create new service (simulating restart)
    //     let service2 = ManifestService::new(backend, config);
    //     service2.restore_from_rocksdb().unwrap();

    //     let cached = service2
    //         .get_or_load(&table_id, Some(&UserId::from("u_123")))
    //         .unwrap();
    //     assert!(cached.is_some());
    // }
}
