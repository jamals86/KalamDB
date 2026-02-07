use datafusion::logical_expr::LogicalPlan;
use kalamdb_commons::{NamespaceId, Role};
use moka::sync::Cache;
use std::sync::Arc;
use std::time::Duration;

/// Default maximum cache entries (prevents unbounded memory growth)
const DEFAULT_MAX_ENTRIES: u64 = 1000;
/// Default cache idle TTL in seconds (evict unused plans)
const DEFAULT_IDLE_TTL_SECS: u64 = 900;

/// Cache key for plan lookup.
///
/// Plans are scoped by namespace + role + SQL text.
/// User ID is NOT included because:
/// - LogicalPlan is user-agnostic (same plan structure for all users)
/// - User filtering happens at scan time in UserTableProvider, not at planning
/// - This allows plan reuse across users for significant cache efficiency
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlanCacheKey {
    pub namespace: NamespaceId,
    pub role: Role,
    pub sql: String,
}

impl PlanCacheKey {
    pub fn new(namespace: NamespaceId, role: Role, sql: impl Into<String>) -> Self {
        Self {
            namespace,
            role,
            sql: sql.into(),
        }
    }
}

/// Caches optimized LogicalPlans to skip parsing and planning overhead.
///
/// DataFusion's planning phase (SQL -> AST -> LogicalPlan -> Optimized Plan)
/// can take 1-5ms. Caching the optimized plan allows skipping these steps
/// for recurring queries.
///
/// **Memory Management**: Uses moka cache with TinyLFU eviction (LRU + LFU admission)
/// for optimal hit rate. Automatically evicts entries when max capacity is reached.
pub struct PlanCache {
    cache: Cache<PlanCacheKey, Arc<LogicalPlan>>,
}

impl PlanCache {
    /// Create a new PlanCache with default max entries (1000)
    pub fn new() -> Self {
        Self::with_config(DEFAULT_MAX_ENTRIES, Duration::from_secs(DEFAULT_IDLE_TTL_SECS))
    }

    /// Create a new PlanCache with specified max entries and idle TTL
    pub fn with_config(max_entries: u64, idle_ttl: Duration) -> Self {
        let cache = Cache::builder().max_capacity(max_entries).time_to_idle(idle_ttl).build();

        Self { cache }
    }

    /// Retrieve a cached plan for the given key
    pub fn get(&self, cache_key: &PlanCacheKey) -> Option<Arc<LogicalPlan>> {
        self.cache.get(cache_key)
    }

    /// Store an optimized plan (moka handles eviction automatically)
    pub fn insert(&self, cache_key: PlanCacheKey, plan: LogicalPlan) {
        self.cache.insert(cache_key, Arc::new(plan));
    }

    /// Clear cache (MUST be called on any DDL operation like CREATE/DROP/ALTER)
    pub fn clear(&self) {
        self.cache.invalidate_all();
        // Moka executes some invalidations asynchronously; flush pending work so
        // follow-up size checks (tests/diagnostics) observe the cleared state.
        self.cache.run_pending_tasks();
    }

    /// Get current cache size
    pub fn len(&self) -> usize {
        // Moka's entry_count can lag behind without flushing pending tasks.
        self.cache.run_pending_tasks();
        self.cache.entry_count() as usize
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.run_pending_tasks();
        self.cache.entry_count() == 0
    }
}

impl Default for PlanCache {
    fn default() -> Self {
        Self::new()
    }
}
