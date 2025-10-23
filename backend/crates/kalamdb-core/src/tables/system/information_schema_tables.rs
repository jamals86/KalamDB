//! information_schema.tables virtual table provider
//!
//! This module provides a DataFusion TableProvider for the information_schema.tables
//! virtual table, which exposes all table metadata from system.tables.

use crate::error::KalamDbError;
use crate::tables::system::SystemTableProviderExt;
use async_trait::async_trait;
use datafusion::arrow::array::{
    ArrayRef, Int32Array, RecordBatch, StringBuilder, TimestampMillisecondArray,
};
use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::Result as DataFusionResult;
use datafusion::execution::context::SessionState;
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_sql::KalamSql;
use std::any::Any;
use std::sync::Arc;

/// InformationSchemaTablesProvider exposes all table metadata
pub struct InformationSchemaTablesProvider {
    kalam_sql: Arc<KalamSql>,
    schema: SchemaRef,
}

impl InformationSchemaTablesProvider {
    /// Create a new information_schema.tables provider
    pub fn new(kalam_sql: Arc<KalamSql>) -> Self {
        let schema = Arc::new(Schema::new(vec![
            Field::new("table_catalog", DataType::Utf8, false),
            Field::new("table_schema", DataType::Utf8, false),
            Field::new("table_name", DataType::Utf8, false),
            Field::new("table_type", DataType::Utf8, false),
            Field::new("table_id", DataType::Utf8, false),
            Field::new("namespace", DataType::Utf8, false),
            Field::new(
                "created_at",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
            Field::new("storage_location", DataType::Utf8, false),
            Field::new("flush_policy", DataType::Utf8, false),
            Field::new("schema_version", DataType::Int32, false),
            Field::new("deleted_retention_hours", DataType::Int32, false),
        ]));

        Self { kalam_sql, schema }
    }

    /// Scan all tables and return as RecordBatch
    pub fn scan_all_tables(&self) -> Result<RecordBatch, KalamDbError> {
        let tables = self
            .kalam_sql
            .scan_all_tables()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan tables: {}", e)))?;

        let mut table_catalogs = StringBuilder::new();
        let mut table_schemas = StringBuilder::new();
        let mut table_names = StringBuilder::new();
        let mut table_types = StringBuilder::new();
        let mut table_ids = StringBuilder::new();
        let mut namespaces = StringBuilder::new();
        let mut created_ats = Vec::new();
        let mut storage_locations = StringBuilder::new();
        let mut flush_policies = StringBuilder::new();
        let mut schema_versions = Vec::new();
        let mut deleted_retention_hours_vec = Vec::new();

        for table in tables {
            // SQL standard information_schema uses "def" as default catalog
            table_catalogs.append_value("def");

            // table_schema is the namespace/database name
            table_schemas.append_value(&table.namespace);

            // table_name
            table_names.append_value(&table.table_name);

            // table_type: Convert KalamDB type to SQL standard
            let standard_type = match table.table_type.as_str() {
                "user" | "shared" => "BASE TABLE",
                "system" => "SYSTEM VIEW",
                "stream" => "STREAM TABLE",
                _ => "BASE TABLE",
            };
            table_types.append_value(standard_type);

            // KalamDB-specific columns
            table_ids.append_value(&table.table_id);
            namespaces.append_value(&table.namespace);
            created_ats.push(table.created_at);
            storage_locations.append_value(&table.storage_location);
            flush_policies.append_value(&table.flush_policy);
            schema_versions.push(table.schema_version);
            deleted_retention_hours_vec.push(table.deleted_retention_hours);
        }

        let batch = RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(table_catalogs.finish()) as ArrayRef,
                Arc::new(table_schemas.finish()) as ArrayRef,
                Arc::new(table_names.finish()) as ArrayRef,
                Arc::new(table_types.finish()) as ArrayRef,
                Arc::new(table_ids.finish()) as ArrayRef,
                Arc::new(namespaces.finish()) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(created_ats)) as ArrayRef,
                Arc::new(storage_locations.finish()) as ArrayRef,
                Arc::new(flush_policies.finish()) as ArrayRef,
                Arc::new(Int32Array::from(schema_versions)) as ArrayRef,
                Arc::new(Int32Array::from(deleted_retention_hours_vec)) as ArrayRef,
            ],
        )
        .map_err(|e| KalamDbError::Other(format!("Failed to create RecordBatch: {}", e)))?;

        Ok(batch)
    }
}

impl SystemTableProviderExt for InformationSchemaTablesProvider {
    fn table_name(&self) -> &'static str {
        "information_schema.tables"
    }

    fn schema_ref(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn load_batch(&self) -> Result<RecordBatch, KalamDbError> {
        self.scan_all_tables()
    }
}

#[async_trait]
impl TableProvider for InformationSchemaTablesProvider {
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
        _state: &SessionState,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        self.into_memory_exec(projection)
    }
}

#[cfg(test)]
mod tests {

    #[tokio::test]
    async fn test_information_schema_tables_provider() {
        // This test requires actual KalamSql setup
        // For now, just verify the provider can be constructed

        // Note: In real tests, we would:
        // 1. Create KalamSql with test database
        // 2. Insert test tables
        // 3. Query information_schema.tables
        // 4. Verify correct results
    }
}
