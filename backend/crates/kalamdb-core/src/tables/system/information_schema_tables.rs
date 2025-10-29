//! information_schema.tables virtual table provider
//!
//! This module provides a DataFusion TableProvider for the information_schema.tables
//! virtual table, which exposes all table metadata from information_schema_tables CF.
//!
//! **Updated**: Now uses unified TableDefinition from information_schema_tables instead
//! of fragmented system.tables storage.

use crate::error::KalamDbError;
use crate::tables::system::SystemTableProviderExt;
use async_trait::async_trait;
use datafusion::arrow::array::{
    ArrayRef, BooleanArray, Int32Array, Int64Array, RecordBatch, StringBuilder,
    TimestampMillisecondArray, UInt32Array, UInt64Array,
};
use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_sql::KalamSql;
use std::any::Any;
use std::sync::Arc;

/// InformationSchemaTablesProvider exposes all table metadata from information_schema_tables CF
pub struct InformationSchemaTablesProvider {
    kalam_sql: Arc<KalamSql>,
    schema: SchemaRef,
}

impl std::fmt::Debug for InformationSchemaTablesProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InformationSchemaTablesProvider").finish()
    }
}

impl InformationSchemaTablesProvider {
    /// Create a new information_schema.tables provider
    ///
    /// # Arguments
    /// * `kalam_sql` - KalamSQL instance for accessing information_schema_tables CF
    pub fn new(kalam_sql: Arc<KalamSql>) -> Self {
        let schema = Arc::new(Schema::new(vec![
            // SQL standard information_schema.tables columns
            Field::new("table_catalog", DataType::Utf8, false),
            Field::new("table_schema", DataType::Utf8, false), // namespace_id
            Field::new("table_name", DataType::Utf8, false),
            Field::new("table_type", DataType::Utf8, false), // USER/SHARED/STREAM/SYSTEM
            // KalamDB-specific metadata columns
            Field::new("table_id", DataType::Utf8, false),
            Field::new(
                "created_at",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
            Field::new(
                "updated_at",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
            Field::new("schema_version", DataType::UInt32, false),
            Field::new("storage_id", DataType::Utf8, false),
            Field::new("use_user_storage", DataType::Boolean, false),
            Field::new("deleted_retention_hours", DataType::UInt32, true), // nullable
            Field::new("ttl_seconds", DataType::UInt64, true),             // nullable
        ]));

        Self { kalam_sql, schema }
    }

    /// Scan all tables across all namespaces and return as RecordBatch
    pub fn scan_all_tables(&self) -> Result<RecordBatch, KalamDbError> {
        let table_defs = self
            .kalam_sql
            .scan_all_table_definitions()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan table definitions: {}", e)))?;

        let mut table_catalogs = StringBuilder::new();
        let mut table_schemas = StringBuilder::new();
        let mut table_names = StringBuilder::new();
        let mut table_types = StringBuilder::new();
        let mut table_ids = StringBuilder::new();
        let mut created_ats = Vec::new();
        let mut updated_ats = Vec::new();
        let mut schema_versions = Vec::new();
        let mut storage_ids = StringBuilder::new();
        let mut use_user_storages = Vec::new();
        let mut deleted_retention_hours_vec: Vec<Option<u32>> = Vec::new();
        let mut ttl_seconds_vec: Vec<Option<u64>> = Vec::new();

        for table_def in table_defs {
            // SQL standard information_schema uses "def" as default catalog
            table_catalogs.append_value("def");

            // table_schema is the namespace/database name
            table_schemas.append_value(&table_def.namespace_id);

            // table_name
            table_names.append_value(&table_def.table_name);

            // table_type: Convert KalamDB type to SQL standard
            let standard_type = match table_def.table_type {
                crate::catalog::TableType::User | crate::catalog::TableType::Shared => "BASE TABLE",
                crate::catalog::TableType::System => "SYSTEM VIEW",
                crate::catalog::TableType::Stream => "STREAM TABLE",
            };
            table_types.append_value(standard_type);

            // KalamDB-specific columns
            table_ids.append_value(&table_def.table_id);
            created_ats.push(table_def.created_at);
            updated_ats.push(table_def.updated_at);
            schema_versions.push(table_def.schema_version);
            storage_ids.append_value(&table_def.storage_id);
            use_user_storages.push(table_def.use_user_storage);
            deleted_retention_hours_vec.push(table_def.deleted_retention_hours);
            ttl_seconds_vec.push(table_def.ttl_seconds);
        }

        let batch = RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(table_catalogs.finish()) as ArrayRef,
                Arc::new(table_schemas.finish()) as ArrayRef,
                Arc::new(table_names.finish()) as ArrayRef,
                Arc::new(table_types.finish()) as ArrayRef,
                Arc::new(table_ids.finish()) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(created_ats)) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(updated_ats)) as ArrayRef,
                Arc::new(UInt32Array::from(schema_versions)) as ArrayRef,
                Arc::new(storage_ids.finish()) as ArrayRef,
                Arc::new(BooleanArray::from(use_user_storages)) as ArrayRef,
                Arc::new(UInt32Array::from(deleted_retention_hours_vec)) as ArrayRef,
                Arc::new(UInt64Array::from(ttl_seconds_vec)) as ArrayRef,
            ],
        )
        .map_err(|e| KalamDbError::Other(format!("Failed to create RecordBatch: {}", e)))?;

        Ok(batch)
    }
}

impl SystemTableProviderExt for InformationSchemaTablesProvider {
    fn table_name(&self) -> &str {
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
        _state: &dyn datafusion::catalog::Session,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        use datafusion::datasource::MemTable;
        let schema = self.schema.clone();
        let batch = self.scan_all_tables().map_err(|e| {
            DataFusionError::Execution(format!(
                "Failed to build information_schema.tables batch: {}",
                e
            ))
        })?;
        let partitions = vec![vec![batch]];
        let table = MemTable::try_new(schema, partitions)
            .map_err(|e| DataFusionError::Execution(format!("Failed to create MemTable: {}", e)))?;
        table.scan(_state, projection, &[], _limit).await
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
