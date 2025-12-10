use arrow::array::{Array, ArrayRef, FixedSizeListArray, Float32Array};
use arrow_schema::{DataType, Field};
use datafusion::scalar::ScalarValue;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;
use thiserror::Error;

/// A unified Row representation that holds DataFusion ScalarValues
/// but serializes to clean, standard JSON for clients.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    // RowEnvelope values as column name -> ScalarValue map
    pub values: BTreeMap<String, ScalarValue>,
}

/// Compact, ordered representation of a row aligned with a physical schema.
#[derive(Debug, Clone, PartialEq)]
pub struct RowEnvelope {
    /// Column-aligned scalar values (Null when column missing)
    pub columns: Vec<ScalarValue>,
}

impl RowEnvelope {
    pub fn new(columns: Vec<ScalarValue>) -> Self {
        Self { columns }
    }

    pub fn len(&self) -> usize {
        self.columns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }
}

#[derive(Debug, Error)]
pub enum RowConversionError {
    #[error("column schema mismatch: expected {expected} columns, got {actual}")]
    ColumnCountMismatch { expected: usize, actual: usize },
}

/// Internal storage representation for ScalarValue
/// Uses derive for bincode compatibility (storage)
/// Note: Int64 and UInt64 are stored as strings to preserve precision in JSON
/// (JavaScript's Number.MAX_SAFE_INTEGER = 2^53-1 causes precision loss for large i64/u64)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum StoredScalarValue {
    Null,
    Boolean(Option<bool>),
    Float32(Option<f32>),
    Float64(Option<f64>),
    Int8(Option<i8>),
    Int16(Option<i16>),
    Int32(Option<i32>),
    /// Int64 stored as string to preserve precision in JSON
    Int64(Option<String>),
    UInt8(Option<u8>),
    UInt16(Option<u16>),
    UInt32(Option<u32>),
    /// UInt64 stored as string to preserve precision in JSON
    UInt64(Option<String>),
    Utf8(Option<String>),
    LargeUtf8(Option<String>),
    Binary(Option<Vec<u8>>),
    LargeBinary(Option<Vec<u8>>),
    FixedSizeBinary {
        size: i32,
        value: Option<Vec<u8>>,
    },
    Date32(Option<i32>),
    Time64Microsecond(Option<i64>),
    TimestampMillisecond {
        value: Option<i64>,
        timezone: Option<String>,
    },
    TimestampMicrosecond {
        value: Option<i64>,
        timezone: Option<String>,
    },
    TimestampNanosecond {
        value: Option<i64>,
        timezone: Option<String>,
    },
    Decimal128 {
        value: Option<i128>,
        precision: u8,
        scale: i8,
    },
    Embedding {
        size: i32,
        values: Option<Vec<Option<f32>>>,
    },
    Fallback(String),
}

impl From<&ScalarValue> for StoredScalarValue {
    fn from(value: &ScalarValue) -> Self {
        match value {
            ScalarValue::Null => StoredScalarValue::Null,
            ScalarValue::Boolean(v) => StoredScalarValue::Boolean(*v),
            ScalarValue::Float32(v) => StoredScalarValue::Float32(*v),
            ScalarValue::Float64(v) => StoredScalarValue::Float64(*v),
            ScalarValue::Int8(v) => StoredScalarValue::Int8(*v),
            ScalarValue::Int16(v) => StoredScalarValue::Int16(*v),
            ScalarValue::Int32(v) => StoredScalarValue::Int32(*v),
            ScalarValue::Int64(v) => StoredScalarValue::Int64(v.map(|i| i.to_string())),
            ScalarValue::UInt8(v) => StoredScalarValue::UInt8(*v),
            ScalarValue::UInt16(v) => StoredScalarValue::UInt16(*v),
            ScalarValue::UInt32(v) => StoredScalarValue::UInt32(*v),
            ScalarValue::UInt64(v) => StoredScalarValue::UInt64(v.map(|i| i.to_string())),
            ScalarValue::Utf8(v) => StoredScalarValue::Utf8(v.clone()),
            ScalarValue::LargeUtf8(v) => StoredScalarValue::LargeUtf8(v.clone()),
            ScalarValue::Binary(v) => StoredScalarValue::Binary(v.clone()),
            ScalarValue::LargeBinary(v) => StoredScalarValue::LargeBinary(v.clone()),
            ScalarValue::FixedSizeBinary(size, v) => StoredScalarValue::FixedSizeBinary {
                size: *size,
                value: v.clone(),
            },
            ScalarValue::Date32(v) => StoredScalarValue::Date32(*v),
            ScalarValue::Time64Microsecond(v) => StoredScalarValue::Time64Microsecond(*v),
            ScalarValue::TimestampMillisecond(v, tz) => StoredScalarValue::TimestampMillisecond {
                value: *v,
                timezone: tz.as_ref().map(|t| t.to_string()),
            },
            ScalarValue::TimestampMicrosecond(v, tz) => StoredScalarValue::TimestampMicrosecond {
                value: *v,
                timezone: tz.as_ref().map(|t| t.to_string()),
            },
            ScalarValue::TimestampNanosecond(v, tz) => StoredScalarValue::TimestampNanosecond {
                value: *v,
                timezone: tz.as_ref().map(|t| t.to_string()),
            },
            ScalarValue::Decimal128(value, precision, scale) => StoredScalarValue::Decimal128 {
                value: *value,
                precision: *precision,
                scale: *scale,
            },
            ScalarValue::FixedSizeList(array) => match encode_embedding_from_list(array) {
                Some(stored) => stored,
                None => StoredScalarValue::Fallback(value.to_string()),
            },
            _ => StoredScalarValue::Fallback(value.to_string()),
        }
    }
}

impl From<StoredScalarValue> for ScalarValue {
    fn from(value: StoredScalarValue) -> Self {
        match value {
            StoredScalarValue::Null => ScalarValue::Null,
            StoredScalarValue::Boolean(v) => ScalarValue::Boolean(v),
            StoredScalarValue::Float32(v) => ScalarValue::Float32(v),
            StoredScalarValue::Float64(v) => ScalarValue::Float64(v),
            StoredScalarValue::Int8(v) => ScalarValue::Int8(v),
            StoredScalarValue::Int16(v) => ScalarValue::Int16(v),
            StoredScalarValue::Int32(v) => ScalarValue::Int32(v),
            StoredScalarValue::Int64(v) => {
                ScalarValue::Int64(v.and_then(|s| s.parse::<i64>().ok()))
            }
            StoredScalarValue::UInt8(v) => ScalarValue::UInt8(v),
            StoredScalarValue::UInt16(v) => ScalarValue::UInt16(v),
            StoredScalarValue::UInt32(v) => ScalarValue::UInt32(v),
            StoredScalarValue::UInt64(v) => {
                ScalarValue::UInt64(v.and_then(|s| s.parse::<u64>().ok()))
            }
            StoredScalarValue::Utf8(v) => ScalarValue::Utf8(v),
            StoredScalarValue::LargeUtf8(v) => ScalarValue::LargeUtf8(v),
            StoredScalarValue::Binary(v) => ScalarValue::Binary(v),
            StoredScalarValue::LargeBinary(v) => ScalarValue::LargeBinary(v),
            StoredScalarValue::FixedSizeBinary { size, value } => {
                ScalarValue::FixedSizeBinary(size, value)
            }
            StoredScalarValue::Date32(v) => ScalarValue::Date32(v),
            StoredScalarValue::Time64Microsecond(v) => ScalarValue::Time64Microsecond(v),
            StoredScalarValue::TimestampMillisecond { value, timezone } => {
                ScalarValue::TimestampMillisecond(value, timezone.map(Arc::<str>::from))
            }
            StoredScalarValue::TimestampMicrosecond { value, timezone } => {
                ScalarValue::TimestampMicrosecond(value, timezone.map(Arc::<str>::from))
            }
            StoredScalarValue::TimestampNanosecond { value, timezone } => {
                ScalarValue::TimestampNanosecond(value, timezone.map(Arc::<str>::from))
            }
            StoredScalarValue::Decimal128 {
                value,
                precision,
                scale,
            } => ScalarValue::Decimal128(value, precision, scale),
            StoredScalarValue::Embedding { size, values } => decode_embedding(size, &values),
            StoredScalarValue::Fallback(s) => ScalarValue::Utf8(Some(s)),
        }
    }
}

fn encode_embedding_from_list(array: &Arc<FixedSizeListArray>) -> Option<StoredScalarValue> {
    if array.len() != 1 {
        return None;
    }

    let size = array.value_length();
    if size <= 0 {
        return None;
    }

    if array.is_null(0) {
        return Some(StoredScalarValue::Embedding { size, values: None });
    }

    let values = array.value(0);
    let floats = values.as_any().downcast_ref::<Float32Array>()?;
    if floats.len() != size as usize {
        return None;
    }

    let mut collected = Vec::with_capacity(size as usize);
    for i in 0..size as usize {
        if floats.is_null(i) {
            collected.push(None);
        } else {
            collected.push(Some(floats.value(i)));
        }
    }

    Some(StoredScalarValue::Embedding {
        size,
        values: Some(collected),
    })
}

fn decode_embedding(size: i32, values: &Option<Vec<Option<f32>>>) -> ScalarValue {
    let coerced_size = size.max(0);
    let field = Arc::new(Field::new("item", DataType::Float32, true));
    let array = match values {
        None => FixedSizeListArray::new_null(field.clone(), coerced_size, 1),
        Some(vals) => {
            let mut padded = vals.clone();
            match padded.len().cmp(&(coerced_size as usize)) {
                Ordering::Less => {
                    padded.resize(coerced_size as usize, None);
                }
                Ordering::Greater => {
                    padded.truncate(coerced_size as usize);
                }
                Ordering::Equal => {}
            }

            let values_array: ArrayRef = Arc::new(Float32Array::from(padded));
            FixedSizeListArray::new(field.clone(), coerced_size, values_array, None)
        }
    };

    ScalarValue::FixedSizeList(Arc::new(array))
}

pub type RowConversionResult<T> = Result<T, RowConversionError>;

impl Row {
    pub fn new(values: BTreeMap<String, ScalarValue>) -> Self {
        Self { values }
    }

    /// Helper to retrieve a value by column name
    pub fn get(&self, key: &str) -> Option<&ScalarValue> {
        self.values.get(key)
    }

    /// Materialize the row into a column-ordered envelope. Missing columns become NULL.
    pub fn to_envelope<'a, I>(&self, column_order: I) -> RowEnvelope
    where
        I: IntoIterator<Item = &'a str>,
    {
        let columns = column_order
            .into_iter()
            .map(|name| self.get(name).cloned().unwrap_or(ScalarValue::Null))
            .collect();
        RowEnvelope::new(columns)
    }

    /// Construct a row from an envelope and matching column names.
    pub fn from_envelope<'a, I>(column_order: I, envelope: RowEnvelope) -> RowConversionResult<Self>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let names: Vec<&str> = column_order.into_iter().collect();
        if names.len() != envelope.columns.len() {
            return Err(RowConversionError::ColumnCountMismatch {
                expected: names.len(),
                actual: envelope.columns.len(),
            });
        }

        let mut values = BTreeMap::new();
        for (name, value) in names.into_iter().zip(envelope.columns.into_iter()) {
            values.insert(name.to_string(), value);
        }
        Ok(Row { values })
    }
}

// --- Serialization (ScalarValue -> JSON) ---

impl Serialize for Row {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.values.len()))?;
        for (k, v) in &self.values {
            let stored = StoredScalarValue::from(v);
            map.serialize_entry(k, &stored)?;
        }
        map.end()
    }
}

// --- Deserialization (JSON -> ScalarValue) ---

impl<'de> Deserialize<'de> for Row {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = BTreeMap::<String, StoredScalarValue>::deserialize(deserializer)?;
        let mut values = BTreeMap::new();
        for (key, value) in raw {
            values.insert(key, ScalarValue::from(value));
        }
        Ok(Row { values })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bincode::config::standard;
    use bincode::serde::{decode_from_slice, encode_to_vec};
    use std::sync::Arc;

    #[test]
    fn binary_roundtrip_preserves_scalar_types() {
        let mut values = BTreeMap::new();
        values.insert("id".to_string(), ScalarValue::Int64(Some(42)));
        values.insert("name".to_string(), ScalarValue::Utf8(Some("Alice".into())));
        values.insert("active".to_string(), ScalarValue::Boolean(Some(true)));
        let row = Row::new(values);

        let config = standard();
        let bytes = encode_to_vec(&row, config).expect("bincode encode");
        let (decoded, _): (Row, usize) = decode_from_slice(&bytes, config).expect("bincode decode");
        assert_eq!(decoded, row);
    }

    #[test]
    fn extended_roundtrip_covers_binary_decimal_and_embedding() {
        let mut values = BTreeMap::new();
        values.insert(
            "bin".to_string(),
            ScalarValue::Binary(Some(vec![1_u8, 2, 3, 4])),
        );
        values.insert(
            "fixed_bin".to_string(),
            ScalarValue::FixedSizeBinary(4, Some(vec![9_u8, 8, 7, 6])),
        );
        values.insert(
            "decimal".to_string(),
            ScalarValue::Decimal128(Some(123_456_i128), 10, 2),
        );
        values.insert(
            "time".to_string(),
            ScalarValue::Time64Microsecond(Some(987_654_i64)),
        );

        let field = Arc::new(Field::new("item", DataType::Float32, true));
        let floats = Float32Array::from(vec![Some(0.1_f32), None, Some(0.3_f32)]);
        let embedding = FixedSizeListArray::new(field.clone(), 3, Arc::new(floats), None);
        values.insert(
            "embedding".to_string(),
            ScalarValue::FixedSizeList(Arc::new(embedding)),
        );

        let null_embedding = FixedSizeListArray::new_null(field, 3, 1);
        values.insert(
            "embedding_null".to_string(),
            ScalarValue::FixedSizeList(Arc::new(null_embedding)),
        );

        let row = Row::new(values);
        let config = standard();
        let bytes = encode_to_vec(&row, config).expect("bincode encode");
        let (decoded, _): (Row, usize) = decode_from_slice(&bytes, config).expect("bincode decode");
        assert_eq!(decoded, row);
    }
}
