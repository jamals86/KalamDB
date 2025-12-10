//! Typed DDL handler for SHOW STATS statements

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::arrow::array::{ArrayRef, RecordBatch, StringArray, UInt64Array};
use datafusion::arrow::datatypes::{DataType, Field, Schema};
use kalamdb_commons::models::{NamespaceId, TableId};
use kalamdb_sql::ddl::ShowTableStatsStatement;
use std::sync::Arc;

/// Typed handler for SHOW STATS statements
pub struct ShowStatsHandler {
    app_context: Arc<AppContext>,
}

impl ShowStatsHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<ShowTableStatsStatement> for ShowStatsHandler {
    async fn execute(
        &self,
        statement: ShowTableStatsStatement,
        _params: Vec<ScalarValue>,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        let start_time = std::time::Instant::now();
        let ns = statement
            .namespace_id
            .clone()
            .unwrap_or_else(|| NamespaceId::new("default"));
        let table_id = TableId::from_strings(ns.as_str(), statement.table_name.as_str());

        // TableDefinition gives us metadata only; stats system not yet implemented.
        // Provide placeholder zero metrics plus schema version.
        let registry = self.app_context.schema_registry();
        let def = registry.get_table_definition(&table_id)?.ok_or_else(|| {
            KalamDbError::NotFound(format!(
                "Table '{}' not found in namespace '{}'",
                statement.table_name.as_str(),
                ns.as_str()
            ))
        })?;

        let logical_rows = 0u64; // TODO: integrate row count tracking
        let flushed_segments = 0u64; // TODO: integrate flush segment counters
        let active_streams = 0u64; // TODO: integrate stream activity metrics
        let memory_bytes = 0u64; // TODO: integrate in-memory size tracking
        let schema_version = def.schema_version as u64;

        let schema = Arc::new(Schema::new(vec![
            Field::new("table_name", DataType::Utf8, false),
            Field::new("namespace", DataType::Utf8, false),
            Field::new("schema_version", DataType::UInt64, false),
            Field::new("logical_rows", DataType::UInt64, false),
            Field::new("flushed_segments", DataType::UInt64, false),
            Field::new("active_streams", DataType::UInt64, false),
            Field::new("memory_bytes", DataType::UInt64, false),
        ]));

        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(vec![def.table_name.as_str().to_string()])) as ArrayRef,
                Arc::new(StringArray::from(vec![def
                    .namespace_id
                    .as_str()
                    .to_string()])) as ArrayRef,
                Arc::new(UInt64Array::from(vec![schema_version])) as ArrayRef,
                Arc::new(UInt64Array::from(vec![logical_rows])) as ArrayRef,
                Arc::new(UInt64Array::from(vec![flushed_segments])) as ArrayRef,
                Arc::new(UInt64Array::from(vec![active_streams])) as ArrayRef,
                Arc::new(UInt64Array::from(vec![memory_bytes])) as ArrayRef,
            ],
        )
        .map_err(|e| KalamDbError::Other(format!("Arrow error: {}", e)))?;

        // Log query operation
        let duration = start_time.elapsed().as_secs_f64() * 1000.0;
        use crate::sql::executor::helpers::audit;
        let audit_entry = audit::log_query_operation(
            context,
            "SHOW",
            &format!("STATS {}.{}", ns.as_str(), statement.table_name.as_str()),
            duration,
            None,
        );
        audit::persist_audit_entry(&self.app_context, &audit_entry).await?;

        Ok(ExecutionResult::Rows {
            batches: vec![batch],
            row_count: 1,
            schema: None,
        })
    }

    async fn check_authorization(
        &self,
        _statement: &ShowTableStatsStatement,
        _context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // SHOW STATS allowed for all authenticated users
        Ok(())
    }
}
