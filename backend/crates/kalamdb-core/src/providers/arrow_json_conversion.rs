//! Shared utilities for converting between Arrow and JSON formats
//!
//! This module provides common functions for converting data between:
//! - Arrow RecordBatch (columnar format for DataFusion queries)
//! - JSON objects (row-oriented format for storage)
//!
//! These utilities are used by user_table_provider, shared_table_provider,
//! and stream_table_provider to avoid code duplication.

use crate::error::KalamDbError;
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

    // Pre-calculate default values for each column to avoid repeated logic in loop
    let defaults: Vec<Option<ScalarValue>> = schema.fields().iter().map(|field| {
        if field.is_nullable() {
            return None;
        }
        match field.data_type() {
            DataType::Boolean => Some(ScalarValue::Boolean(Some(false))),
            DataType::Int8 => Some(ScalarValue::Int8(Some(0))),
            DataType::Int16 => Some(ScalarValue::Int16(Some(0))),
            DataType::Int32 => Some(ScalarValue::Int32(Some(0))),
            DataType::Int64 => Some(ScalarValue::Int64(Some(0))),
            DataType::UInt8 => Some(ScalarValue::UInt8(Some(0))),
            DataType::UInt16 => Some(ScalarValue::UInt16(Some(0))),
            DataType::UInt32 => Some(ScalarValue::UInt32(Some(0))),
            DataType::UInt64 => Some(ScalarValue::UInt64(Some(0))),
            DataType::Float32 => Some(ScalarValue::Float32(Some(0.0))),
            DataType::Float64 => Some(ScalarValue::Float64(Some(0.0))),
            DataType::Utf8 => Some(ScalarValue::Utf8(Some("".to_string()))),
            DataType::LargeUtf8 => Some(ScalarValue::LargeUtf8(Some("".to_string()))),
            DataType::Timestamp(TimeUnit::Microsecond, tz_opt) => {
                let micros = Utc::now().timestamp_micros();
                Some(ScalarValue::TimestampMicrosecond(
                    Some(micros),
                    tz_opt.clone(),
                ))
            }
            DataType::Timestamp(TimeUnit::Millisecond, tz_opt) => {
                let millis = Utc::now().timestamp_millis();
                Some(ScalarValue::TimestampMillisecond(
                    Some(millis),
                    tz_opt.clone(),
                ))
            }
            DataType::Date32 => Some(ScalarValue::Date32(Some(0))),
            DataType::Date64 => Some(ScalarValue::Date64(Some(0))),
            _ => None,
        }
    }).collect();

    let typed_nulls: Vec<ScalarValue> = schema.fields().iter().map(|field| {
        ScalarValue::try_from(field.data_type()).unwrap_or(ScalarValue::Null)
    }).collect();

    // Transpose rows to columns (Row-oriented -> Column-oriented)
    // We use move semantics (row.values.remove) to avoid cloning strings/blobs
    let mut columns: Vec<Vec<ScalarValue>> = (0..schema.fields().len())
        .map(|_| Vec::with_capacity(rows.len()))
        .collect();

    for mut row in rows {
        for (i, field) in schema.fields().iter().enumerate() {
            let field_name = field.name().as_str();
            
            // Take value from map (move) instead of cloning
            let raw_value = row.values.remove(field_name)
                .or_else(|| defaults[i].clone())
                .unwrap_or_else(|| typed_nulls[i].clone());
            
            // We still need to coerce, but now we own the value
            let coerced = coerce_scalar_to_field(raw_value, field)
                .map_err(|e| format!("Failed to coerce column '{}': {}", field.name(), e))?;
            
            columns[i].push(coerced);
        }
    }

    // Build arrays from columns
    let mut arrays: Vec<ArrayRef> = Vec::with_capacity(schema.fields().len());
    for (i, col_values) in columns.into_iter().enumerate() {
        let field_name = schema.field(i).name();
        let array = ScalarValue::iter_to_array(col_values.into_iter())
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
        DataType::Date32 => {
            let arr = array.as_any().downcast_ref::<Date32Array>().unwrap();
            Ok(ScalarValue::Date32(Some(arr.value(row_idx))))
        }
        DataType::Date64 => {
            let arr = array.as_any().downcast_ref::<Date64Array>().unwrap();
            Ok(ScalarValue::Date64(Some(arr.value(row_idx))))
        }
        DataType::Time32(TimeUnit::Second) => {
            let arr = array.as_any().downcast_ref::<Time32SecondArray>().unwrap();
            Ok(ScalarValue::Time32Second(Some(arr.value(row_idx))))
        }
        DataType::Time32(TimeUnit::Millisecond) => {
            let arr = array
                .as_any()
                .downcast_ref::<Time32MillisecondArray>()
                .unwrap();
            Ok(ScalarValue::Time32Millisecond(Some(arr.value(row_idx))))
        }
        DataType::Time64(TimeUnit::Microsecond) => {
            let arr = array
                .as_any()
                .downcast_ref::<Time64MicrosecondArray>()
                .unwrap();
            Ok(ScalarValue::Time64Microsecond(Some(arr.value(row_idx))))
        }
        DataType::Time64(TimeUnit::Nanosecond) => {
            let arr = array
                .as_any()
                .downcast_ref::<Time64NanosecondArray>()
                .unwrap();
            Ok(ScalarValue::Time64Nanosecond(Some(arr.value(row_idx))))
        }
        DataType::FixedSizeBinary(size) => {
            let arr = array
                .as_any()
                .downcast_ref::<FixedSizeBinaryArray>()
                .unwrap();
            Ok(ScalarValue::FixedSizeBinary(
                *size,
                Some(arr.value(row_idx).to_vec()),
            ))
        }
        DataType::Decimal128(precision, scale) => {
            let arr = array.as_any().downcast_ref::<Decimal128Array>().unwrap();
            Ok(ScalarValue::Decimal128(
                Some(arr.value(row_idx)),
                *precision,
                *scale,
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

/// Convert DataFusion ScalarValue to serde_json::Value
pub fn scalar_value_to_json(value: &ScalarValue) -> Result<JsonValue, KalamDbError> {
    match value {
        ScalarValue::Null => Ok(JsonValue::Null),
        ScalarValue::Boolean(Some(b)) => Ok(JsonValue::Bool(*b)),
        ScalarValue::Int8(Some(i)) => Ok(JsonValue::Number((*i).into())),
        ScalarValue::Int16(Some(i)) => Ok(JsonValue::Number((*i).into())),
        ScalarValue::Int32(Some(i)) => Ok(JsonValue::Number((*i).into())),
        ScalarValue::Int64(Some(i)) => Ok(JsonValue::Number((*i).into())),
        ScalarValue::UInt8(Some(i)) => Ok(JsonValue::Number((*i).into())),
        ScalarValue::UInt16(Some(i)) => Ok(JsonValue::Number((*i).into())),
        ScalarValue::UInt32(Some(i)) => Ok(JsonValue::Number((*i).into())),
        ScalarValue::UInt64(Some(i)) => Ok(JsonValue::Number((*i).into())),
        ScalarValue::Float32(Some(f)) => serde_json::Number::from_f64(*f as f64)
            .map(JsonValue::Number)
            .ok_or_else(|| KalamDbError::InvalidOperation("Invalid float value".into())),
        ScalarValue::Float64(Some(f)) => serde_json::Number::from_f64(*f)
            .map(JsonValue::Number)
            .ok_or_else(|| KalamDbError::InvalidOperation("Invalid float value".into())),
        ScalarValue::Utf8(Some(s)) | ScalarValue::LargeUtf8(Some(s)) => {
            Ok(JsonValue::String(s.clone()))
        }
        ScalarValue::Date32(Some(d)) => Ok(JsonValue::Number((*d).into())),
        ScalarValue::Date64(Some(d)) => Ok(JsonValue::Number((*d).into())),
        ScalarValue::TimestampMillisecond(Some(ts), _) => Ok(JsonValue::Number((*ts).into())),
        ScalarValue::TimestampMicrosecond(Some(ts), _) => Ok(JsonValue::Number((*ts).into())),
        ScalarValue::TimestampNanosecond(Some(ts), _) => Ok(JsonValue::Number((*ts).into())),
        ScalarValue::Decimal128(Some(_v), _, _scale) => {
            // Convert to string to preserve precision, or float?
            // For now, let's use string representation of the decimal
            // We can construct it from the i128 value and scale
            // But simpler might be to just cast to f64 for JSON if precision allows,
            // or string.
            // Let's use string to be safe.
            // Actually, let's just use the string representation provided by ScalarValue display
            Ok(JsonValue::String(value.to_string()))
        }
        ScalarValue::FixedSizeBinary(_, Some(bytes)) => {
            // If it looks like a UUID (16 bytes), try to format as UUID string
            if bytes.len() == 16 {
                if let Ok(uuid) = uuid::Uuid::from_slice(bytes) {
                    return Ok(JsonValue::String(uuid.to_string()));
                }
            }
            // Otherwise, maybe base64? Or just array of bytes?
            // Let's use array of numbers for generic binary
            Ok(JsonValue::Array(
                bytes.iter().map(|&b| JsonValue::Number(b.into())).collect(),
            ))
        }
        _ => Err(KalamDbError::InvalidOperation(format!(
            "Unsupported ScalarValue conversion to JSON: {:?}",
            value
        ))),
    }
}
