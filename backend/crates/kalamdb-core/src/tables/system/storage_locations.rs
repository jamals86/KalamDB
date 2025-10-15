//! System.storage_locations table schema
//!
//! This module defines the schema for the system.storage_locations table.

use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use std::sync::Arc;

/// System storage_locations table schema
pub struct StorageLocationsTable;

impl StorageLocationsTable {
    /// Get the schema for the system.storage_locations table
    pub fn schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("location_name", DataType::Utf8, false),
            Field::new("location_type", DataType::Utf8, false), // "filesystem", "s3", etc.
            Field::new("path", DataType::Utf8, false),
            Field::new("credentials_ref", DataType::Utf8, true),
            Field::new("usage_count", DataType::Int64, false),
            Field::new(
                "created_at",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
            Field::new(
                "updated_at",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
        ]))
    }

    /// Get the table name
    pub fn table_name() -> &'static str {
        "storage_locations"
    }

    /// Get the column family name
    pub fn column_family_name() -> String {
        format!("system_table:{}", Self::table_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_locations_table_schema() {
        let schema = StorageLocationsTable::schema();
        assert_eq!(schema.fields().len(), 7);
        assert_eq!(schema.field(0).name(), "location_name");
        assert_eq!(schema.field(1).name(), "location_type");
        assert_eq!(schema.field(2).name(), "path");
        assert_eq!(schema.field(3).name(), "credentials_ref");
        assert_eq!(schema.field(4).name(), "usage_count");
        assert_eq!(schema.field(5).name(), "created_at");
        assert_eq!(schema.field(6).name(), "updated_at");
    }

    #[test]
    fn test_storage_locations_table_name() {
        assert_eq!(StorageLocationsTable::table_name(), "storage_locations");
        assert_eq!(StorageLocationsTable::column_family_name(), "system_table:storage_locations");
    }
}
