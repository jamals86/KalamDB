//! Typed DDL handler for SHOW TABLES statements

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::arrow::array::{
    ArrayRef, Int32Array, RecordBatch, StringBuilder, TimestampMillisecondArray,
};
use kalamdb_commons::schemas::TableDefinition;
use kalamdb_sql::ddl::ShowTablesStatement;
use std::sync::Arc;

/// Typed handler for SHOW TABLES statements
pub struct ShowTablesHandler {
    app_context: Arc<AppContext>,
}

impl ShowTablesHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<ShowTablesStatement> for ShowTablesHandler {
    async fn execute(
        &self,
        statement: ShowTablesStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        let tables_provider = self.app_context.system_tables().tables();

        // If namespace filter provided, build filtered batch manually
        if let Some(ns) = statement.namespace_id {
            let defs = tables_provider.list_tables()?;
            let filtered: Vec<TableDefinition> =
                defs.into_iter().filter(|t| t.namespace_id == ns).collect();
            let batch = build_tables_batch(filtered)?;
            let row_count = batch.num_rows();
            return Ok(ExecutionResult::Rows {
                batches: vec![batch],
                row_count,
            });
        }

        // Otherwise, reuse provider's batch for all tables
        let batch = tables_provider.scan_all_tables()?;
        let row_count = batch.num_rows();
        Ok(ExecutionResult::Rows {
            batches: vec![batch],
            row_count,
        })
    }

    async fn check_authorization(
        &self,
        _statement: &ShowTablesStatement,
        _context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // SHOW TABLES allowed for all authenticated users
        Ok(())
    }
}

/// Build a RecordBatch for system.tables-like view from definitions
fn build_tables_batch(tables: Vec<TableDefinition>) -> Result<RecordBatch, KalamDbError> {
    let mut table_ids = StringBuilder::new();
    let mut table_names = StringBuilder::new();
    let mut namespaces = StringBuilder::new();
    let mut table_types = StringBuilder::new();
    let mut created_ats = Vec::new();
    let mut schema_versions = Vec::new();
    let mut table_comments = StringBuilder::new();
    let mut updated_ats = Vec::new();

    for t in tables.iter() {
        let table_id_str = format!("{}:{}", t.namespace_id.as_str(), t.table_name.as_str());
        table_ids.append_value(&table_id_str);
        table_names.append_value(t.table_name.as_str());
        namespaces.append_value(t.namespace_id.as_str());
        table_types.append_value(t.table_type.as_str());
        created_ats.push(Some(t.created_at.timestamp_millis()));
        schema_versions.push(Some(t.schema_version as i32));
        table_comments.append_option(t.table_comment.as_deref());
        updated_ats.push(Some(t.updated_at.timestamp_millis()));
    }

    // Reuse system.tables schema via provider
    let provider = self_tables_provider_schema();
    let batch = RecordBatch::try_new(
        provider,
        vec![
            Arc::new(table_ids.finish()) as ArrayRef,
            Arc::new(table_names.finish()) as ArrayRef,
            Arc::new(namespaces.finish()) as ArrayRef,
            Arc::new(table_types.finish()) as ArrayRef,
            Arc::new(TimestampMillisecondArray::from(created_ats)) as ArrayRef,
            Arc::new(Int32Array::from(schema_versions)) as ArrayRef,
            Arc::new(table_comments.finish()) as ArrayRef,
            Arc::new(TimestampMillisecondArray::from(updated_ats)) as ArrayRef,
        ],
    )
    .map_err(|e| KalamDbError::Other(format!("Arrow error: {}", e)))?;

    Ok(batch)
}

/// Fetch system.tables schema from provider to ensure column order/types match
fn self_tables_provider_schema() -> datafusion::arrow::datatypes::SchemaRef {
    use crate::app_context::AppContext;
    use datafusion::datasource::TableProvider;
    let app = AppContext::get();
    let provider = app.system_tables().tables();
    provider.schema()
}
