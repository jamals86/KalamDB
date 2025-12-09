//! system.stats virtual view
//!
//! **Type**: Virtual View (not backed by persistent storage)
//!
//! Provides runtime metrics as key-value pairs for observability by gathering
//! data from the system and returning it to DataFusion as a query result.
//!
//! **DataFusion Pattern**: Implements VirtualView trait for consistent view behavior
//! - Computes metrics dynamically on each query
//! - No persistent state in RocksDB
//! - Uses MemTable for efficient DataFusion integration

use super::view_base::VirtualView;
use datafusion::arrow::array::{ArrayRef, StringBuilder};
use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use datafusion::arrow::record_batch::RecordBatch;
use std::sync::{Arc, OnceLock, RwLock};

/// Metrics provider callback type
/// Returns a vector of (metric_name, metric_value) tuples
pub type MetricsCallback = Arc<dyn Fn() -> Vec<(String, String)> + Send + Sync>;

/// Static schema for system.stats
static STATS_SCHEMA: OnceLock<SchemaRef> = OnceLock::new();

/// Get or initialize the stats schema
fn stats_schema() -> SchemaRef {
    STATS_SCHEMA
        .get_or_init(|| {
            Arc::new(Schema::new(vec![
                Field::new("metric_name", DataType::Utf8, false),
                Field::new("metric_value", DataType::Utf8, false),
            ]))
        })
        .clone()
}

/// Virtual view that emits key-value metrics computed at query time
///
/// **DataFusion Design**:
/// - Implements VirtualView trait
/// - Returns TableType::View
/// - Computes batch dynamically in compute_batch() using the metrics callback
pub struct StatsView {
    /// Callback to fetch metrics from AppContext
    /// Set after AppContext initialization to avoid circular dependencies
    metrics_callback: Arc<RwLock<Option<MetricsCallback>>>,
}

impl std::fmt::Debug for StatsView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StatsView")
            .field("has_callback", &self.metrics_callback.read().unwrap().is_some())
            .finish()
    }
}

impl StatsView {
    /// Create a new stats view without a callback (placeholder mode)
    pub fn new() -> Self {
        Self {
            metrics_callback: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a new stats view with a metrics callback
    pub fn with_callback(callback: MetricsCallback) -> Self {
        Self {
            metrics_callback: Arc::new(RwLock::new(Some(callback))),
        }
    }

    /// Set the metrics callback (called after AppContext init)
    pub fn set_metrics_callback(&self, callback: MetricsCallback) {
        *self.metrics_callback.write().unwrap() = Some(callback);
    }
}

impl Default for StatsView {
    fn default() -> Self {
        Self::new()
    }
}

impl VirtualView for StatsView {
    fn schema(&self) -> SchemaRef {
        stats_schema()
    }

    fn compute_batch(&self) -> Result<RecordBatch, super::super::error::RegistryError> {
        let mut names = StringBuilder::new();
        let mut values = StringBuilder::new();

        // Try to get metrics from the callback
        let callback_guard = self.metrics_callback.read().unwrap();
        if let Some(ref callback) = *callback_guard {
            // Use real metrics from AppContext
            let metrics = callback();
            for (name, value) in metrics {
                names.append_value(&name);
                values.append_value(&value);
            }
        } else {
            // Fallback to placeholder metrics when callback not set
            names.append_value("server_uptime_seconds");
            values.append_value("N/A (metrics callback not initialized)");

            names.append_value("total_users");
            values.append_value("N/A");

            names.append_value("total_namespaces");
            values.append_value("N/A");

            names.append_value("total_tables");
            values.append_value("N/A");
        }

        RecordBatch::try_new(
            self.schema(),
            vec![
                Arc::new(names.finish()) as ArrayRef,
                Arc::new(values.finish()) as ArrayRef,
            ],
        )
        .map_err(|e| {
            super::super::error::RegistryError::Other(format!("Failed to build stats batch: {}", e))
        })
    }

    fn view_name(&self) -> &str {
        "system.stats"
    }
}

// Re-export as StatsTableProvider for backward compatibility
pub type StatsTableProvider = super::view_base::ViewTableProvider<StatsView>;

/// Helper function to create a stats table provider
pub fn create_stats_provider() -> StatsTableProvider {
    StatsTableProvider::new(Arc::new(StatsView::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats_schema() {
        let schema = stats_schema();
        assert_eq!(schema.fields().len(), 2);
        assert_eq!(schema.field(0).name(), "metric_name");
        assert_eq!(schema.field(1).name(), "metric_value");
    }

    #[test]
    fn test_stats_view_compute_without_callback() {
        let view = StatsView::new();
        let batch = view.compute_batch().expect("compute batch");
        assert!(batch.num_rows() >= 3); // at least the placeholder metrics
        assert_eq!(batch.num_columns(), 2);
    }

    #[test]
    fn test_stats_view_compute_with_callback() {
        let callback: MetricsCallback = Arc::new(|| {
            vec![
                ("test_metric_1".to_string(), "100".to_string()),
                ("test_metric_2".to_string(), "200".to_string()),
            ]
        });
        let view = StatsView::with_callback(callback);
        let batch = view.compute_batch().expect("compute batch");
        assert_eq!(batch.num_rows(), 2);
        assert_eq!(batch.num_columns(), 2);
    }

    #[test]
    fn test_table_provider() {
        let view = Arc::new(StatsView::new());
        let provider = StatsTableProvider::new(view);
        use datafusion::datasource::TableProvider;
        use datafusion::datasource::TableType;

        assert_eq!(provider.table_type(), TableType::View);
        assert_eq!(provider.schema().fields().len(), 2);
    }
}
