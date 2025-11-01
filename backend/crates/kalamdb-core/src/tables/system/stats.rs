//! system.stats virtual table
//!
//! Provides runtime metrics as key-value pairs for observability.
//! Initial implementation focuses on schema cache metrics.

use crate::error::KalamDbError;
use crate::tables::system::schemas::SchemaCache as TableSchemaCache;
use crate::tables::system::SystemTableProviderExt;
use async_trait::async_trait;
use datafusion::arrow::array::{ArrayRef, Float64Builder, StringBuilder};
use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use std::any::Any;
use std::sync::{Arc, OnceLock};

/// Static schema for system.stats
static STATS_SCHEMA: OnceLock<Arc<Schema>> = OnceLock::new();

/// Schema helper for system.stats
pub struct StatsTableSchema;

impl StatsTableSchema {
    pub fn schema() -> SchemaRef {
        STATS_SCHEMA
            .get_or_init(|| {
                Arc::new(Schema::new(vec![
                    Field::new("metric_name", DataType::Utf8, false),
                    Field::new("metric_value", DataType::Utf8, false),
                ]))
            })
            .clone()
    }

    pub fn table_name() -> &'static str {
        "stats"
    }
}

/// Virtual table that emits key-value metrics
pub struct StatsTableProvider {
    schema: SchemaRef,
    table_schema_cache: Option<Arc<TableSchemaCache>>, // optional, to avoid tight coupling
}

impl std::fmt::Debug for StatsTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StatsTableProvider").finish()
    }
}

impl StatsTableProvider {
    /// Create a new stats table provider
    pub fn new(table_schema_cache: Option<Arc<TableSchemaCache>>) -> Self {
        Self {
            schema: StatsTableSchema::schema(),
            table_schema_cache,
        }
    }

    /// Build a RecordBatch with current metrics
    fn build_metrics_batch(&self) -> Result<RecordBatch, KalamDbError> {
        let mut names = StringBuilder::new();
        let mut values = StringBuilder::new();

        // Schema cache metrics (TableDefinition cache)
        if let Some(cache) = &self.table_schema_cache {
            let (hits, misses, evictions, size) = cache.stats();
            let hit_rate = cache.hit_rate();

            names.append_value("schema_cache_hit_rate");
            values.append_value(format!("{:.6}", hit_rate));

            names.append_value("schema_cache_size");
            values.append_value(size.to_string());

            names.append_value("schema_cache_hits");
            values.append_value(hits.to_string());

            names.append_value("schema_cache_misses");
            values.append_value(misses.to_string());

            names.append_value("schema_cache_evictions");
            values.append_value(evictions.to_string());
        } else {
            names.append_value("schema_cache_hit_rate");
            values.append_value("N/A");

            names.append_value("schema_cache_size");
            values.append_value("0");
        }

        // Placeholders for future metrics
        names.append_value("type_conversion_cache_hit_rate");
        values.append_value("N/A");

        names.append_value("server_uptime_seconds");
        values.append_value("N/A");

        let batch = RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(names.finish()) as ArrayRef,
                Arc::new(values.finish()) as ArrayRef,
            ],
        )
        .map_err(|e| KalamDbError::Other(format!("Failed to build stats batch: {}", e)))?;

        Ok(batch)
    }
}

impl SystemTableProviderExt for StatsTableProvider {
    fn table_name(&self) -> &str {
        StatsTableSchema::table_name()
    }

    fn schema_ref(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn load_batch(&self) -> Result<RecordBatch, KalamDbError> {
        self.build_metrics_batch()
    }
}

#[async_trait]
impl TableProvider for StatsTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn table_type(&self) -> TableType {
        TableType::View
    }

    async fn scan(
        &self,
        state: &dyn datafusion::catalog::Session,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        use datafusion::datasource::MemTable;
        let schema = self.schema.clone();
        let batch = self.build_metrics_batch().map_err(|e| {
            DataFusionError::Execution(format!("Failed to build stats batch: {}", e))
        })?;
        let partitions = vec![vec![batch]];
        let table = MemTable::try_new(schema, partitions)
            .map_err(|e| DataFusionError::Execution(format!("Failed to create MemTable: {}", e)))?;
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
        assert_eq!(schema.field(0).name(), "metric_name");
        assert_eq!(schema.field(1).name(), "metric_value");
    }

    #[test]
    fn test_stats_provider_batch() {
        let provider = StatsTableProvider::new(None);
        let batch = provider.load_batch().expect("stats batch");
        assert!(batch.num_rows() >= 3); // at least the placeholder metrics
        assert_eq!(batch.num_columns(), 2);
    }
}
