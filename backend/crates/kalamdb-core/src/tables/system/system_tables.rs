//! System.tables table schema definition

use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use std::sync::Arc;

/// Schema for system.tables
pub struct SystemTables;

impl SystemTables {
    /// Arrow schema for system.tables
    pub fn schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("table_id", DataType::Utf8, false),
            Field::new("table_name", DataType::Utf8, false),
            Field::new("namespace", DataType::Utf8, false),
            Field::new("table_type", DataType::Utf8, false),
            Field::new(
                "created_at",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
            Field::new("storage_location", DataType::Utf8, false),
            Field::new("storage_id", DataType::Utf8, true), // T163b: FK to system.storages
            Field::new("use_user_storage", DataType::Boolean, false), // T168a: Allow user storage override
            Field::new("flush_policy", DataType::Utf8, false),
            Field::new("schema_version", DataType::Int32, false),
            Field::new("deleted_retention_hours", DataType::Int32, false),
        ]))
    }

    /// Logical table name
    pub fn table_name() -> &'static str {
        "tables"
    }

    /// Column family backing this table
    pub fn column_family_name() -> &'static str {
        "system_tables"
    }
}
