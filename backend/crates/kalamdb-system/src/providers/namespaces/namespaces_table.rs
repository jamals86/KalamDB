//! Schema definition for system.namespaces table
//!
//! Provides Arrow schema for the namespaces table with 5 fields.
//!
//! Phase 4 (Column Ordering): Uses namespaces_table_definition().to_arrow_schema()
//! to ensure consistent column ordering via ordinal_position field.

use crate::system_table_definitions::namespaces_table_definition;
use datafusion::arrow::datatypes::SchemaRef;
use std::sync::OnceLock;

/// Cached schema for system.namespaces table
static NAMESPACES_SCHEMA: OnceLock<SchemaRef> = OnceLock::new();

/// Schema provider for system.namespaces table
pub struct NamespacesTableSchema;

impl NamespacesTableSchema {
    /// Get the Arrow schema for system.namespaces table
    ///
    /// Schema fields (in ordinal_position order):
    /// - namespace_id: Utf8 (primary key)
    /// - name: Utf8
    /// - created_at: Timestamp(Millisecond, None)
    /// - options: Utf8 (nullable, JSON configuration)
    /// - table_count: Int32
    pub fn schema() -> SchemaRef {
        NAMESPACES_SCHEMA
            .get_or_init(|| {
                namespaces_table_definition()
                    .to_arrow_schema()
                    .expect("Failed to convert namespaces TableDefinition to Arrow schema")
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
