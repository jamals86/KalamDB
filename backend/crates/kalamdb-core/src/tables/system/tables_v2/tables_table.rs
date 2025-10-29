//! System.tables table schema (system_tables in RocksDB)
//!
//! This module defines the Arrow schema for the system.tables table.
//! Uses OnceLock for zero-overhead static schema caching.

use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use std::sync::{Arc, OnceLock};

/// Static schema cache for the tables table
static TABLES_SCHEMA: OnceLock<Arc<Schema>> = OnceLock::new();

/// System tables table schema definition
pub struct TablesTableSchema;

impl TablesTableSchema {
    /// Get the cached schema for the system.tables table
    ///
    /// Uses OnceLock to ensure the schema is created exactly once and reused
    /// across all providers without synchronization overhead.
    pub fn schema() -> SchemaRef {
        TABLES_SCHEMA
            .get_or_init(|| {
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
                    Field::new("storage_id", DataType::Utf8, true),
                    Field::new("use_user_storage", DataType::Boolean, false),
                    Field::new("flush_policy", DataType::Utf8, false),
                    Field::new("schema_version", DataType::Int32, false),
                    Field::new("deleted_retention_hours", DataType::Int32, false),
                    Field::new("access_level", DataType::Utf8, true),
                ]))
            })
            .clone()
    }

    /// Get the table name
    pub fn table_name() -> &'static str {
        "tables"
    }

    /// Get the column family name in RocksDB
    pub fn column_family_name() -> &'static str {
        "system_tables"
    }

    /// Get the partition key for storage
    pub fn partition() -> &'static str {
        "system_tables"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tables_table_schema() {
        let schema = TablesTableSchema::schema();
        assert_eq!(schema.fields().len(), 12);
        assert_eq!(schema.field(0).name(), "table_id");
        assert_eq!(schema.field(1).name(), "table_name");
        assert_eq!(schema.field(2).name(), "namespace");
        assert_eq!(schema.field(3).name(), "table_type");
        assert_eq!(schema.field(4).name(), "created_at");
        assert_eq!(schema.field(5).name(), "storage_location");
        assert_eq!(schema.field(6).name(), "storage_id");
        assert_eq!(schema.field(7).name(), "use_user_storage");
        assert_eq!(schema.field(8).name(), "flush_policy");
        assert_eq!(schema.field(9).name(), "schema_version");
        assert_eq!(schema.field(10).name(), "deleted_retention_hours");
        assert_eq!(schema.field(11).name(), "access_level");
    }

    #[test]
    fn test_tables_table_name() {
        assert_eq!(TablesTableSchema::table_name(), "tables");
        assert_eq!(TablesTableSchema::column_family_name(), "system_tables");
    }

    #[test]
    fn test_schema_caching() {
        let schema1 = TablesTableSchema::schema();
        let schema2 = TablesTableSchema::schema();
        assert!(Arc::ptr_eq(&schema1, &schema2), "Schema should be cached");
    }
}
