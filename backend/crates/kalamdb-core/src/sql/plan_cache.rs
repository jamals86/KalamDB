use dashmap::DashMap;
use datafusion::logical_expr::LogicalPlan;
use std::sync::Arc;

/// Caches optimized LogicalPlans to skip parsing and planning overhead.
///
/// DataFusion's planning phase (SQL -> AST -> LogicalPlan -> Optimized Plan)
/// can take 1-5ms. Caching the optimized plan allows skipping these steps
/// for recurring queries.
#[derive(Debug, Clone)]
pub struct PlanCache {
    /// Map of SQL query string -> Optimized LogicalPlan
    cache: Arc<DashMap<String, LogicalPlan>>,
}

impl PlanCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
        }
    }

    /// Retrieve a cached plan for the given SQL
    pub fn get(&self, sql: &str) -> Option<LogicalPlan> {
        self.cache.get(sql).map(|entry| entry.value().clone())
    }

    /// Store an optimized plan
    pub fn insert(&self, sql: String, plan: LogicalPlan) {
        self.cache.insert(sql, plan);
    }

    /// Clear cache (MUST be called on any DDL operation like CREATE/DROP/ALTER)
    pub fn clear(&self) {
        self.cache.clear();
    }
}

impl Default for PlanCache {
    fn default() -> Self {
        Self::new()
    }
}
