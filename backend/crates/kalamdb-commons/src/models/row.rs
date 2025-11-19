use datafusion::scalar::ScalarValue;
use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::fmt;

/// A unified Row representation that holds DataFusion ScalarValues
/// but serializes to clean, standard JSON for clients.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub values: BTreeMap<String, ScalarValue>,
}

impl Row {
    pub fn new(values: BTreeMap<String, ScalarValue>) -> Self {
        Self { values }
    }

    /// Helper to retrieve a value by column name
    pub fn get(&self, key: &str) -> Option<&ScalarValue> {
        self.values.get(key)
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
            map.serialize_entry(k, &JsonScalarValue(v))?;
        }
        map.end()
    }
}

struct JsonScalarValue<'a>(&'a ScalarValue);

impl<'a> Serialize for JsonScalarValue<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.0 {
            ScalarValue::Boolean(Some(v)) => serializer.serialize_bool(*v),
            ScalarValue::Float32(Some(v)) => serializer.serialize_f32(*v),
            ScalarValue::Float64(Some(v)) => serializer.serialize_f64(*v),
            ScalarValue::Int8(Some(v)) => serializer.serialize_i8(*v),
            ScalarValue::Int16(Some(v)) => serializer.serialize_i16(*v),
            ScalarValue::Int32(Some(v)) => serializer.serialize_i32(*v),
            ScalarValue::Int64(Some(v)) => serializer.serialize_i64(*v),
            ScalarValue::UInt8(Some(v)) => serializer.serialize_u8(*v),
            ScalarValue::UInt16(Some(v)) => serializer.serialize_u16(*v),
            ScalarValue::UInt32(Some(v)) => serializer.serialize_u32(*v),
            ScalarValue::UInt64(Some(v)) => serializer.serialize_u64(*v),
            ScalarValue::Utf8(Some(v)) | ScalarValue::LargeUtf8(Some(v)) => {
                serializer.serialize_str(v)
            }
            ScalarValue::Date32(Some(v)) => serializer.serialize_i32(*v),
            ScalarValue::TimestampMicrosecond(Some(v), _) => serializer.serialize_i64(*v),
            ScalarValue::Null => serializer.serialize_none(),
            _ => serializer.serialize_str(&format!("{}", self.0)),
        }
    }
}

// --- Deserialization (JSON -> ScalarValue) ---

impl<'de> Deserialize<'de> for Row {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RowVisitor;

        impl<'de> Visitor<'de> for RowVisitor {
            type Value = Row;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a JSON object representing a row")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut values = BTreeMap::new();
                while let Some(key) = map.next_key::<String>()? {
                    // We deserialize into serde_json::Value first to inspect the type
                    let value: serde_json::Value = map.next_value()?;
                    values.insert(key, json_to_scalar(value));
                }
                Ok(Row { values })
            }
        }

        deserializer.deserialize_map(RowVisitor)
    }
}

/// Best-effort conversion from JSON Value to DataFusion ScalarValue
fn json_to_scalar(v: serde_json::Value) -> ScalarValue {
    match v {
        serde_json::Value::Null => ScalarValue::Null,
        serde_json::Value::Bool(b) => ScalarValue::Boolean(Some(b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                ScalarValue::Int64(Some(i))
            } else if let Some(f) = n.as_f64() {
                ScalarValue::Float64(Some(f))
            } else {
                ScalarValue::Float64(None)
            }
        }
        serde_json::Value::String(s) => ScalarValue::Utf8(Some(s)),
        // For Arrays and Objects, we fallback to string representation for now
        // or you could implement List/Struct support if needed.
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            ScalarValue::Utf8(Some(v.to_string()))
        }
    }
}
