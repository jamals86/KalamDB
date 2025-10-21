//! DataFusion TableProvider for system.tables

use crate::error::KalamDbError;
use crate::tables::system::system_tables::SystemTables;
use async_trait::async_trait;
use datafusion::arrow::array::{
    ArrayRef, Int32Array, RecordBatch, StringBuilder, TimestampMillisecondArray,
};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::execution::context::SessionState;
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::memory::MemoryExec;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_sql::KalamSql;
use std::any::Any;
use std::sync::Arc;

/// system.tables provider backed by kalamdb-sql metadata
pub struct SystemTablesTableProvider {
    kalam_sql: Arc<KalamSql>,
    schema: SchemaRef,
}

impl SystemTablesTableProvider {
    /// Create a new provider instance
    pub fn new(kalam_sql: Arc<KalamSql>) -> Self {
        Self {
            kalam_sql,
            schema: SystemTables::schema(),
        }
    }

    fn load_records(&self) -> Result<RecordBatch, KalamDbError> {
        let tables = self
            .kalam_sql
            .scan_all_tables()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan system.tables: {}", e)))?;

        let mut table_ids = StringBuilder::new();
        let mut table_names = StringBuilder::new();
        let mut namespaces = StringBuilder::new();
        let mut table_types = StringBuilder::new();
        let mut created_ats = Vec::with_capacity(tables.len());
        let mut storage_locations = StringBuilder::new();
        let mut flush_policies = StringBuilder::new();
        let mut schema_versions = Vec::with_capacity(tables.len());
        let mut retention_hours = Vec::with_capacity(tables.len());

        for table in tables {
            table_ids.append_value(&table.table_id);
            table_names.append_value(&table.table_name);
            namespaces.append_value(&table.namespace);
            table_types.append_value(&table.table_type);
            created_ats.push(Some(table.created_at));
            storage_locations.append_value(&table.storage_location);
            flush_policies.append_value(&table.flush_policy);
            schema_versions.push(Some(table.schema_version));
            retention_hours.push(Some(table.deleted_retention_hours));
        }

        let batch = RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(table_ids.finish()) as ArrayRef,
                Arc::new(table_names.finish()) as ArrayRef,
                Arc::new(namespaces.finish()) as ArrayRef,
                Arc::new(table_types.finish()) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(created_ats)) as ArrayRef,
                Arc::new(storage_locations.finish()) as ArrayRef,
                Arc::new(flush_policies.finish()) as ArrayRef,
                Arc::new(Int32Array::from(schema_versions)) as ArrayRef,
                Arc::new(Int32Array::from(retention_hours)) as ArrayRef,
            ],
        )
        .map_err(|e| KalamDbError::Other(format!("Failed to create system.tables batch: {}", e)))?;

        Ok(batch)
    }
}

#[async_trait]
impl TableProvider for SystemTablesTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }

    async fn scan(
        &self,
        _state: &SessionState,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        let batch = self.load_records().map_err(|e| {
            DataFusionError::Execution(format!("Failed to load system.tables records: {}", e))
        })?;

        let partitions = vec![vec![batch]];
        let exec = MemoryExec::try_new(&partitions, self.schema.clone(), projection.cloned())
            .map_err(|e| {
                DataFusionError::Execution(format!("Failed to build MemoryExec: {}", e))
            })?;

        Ok(Arc::new(exec))
    }
}
