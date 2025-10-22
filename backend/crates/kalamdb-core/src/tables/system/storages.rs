//! System.storages table schema definition

use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use std::sync::Arc;

/// Schema for system.storages
pub struct SystemStorages;

impl SystemStorages {
    /// Arrow schema for system.storages
    pub fn schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("storage_id", DataType::Utf8, false),
            Field::new("storage_name", DataType::Utf8, false),
            Field::new("description", DataType::Utf8, true),
            Field::new("storage_type", DataType::Utf8, false),
            Field::new("base_directory", DataType::Utf8, false),
            Field::new("shared_tables_template", DataType::Utf8, false),
            Field::new("user_tables_template", DataType::Utf8, false),
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

    /// Logical table name
    pub fn table_name() -> &'static str {
        "storages"
    }

    /// Column family backing this table
    pub fn column_family_name() -> &'static str {
        "system_storages"
    }
}
