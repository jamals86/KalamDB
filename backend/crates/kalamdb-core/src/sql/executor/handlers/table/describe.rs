//! Typed DDL handler for DESCRIBE TABLE statements

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::error_extensions::KalamDbResultExt;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::arrow::array::{ArrayRef, BooleanArray, RecordBatch, StringArray, UInt32Array};
use datafusion::arrow::datatypes::{DataType, Field, Schema};
use kalamdb_commons::models::schemas::TableDefinition;
use kalamdb_commons::models::{NamespaceId, TableId};
use kalamdb_sql::ddl::DescribeTableStatement;
use std::sync::Arc;

/// Typed handler for DESCRIBE TABLE statements
pub struct DescribeTableHandler {
    app_context: Arc<AppContext>,
}

impl DescribeTableHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<DescribeTableStatement> for DescribeTableHandler {
    async fn execute(
        &self,
        statement: DescribeTableStatement,
        _params: Vec<ScalarValue>,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        let start_time = std::time::Instant::now();
        let ns = statement
            .namespace_id
            .clone()
            .unwrap_or_else(|| NamespaceId::new("default"));
        let table_id = TableId::from_strings(ns.as_str(), statement.table_name.as_str());
        let def = self
            .app_context
            .schema_registry()
            .get_table_definition(self.app_context.as_ref(), &table_id)?
            .ok_or_else(|| {
                KalamDbError::NotFound(format!(
                    "Table '{}' not found in namespace '{}'",
                    statement.table_name.as_str(),
                    ns.as_str()
                ))
            })?;

        let batch = build_describe_batch(&def)?;
        let row_count = batch.num_rows();

        // Log query operation
        let duration = start_time.elapsed().as_secs_f64() * 1000.0;
        use crate::sql::executor::helpers::audit;
        let audit_entry = audit::log_query_operation(
            context,
            "DESCRIBE",
            &table_id.full_name(),
            duration,
            None,
        );
        audit::persist_audit_entry(&self.app_context, &audit_entry).await?;

        Ok(ExecutionResult::Rows {
            batches: vec![batch],
            row_count,
            schema: None,
        })
    }

    async fn check_authorization(
        &self,
        _statement: &DescribeTableStatement,
        _context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // DESCRIBE TABLE allowed for all authenticated users who can access the table
        Ok(())
    }
}

fn build_describe_batch(def: &TableDefinition) -> Result<RecordBatch, KalamDbError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("column_name", DataType::Utf8, false),
        Field::new("ordinal_position", DataType::UInt32, false),
        Field::new("data_type", DataType::Utf8, false),
        Field::new("is_nullable", DataType::Boolean, false),
        Field::new("is_primary_key", DataType::Boolean, false),
        Field::new("column_default", DataType::Utf8, true),
        Field::new("column_comment", DataType::Utf8, true),
    ]));

    // Pre-allocate based on column count
    let col_count = def.columns.len();
    let mut names: Vec<String> = Vec::with_capacity(col_count);
    let mut ordinals: Vec<u32> = Vec::with_capacity(col_count);
    let mut types: Vec<String> = Vec::with_capacity(col_count);
    let mut nulls: Vec<bool> = Vec::with_capacity(col_count);
    let mut pks: Vec<bool> = Vec::with_capacity(col_count);
    let mut defaults: Vec<Option<String>> = Vec::with_capacity(col_count);
    let mut comments: Vec<Option<String>> = Vec::with_capacity(col_count);

    for c in &def.columns {
        names.push(c.column_name.clone());
        ordinals.push(c.ordinal_position);
        types.push(c.data_type.sql_name().to_string());
        nulls.push(c.is_nullable);
        pks.push(c.is_primary_key);
        defaults.push(if c.default_value.is_none() {
            None
        } else {
            Some(c.default_value.to_sql())
        });
        comments.push(c.column_comment.clone());
    }

    let batch = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from(names)) as ArrayRef,
            Arc::new(UInt32Array::from(ordinals)) as ArrayRef,
            Arc::new(StringArray::from(types)) as ArrayRef,
            Arc::new(BooleanArray::from(nulls)) as ArrayRef,
            Arc::new(BooleanArray::from(pks)) as ArrayRef,
            Arc::new(StringArray::from(defaults)) as ArrayRef,
            Arc::new(StringArray::from(comments)) as ArrayRef,
        ],
    )
    .into_arrow_error()?;

    Ok(batch)
}
