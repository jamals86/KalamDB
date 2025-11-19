//! Shared utilities for converting between Arrow and JSON formats
//!
//! This module provides common functions for converting data between:
//! - Arrow RecordBatch (columnar format for DataFusion queries)
//! - JSON objects (row-oriented format for storage)
//!
//! These utilities are used by user_table_provider, shared_table_provider,
//! and stream_table_provider to avoid code duplication.

use chrono::Utc;
use datafusion::arrow::array::*;
use datafusion::arrow::datatypes::{DataType, SchemaRef, TimeUnit};
use datafusion::arrow::record_batch::RecordBatch;
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// Type alias for Arc<dyn Array> to improve readability
type ArrayRef = Arc<dyn datafusion::arrow::array::Array>;

/// Convert JSON rows to Arrow RecordBatch
///
/// This is used for SELECT operations - converts row-oriented
/// JSON objects to Arrow columnar format.
///
/// # Arguments
/// * `schema` - Arrow schema defining column types
/// * `rows` - Vector of JSON objects (one per row)
///
/// # Returns
/// Arrow RecordBatch with data in columnar format
///
/// # Supported Arrow Types
/// - Utf8 (String)
/// - Int32, Int64 (Integers)
/// - Float64 (Floating point)
/// - Boolean
/// - Timestamp (milliseconds, microseconds) - can parse from i64 or RFC3339 string
///
/// # Errors
/// Returns error if:
/// - Unsupported data type in schema
/// - Failed to create RecordBatch (mismatched column counts, etc.)
pub fn json_rows_to_arrow_batch(
    schema: &SchemaRef,
    rows: Vec<JsonValue>,
) -> Result<RecordBatch, String> {
    if rows.is_empty() {
        // Return empty batch with correct schema
        let empty_arrays: Vec<ArrayRef> = schema
            .fields()
            .iter()
            .map(|field| create_empty_array(field.data_type()))
            .collect();

        return RecordBatch::try_new(schema.clone(), empty_arrays)
            .map_err(|e| format!("Failed to create empty batch: {}", e));
    }

    // Build arrays for each column
    let mut arrays: Vec<ArrayRef> = Vec::new();

    for field in schema.fields() {
        let array = match field.data_type() {
            DataType::Utf8 => {
                let values: Vec<Option<String>> = rows
                    .iter()
                    .map(|row| {
                        row.get(field.name())
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    })
                    .collect();
                Arc::new(StringArray::from(values)) as ArrayRef
            }
            DataType::Int32 => {
                let values: Vec<Option<i32>> = rows
                    .iter()
                    .map(|row| {
                        row.get(field.name())
                            .and_then(|v| v.as_i64().map(|i| i as i32))
                    })
                    .collect();
                Arc::new(Int32Array::from(values)) as ArrayRef
            }
            DataType::Int64 => {
                let values: Vec<Option<i64>> = rows
                    .iter()
                    .map(|row| row.get(field.name()).and_then(|v| v.as_i64()))
                    .collect();
                Arc::new(Int64Array::from(values)) as ArrayRef
            }
            DataType::Float64 => {
                let values: Vec<Option<f64>> = rows
                    .iter()
                    .map(|row| row.get(field.name()).and_then(|v| v.as_f64()))
                    .collect();
                Arc::new(Float64Array::from(values)) as ArrayRef
            }
            DataType::Boolean => {
                let values: Vec<Option<bool>> = rows
                    .iter()
                    .map(|row| row.get(field.name()).and_then(|v| v.as_bool()))
                    .collect();
                Arc::new(BooleanArray::from(values)) as ArrayRef
            }
            DataType::Timestamp(TimeUnit::Millisecond, tz_opt) => {
                let values: Vec<Option<i64>> = rows
                    .iter()
                    .map(|row| {
                        row.get(field.name()).and_then(|v| {
                            // Try to parse as i64 timestamp or RFC3339 string
                            v.as_i64().or_else(|| {
                                v.as_str().and_then(|s| {
                                    chrono::DateTime::parse_from_rfc3339(s)
                                        .ok()
                                        .map(|dt| dt.timestamp_millis())
                                })
                            })
                        })
                    })
                    .collect();
                let arr = TimestampMillisecondArray::from(values);
                let arr = if let Some(tz) = tz_opt.as_deref() {
                    arr.with_timezone(tz)
                } else {
                    arr
                };
                Arc::new(arr) as ArrayRef
            }
            DataType::Timestamp(TimeUnit::Microsecond, tz_opt) => {
                // Default for non-nullable timestamp columns if missing
                let default_now_micros: i64 = {
                    let now = Utc::now();
                    now.timestamp() * 1_000_000 + (now.timestamp_subsec_nanos() as i64) / 1_000
                };
                let values: Vec<Option<i64>> = rows
                    .iter()
                    .map(|row| {
                        // Provide default for non-nullable timestamp fields if missing/null
                        match row.get(field.name()) {
                            Some(v) => v.as_i64().or_else(|| {
                                v.as_str().and_then(|s| {
                                    chrono::DateTime::parse_from_rfc3339(s).ok().map(|dt| {
                                        let secs = dt.timestamp();
                                        let micros = (dt.timestamp_subsec_nanos() as i64) / 1_000;
                                        secs * 1_000_000 + micros
                                    })
                                })
                            }),
                            None => {
                                if !field.is_nullable() {
                                    Some(default_now_micros)
                                } else {
                                    None
                                }
                            }
                        }
                    })
                    .collect();
                let arr = TimestampMicrosecondArray::from(values);
                let arr = if let Some(tz) = tz_opt.as_deref() {
                    arr.with_timezone(tz)
                } else {
                    arr
                };
                Arc::new(arr) as ArrayRef
            }
            _ => {
                return Err(format!(
                    "Unsupported data type for field {}: {:?}",
                    field.name(),
                    field.data_type()
                ));
            }
        };

        arrays.push(array);
    }

    RecordBatch::try_new(schema.clone(), arrays)
        .map_err(|e| format!("Failed to create RecordBatch: {}", e))
}

/// Helper function to create an empty array of the given type
///
/// Used for creating empty RecordBatches with correct schema
fn create_empty_array(data_type: &DataType) -> ArrayRef {
    match data_type {
        DataType::Utf8 => Arc::new(StringArray::from(Vec::<Option<String>>::new())),
        DataType::Int32 => Arc::new(Int32Array::from(Vec::<Option<i32>>::new())),
        DataType::Int64 => Arc::new(Int64Array::from(Vec::<Option<i64>>::new())),
        DataType::Float64 => Arc::new(Float64Array::from(Vec::<Option<f64>>::new())),
        DataType::Boolean => Arc::new(BooleanArray::from(Vec::<Option<bool>>::new())),
        DataType::Timestamp(TimeUnit::Millisecond, tz_opt) => {
            let arr = TimestampMillisecondArray::from(Vec::<Option<i64>>::new());
            let arr = if let Some(tz) = tz_opt.as_deref() {
                arr.with_timezone(tz)
            } else {
                arr
            };
            Arc::new(arr)
        }
        DataType::Timestamp(TimeUnit::Microsecond, tz_opt) => {
            let arr = TimestampMicrosecondArray::from(Vec::<Option<i64>>::new());
            let arr = if let Some(tz) = tz_opt.as_deref() {
                arr.with_timezone(tz)
            } else {
                arr
            };
            Arc::new(arr)
        }
        _ => Arc::new(StringArray::from(Vec::<Option<String>>::new())), // Fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::arrow::datatypes::{DataType, Field, Schema};
    use serde_json::json;
    use std::sync::Arc;

    #[test]
    fn test_json_rows_to_arrow_batch_basic() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, true),
        ]));

        let json_rows = vec![
            json!({"id": 1, "name": "Alice"}),
            json!({"id": 2, "name": null}),
            json!({"id": 3, "name": "Charlie"}),
        ];

        let batch = json_rows_to_arrow_batch(&schema, json_rows).unwrap();
        assert_eq!(batch.num_rows(), 3);
        assert_eq!(batch.num_columns(), 2);

        let id_col = batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        assert_eq!(id_col.value(0), 1);
        assert_eq!(id_col.value(1), 2);
        assert_eq!(id_col.value(2), 3);

        let name_col = batch
            .column(1)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(name_col.value(0), "Alice");
        assert!(name_col.is_null(1));
        assert_eq!(name_col.value(2), "Charlie");
    }

    #[test]
    fn test_empty_batch() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, true),
        ]));

        let batch = json_rows_to_arrow_batch(&schema, vec![]).unwrap();
        assert_eq!(batch.num_rows(), 0);
        assert_eq!(batch.num_columns(), 2);
    }

    #[test]
    fn test_timestamp_conversion() {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "ts",
            DataType::Timestamp(TimeUnit::Millisecond, None),
            false,
        )]));

        // Test i64 timestamp
        let json_rows = vec![json!({"ts": 1609459200000i64})]; // 2021-01-01 00:00:00 UTC
        let batch = json_rows_to_arrow_batch(&schema, json_rows).unwrap();
        let ts_col = batch
            .column(0)
            .as_any()
            .downcast_ref::<TimestampMillisecondArray>()
            .unwrap();
        assert_eq!(ts_col.value(0), 1609459200000i64);

        // Test RFC3339 string
        let json_rows = vec![json!({"ts": "2021-01-01T00:00:00Z"})];
        let batch = json_rows_to_arrow_batch(&schema, json_rows).unwrap();
        let ts_col = batch
            .column(0)
            .as_any()
            .downcast_ref::<TimestampMillisecondArray>()
            .unwrap();
        assert_eq!(ts_col.value(0), 1609459200000i64);
    }

    #[test]
    fn test_record_batch_to_json_rows_basic() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, true),
        ]));

        let json_rows = vec![
            json!({"id": 1, "name": "Alice"}),
            json!({"id": 2, "name": null}),
            json!({"id": 3, "name": "Charlie"}),
        ];

        let batch = json_rows_to_arrow_batch(&schema, json_rows).unwrap();
        let json_output = record_batch_to_json_rows(&batch).unwrap();

        assert_eq!(json_output.len(), 3);
        assert_eq!(json_output[0], json!({"id": 1, "name": "Alice"}));
        assert_eq!(json_output[1], json!({"id": 2, "name": null}));
        assert_eq!(json_output[2], json!({"id": 3, "name": "Charlie"}));
    }
}

/// Add system columns to an Arrow schema if they don't already exist
///
/// **Moved from base_table_provider.rs (Phase 13.6 cleanup)**
///
/// Ensures the schema contains _seq (Int64) and _deleted (Boolean).
/// This is used when scanning hot/cold storage for UPDATE/DELETE operations.
///
/// # Arguments
/// * `base_schema` - Base Arrow schema
///
/// # Returns
/// New schema with system columns added if missing
pub fn schema_with_system_columns(base_schema: &SchemaRef) -> SchemaRef {
    use datafusion::arrow::datatypes::{DataType, Field, Schema};

    let mut fields = base_schema.fields().to_vec();

    // Check if system columns already exist (added by SystemColumnsService during CREATE TABLE)
    let has_deleted = fields.iter().any(|f| f.name() == "_deleted");

    // Only add missing system columns
    // Note: SystemColumnsService adds them as (BigInt, Timestamp, Boolean) to TableDefinition
    // which becomes (Int64, Timestamp(Millisecond, None), Boolean) in Arrow schema

    if !has_deleted {
        fields.push(Arc::new(Field::new(
            "_deleted",
            DataType::Boolean,
            false, // NOT NULL
        )));
    }

    Arc::new(Schema::new(fields))
}

/// Convert Arrow array value at given index to serde_json::Value
///
/// Helper function for converting Arrow data types to JSON representation.
/// Used when extracting user-defined fields from RecordBatch.
///
/// # Arguments
/// * `array` - Arrow array
/// * `row_idx` - Row index to extract
///
/// # Returns
/// JSON value representation of the Arrow value
///
/// # Moved from base_table_provider.rs (Phase 13.6 cleanup)
pub fn arrow_value_to_json(
    array: &dyn datafusion::arrow::array::Array,
    row_idx: usize,
) -> Result<serde_json::Value, datafusion::error::DataFusionError> {
    use datafusion::arrow::array::*;
    use datafusion::arrow::datatypes::*;
    use serde_json::Value as JsonValue;

    if array.is_null(row_idx) {
        return Ok(JsonValue::Null);
    }

    match array.data_type() {
        DataType::Boolean => {
            let arr = array.as_any().downcast_ref::<BooleanArray>().unwrap();
            Ok(JsonValue::Bool(arr.value(row_idx)))
        }
        DataType::Int8 => {
            let arr = array.as_any().downcast_ref::<Int8Array>().unwrap();
            Ok(JsonValue::Number(arr.value(row_idx).into()))
        }
        DataType::Int16 => {
            let arr = array.as_any().downcast_ref::<Int16Array>().unwrap();
            Ok(JsonValue::Number(arr.value(row_idx).into()))
        }
        DataType::Int32 => {
            let arr = array.as_any().downcast_ref::<Int32Array>().unwrap();
            Ok(JsonValue::Number(arr.value(row_idx).into()))
        }
        DataType::Int64 => {
            let arr = array.as_any().downcast_ref::<Int64Array>().unwrap();
            Ok(JsonValue::Number(arr.value(row_idx).into()))
        }
        DataType::UInt8 => {
            let arr = array.as_any().downcast_ref::<UInt8Array>().unwrap();
            Ok(JsonValue::Number(arr.value(row_idx).into()))
        }
        DataType::UInt16 => {
            let arr = array.as_any().downcast_ref::<UInt16Array>().unwrap();
            Ok(JsonValue::Number(arr.value(row_idx).into()))
        }
        DataType::UInt32 => {
            let arr = array.as_any().downcast_ref::<UInt32Array>().unwrap();
            Ok(JsonValue::Number(arr.value(row_idx).into()))
        }
        DataType::UInt64 => {
            let arr = array.as_any().downcast_ref::<UInt64Array>().unwrap();
            Ok(JsonValue::Number(arr.value(row_idx).into()))
        }
        DataType::Float32 => {
            let arr = array.as_any().downcast_ref::<Float32Array>().unwrap();
            let val = arr.value(row_idx);
            Ok(serde_json::Number::from_f64(val as f64)
                .map(JsonValue::Number)
                .unwrap_or(JsonValue::Null))
        }
        DataType::Float64 => {
            let arr = array.as_any().downcast_ref::<Float64Array>().unwrap();
            let val = arr.value(row_idx);
            Ok(serde_json::Number::from_f64(val)
                .map(JsonValue::Number)
                .unwrap_or(JsonValue::Null))
        }
        DataType::Utf8 => {
            let arr = array.as_any().downcast_ref::<StringArray>().unwrap();
            Ok(JsonValue::String(arr.value(row_idx).to_string()))
        }
        DataType::LargeUtf8 => {
            let arr = array.as_any().downcast_ref::<LargeStringArray>().unwrap();
            Ok(JsonValue::String(arr.value(row_idx).to_string()))
        }
        DataType::Timestamp(TimeUnit::Millisecond, _) => {
            let arr = array
                .as_any()
                .downcast_ref::<TimestampMillisecondArray>()
                .unwrap();
            Ok(JsonValue::Number(arr.value(row_idx).into()))
        }
        DataType::Timestamp(TimeUnit::Microsecond, _) => {
            let arr = array
                .as_any()
                .downcast_ref::<TimestampMicrosecondArray>()
                .unwrap();
            Ok(JsonValue::Number(arr.value(row_idx).into()))
        }
        DataType::Timestamp(TimeUnit::Nanosecond, _) => {
            let arr = array
                .as_any()
                .downcast_ref::<TimestampNanosecondArray>()
                .unwrap();
            Ok(JsonValue::Number(arr.value(row_idx).into()))
        }
        _ => {
            // Fallback: convert to string representation
            Ok(JsonValue::String(format!("{:?}", array.slice(row_idx, 1))))
        }
    }
}

/// Convert Arrow RecordBatch to vector of JSON objects
///
/// This is used for converting query results back to JSON format
/// for API responses or internal processing.
///
/// # Arguments
/// * `batch` - Arrow RecordBatch
///
/// # Returns
/// Vector of JSON objects (one per row)
pub fn record_batch_to_json_rows(
    batch: &RecordBatch,
) -> Result<Vec<JsonValue>, datafusion::error::DataFusionError> {
    let mut rows = Vec::with_capacity(batch.num_rows());
    let schema = batch.schema();
    let num_rows = batch.num_rows();
    let num_cols = batch.num_columns();

    for row_idx in 0..num_rows {
        let mut row_map = serde_json::Map::with_capacity(num_cols);

        for col_idx in 0..num_cols {
            let field = schema.field(col_idx);
            let col = batch.column(col_idx);
            let value = arrow_value_to_json(col.as_ref(), row_idx)?;
            row_map.insert(field.name().clone(), value);
        }

        rows.push(JsonValue::Object(row_map));
    }

    Ok(rows)
}
