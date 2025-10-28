//! Table provider for system.table_schemas

use crate::error::KalamDbError;
use crate::tables::system::table_schemas::TableSchemasTable;
use crate::tables::system::SystemTableProviderExt;
use async_trait::async_trait;
use datafusion::arrow::array::{ArrayRef, Int32Array, RecordBatch, StringBuilder, TimestampMillisecondArray};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::Result as DataFusionResult;
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_sql::KalamSql;
use std::any::Any;
use std::sync::Arc;

/// Provider exposing schema history from information_schema tables.
pub struct TableSchemasProvider {
    kalam_sql: Arc<KalamSql>,
    schema: SchemaRef,
}

impl std::fmt::Debug for TableSchemasProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TableSchemasProvider").finish()
    }
}

impl TableSchemasProvider {
    /// Create a new provider instance.
    pub fn new(kalam_sql: Arc<KalamSql>) -> Self {
        Self {
            kalam_sql,
            schema: TableSchemasTable::schema(),
        }
    }

    fn build_batch(&self) -> Result<RecordBatch, KalamDbError> {
        let table_defs = self
            .kalam_sql
            .scan_all_table_definitions()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan table definitions: {}", e)))?;

        let mut schema_ids = StringBuilder::new();
        let mut table_ids = StringBuilder::new();
        let mut namespace_ids = StringBuilder::new();
        let mut table_names = StringBuilder::new();
        let mut versions = Vec::new();
        let mut created_ats = Vec::new();
        let mut changes_builder = StringBuilder::new();
        let mut arrow_schemas = StringBuilder::new();

        for def in table_defs {
            if def.schema_history.is_empty() {
                continue;
            }

            for schema_version in def.schema_history {
                let schema_id = format!("{}:{}", def.table_id, schema_version.version);
                schema_ids.append_value(&schema_id);
                table_ids.append_value(&def.table_id);
                namespace_ids.append_value(&def.namespace_id);
                table_names.append_value(&def.table_name);
                versions.push(Some(schema_version.version as i32));
                created_ats.push(Some(schema_version.created_at));

                if schema_version.changes.is_empty() {
                    changes_builder.append_null();
                } else {
                    changes_builder.append_value(&schema_version.changes);
                }

                arrow_schemas.append_value(&schema_version.arrow_schema_json);
            }
        }

        let batch = RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(schema_ids.finish()) as ArrayRef,
                Arc::new(table_ids.finish()) as ArrayRef,
                Arc::new(namespace_ids.finish()) as ArrayRef,
                Arc::new(table_names.finish()) as ArrayRef,
                Arc::new(Int32Array::from(versions)) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(created_ats)) as ArrayRef,
                Arc::new(changes_builder.finish()) as ArrayRef,
                Arc::new(arrow_schemas.finish()) as ArrayRef,
            ],
        )
        .map_err(|e| KalamDbError::Other(format!("Failed to build system.table_schemas batch: {}", e)))?;

        Ok(batch)
    }
}

impl SystemTableProviderExt for TableSchemasProvider {
    fn table_name(&self) -> &'static str {
        kalamdb_commons::constants::SystemTableNames::TABLE_SCHEMAS
    }

    fn schema_ref(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn load_batch(&self) -> Result<RecordBatch, KalamDbError> {
        self.build_batch()
    }
}

#[async_trait]
impl TableProvider for TableSchemasProvider {
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
        self.into_memory_exec(_state, projection, _limit)
    }
}
