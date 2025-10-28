//! DataFusion TableProvider for system.tables

use crate::error::KalamDbError;
use crate::tables::system::system_tables::SystemTables;
use crate::tables::system::SystemTableProviderExt;
use async_trait::async_trait;
use datafusion::arrow::array::{
    ArrayRef, BooleanBuilder, Int32Array, RecordBatch, StringBuilder, TimestampMillisecondArray,
};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_sql::KalamSql;
use std::any::Any;
use std::sync::Arc;

/// system.tables provider backed by kalamdb-sql metadata
pub struct SystemTablesTableProvider {
    kalam_sql: Arc<KalamSql>,
    schema: SchemaRef,
}

impl std::fmt::Debug for SystemTablesTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemTablesTableProvider").finish()
    }
}

impl SystemTablesTableProvider {
    /// Create a new provider instance
    pub fn new(kalam_sql: Arc<KalamSql>) -> Self {
        Self {
            kalam_sql,
            schema: SystemTables::schema(),
        }
    }

    fn build_batch(&self) -> Result<RecordBatch, KalamDbError> {
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
        let mut storage_ids = StringBuilder::new();
        let mut use_user_storage_flags = BooleanBuilder::new();
        let mut flush_policies = StringBuilder::new();
        let mut schema_versions = Vec::with_capacity(tables.len());
        let mut retention_hours = Vec::with_capacity(tables.len());

        for table in tables {
            table_ids.append_value(&table.table_id);
            table_names.append_value(&table.table_name);
            namespaces.append_value(&table.namespace);
            table_types.append_value(table.table_type.as_str());
            created_ats.push(Some(table.created_at));
            storage_locations.append_value(&table.storage_location);
            if let Some(ref storage_id) = table.storage_id {
                storage_ids.append_value(storage_id);
            } else {
                storage_ids.append_null();
            }
            use_user_storage_flags.append_value(table.use_user_storage);
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
                Arc::new(storage_ids.finish()) as ArrayRef,
                Arc::new(use_user_storage_flags.finish()) as ArrayRef,
                Arc::new(flush_policies.finish()) as ArrayRef,
                Arc::new(Int32Array::from(schema_versions)) as ArrayRef,
                Arc::new(Int32Array::from(retention_hours)) as ArrayRef,
            ],
        )
        .map_err(|e| KalamDbError::Other(format!("Failed to create system.tables batch: {}", e)))?;

        Ok(batch)
    }
}

impl SystemTableProviderExt for SystemTablesTableProvider {
    fn table_name(&self) -> &'static str {
        kalamdb_commons::constants::SystemTableNames::TABLES
    }

    fn schema_ref(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn load_batch(&self) -> Result<RecordBatch, KalamDbError> {
        self.build_batch()
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
        _state: &dyn datafusion::catalog::Session,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        use datafusion::datasource::MemTable;
        let schema = self.schema.clone();
        let batch = self.build_batch().map_err(|e| {
            DataFusionError::Execution(format!("Failed to build system.tables batch: {}", e))
        })?;
        let partitions = vec![vec![batch]];
        let table = MemTable::try_new(schema, partitions).map_err(|e| {
            DataFusionError::Execution(format!("Failed to create MemTable: {}", e))
        })?;
        table.scan(_state, projection, &[], _limit).await
    }
}
