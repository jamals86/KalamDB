use datafusion::logical_expr::LogicalPlan;
use moka::sync::Cache;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Default maximum cache entries (prevents unbounded memory growth)
const DEFAULT_MAX_ENTRIES: u64 = 1000;

/// Caches optimized LogicalPlans to skip parsing and planning overhead.
///
/// DataFusion's planning phase (SQL -> AST -> LogicalPlan -> Optimized Plan)
/// can take 1-5ms. Caching the optimized plan allows skipping these steps
/// for recurring queries.
///
/// **Memory Management**: Uses moka cache with TinyLFU eviction (LRU + LFU admission)
/// for optimal hit rate. Automatically evicts entries when max capacity is reached.
#[derive(Debug, Clone)]
pub struct PlanCache {
    /// Moka cache with automatic eviction
    cache: Arc<Cache<String, LogicalPlan>>,
    /// Hit counter for metrics
    hits: Arc<AtomicU64>,
    /// Miss counter for metrics
    misses: Arc<AtomicU64>,
}

impl PlanCache {
    /// Create a new PlanCache with default max entries (1000)
    pub fn new() -> Self {
        Self::with_max_entries(DEFAULT_MAX_ENTRIES)
    }

    /// Create a new PlanCache with specified max entries
    pub fn with_max_entries(max_entries: u64) -> Self {
        let cache = Cache::builder()
            .max_capacity(max_entries)
            .build();

        Self {
            cache: Arc::new(cache),
            hits: Arc::new(AtomicU64::new(0)),
            misses: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Retrieve a cached plan for the given key
    pub fn get(&self, cache_key: &str) -> Option<LogicalPlan> {
        if let Some(plan) = self.cache.get(cache_key) {
            self.hits.fetch_add(1, Ordering::Relaxed);
            Some(plan)
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// Store an optimized plan (moka handles eviction automatically)
    pub fn insert(&self, cache_key: String, plan: LogicalPlan) {
        self.cache.insert(cache_key, plan);
    }

    /// Clear cache (MUST be called on any DDL operation like CREATE/DROP/ALTER)
    pub fn clear(&self) {
        self.cache.invalidate_all();
    }

    /// Get current cache size
    pub fn len(&self) -> usize {
        self.cache.entry_count() as usize
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.entry_count() == 0
    }

    /// Get cache hit count
    pub fn hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    /// Get cache miss count
    pub fn misses(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }
}

impl Default for PlanCache {
    fn default() -> Self {
        Self::new()
    }
}
