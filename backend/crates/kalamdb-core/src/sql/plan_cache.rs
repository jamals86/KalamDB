use dashmap::DashMap;
use datafusion::logical_expr::LogicalPlan;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Default maximum cache entries (prevents unbounded memory growth)
const DEFAULT_MAX_ENTRIES: usize = 1000;

/// Caches optimized LogicalPlans to skip parsing and planning overhead.
///
/// DataFusion's planning phase (SQL -> AST -> LogicalPlan -> Optimized Plan)
/// can take 1-5ms. Caching the optimized plan allows skipping these steps
/// for recurring queries.
///
/// **Memory Management**: Limited to `max_entries` to prevent unbounded growth.
/// Uses random eviction when full (simple and fast for concurrent access).
#[derive(Debug, Clone)]
pub struct PlanCache {
    /// Map of scoped cache key -> Optimized LogicalPlan
    cache: Arc<DashMap<String, LogicalPlan>>,
    /// Maximum number of entries before eviction
    max_entries: usize,
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
    pub fn with_max_entries(max_entries: usize) -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
            max_entries,
            hits: Arc::new(AtomicU64::new(0)),
            misses: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Retrieve a cached plan for the given key
    pub fn get(&self, cache_key: &str) -> Option<LogicalPlan> {
        if let Some(entry) = self.cache.get(cache_key) {
            self.hits.fetch_add(1, Ordering::Relaxed);
            Some(entry.value().clone())
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// Store an optimized plan (evicts random entry if cache is full)
    pub fn insert(&self, cache_key: String, plan: LogicalPlan) {
        // Evict if at capacity (simple random eviction for performance)
        if self.max_entries > 0 && self.cache.len() >= self.max_entries {
            // Remove first entry we can find (fast, no ordering overhead)
            if let Some(entry) = self.cache.iter().next() {
                let key = entry.key().clone();
                drop(entry); // Release lock before removing
                self.cache.remove(&key);
            }
        }
        self.cache.insert(cache_key, plan);
    }

    /// Clear cache (MUST be called on any DDL operation like CREATE/DROP/ALTER)
    pub fn clear(&self) {
        self.cache.clear();
    }

    /// Get current cache size
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
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
