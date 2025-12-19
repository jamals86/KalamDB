//! Schema field for API response serialization
//!
//! This module defines the `SchemaField` struct used in REST API responses
//! to provide type-safe schema information to clients.

use crate::models::datatypes::KalamDataType;
use serde::{Deserialize, Serialize};

/// A field in the result schema returned by SQL queries
///
/// Contains all the information a client needs to properly interpret
/// column data, including the name, data type, and index.
///
/// # Example (JSON representation)
///
/// ```json
/// {
///   "name": "user_id",
///   "data_type": "BigInt",
///   "index": 0
/// }
/// ```
///
/// # Example (with parameterized type)
///
/// ```json
/// {
///   "name": "vector",
///   "data_type": { "Embedding": 384 },
///   "index": 5
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaField {
    /// Column name
    pub name: String,

    /// Data type using KalamDB's unified type system
    pub data_type: KalamDataType,

    /// Column position (0-indexed) in the result set
    pub index: usize,
}

impl SchemaField {
    /// Create a new schema field
    pub fn new(name: impl Into<String>, data_type: KalamDataType, index: usize) -> Self {
        Self {
            name: name.into(),
            data_type,
            index,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_schema_field_serialization() {
        let field = SchemaField::new("user_id", KalamDataType::BigInt, 0);
        let json = serde_json::to_string(&field).unwrap();
        assert!(json.contains("\"name\":\"user_id\""));
        assert!(json.contains("\"data_type\":\"BigInt\""));
        assert!(json.contains("\"index\":0"));
    }

    #[test]
    fn test_schema_field_deserialization() {
        let json = r#"{"name":"email","data_type":"Text","index":1}"#;
        let field: SchemaField = serde_json::from_str(json).unwrap();
        assert_eq!(field.name, "email");
        assert_eq!(field.data_type, KalamDataType::Text);
        assert_eq!(field.index, 1);
    }

    #[test]
    fn test_schema_field_with_embedding() {
        let field = SchemaField::new("vector", KalamDataType::Embedding(384), 5);
        let json = serde_json::to_string(&field).unwrap();
        assert!(json.contains("\"Embedding\":384"));
    }

    #[test]
    fn test_schema_field_with_decimal() {
        let field = SchemaField::new(
            "price",
            KalamDataType::Decimal {
                precision: 10,
                scale: 2,
            },
            2,
        );
        let json = serde_json::to_string(&field).unwrap();
        assert!(json.contains("\"Decimal\""));
    }
}
