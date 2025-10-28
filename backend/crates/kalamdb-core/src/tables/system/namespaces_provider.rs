//! DataFusion TableProvider for system.namespaces

use crate::error::KalamDbError;
use crate::tables::system::namespaces::NamespacesTable;
use crate::tables::system::SystemTableProviderExt;
use async_trait::async_trait;
use datafusion::arrow::array::{
    ArrayRef, Int32Array, RecordBatch, StringBuilder, TimestampMillisecondArray,
};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_sql::KalamSql;
use std::any::Any;
use std::sync::Arc;

/// system.namespaces provider backed by kalamdb-sql
pub struct NamespacesTableProvider {
    kalam_sql: Arc<KalamSql>,
    schema: SchemaRef,
}

impl std::fmt::Debug for NamespacesTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NamespacesTableProvider").finish()
    }
}

impl NamespacesTableProvider {
    /// Create a new provider instance
    pub fn new(kalam_sql: Arc<KalamSql>) -> Self {
        Self {
            kalam_sql,
            schema: NamespacesTable::schema(),
        }
    }

    fn build_batch(&self) -> Result<RecordBatch, KalamDbError> {
        let namespaces = self
            .kalam_sql
            .scan_all_namespaces()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan namespaces: {}", e)))?;

        let mut namespace_ids = StringBuilder::new();
        let mut names = StringBuilder::new();
        let mut options = StringBuilder::new();
        let mut table_counts = Vec::with_capacity(namespaces.len());
        let mut created_ats = Vec::with_capacity(namespaces.len());

        for ns in namespaces {
            namespace_ids.append_value(ns.namespace_id.as_str());
            names.append_value(&ns.name);
            if let Some(ref opts) = ns.options {
                options.append_value(opts);
            } else {
                options.append_null();
            }
            table_counts.push(Some(ns.table_count));
            // Commons Namespace.created_at is already in milliseconds
            created_ats.push(Some(ns.created_at));
        }

        let batch = RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(namespace_ids.finish()) as ArrayRef,
                Arc::new(names.finish()) as ArrayRef,
                Arc::new(options.finish()) as ArrayRef,
                Arc::new(Int32Array::from(table_counts)) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(created_ats)) as ArrayRef,
            ],
        )
        .map_err(|e| KalamDbError::Other(format!("Failed to create namespaces batch: {}", e)))?;

        Ok(batch)
    }
}

impl SystemTableProviderExt for NamespacesTableProvider {
    fn table_name(&self) -> &'static str {
        kalamdb_commons::constants::SystemTableNames::NAMESPACES
    }

    fn schema_ref(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn load_batch(&self) -> Result<RecordBatch, KalamDbError> {
        self.build_batch()
    }
}

#[async_trait]
impl TableProvider for NamespacesTableProvider {
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
            DataFusionError::Execution(format!("Failed to build namespaces batch: {}", e))
        })?;
        let partitions = vec![vec![batch]];
        let table = MemTable::try_new(schema, partitions).map_err(|e| {
            DataFusionError::Execution(format!("Failed to create MemTable: {}", e))
        })?;
        table.scan(_state, projection, &[], _limit)
    }
}
