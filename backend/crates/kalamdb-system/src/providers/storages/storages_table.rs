//! Schema definition for system.storages table
//!
//! Provides Arrow schema for the storages table with 11 fields.
//!
//! Phase 4 (Column Ordering): Uses storages_table_definition().to_arrow_schema()
//! to ensure consistent column ordering via ordinal_position field.

use crate::system_table_definitions::storages_table_definition;
use datafusion::arrow::datatypes::SchemaRef;
use std::sync::OnceLock;

/// Cached schema for system.storages table
static STORAGES_SCHEMA: OnceLock<SchemaRef> = OnceLock::new();

/// Schema provider for system.storages table
pub struct StoragesTableSchema;

impl StoragesTableSchema {
    /// Get the Arrow schema for system.storages table
    ///
    /// Schema fields (in ordinal_position order):
    /// - storage_id: Utf8 (primary key)
    /// - storage_name: Utf8
    /// - description: Utf8 (nullable)
    /// - storage_type: Utf8
    /// - base_directory: Utf8
    /// - credentials: Utf8 (nullable)
    /// - config_json: Utf8 (nullable)
    /// - shared_tables_template: Utf8
    /// - user_tables_template: Utf8
    /// - created_at: Timestamp(Millisecond, None)
    /// - updated_at: Timestamp(Millisecond, None)
    pub fn schema() -> SchemaRef {
        STORAGES_SCHEMA
            .get_or_init(|| {
                storages_table_definition()
                    .to_arrow_schema()
                    .expect("Failed to convert storages TableDefinition to Arrow schema")
            })
            .clone()
    }

    /// Get the table name
    pub fn table_name() -> &'static str {
        kalamdb_commons::SystemTable::Storages.table_name()
    }

    /// Get the column family name in RocksDB
    pub fn column_family_name() -> &'static str {
        kalamdb_commons::SystemTable::Storages.column_family_name()
    }

    /// Get the partition key for storage
    pub fn partition() -> &'static str {
        kalamdb_commons::SystemTable::Storages.column_family_name()
    }
}
