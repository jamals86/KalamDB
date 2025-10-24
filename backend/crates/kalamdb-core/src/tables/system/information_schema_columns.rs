//! Information Schema Columns Provider
//!
//! Provides column metadata by flattening the columns array from TableDefinition.
//! Reads from information_schema_tables CF and exposes per-column records.

use crate::error::KalamDbError;
use async_trait::async_trait;
use datafusion::arrow::array::{BooleanArray, RecordBatch, StringArray, UInt32Array};
use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use datafusion::datasource::TableProvider;
use datafusion::error::Result as DataFusionResult;
use datafusion::execution::context::SessionState;
use datafusion::logical_expr::{Expr, TableType};
use datafusion::physical_plan::{memory::MemoryExec, ExecutionPlan};
use kalamdb_sql::KalamSql;
use std::any::Any;
use std::sync::Arc;

pub struct InformationSchemaColumnsProvider {
    kalam_sql: Arc<KalamSql>,
    schema: SchemaRef,
}

impl InformationSchemaColumnsProvider {
    pub fn new(kalam_sql: Arc<KalamSql>) -> Self {
        let schema = Arc::new(Schema::new(vec![
            Field::new("table_catalog", DataType::Utf8, true),
            Field::new("table_schema", DataType::Utf8, false),
            Field::new("table_name", DataType::Utf8, false),
            Field::new("column_name", DataType::Utf8, false),
            Field::new("ordinal_position", DataType::UInt32, false), // 1-indexed
            Field::new("data_type", DataType::Utf8, false),
            Field::new("is_nullable", DataType::Boolean, false),
            Field::new("column_default", DataType::Utf8, true),
            Field::new("is_primary_key", DataType::Boolean, false),
        ]));

        Self { kalam_sql, schema }
    }

    async fn scan_all_columns(&self) -> Result<RecordBatch, KalamDbError> {
        // Read all table definitions across all namespaces
        let tables = self.kalam_sql.scan_all_table_definitions()?;

        // Flatten columns from all tables
        let mut table_catalog_values = Vec::new();
        let mut table_schema_values = Vec::new();
        let mut table_name_values = Vec::new();
        let mut column_name_values = Vec::new();
        let mut ordinal_position_values = Vec::new();
        let mut data_type_values = Vec::new();
        let mut is_nullable_values = Vec::new();
        let mut column_default_values = Vec::new();
        let mut is_primary_key_values = Vec::new();

        for table_def in tables {
            for column in &table_def.columns {
                table_catalog_values.push(None::<String>); // NULL for now
                table_schema_values.push(table_def.namespace_id.to_string());
                table_name_values.push(table_def.table_name.clone());
                column_name_values.push(column.column_name.clone());
                ordinal_position_values.push(column.ordinal_position);
                data_type_values.push(column.data_type.clone());
                is_nullable_values.push(column.is_nullable);
                column_default_values.push(column.column_default.clone());
                is_primary_key_values.push(column.is_primary_key);
            }
        }

        // Create RecordBatch
        let batch = RecordBatch::try_new(
            Arc::clone(&self.schema),
            vec![
                Arc::new(StringArray::from(table_catalog_values)),
                Arc::new(StringArray::from(table_schema_values)),
                Arc::new(StringArray::from(table_name_values)),
                Arc::new(StringArray::from(column_name_values)),
                Arc::new(UInt32Array::from(ordinal_position_values)),
                Arc::new(StringArray::from(data_type_values)),
                Arc::new(BooleanArray::from(is_nullable_values)),
                Arc::new(StringArray::from(column_default_values)),
                Arc::new(BooleanArray::from(is_primary_key_values)),
            ],
        )
        .map_err(|e| KalamDbError::Other(format!("Failed to create RecordBatch: {}", e)))?;

        Ok(batch)
    }
}

#[async_trait]
impl TableProvider for InformationSchemaColumnsProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        Arc::clone(&self.schema)
    }

    fn table_type(&self) -> TableType {
        TableType::View
    }

    async fn scan(
        &self,
        _state: &SessionState,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        let schema = Arc::clone(&self.schema);
        let batch = self
            .scan_all_columns()
            .await
            .map_err(|e| datafusion::error::DataFusionError::Execution(e.to_string()))?;

        let partitions = vec![vec![batch]];
        let exec = MemoryExec::try_new(&partitions, schema, projection.cloned())?;

        Ok(Arc::new(exec))
    }
}
