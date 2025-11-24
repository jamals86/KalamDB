use crate::error::KalamDbError;
use crate::providers::arrow_json_conversion::json_rows_to_arrow_batch;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::{RecordBatch, RecordBatchOptions};
use datafusion::scalar::ScalarValue;
use kalamdb_commons::constants::SystemColumnNames;
use kalamdb_commons::models::Row;
use kalamdb_tables::{SharedTableRow, StreamTableRow, UserTableRow};
use std::sync::Arc;

/// Helper function to inject system columns (_seq, _deleted) into Row values
pub fn inject_system_columns(
    schema: &SchemaRef,
    row: &mut Row,
    seq_value: i64,
    deleted_value: bool,
) {
    if schema.field_with_name(SystemColumnNames::SEQ).is_ok() {
        row.values.insert(
            SystemColumnNames::SEQ.to_string(),
            ScalarValue::Int64(Some(seq_value)),
        );
    }
    if schema.field_with_name(SystemColumnNames::DELETED).is_ok() {
        row.values.insert(
            SystemColumnNames::DELETED.to_string(),
            ScalarValue::Boolean(Some(deleted_value)),
        );
    }
}

/// Trait implemented by provider row types to expose system columns and JSON payload
pub trait ScanRow {
    fn row(&self) -> &Row;
    fn seq_value(&self) -> i64;
    fn deleted_flag(&self) -> bool;
}

impl ScanRow for SharedTableRow {
    fn row(&self) -> &Row {
        &self.fields
    }

    fn seq_value(&self) -> i64 {
        self._seq.as_i64()
    }

    fn deleted_flag(&self) -> bool {
        self._deleted
    }
}

impl ScanRow for UserTableRow {
    fn row(&self) -> &Row {
        &self.fields
    }

    fn seq_value(&self) -> i64 {
        self._seq.as_i64()
    }

    fn deleted_flag(&self) -> bool {
        self._deleted
    }
}

impl ScanRow for StreamTableRow {
    fn row(&self) -> &Row {
        &self.fields
    }

    fn seq_value(&self) -> i64 {
        self._seq.as_i64()
    }

    fn deleted_flag(&self) -> bool {
        false
    }
}

/// Convert resolved key-value rows into an Arrow RecordBatch with system columns injected
pub fn rows_to_arrow_batch<K, R, F>(
    schema: &SchemaRef,
    kvs: Vec<(K, R)>,
    projection: Option<&Vec<usize>>,
    mut enrich_row: F,
) -> Result<RecordBatch, KalamDbError>
where
    R: ScanRow,
    F: FnMut(&mut Row, &R),
{
    let row_count = kvs.len();

    if let Some(proj) = projection {
        if proj.is_empty() {
            let empty_fields: Vec<datafusion::arrow::datatypes::Field> = Vec::new();
            let empty_schema = Arc::new(datafusion::arrow::datatypes::Schema::new(empty_fields));
            if row_count == 0 {
                return Ok(RecordBatch::new_empty(empty_schema));
            }

            let options = RecordBatchOptions::new().with_row_count(Some(row_count));
            return RecordBatch::try_new_with_options(empty_schema, vec![], &options).map_err(
                |e| KalamDbError::InvalidOperation(format!("Failed to build Arrow batch: {}", e)),
            );
        }
    }

    let mut rows: Vec<Row> = Vec::with_capacity(row_count);

    for (_key, row) in kvs.into_iter() {
        let mut materialized = row.row().clone();

        enrich_row(&mut materialized, &row);

        inject_system_columns(
            schema,
            &mut materialized,
            row.seq_value(),
            row.deleted_flag(),
        );
        rows.push(materialized);
    }

    // If projection is provided, we should project the schema and only convert relevant fields
    // However, the conversion helper currently takes the full schema and rows.
    // For now, we pass the full schema and let DataFusion handle projection later,
    // OR we can project the schema here.
    //
    // Optimization: Project schema here to avoid converting unused fields.
    let target_schema = if let Some(proj) = projection {
        let fields: Vec<datafusion::arrow::datatypes::Field> =
            proj.iter().map(|i| schema.field(*i).clone()).collect();
        Arc::new(datafusion::arrow::datatypes::Schema::new(fields))
    } else {
        schema.clone()
    };

    json_rows_to_arrow_batch(&target_schema, rows)
        .map_err(|e| KalamDbError::InvalidOperation(format!("Failed to build Arrow batch: {}", e)))
}
