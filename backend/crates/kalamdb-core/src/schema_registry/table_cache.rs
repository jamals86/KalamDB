use crate::schema_registry::cached_table_data::CachedTableData;
use dashmap::DashMap;
use kalamdb_commons::models::{NamespaceId, TableId, TableName};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Unified schema cache for table metadata and schemas
///
/// Replaces dual-cache architecture with single DashMap for all table data.
///
/// **Performance Optimization**: LRU timestamps are stored separately to avoid
/// cloning large CachedTableData structs on every access.
#[derive(Debug)]
pub struct TableCache {
    /// Cached table data indexed by TableId
    cache: DashMap<TableId, Arc<CachedTableData>>,

    /// LRU timestamps indexed by TableId (separate to avoid cloning CachedTableData)
    lru_timestamps: DashMap<TableId, AtomicU64>,

    /// Maximum number of entries before LRU eviction
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
            lru_timestamps: DashMap::new(),
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
    pub fn get_by_name(
        &self,
        namespace: &NamespaceId,
        table_name: &TableName,
    ) -> Option<Arc<CachedTableData>> {
        let table_id = TableId::new(namespace.clone(), table_name.clone());
        self.get(&table_id)
    }

    /// Insert or update cached table data
    pub fn insert(&self, table_id: TableId, data: Arc<CachedTableData>) {
        // Check if we need to evict before inserting
        if self.max_size > 0 && self.cache.len() >= self.max_size {
            self.evict_lru();
        }

        // Insert data and initialize LRU timestamp
        self.lru_timestamps
            .insert(table_id.clone(), AtomicU64::new(Self::current_timestamp()));
        self.cache.insert(table_id, data);
    }

    /// Invalidate (remove) cached table data
    pub fn invalidate(&self, table_id: &TableId) {
        self.cache.remove(table_id);
        self.lru_timestamps.remove(table_id);
    }

    /// Evict least-recently-used entry from cache
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
