//! Schema definition for system.namespaces table
//!
//! Provides Arrow schema for the namespaces table with 5 fields.

use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use std::sync::{Arc, OnceLock};

/// Cached schema for system.namespaces table
static NAMESPACES_SCHEMA: OnceLock<SchemaRef> = OnceLock::new();

/// Schema provider for system.namespaces table
pub struct NamespacesTableSchema;

impl NamespacesTableSchema {
    /// Get the Arrow schema for system.namespaces table
    ///
    /// Schema fields:
    /// - namespace_id: Utf8 (primary key)
    /// - name: Utf8
    /// - created_at: Timestamp(Millisecond, None)
    /// - options: Utf8 (nullable, JSON configuration)
    /// - table_count: Int32
    pub fn schema() -> SchemaRef {
        NAMESPACES_SCHEMA
            .get_or_init(|| {
                Arc::new(Schema::new(vec![
                    Field::new("namespace_id", DataType::Utf8, false),
                    Field::new("name", DataType::Utf8, false),
                    Field::new(
                        "created_at",
                        DataType::Timestamp(
                            datafusion::arrow::datatypes::TimeUnit::Millisecond,
                            None,
                        ),
                        false,
                    ),
                    Field::new("options", DataType::Utf8, true),
                    Field::new("table_count", DataType::Int32, false),
                ]))
            })
            .clone()
    }

    /// Get the table name
    pub fn table_name() -> &'static str {
        kalamdb_commons::SystemTable::Namespaces.table_name()
    }

    /// Get the column family name in RocksDB
    pub fn column_family_name() -> &'static str {
        kalamdb_commons::SystemTable::Namespaces.column_family_name()
    }

    /// Get the partition key for storage
    pub fn partition() -> &'static str {
        kalamdb_commons::SystemTable::Namespaces.column_family_name()
    }
}
