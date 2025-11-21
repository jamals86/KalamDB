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
use datafusion::arrow::datatypes::{DataType, Field, SchemaRef, TimeUnit};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::scalar::ScalarValue;
use kalamdb_commons::models::Row;
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::convert::TryFrom;
use std::sync::Arc;

/// Type alias for Arc<dyn Array> to improve readability
type ArrayRef = Arc<dyn datafusion::arrow::array::Array>;

/// Convert ScalarValue-backed rows to Arrow RecordBatch
///
/// SELECT operations build row-oriented `Row` values (ScalarValue map) directly from storage.
/// This helper converts them into Arrow's columnar format without JSON round-trips.
pub fn json_rows_to_arrow_batch(schema: &SchemaRef, rows: Vec<Row>) -> Result<RecordBatch, String> {
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
        let field_name = field.name().clone();
        let field_name_str = field_name.as_str();
        let typed_null = ScalarValue::try_from(field.data_type()).unwrap_or(ScalarValue::Null);

        // Auto-populate NOT NULL timestamp columns when value missing (matches legacy behavior)
        let default_value = match (field.data_type(), field.is_nullable()) {
            (DataType::Timestamp(TimeUnit::Microsecond, tz_opt), false) => {
                let micros = Utc::now().timestamp_micros();
                Some(ScalarValue::TimestampMicrosecond(
                    Some(micros),
                    tz_opt.clone(),
                ))
            }
            _ => None,
        };

        let mut column_values: Vec<ScalarValue> = Vec::with_capacity(rows.len());
        for row in &rows {
            let raw_value = row
                .values
                .get(field_name_str)
                .cloned()
                .or_else(|| default_value.clone())
                .unwrap_or_else(|| typed_null.clone());
            let coerced = coerce_scalar_to_field(raw_value, field)
                .map_err(|e| format!("Failed to coerce column '{}': {}", field_name, e))?;
            column_values.push(coerced);
        }

        let array = ScalarValue::iter_to_array(column_values.into_iter())
            .map_err(|e| format!("Failed to build column '{}': {}", field_name, e))?;

        arrays.push(array);
    }

    RecordBatch::try_new(schema.clone(), arrays)
        .map_err(|e| format!("Failed to build record batch: {}", e))
}

fn create_empty_array(data_type: &DataType) -> ArrayRef {
    new_empty_array(data_type)
}

fn coerce_scalar_to_field(value: ScalarValue, field: &Field) -> Result<ScalarValue, String> {
    if matches!(value, ScalarValue::Null) {
        return ScalarValue::try_from(field.data_type())
            .map_err(|e| format!("Failed to build typed NULL for '{}': {}", field.name(), e));
    }

    if &value.data_type() == field.data_type() {
        return Ok(value);
    }

    value.cast_to(field.data_type()).map_err(|e| {
        format!(
            "Unable to cast value {:?} to {:?} for column '{}': {}",
            value.data_type(),
            field.data_type(),
            field.name(),
            e
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::arrow::datatypes::{DataType, Field, Schema};
    use std::collections::BTreeMap;
    use std::sync::Arc;

    fn make_row(entries: Vec<(&str, ScalarValue)>) -> Row {
        let mut values = BTreeMap::new();
        for (key, value) in entries {
            values.insert(key.to_string(), value);
        }
        Row::new(values)
    }

    #[test]
    fn test_json_rows_to_arrow_batch_basic() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, true),
        ]));

        let rows = vec![
            make_row(vec![
                ("id", ScalarValue::Int64(Some(1))),
                ("name", ScalarValue::Utf8(Some("Alice".into()))),
            ]),
            make_row(vec![
                ("id", ScalarValue::Int64(Some(2))),
                ("name", ScalarValue::Null),
            ]),
            make_row(vec![
                ("id", ScalarValue::Int64(Some(3))),
                ("name", ScalarValue::Utf8(Some("Charlie".into()))),
            ]),
        ];

        let batch = json_rows_to_arrow_batch(&schema, rows).unwrap();
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
        let rows = vec![make_row(vec![(
            "ts",
            ScalarValue::Int64(Some(1609459200000)),
        )])];
        let batch = json_rows_to_arrow_batch(&schema, rows).unwrap();
        let ts_col = batch
            .column(0)
            .as_any()
            .downcast_ref::<TimestampMillisecondArray>()
            .unwrap();
        assert_eq!(ts_col.value(0), 1609459200000i64);

        // Test RFC3339 string
        let rows = vec![make_row(vec![(
            "ts",
            ScalarValue::Utf8(Some("2021-01-01T00:00:00Z".into())),
        )])];
        let batch = json_rows_to_arrow_batch(&schema, rows).unwrap();
        let ts_col = batch
            .column(0)
            .as_any()
            .downcast_ref::<TimestampMillisecondArray>()
            .unwrap();
        assert_eq!(ts_col.value(0), 1609459200000i64);
    }
}

/// Convert Arrow array value at given index to DataFusion ScalarValue
pub fn arrow_value_to_scalar(
    array: &dyn datafusion::arrow::array::Array,
    row_idx: usize,
) -> Result<ScalarValue, datafusion::error::DataFusionError> {
    use datafusion::arrow::array::*;
    use datafusion::arrow::datatypes::*;

    if array.is_null(row_idx) {
        return Ok(ScalarValue::try_from(array.data_type()).unwrap_or(ScalarValue::Null));
    }

    match array.data_type() {
        DataType::Boolean => {
            let arr = array.as_any().downcast_ref::<BooleanArray>().unwrap();
            Ok(ScalarValue::Boolean(Some(arr.value(row_idx))))
        }
        DataType::Int8 => {
            let arr = array.as_any().downcast_ref::<Int8Array>().unwrap();
            Ok(ScalarValue::Int8(Some(arr.value(row_idx))))
        }
        DataType::Int16 => {
            let arr = array.as_any().downcast_ref::<Int16Array>().unwrap();
            Ok(ScalarValue::Int16(Some(arr.value(row_idx))))
        }
        DataType::Int32 => {
            let arr = array.as_any().downcast_ref::<Int32Array>().unwrap();
            Ok(ScalarValue::Int32(Some(arr.value(row_idx))))
        }
        DataType::Int64 => {
            let arr = array.as_any().downcast_ref::<Int64Array>().unwrap();
            Ok(ScalarValue::Int64(Some(arr.value(row_idx))))
        }
        DataType::UInt8 => {
            let arr = array.as_any().downcast_ref::<UInt8Array>().unwrap();
            Ok(ScalarValue::UInt8(Some(arr.value(row_idx))))
        }
        DataType::UInt16 => {
            let arr = array.as_any().downcast_ref::<UInt16Array>().unwrap();
            Ok(ScalarValue::UInt16(Some(arr.value(row_idx))))
        }
        DataType::UInt32 => {
            let arr = array.as_any().downcast_ref::<UInt32Array>().unwrap();
            Ok(ScalarValue::UInt32(Some(arr.value(row_idx))))
        }
        DataType::UInt64 => {
            let arr = array.as_any().downcast_ref::<UInt64Array>().unwrap();
            Ok(ScalarValue::UInt64(Some(arr.value(row_idx))))
        }
        DataType::Float32 => {
            let arr = array.as_any().downcast_ref::<Float32Array>().unwrap();
            Ok(ScalarValue::Float32(Some(arr.value(row_idx))))
        }
        DataType::Float64 => {
            let arr = array.as_any().downcast_ref::<Float64Array>().unwrap();
            Ok(ScalarValue::Float64(Some(arr.value(row_idx))))
        }
        DataType::Utf8 => {
            let arr = array.as_any().downcast_ref::<StringArray>().unwrap();
            Ok(ScalarValue::Utf8(Some(arr.value(row_idx).to_string())))
        }
        DataType::LargeUtf8 => {
            let arr = array.as_any().downcast_ref::<LargeStringArray>().unwrap();
            Ok(ScalarValue::LargeUtf8(Some(arr.value(row_idx).to_string())))
        }
        DataType::Timestamp(TimeUnit::Millisecond, tz) => {
            let arr = array
                .as_any()
                .downcast_ref::<TimestampMillisecondArray>()
                .unwrap();
            Ok(ScalarValue::TimestampMillisecond(
                Some(arr.value(row_idx)),
                tz.clone(),
            ))
        }
        DataType::Timestamp(TimeUnit::Microsecond, tz) => {
            let arr = array
                .as_any()
                .downcast_ref::<TimestampMicrosecondArray>()
                .unwrap();
            Ok(ScalarValue::TimestampMicrosecond(
                Some(arr.value(row_idx)),
                tz.clone(),
            ))
        }
        DataType::Timestamp(TimeUnit::Nanosecond, tz) => {
            let arr = array
                .as_any()
                .downcast_ref::<TimestampNanosecondArray>()
                .unwrap();
            Ok(ScalarValue::TimestampNanosecond(
                Some(arr.value(row_idx)),
                tz.clone(),
            ))
        }
        _ => {
            // Fallback: convert to string representation
            Ok(ScalarValue::Utf8(Some(format!(
                "{:?}",
                array.slice(row_idx, 1)
            ))))
        }
    }
}

/// Convert JSON Object to Row
///
/// Used for converting storage JSON rows to Row format for live query notifications.
pub fn json_to_row(json: &JsonValue) -> Option<Row> {
    if let JsonValue::Object(map) = json {
        let mut values = BTreeMap::new();
        for (k, v) in map {
            values.insert(k.clone(), json_value_to_scalar(v));
        }
        Some(Row::new(values))
    } else {
        None
    }
}

/// Convert serde_json::Value to DataFusion ScalarValue
pub fn json_value_to_scalar(v: &JsonValue) -> ScalarValue {
    match v {
        JsonValue::Null => ScalarValue::Null,
        JsonValue::Bool(b) => ScalarValue::Boolean(Some(*b)),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                ScalarValue::Int64(Some(i))
            } else if let Some(f) = n.as_f64() {
                ScalarValue::Float64(Some(f))
            } else {
                ScalarValue::Float64(None)
            }
        }
        JsonValue::String(s) => ScalarValue::Utf8(Some(s.clone())),
        JsonValue::Array(_) => ScalarValue::Utf8(Some(v.to_string())), // Fallback for arrays
        JsonValue::Object(_) => ScalarValue::Utf8(Some(v.to_string())), // Fallback for objects
    }
}
