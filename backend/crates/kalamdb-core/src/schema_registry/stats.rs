//! system.stats virtual table
//!
//! Provides runtime metrics as key-value pairs for observability.
//! Uses a callback pattern to fetch metrics from AppContext to avoid circular dependencies.
//! **Schema**: TableDefinition provides consistent metadata for views

use crate::error::KalamDbError;
use crate::error_extensions::KalamDbResultExt;
use async_trait::async_trait;
use datafusion::arrow::array::{ArrayRef, StringBuilder};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::datatypes::KalamDataType;
use kalamdb_commons::schemas::{
    ColumnDefault, ColumnDefinition, TableDefinition, TableOptions,
    TableType as KalamTableType,
};
use kalamdb_commons::{NamespaceId, TableName};
use kalamdb_system::{SystemError, SystemTableProviderExt};
use std::any::Any;
use std::sync::{Arc, OnceLock, RwLock};

/// Metrics provider callback type
/// Returns a vector of (metric_name, metric_value) tuples
pub type MetricsCallback = Arc<dyn Fn() -> Vec<(String, String)> + Send + Sync>;

/// Static schema for system.stats
static STATS_SCHEMA: OnceLock<SchemaRef> = OnceLock::new();

/// Schema helper for system.stats
pub struct StatsTableSchema;

impl StatsTableSchema {
    /// Get the TableDefinition for system.stats view
    ///
    /// Schema:
    /// - metric_name TEXT NOT NULL (metric identifier)
    /// - metric_value TEXT NOT NULL (metric value as string)
    pub fn definition() -> TableDefinition {
        let columns = vec![
            ColumnDefinition::new(
                1,
                "metric_name",
                1,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Metric identifier (e.g., server_uptime_seconds, total_users)".to_string()),
            ),
            ColumnDefinition::new(
                2,
                "metric_value",
                2,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Metric value as string".to_string()),
            ),
        ];

        TableDefinition::new(
            NamespaceId::system(),
            TableName::new("stats"),
            KalamTableType::System,
            columns,
            TableOptions::system(),
            Some("Runtime server metrics as key-value pairs (read-only view)".to_string()),
        )
        .expect("Failed to create system.stats view definition")
    }

    pub fn schema() -> SchemaRef {
        STATS_SCHEMA
            .get_or_init(|| {
                Self::definition()
                    .to_arrow_schema()
                    .expect("Failed to convert stats TableDefinition to Arrow schema")
            })
            .clone()
    }

    pub fn table_name() -> &'static str {
        "stats"
    }
}

/// Virtual table that emits key-value metrics computed at query time
pub struct StatsTableProvider {
    schema: SchemaRef,
    metrics_callback: Arc<RwLock<Option<MetricsCallback>>>,
}

impl std::fmt::Debug for StatsTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StatsTableProvider")
            .field("has_callback", &self.metrics_callback.read().unwrap().is_some())
            .finish()
    }
}

impl StatsTableProvider {
    pub fn new() -> Self {
        Self {
            schema: StatsTableSchema::schema(),
            metrics_callback: Arc::new(RwLock::new(None)),
        }
    }

    pub fn with_callback(callback: MetricsCallback) -> Self {
        Self {
            schema: StatsTableSchema::schema(),
            metrics_callback: Arc::new(RwLock::new(Some(callback))),
        }
    }

    pub fn set_metrics_callback(&self, callback: MetricsCallback) {
        *self.metrics_callback.write().unwrap() = Some(callback);
    }

    fn build_metrics_batch(&self) -> Result<RecordBatch, KalamDbError> {
        let mut names = StringBuilder::new();
        let mut values = StringBuilder::new();

        let callback_guard = self.metrics_callback.read().unwrap();
        if let Some(ref callback) = *callback_guard {
            let metrics = callback();
            for (name, value) in metrics {
                names.append_value(&name);
                values.append_value(&value);
            }
        } else {
            names.append_value("server_uptime_seconds");
            values.append_value("N/A (callback not set)");
            names.append_value("total_users");
            values.append_value("N/A");
            names.append_value("total_namespaces");
            values.append_value("N/A");
            names.append_value("total_tables");
            values.append_value("N/A");
        }

        RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(names.finish()) as ArrayRef,
                Arc::new(values.finish()) as ArrayRef,
            ],
        ).into_kalamdb_error("Failed to build stats batch")
    }
}

impl Default for StatsTableProvider {
    fn default() -> Self { Self::new() }
}

impl SystemTableProviderExt for StatsTableProvider {
    fn table_name(&self) -> &str { StatsTableSchema::table_name() }
    fn schema_ref(&self) -> SchemaRef { self.schema.clone() }
    fn load_batch(&self) -> Result<RecordBatch, SystemError> {
        self.build_metrics_batch().map_err(|e| SystemError::Other(e.to_string()))
    }
}

#[async_trait]
impl TableProvider for StatsTableProvider {
    fn as_any(&self) -> &dyn Any { self }
    fn schema(&self) -> SchemaRef { self.schema.clone() }
    fn table_type(&self) -> TableType { TableType::View }

    async fn scan(
        &self,
        state: &dyn datafusion::catalog::Session,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        use datafusion::datasource::MemTable;
        let batch = self.build_metrics_batch().map_err(|e| {
            DataFusionError::Execution(format!("Failed to build stats batch: {}", e))
        })?;
        let table = MemTable::try_new(self.schema.clone(), vec![vec![batch]])?;
        table.scan(state, projection, &[], _limit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats_schema() {
        let schema = StatsTableSchema::schema();
        assert_eq!(schema.fields().len(), 2);
    }

    #[test]
    fn test_stats_provider_without_callback() {
        let provider = StatsTableProvider::new();
        let batch = provider.load_batch().expect("stats batch");
        assert!(batch.num_rows() >= 3);
    }

    #[test]
    fn test_stats_provider_with_callback() {
        let callback: MetricsCallback = Arc::new(|| vec![("m1".to_string(), "v1".to_string())]);
        let provider = StatsTableProvider::with_callback(callback);
        let batch = provider.load_batch().expect("stats batch");
        assert_eq!(batch.num_rows(), 1);
    }
}
