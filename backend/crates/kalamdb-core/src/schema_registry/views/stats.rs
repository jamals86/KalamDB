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
use std::sync::{Arc, OnceLock};

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
/// - Computes batch dynamically in compute_batch()
#[derive(Debug)]
pub struct StatsView {
    // SchemaRegistry moved to kalamdb-core, so we can't use it here
    // TODO: Pass metrics via a trait or callback if needed
}

impl StatsView {
    /// Create a new stats view
    pub fn new(_schema_registry: Option<Arc<()>>) -> Self {
        Self {}
    }
}

impl VirtualView for StatsView {
    fn schema(&self) -> SchemaRef {
        stats_schema()
    }

    fn compute_batch(&self) -> Result<RecordBatch, super::super::error::RegistryError> {
        let mut names = StringBuilder::new();
        let mut values = StringBuilder::new();

        // Schema cache metrics (SchemaRegistry moved to kalamdb-core)
        names.append_value("schema_cache_hit_rate");
        values.append_value("N/A");

        names.append_value("schema_cache_size");
        values.append_value("0");

        // Placeholders for future metrics
        names.append_value("type_conversion_cache_hit_rate");
        values.append_value("N/A");

        names.append_value("server_uptime_seconds");
        values.append_value("N/A");

        RecordBatch::try_new(
            self.schema(),
            vec![
                Arc::new(names.finish()) as ArrayRef,
                Arc::new(values.finish()) as ArrayRef,
            ],
        )
        .map_err(|e| super::super::error::RegistryError::Other(format!("Failed to build stats batch: {}", e)))
    }

    fn view_name(&self) -> &str {
        "system.stats"
    }
}

// Re-export as StatsTableProvider for backward compatibility
pub type StatsTableProvider = super::view_base::ViewTableProvider<StatsView>;

/// Helper function to create a stats table provider
pub fn create_stats_provider(_schema_registry: Option<Arc<()>>) -> StatsTableProvider {
    StatsTableProvider::new(Arc::new(StatsView::new(None)))
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
    fn test_stats_view_compute() {
        let view = StatsView::new(None);
        let batch = view.compute_batch().expect("compute batch");
        assert!(batch.num_rows() >= 3); // at least the placeholder metrics
        assert_eq!(batch.num_columns(), 2);
    }

    #[test]
    fn test_table_provider() {
        let view = Arc::new(StatsView::new(None));
        let provider = StatsTableProvider::new(view);
        use datafusion::datasource::TableProvider;
        use datafusion::datasource::TableType;
        
        assert_eq!(provider.table_type(), TableType::View);
        assert_eq!(provider.schema().fields().len(), 2);
    }
}
