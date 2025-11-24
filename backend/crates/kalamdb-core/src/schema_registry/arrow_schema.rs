//! Arrow schema serialization and deserialization
//!
//! This module provides utilities for serializing and deserializing Arrow schemas
//! using Arrow IPC format.

use super::error::RegistryError;
use arrow::datatypes::{Schema, SchemaRef};
use kalamdb_commons::models::datatypes::{FromArrowType, KalamDataType, ToArrowType};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Arrow schema with table options
///
/// Wraps an Arrow schema with additional table-specific options.
#[derive(Debug, Clone)]
pub struct ArrowSchemaWithOptions {
    /// The Arrow schema
    pub schema: SchemaRef,

    /// Table-specific options (stored alongside schema in JSON)
    pub options: HashMap<String, Value>,
}

impl ArrowSchemaWithOptions {
    /// Create a new schema with options
    pub fn new(schema: SchemaRef) -> Self {
        Self {
            schema,
            options: HashMap::new(),
        }
    }

    /// Create with explicit options
    pub fn with_options(schema: SchemaRef, options: HashMap<String, Value>) -> Self {
        Self { schema, options }
    }

    /// Serialize schema and options to JSON
    ///
    /// Uses KalamDataType for robust type serialization.
    pub fn to_json(&self) -> Result<Value, RegistryError> {
        // Serialize schema fields manually
        let fields: Result<Vec<serde_json::Map<String, Value>>, RegistryError> = self
            .schema
            .fields()
            .iter()
            .map(|field| {
                let mut field_map = serde_json::Map::new();
                field_map.insert("name".to_string(), serde_json::json!(field.name()));

                // Convert to KalamDataType for stable serialization
                let kalam_type =
                    KalamDataType::from_arrow_type(field.data_type()).map_err(|e| {
                        RegistryError::SchemaError(format!(
                            "Unsupported type for schema serialization: {}",
                            e
                        ))
                    })?;

                field_map.insert(
                    "data_type".to_string(),
                    serde_json::to_value(&kalam_type)
                        .map_err(|e| RegistryError::SchemaError(e.to_string()))?,
                );

                field_map.insert(
                    "nullable".to_string(),
                    serde_json::json!(field.is_nullable()),
                );
                Ok(field_map)
            })
            .collect();

        // Combine schema and options
        let mut combined = serde_json::Map::new();
        combined.insert("fields".to_string(), serde_json::json!(fields?));
        combined.insert(
            "options".to_string(),
            serde_json::to_value(&self.options).map_err(|e| {
                RegistryError::SchemaError(format!("Failed to serialize options: {}", e))
            })?,
        );

        Ok(Value::Object(combined))
    }

    /// Deserialize schema and options from JSON
    ///
    /// Reconstructs the Arrow schema using KalamDataType.
    pub fn from_json(json: &Value) -> Result<Self, RegistryError> {
        let obj = json.as_object().ok_or_else(|| {
            RegistryError::SchemaError("Expected object for schema JSON".to_string())
        })?;

        // Extract fields
        let fields_json = obj
            .get("fields")
            .ok_or_else(|| {
                RegistryError::SchemaError("Missing 'fields' field in JSON".to_string())
            })?
            .as_array()
            .ok_or_else(|| RegistryError::SchemaError("'fields' must be an array".to_string()))?;

        let fields: Result<Vec<arrow::datatypes::Field>, RegistryError> = fields_json
            .iter()
            .map(|field_json| {
                let field_obj = field_json.as_object().ok_or_else(|| {
                    RegistryError::SchemaError("Field must be an object".to_string())
                })?;

                let name = field_obj
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        RegistryError::SchemaError("Field missing 'name'".to_string())
                    })?;

                // Deserialize KalamDataType
                let kalam_type_val = field_obj.get("data_type").ok_or_else(|| {
                    RegistryError::SchemaError("Field missing 'data_type'".to_string())
                })?;

                let kalam_type: KalamDataType = serde_json::from_value(kalam_type_val.clone())
                    .or_else(|_| {
                        // Fallback for legacy string formats
                        if let Some(s) = kalam_type_val.as_str() {
                            match s {
                                "Int64" => Ok(KalamDataType::BigInt),
                                "Int32" => Ok(KalamDataType::Int),
                                "Utf8" | "LargeUtf8" => Ok(KalamDataType::Text),
                                "Boolean" => Ok(KalamDataType::Boolean),
                                "Float64" => Ok(KalamDataType::Double),
                                "Float32" => Ok(KalamDataType::Float),
                                // Add other legacy mappings if necessary
                                _ => Err(RegistryError::SchemaError(format!(
                                    "Unknown legacy data type: {}",
                                    s
                                ))),
                            }
                        } else {
                            Err(RegistryError::SchemaError(format!(
                                "Invalid data type format: {:?}",
                                kalam_type_val
                            )))
                        }
                    })
                    .map_err(|e| RegistryError::SchemaError(format!("Invalid data type: {}", e)))?;

                let data_type = kalam_type.to_arrow_type().map_err(|e| {
                    RegistryError::SchemaError(format!("Failed to convert to Arrow type: {}", e))
                })?;

                let nullable = field_obj
                    .get("nullable")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);

                Ok(arrow::datatypes::Field::new(name, data_type, nullable))
            })
            .collect();

        let schema = Arc::new(Schema::new(fields?));

        // Extract options (optional)
        let options = if let Some(options_json) = obj.get("options") {
            serde_json::from_value(options_json.clone()).map_err(|e| {
                RegistryError::SchemaError(format!("Failed to deserialize options: {}", e))
            })?
        } else {
            HashMap::new()
        };

        Ok(Self::with_options(schema, options))
    }

    /// Serialize to JSON string
    pub fn to_json_string(&self) -> Result<String, RegistryError> {
        let json = self.to_json()?;
        serde_json::to_string_pretty(&json).map_err(|e| {
            RegistryError::SchemaError(format!("Failed to convert to JSON string: {}", e))
        })
    }

    /// Deserialize from JSON string
    pub fn from_json_string(json_str: &str) -> Result<Self, RegistryError> {
        let json: Value = serde_json::from_str(json_str).map_err(|e| {
            RegistryError::SchemaError(format!("Failed to parse JSON string: {}", e))
        })?;
        Self::from_json(&json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::datatypes::{DataType, Field, Schema};

    #[test]
    fn test_arrow_schema_serialization() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, true),
        ]));

        let schema_with_opts = ArrowSchemaWithOptions::new(schema.clone());

        // Serialize
        let json_str = schema_with_opts.to_json_string().unwrap();
        assert!(json_str.contains("\"id\""));
        assert!(json_str.contains("\"name\""));

        // Deserialize
        let loaded = ArrowSchemaWithOptions::from_json_string(&json_str).unwrap();
        assert_eq!(loaded.schema.fields().len(), 2);
        assert_eq!(loaded.schema.field(0).name(), "id");
        assert_eq!(loaded.schema.field(1).name(), "name");
    }

    #[test]
    fn test_schema_with_options() {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Float64,
            false,
        )]));

        let mut options = HashMap::new();
        options.insert(
            "flush_policy".to_string(),
            serde_json::json!({"row_limit": 1000}),
        );

        let schema_with_opts = ArrowSchemaWithOptions::with_options(schema.clone(), options);

        // Serialize and deserialize
        let json_str = schema_with_opts.to_json_string().unwrap();
        let loaded = ArrowSchemaWithOptions::from_json_string(&json_str).unwrap();

        assert_eq!(loaded.options.len(), 1);
        assert!(loaded.options.contains_key("flush_policy"));
    }
}
