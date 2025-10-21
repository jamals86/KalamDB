//! System.namespaces table schema
//!
//! Defines the Arrow schema for the system.namespaces table exposed via DataFusion.

use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use std::sync::Arc;

/// Schema definition for system.namespaces
pub struct NamespacesTable;

impl NamespacesTable {
    /// Return the Arrow schema for system.namespaces
    pub fn schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("namespace_id", DataType::Utf8, false),
            Field::new("name", DataType::Utf8, false),
            Field::new("options", DataType::Utf8, true),
            Field::new("table_count", DataType::Int32, false),
            Field::new(
                "created_at",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
        ]))
    }

    /// Logical table name
    pub fn table_name() -> &'static str {
        "namespaces"
    }

    /// Column family backing this table in RocksDB
    pub fn column_family_name() -> &'static str {
        "system_namespaces"
    }
}
