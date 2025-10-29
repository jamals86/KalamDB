//! Schema definition for system.storages table
//!
//! Provides Arrow schema for the storages table with 11 fields.

use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use std::sync::{Arc, OnceLock};

/// Cached schema for system.storages table
static STORAGES_SCHEMA: OnceLock<SchemaRef> = OnceLock::new();

/// Schema provider for system.storages table
pub struct StoragesTableSchema;

impl StoragesTableSchema {
    /// Get the Arrow schema for system.storages table
    ///
    /// Schema fields:
    /// - storage_id: Utf8 (primary key)
    /// - storage_name: Utf8
    /// - description: Utf8 (nullable)
    /// - storage_type: Utf8
    /// - base_directory: Utf8
    /// - credentials: Utf8 (nullable)
    /// - shared_tables_template: Utf8
    /// - user_tables_template: Utf8
    /// - created_at: Timestamp(Millisecond, None)
    /// - updated_at: Timestamp(Millisecond, None)
    pub fn schema() -> SchemaRef {
        STORAGES_SCHEMA
            .get_or_init(|| {
                Arc::new(Schema::new(vec![
                    Field::new("storage_id", DataType::Utf8, false),
                    Field::new("storage_name", DataType::Utf8, false),
                    Field::new("description", DataType::Utf8, true),
                    Field::new("storage_type", DataType::Utf8, false),
                    Field::new("base_directory", DataType::Utf8, false),
                    Field::new("credentials", DataType::Utf8, true),
                    Field::new("shared_tables_template", DataType::Utf8, false),
                    Field::new("user_tables_template", DataType::Utf8, false),
                    Field::new(
                        "created_at",
                        DataType::Timestamp(datafusion::arrow::datatypes::TimeUnit::Millisecond, None),
                        false,
                    ),
                    Field::new(
                        "updated_at",
                        DataType::Timestamp(datafusion::arrow::datatypes::TimeUnit::Millisecond, None),
                        false,
                    ),
                ]))
            })
            .clone()
    }
}
