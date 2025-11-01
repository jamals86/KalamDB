//! Shared utilities for converting between Arrow and JSON formats
//!
//! This module provides common functions for converting data between:
//! - Arrow RecordBatch (columnar format for DataFusion queries)
//! - JSON objects (row-oriented format for storage)
//!
//! These utilities are used by user_table_provider, shared_table_provider,
//! and stream_table_provider to avoid code duplication.

use datafusion::arrow::array::*;
use datafusion::arrow::datatypes::{DataType, SchemaRef, TimeUnit};
use datafusion::arrow::record_batch::RecordBatch;
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// Type alias for Arc<dyn Array> to improve readability
type ArrayRef = Arc<dyn datafusion::arrow::array::Array>;

/// Convert Arrow RecordBatch to Vec of JSON objects
///
/// This is used for INSERT operations - converts Arrow columnar
/// data to row-oriented JSON objects suitable for storage.
///
/// # Arguments
/// * `batch` - Arrow RecordBatch to convert
/// * `skip_system_columns` - If true, skip _updated and _deleted columns
///
/// # Returns
/// Vector of JSON objects (one per row)
///
/// # Supported Arrow Types
/// - Utf8 (String)
/// - Int32, Int64 (Integers)
/// - Float64 (Floating point)
/// - Boolean
/// - Timestamp (seconds, milliseconds, microseconds, nanoseconds)
///
/// # Errors
/// Returns error if:
/// - Column type doesn't match expected Arrow type
/// - Unsupported data type encountered
pub fn arrow_batch_to_json(
    batch: &RecordBatch,
    skip_system_columns: bool,
) -> Result<Vec<JsonValue>, String> {
    let schema = batch.schema();
    let num_rows = batch.num_rows();
    let mut rows = Vec::with_capacity(num_rows);

    for row_idx in 0..num_rows {
        let mut row_map = serde_json::Map::new();

        for (col_idx, field) in schema.fields().iter().enumerate() {
            let column = batch.column(col_idx);
            let field_name = field.name();

            // Skip system columns if requested (they'll be added by insert handlers)
            if skip_system_columns && (field_name == "_updated" || field_name == "_deleted") {
                continue;
            }

            // Extract value based on data type
            let value = match field.data_type() {
                DataType::Utf8 => {
                    let array = column
                        .as_any()
                        .downcast_ref::<StringArray>()
                        .ok_or_else(|| {
                            format!("Failed to downcast column {} to StringArray", field_name)
                        })?;

                    if array.is_null(row_idx) {
                        JsonValue::Null
                    } else {
                        JsonValue::String(array.value(row_idx).to_string())
                    }
                }
                DataType::Int32 => {
                    let array = column
                        .as_any()
                        .downcast_ref::<Int32Array>()
                        .ok_or_else(|| {
                            format!("Failed to downcast column {} to Int32Array", field_name)
                        })?;

                    if array.is_null(row_idx) {
                        JsonValue::Null
                    } else {
                        JsonValue::Number(array.value(row_idx).into())
                    }
                }
                DataType::Int64 => {
                    let array = column
                        .as_any()
                        .downcast_ref::<Int64Array>()
                        .ok_or_else(|| {
                            format!("Failed to downcast column {} to Int64Array", field_name)
                        })?;

                    if array.is_null(row_idx) {
                        JsonValue::Null
                    } else {
                        JsonValue::Number(array.value(row_idx).into())
                    }
                }
                DataType::Float64 => {
                    let array =
                        column
                            .as_any()
                            .downcast_ref::<Float64Array>()
                            .ok_or_else(|| {
                                format!("Failed to downcast column {} to Float64Array", field_name)
                            })?;

                    if array.is_null(row_idx) {
                        JsonValue::Null
                    } else {
                        let f = array.value(row_idx);
                        JsonValue::Number(
                            serde_json::Number::from_f64(f)
                                .ok_or_else(|| format!("Invalid f64 value: {}", f))?,
                        )
                    }
                }
                DataType::Boolean => {
                    let array =
                        column
                            .as_any()
                            .downcast_ref::<BooleanArray>()
                            .ok_or_else(|| {
                                format!("Failed to downcast column {} to BooleanArray", field_name)
                            })?;

                    if array.is_null(row_idx) {
                        JsonValue::Null
                    } else {
                        JsonValue::Bool(array.value(row_idx))
                    }
                }
                DataType::Timestamp(unit, _) => {
                    // Convert timestamp to milliseconds for consistency
                    let value_ms = match unit {
                        TimeUnit::Second => {
                            let array = column
                                .as_any()
                                .downcast_ref::<TimestampSecondArray>()
                                .ok_or_else(|| {
                                    format!(
                                        "Failed to downcast column {} to TimestampSecondArray",
                                        field_name
                                    )
                                })?;
                            if array.is_null(row_idx) {
                                JsonValue::Null
                            } else {
                                JsonValue::Number((array.value(row_idx) * 1000).into())
                            }
                        }
                        TimeUnit::Millisecond => {
                            let array = column
                                .as_any()
                                .downcast_ref::<TimestampMillisecondArray>()
                                .ok_or_else(|| {
                                    format!(
                                        "Failed to downcast column {} to TimestampMillisecondArray",
                                        field_name
                                    )
                                })?;
                            if array.is_null(row_idx) {
                                JsonValue::Null
                            } else {
                                JsonValue::Number(array.value(row_idx).into())
                            }
                        }
                        TimeUnit::Microsecond => {
                            let array = column
                                .as_any()
                                .downcast_ref::<TimestampMicrosecondArray>()
                                .ok_or_else(|| {
                                    format!(
                                        "Failed to downcast column {} to TimestampMicrosecondArray",
                                        field_name
                                    )
                                })?;
                            if array.is_null(row_idx) {
                                JsonValue::Null
                            } else {
                                JsonValue::Number((array.value(row_idx) / 1000).into())
                            }
                        }
                        TimeUnit::Nanosecond => {
                            let array = column
                                .as_any()
                                .downcast_ref::<TimestampNanosecondArray>()
                                .ok_or_else(|| {
                                    format!(
                                        "Failed to downcast column {} to TimestampNanosecondArray",
                                        field_name
                                    )
                                })?;
                            if array.is_null(row_idx) {
                                JsonValue::Null
                            } else {
                                JsonValue::Number((array.value(row_idx) / 1_000_000).into())
                            }
                        }
                    };

                    value_ms
                }
                _ => {
                    return Err(format!(
                        "Unsupported data type {:?} for column {}",
                        field.data_type(),
                        field_name
                    ));
                }
            };

            row_map.insert(field_name.clone(), value);
        }

        rows.push(JsonValue::Object(row_map));
    }

    Ok(rows)
}

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
/// - Timestamp (milliseconds) - can parse from i64 or RFC3339 string
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
            DataType::Timestamp(TimeUnit::Millisecond, _) => {
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
                Arc::new(TimestampMillisecondArray::from(values)) as ArrayRef
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
        DataType::Timestamp(TimeUnit::Millisecond, _) => {
            Arc::new(TimestampMillisecondArray::from(Vec::<Option<i64>>::new()))
        }
        _ => Arc::new(StringArray::from(Vec::<Option<String>>::new())), // Fallback
    }
}

/// Validate that INSERT rows comply with schema constraints
///
/// Checks:
/// - NOT NULL constraints (non-nullable columns must have values)
/// - Data type compatibility (basic type checking)
///
/// # Arguments
/// * `schema` - Arrow schema defining column constraints
/// * `rows` - Vector of JSON objects to validate
///
/// # Returns
/// Ok(()) if all rows are valid, Err with detailed message otherwise
///
/// # Notes
/// - Skips system columns (_updated, _deleted) - they're auto-populated
/// - Skips auto-generated columns (id, created_at) - handled by insert handlers
pub fn validate_insert_rows(schema: &SchemaRef, rows: &[JsonValue]) -> Result<(), String> {
    for (row_idx, row) in rows.iter().enumerate() {
        let obj = row
            .as_object()
            .ok_or_else(|| format!("Row {} is not a JSON object", row_idx))?;

        // Check each field in the schema
        for field in schema.fields() {
            let field_name = field.name();

            // Skip system columns - they're auto-populated
            if field_name == "_updated" || field_name == "_deleted" {
                continue;
            }

            // Skip auto-generated columns if any
            if field_name == "id" || field_name == "created_at" {
                continue;
            }

            let value = obj.get(field_name);

            // Check NOT NULL constraint
            if !field.is_nullable() {
                match value {
                    None | Some(JsonValue::Null) => {
                        return Err(format!(
                            "Row {}: Column '{}' is declared as non-nullable but received NULL value. Please provide a value for this column.",
                            row_idx, field_name
                        ));
                    }
                    _ => {} // Has a value, constraint satisfied
                }
            }

            // Optional: Basic type validation
            if let Some(val) = value {
                if !val.is_null() {
                    let type_valid = match field.data_type() {
                        DataType::Int32 | DataType::Int64 => val.is_i64() || val.is_u64(),
                        DataType::Float64 => val.is_f64() || val.is_i64() || val.is_u64(),
                        DataType::Utf8 => val.is_string(),
                        DataType::Boolean => val.is_boolean(),
                        _ => true, // Skip validation for other types
                    };

                    if !type_valid {
                        return Err(format!(
                            "Row {}: Column '{}' has incompatible type. Expected {:?}, got {:?}",
                            row_idx,
                            field_name,
                            field.data_type(),
                            val
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::arrow::datatypes::{DataType, Field, Schema};
    use serde_json::json;
    use std::sync::Arc;

    #[test]
    fn test_arrow_batch_to_json_basic() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, true),
            Field::new("active", DataType::Boolean, false),
        ]));

        let id_array = Int64Array::from(vec![1, 2, 3]);
        let name_array = StringArray::from(vec![Some("Alice"), None, Some("Charlie")]);
        let active_array = BooleanArray::from(vec![true, false, true]);

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(id_array),
                Arc::new(name_array),
                Arc::new(active_array),
            ],
        )
        .unwrap();

        let json_rows = arrow_batch_to_json(&batch, false).unwrap();
        assert_eq!(json_rows.len(), 3);

        assert_eq!(json_rows[0]["id"], 1);
        assert_eq!(json_rows[0]["name"], "Alice");
        assert_eq!(json_rows[0]["active"], true);

        assert_eq!(json_rows[1]["id"], 2);
        assert_eq!(json_rows[1]["name"], JsonValue::Null);
        assert_eq!(json_rows[1]["active"], false);
    }

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
    fn test_validate_insert_rows_success() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false), // NOT NULL
            Field::new("name", DataType::Utf8, true), // NULLABLE
        ]));

        let rows = vec![
            json!({"id": 1, "name": "Alice"}),
            json!({"id": 2, "name": null}), // Null is OK for nullable column
        ];

        assert!(validate_insert_rows(&schema, &rows).is_ok());
    }

    #[test]
    fn test_validate_insert_rows_not_null_violation() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),  // NOT NULL
            Field::new("name", DataType::Utf8, false), // NOT NULL
        ]));

        let rows = vec![
            json!({"id": 1, "name": "Alice"}),
            json!({"id": 2, "name": null}), // Should fail - NOT NULL violation
        ];

        let result = validate_insert_rows(&schema, &rows);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("non-nullable"));
    }

    #[test]
    fn test_validate_insert_rows_type_mismatch() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("active", DataType::Boolean, false),
        ]));

        let rows = vec![
            json!({"id": 1, "active": true}),
            json!({"id": 2, "active": "not_a_boolean"}), // Type mismatch
        ];

        let result = validate_insert_rows(&schema, &rows);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("incompatible type"));
    }

    #[test]
    fn test_skip_system_columns() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("_updated", DataType::Int64, false),
            Field::new("_deleted", DataType::Boolean, false),
        ]));

        let id_array = Int64Array::from(vec![1]);
        let updated_array = Int64Array::from(vec![1234567890]);
        let deleted_array = BooleanArray::from(vec![false]);

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(id_array),
                Arc::new(updated_array),
                Arc::new(deleted_array),
            ],
        )
        .unwrap();

        // With skip_system_columns = true
        let json_rows = arrow_batch_to_json(&batch, true).unwrap();
        assert_eq!(json_rows[0].as_object().unwrap().len(), 1); // Only "id"
        assert!(json_rows[0].get("_updated").is_none());
        assert!(json_rows[0].get("_deleted").is_none());

        // With skip_system_columns = false
        let json_rows = arrow_batch_to_json(&batch, false).unwrap();
        assert_eq!(json_rows[0].as_object().unwrap().len(), 3); // All columns
        assert!(json_rows[0].get("_updated").is_some());
        assert!(json_rows[0].get("_deleted").is_some());
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
}
