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

#[derive(Debug, Clone, PartialEq)]
enum StoredScalarValue {
    Null,
    Boolean(Option<bool>),
    Float32(Option<f32>),
    Float64(Option<f64>),
    Int8(Option<i8>),
    Int16(Option<i16>),
    Int32(Option<i32>),
    Int64(Option<i64>),
    UInt8(Option<u8>),
    UInt16(Option<u16>),
    UInt32(Option<u32>),
    UInt64(Option<u64>),
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

/// Custom serialization for StoredScalarValue that converts Int64/UInt64 to strings
/// to avoid JavaScript precision loss for values > Number.MAX_SAFE_INTEGER (2^53 - 1)
impl Serialize for StoredScalarValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap;
        
        match self {
            // Serialize Int64 as string to preserve precision in JavaScript
            StoredScalarValue::Int64(v) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("Int64", &v.map(|n| n.to_string()))?;
                map.end()
            }
            // Serialize UInt64 as string to preserve precision in JavaScript
            StoredScalarValue::UInt64(v) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("UInt64", &v.map(|n| n.to_string()))?;
                map.end()
            }
            // Serialize TimestampMillisecond value as string
            StoredScalarValue::TimestampMillisecond { value, timezone } => {
                #[derive(Serialize)]
                struct TsMs {
                    value: Option<String>,
                    timezone: Option<String>,
                }
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("TimestampMillisecond", &TsMs {
                    value: value.map(|v| v.to_string()),
                    timezone: timezone.clone(),
                })?;
                map.end()
            }
            // Serialize TimestampMicrosecond value as string  
            StoredScalarValue::TimestampMicrosecond { value, timezone } => {
                #[derive(Serialize)]
                struct TsUs {
                    value: Option<String>,
                    timezone: Option<String>,
                }
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("TimestampMicrosecond", &TsUs {
                    value: value.map(|v| v.to_string()),
                    timezone: timezone.clone(),
                })?;
                map.end()
            }
            // Serialize TimestampNanosecond value as string
            StoredScalarValue::TimestampNanosecond { value, timezone } => {
                #[derive(Serialize)]
                struct TsNs {
                    value: Option<String>,
                    timezone: Option<String>,
                }
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("TimestampNanosecond", &TsNs {
                    value: value.map(|v| v.to_string()),
                    timezone: timezone.clone(),
                })?;
                map.end()
            }
            // All other types use default derive behavior - use a helper enum
            _ => {
                // For all other types, use derive-like serialization
                #[derive(Serialize)]
                #[serde(untagged)]
                enum Helper<'a> {
                    Null,
                    Boolean { Boolean: &'a Option<bool> },
                    Float32 { Float32: &'a Option<f32> },
                    Float64 { Float64: &'a Option<f64> },
                    Int8 { Int8: &'a Option<i8> },
                    Int16 { Int16: &'a Option<i16> },
                    Int32 { Int32: &'a Option<i32> },
                    UInt8 { UInt8: &'a Option<u8> },
                    UInt16 { UInt16: &'a Option<u16> },
                    UInt32 { UInt32: &'a Option<u32> },
                    Utf8 { Utf8: &'a Option<String> },
                    LargeUtf8 { LargeUtf8: &'a Option<String> },
                    Binary { Binary: &'a Option<Vec<u8>> },
                    LargeBinary { LargeBinary: &'a Option<Vec<u8>> },
                    FixedSizeBinary { FixedSizeBinary: FixedSizeBinaryHelper<'a> },
                    Date32 { Date32: &'a Option<i32> },
                    Time64Microsecond { Time64Microsecond: &'a Option<i64> },
                    Decimal128 { Decimal128: Decimal128Helper<'a> },
                    Embedding { Embedding: EmbeddingHelper<'a> },
                    Fallback { Fallback: &'a String },
                }
                
                #[derive(Serialize)]
                struct FixedSizeBinaryHelper<'a> {
                    size: &'a i32,
                    value: &'a Option<Vec<u8>>,
                }
                
                #[derive(Serialize)]
                struct Decimal128Helper<'a> {
                    value: &'a Option<i128>,
                    precision: &'a u8,
                    scale: &'a i8,
                }
                
                #[derive(Serialize)]
                struct EmbeddingHelper<'a> {
                    size: &'a i32,
                    values: &'a Option<Vec<Option<f32>>>,
                }
                
                let helper = match self {
                    StoredScalarValue::Null => Helper::Null,
                    StoredScalarValue::Boolean(v) => Helper::Boolean { Boolean: v },
                    StoredScalarValue::Float32(v) => Helper::Float32 { Float32: v },
                    StoredScalarValue::Float64(v) => Helper::Float64 { Float64: v },
                    StoredScalarValue::Int8(v) => Helper::Int8 { Int8: v },
                    StoredScalarValue::Int16(v) => Helper::Int16 { Int16: v },
                    StoredScalarValue::Int32(v) => Helper::Int32 { Int32: v },
                    StoredScalarValue::UInt8(v) => Helper::UInt8 { UInt8: v },
                    StoredScalarValue::UInt16(v) => Helper::UInt16 { UInt16: v },
                    StoredScalarValue::UInt32(v) => Helper::UInt32 { UInt32: v },
                    StoredScalarValue::Utf8(v) => Helper::Utf8 { Utf8: v },
                    StoredScalarValue::LargeUtf8(v) => Helper::LargeUtf8 { LargeUtf8: v },
                    StoredScalarValue::Binary(v) => Helper::Binary { Binary: v },
                    StoredScalarValue::LargeBinary(v) => Helper::LargeBinary { LargeBinary: v },
                    StoredScalarValue::FixedSizeBinary { size, value } => Helper::FixedSizeBinary {
                        FixedSizeBinary: FixedSizeBinaryHelper { size, value },
                    },
                    StoredScalarValue::Date32(v) => Helper::Date32 { Date32: v },
                    StoredScalarValue::Time64Microsecond(v) => Helper::Time64Microsecond { Time64Microsecond: v },
                    StoredScalarValue::Decimal128 { value, precision, scale } => Helper::Decimal128 {
                        Decimal128: Decimal128Helper { value, precision, scale },
                    },
                    StoredScalarValue::Embedding { size, values } => Helper::Embedding {
                        Embedding: EmbeddingHelper { size, values },
                    },
                    StoredScalarValue::Fallback(s) => Helper::Fallback { Fallback: s },
                    // These are handled above
                    StoredScalarValue::Int64(_) | StoredScalarValue::UInt64(_) |
                    StoredScalarValue::TimestampMillisecond { .. } |
                    StoredScalarValue::TimestampMicrosecond { .. } |
                    StoredScalarValue::TimestampNanosecond { .. } => unreachable!(),
                };
                helper.serialize(serializer)
            }
        }
    }
}

/// Custom deserialization for StoredScalarValue that handles Int64/UInt64 as strings
impl<'de> Deserialize<'de> for StoredScalarValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::{MapAccess, Visitor};
        
        struct StoredScalarValueVisitor;
        
        impl<'de> Visitor<'de> for StoredScalarValueVisitor {
            type Value = StoredScalarValue;
            
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a StoredScalarValue")
            }
            
            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(StoredScalarValue::Null)
            }
            
            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                use serde::de::Error;
                
                let key: String = map.next_key()?.ok_or_else(|| Error::custom("empty map"))?;
                
                match key.as_str() {
                    "Null" => Ok(StoredScalarValue::Null),
                    "Boolean" => Ok(StoredScalarValue::Boolean(map.next_value()?)),
                    "Float32" => Ok(StoredScalarValue::Float32(map.next_value()?)),
                    "Float64" => Ok(StoredScalarValue::Float64(map.next_value()?)),
                    "Int8" => Ok(StoredScalarValue::Int8(map.next_value()?)),
                    "Int16" => Ok(StoredScalarValue::Int16(map.next_value()?)),
                    "Int32" => Ok(StoredScalarValue::Int32(map.next_value()?)),
                    "Int64" => {
                        // Accept either number or string
                        #[derive(Deserialize)]
                        #[serde(untagged)]
                        enum Int64Value {
                            Num(Option<i64>),
                            Str(Option<String>),
                        }
                        let v: Int64Value = map.next_value()?;
                        let parsed = match v {
                            Int64Value::Num(n) => n,
                            Int64Value::Str(s) => s.and_then(|s| s.parse().ok()),
                        };
                        Ok(StoredScalarValue::Int64(parsed))
                    }
                    "UInt8" => Ok(StoredScalarValue::UInt8(map.next_value()?)),
                    "UInt16" => Ok(StoredScalarValue::UInt16(map.next_value()?)),
                    "UInt32" => Ok(StoredScalarValue::UInt32(map.next_value()?)),
                    "UInt64" => {
                        // Accept either number or string
                        #[derive(Deserialize)]
                        #[serde(untagged)]
                        enum UInt64Value {
                            Num(Option<u64>),
                            Str(Option<String>),
                        }
                        let v: UInt64Value = map.next_value()?;
                        let parsed = match v {
                            UInt64Value::Num(n) => n,
                            UInt64Value::Str(s) => s.and_then(|s| s.parse().ok()),
                        };
                        Ok(StoredScalarValue::UInt64(parsed))
                    }
                    "Utf8" => Ok(StoredScalarValue::Utf8(map.next_value()?)),
                    "LargeUtf8" => Ok(StoredScalarValue::LargeUtf8(map.next_value()?)),
                    "Binary" => Ok(StoredScalarValue::Binary(map.next_value()?)),
                    "LargeBinary" => Ok(StoredScalarValue::LargeBinary(map.next_value()?)),
                    "FixedSizeBinary" => {
                        #[derive(Deserialize)]
                        struct Inner { size: i32, value: Option<Vec<u8>> }
                        let inner: Inner = map.next_value()?;
                        Ok(StoredScalarValue::FixedSizeBinary { size: inner.size, value: inner.value })
                    }
                    "Date32" => Ok(StoredScalarValue::Date32(map.next_value()?)),
                    "Time64Microsecond" => Ok(StoredScalarValue::Time64Microsecond(map.next_value()?)),
                    "TimestampMillisecond" => {
                        #[derive(Deserialize)]
                        #[serde(untagged)]
                        enum TsValue {
                            Str { value: Option<String>, timezone: Option<String> },
                            Num { value: Option<i64>, timezone: Option<String> },
                        }
                        let inner: TsValue = map.next_value()?;
                        let (value, timezone) = match inner {
                            TsValue::Str { value, timezone } => (value.and_then(|s| s.parse().ok()), timezone),
                            TsValue::Num { value, timezone } => (value, timezone),
                        };
                        Ok(StoredScalarValue::TimestampMillisecond { value, timezone })
                    }
                    "TimestampMicrosecond" => {
                        #[derive(Deserialize)]
                        #[serde(untagged)]
                        enum TsValue {
                            Str { value: Option<String>, timezone: Option<String> },
                            Num { value: Option<i64>, timezone: Option<String> },
                        }
                        let inner: TsValue = map.next_value()?;
                        let (value, timezone) = match inner {
                            TsValue::Str { value, timezone } => (value.and_then(|s| s.parse().ok()), timezone),
                            TsValue::Num { value, timezone } => (value, timezone),
                        };
                        Ok(StoredScalarValue::TimestampMicrosecond { value, timezone })
                    }
                    "TimestampNanosecond" => {
                        #[derive(Deserialize)]
                        #[serde(untagged)]
                        enum TsValue {
                            Str { value: Option<String>, timezone: Option<String> },
                            Num { value: Option<i64>, timezone: Option<String> },
                        }
                        let inner: TsValue = map.next_value()?;
                        let (value, timezone) = match inner {
                            TsValue::Str { value, timezone } => (value.and_then(|s| s.parse().ok()), timezone),
                            TsValue::Num { value, timezone } => (value, timezone),
                        };
                        Ok(StoredScalarValue::TimestampNanosecond { value, timezone })
                    }
                    "Decimal128" => {
                        #[derive(Deserialize)]
                        struct Inner { value: Option<i128>, precision: u8, scale: i8 }
                        let inner: Inner = map.next_value()?;
                        Ok(StoredScalarValue::Decimal128 { value: inner.value, precision: inner.precision, scale: inner.scale })
                    }
                    "Embedding" => {
                        #[derive(Deserialize)]
                        struct Inner { size: i32, values: Option<Vec<Option<f32>>> }
                        let inner: Inner = map.next_value()?;
                        Ok(StoredScalarValue::Embedding { size: inner.size, values: inner.values })
                    }
                    "Fallback" => Ok(StoredScalarValue::Fallback(map.next_value()?)),
                    _ => Err(Error::unknown_variant(&key, &["Null", "Boolean", "Int64", "..."]))
                }
            }
        }
        
        deserializer.deserialize_any(StoredScalarValueVisitor)
    }
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
            ScalarValue::Int64(v) => StoredScalarValue::Int64(*v),
            ScalarValue::UInt8(v) => StoredScalarValue::UInt8(*v),
            ScalarValue::UInt16(v) => StoredScalarValue::UInt16(*v),
            ScalarValue::UInt32(v) => StoredScalarValue::UInt32(*v),
            ScalarValue::UInt64(v) => StoredScalarValue::UInt64(*v),
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
            StoredScalarValue::Int64(v) => ScalarValue::Int64(v),
            StoredScalarValue::UInt8(v) => ScalarValue::UInt8(v),
            StoredScalarValue::UInt16(v) => ScalarValue::UInt16(v),
            StoredScalarValue::UInt32(v) => ScalarValue::UInt32(v),
            StoredScalarValue::UInt64(v) => ScalarValue::UInt64(v),
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
