use crate::schema_registry::cached_table_data::CachedTableData;
use dashmap::DashMap;
use kalamdb_commons::models::{TableId, TableVersionId};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Unified schema cache for table metadata and schemas
///
/// Replaces dual-cache architecture with single DashMap for all table data.
///
/// **Phase 16 - Versioned Cache**: Supports both:
/// 1. Latest version lookups by `TableId` (fast path for most operations)
/// 2. Specific version lookups by `TableVersionId` (for reading old Parquet files)
///
/// **Performance Optimization**: Cache access updates `last_accessed_ms` via an
/// atomic field stored inside `CachedTableData`. This avoids a separate
/// timestamps map (which would duplicate keys and add extra DashMap overhead)
/// while keeping per-access work O(1) and avoiding any deep clones.
#[derive(Debug)]
pub struct TableCache {
    /// Cached table data indexed by TableId (latest versions)
    cache: DashMap<TableId, Arc<CachedTableData>>,

    /// Cached table data indexed by TableVersionId (specific versions)
    /// Used for reading Parquet files with older schemas
    version_cache: DashMap<TableVersionId, Arc<CachedTableData>>,

    /// Maximum number of entries before LRU eviction (applies to both caches combined)
    max_size: usize,

    /// Cache hit count (for metrics)
    hits: AtomicU64,

    /// Cache miss count (for metrics)
    misses: AtomicU64,
}

impl TableCache {
    /// Create a new table cache with specified maximum size
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: DashMap::new(),
            version_cache: DashMap::new(),
            max_size,
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
    pub fn get(&self, table_id: &TableId) -> Option<Arc<CachedTableData>> {
        if let Some(entry) = self.cache.get(table_id) {
            self.hits.fetch_add(1, Ordering::Relaxed);

            // Update LRU timestamp directly on the cached value (atomic, no extra map)
            entry.value().touch_at(Self::current_timestamp());

            // Return Arc clone (cheap - just increments reference count)
            Some(Arc::clone(entry.value()))
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// Insert or update cached table data
    /// Returns the TableId of the evicted entry, if any
    pub fn insert(&self, table_id: TableId, data: Arc<CachedTableData>) -> Option<TableId> {
        let mut evicted = None;
        // Check if we need to evict before inserting
        if self.max_size > 0 && self.cache.len() >= self.max_size {
            evicted = self.evict_lru();
        }

        // Initialize last-accessed timestamp for LRU tracking
        data.touch_at(Self::current_timestamp());
        self.cache.insert(table_id, data);

        evicted
    }

    /// Invalidate (remove) cached table data
    pub fn invalidate(&self, table_id: &TableId) {
        self.cache.remove(table_id);
    }

    /// Invalidate all versions of a table (for DROP TABLE)
    pub fn invalidate_all_versions(&self, table_id: &TableId) {
        // Remove from latest cache
        self.cache.remove(table_id);
        
        // Remove all versioned entries for this table
        // Note: DashMap doesn't support prefix deletion, so we iterate
        let keys_to_remove: Vec<TableVersionId> = self
            .version_cache
            .iter()
            .filter(|entry| entry.key().table_id() == table_id)
            .map(|entry| entry.key().clone())
            .collect();
        
        for key in keys_to_remove {
            self.version_cache.remove(&key);
        }
    }

    // ===== Versioned Cache Methods (Phase 16) =====

    /// Get cached table data by specific version
    ///
    /// Used when reading Parquet files that were written with older schemas.
    pub fn get_version(&self, version_id: &TableVersionId) -> Option<Arc<CachedTableData>> {
        if let Some(entry) = self.version_cache.get(version_id) {
            self.hits.fetch_add(1, Ordering::Relaxed);
            entry.value().touch_at(Self::current_timestamp());
            Some(Arc::clone(entry.value()))
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// Insert a specific version into the cache
    pub fn insert_version(&self, version_id: TableVersionId, data: Arc<CachedTableData>) {
        // Check combined size for eviction
        let total_size = self.cache.len() + self.version_cache.len();
        if self.max_size > 0 && total_size >= self.max_size {
            self.evict_version_lru();
        }

        data.touch_at(Self::current_timestamp());
        self.version_cache.insert(version_id, data);
    }

    /// Evict least-recently-used entry from version cache
    fn evict_version_lru(&self) {
        let mut oldest_key: Option<TableVersionId> = None;
        let mut oldest_timestamp = u64::MAX;

        for entry in self.version_cache.iter() {
            let ts = entry.value().last_accessed_ms();
            if ts < oldest_timestamp {
                oldest_timestamp = ts;
                oldest_key = Some(entry.key().clone());
            }
        }

        if let Some(key) = &oldest_key {
            self.version_cache.remove(key);
        }
    }

    /// Evict least-recently-used entry from cache
    fn evict_lru(&self) -> Option<TableId> {
        let mut oldest_key: Option<TableId> = None;
        let mut oldest_timestamp = u64::MAX;

        // Find entry with oldest last_accessed timestamp
        for entry in self.cache.iter() {
            let ts = entry.value().last_accessed_ms();
            if ts < oldest_timestamp {
                oldest_timestamp = ts;
                oldest_key = Some(entry.key().clone());
            }
        }

        // Remove oldest entry
        if let Some(key) = &oldest_key {
            self.cache.remove(key);
        }

        oldest_key
    }

    /// Get cache hit rate (for metrics)
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
    pub fn stats(&self) -> (usize, u64, u64, f64) {
        let size = self.cache.len();
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let hit_rate = self.hit_rate();

        (size, hits, misses, hit_rate)
    }

    /// Clear all cached data
    pub fn clear(&self) {
        self.cache.clear();
        self.version_cache.clear();
        self.hits.store(0, Ordering::Relaxed);
        self.misses.store(0, Ordering::Relaxed);
    }

    /// Get number of cached entries (latest versions only)
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Get total number of cached entries (latest + versioned)
    pub fn total_len(&self) -> usize {
        self.cache.len() + self.version_cache.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty() && self.version_cache.is_empty()
    }
}
