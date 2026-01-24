use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

use super::schema_field::SchemaField;

/// Individual query result within a SQL response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// Schema describing the columns in the result set
    /// Each field contains: name, data_type (KalamDataType), and index
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub schema: Vec<SchemaField>,

    /// The result rows as arrays of values (ordered by schema index)
    /// Example: [["123", "Alice", 1699000000000000], ["456", "Bob", 1699000001000000]]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<Vec<Vec<JsonValue>>>,

    /// Number of rows affected or returned
    pub row_count: usize,

    /// Optional message for non-query statements
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl QueryResult {
    /// Get column names from schema
    pub fn column_names(&self) -> Vec<String> {
        let mut names = Vec::with_capacity(self.schema.len());
        for field in &self.schema {
            names.push(field.name.clone());
        }
        names
    }

    /// Get a row as a HashMap by index (for convenience)
    pub fn row_as_map(&self, row_idx: usize) -> Option<HashMap<String, JsonValue>> {
        let row = self.rows.as_ref()?.get(row_idx)?;
        let mut map = HashMap::with_capacity(self.schema.len());
        for (i, field) in self.schema.iter().enumerate() {
            if let Some(value) = row.get(i) {
                map.insert(field.name.clone(), value.clone());
            }
        }
        Some(map)
    }

    /// Get all rows as HashMaps (for convenience)
    pub fn rows_as_maps(&self) -> Vec<HashMap<String, JsonValue>> {
        let Some(rows) = &self.rows else {
            return vec![];
        };

        let mut mapped = Vec::with_capacity(rows.len());
        for (i, _row) in rows.iter().enumerate() {
            if let Some(map) = self.row_as_map(i) {
                mapped.push(map);
            }
        }
        mapped
    }
}
