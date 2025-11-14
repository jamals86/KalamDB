//! System.manifest table schema
//!
//! Defines the Arrow schema for the system.manifest table (manifest cache).
//!
//! Uses manifest_table_definition().to_arrow_schema() to ensure consistent
//! column ordering via ordinal_position field.

use crate::system_table_definitions::manifest_table_definition;
use datafusion::arrow::datatypes::SchemaRef;
use kalamdb_commons::SystemTable;
use std::sync::OnceLock;

/// Cached schema for system.manifest table
static MANIFEST_SCHEMA: OnceLock<SchemaRef> = OnceLock::new();

/// Schema provider for system.manifest table
pub struct ManifestTableSchema;

impl ManifestTableSchema {
    /// Get the Arrow schema for system.manifest table
    ///
    /// Schema includes (in ordinal_position order):
    /// - cache_key: Utf8 (PK) - Format: namespace:table:scope
    /// - namespace_id: Utf8
    /// - table_name: Utf8
    /// - scope: Utf8
    /// - etag: Utf8 (nullable)
    /// - last_refreshed: TimestampMillisecond
    /// - last_accessed: TimestampMillisecond
    /// - ttl_seconds: Int64
    /// - source_path: Utf8
    /// - sync_state: Utf8
    pub fn schema() -> SchemaRef {
        MANIFEST_SCHEMA
            .get_or_init(|| {
                manifest_table_definition()
                    .to_arrow_schema()
                    .expect("Failed to convert manifest TableDefinition to Arrow schema")
            })
            .clone()
    }

    /// Get the table name
    pub fn table_name() -> &'static str {
        SystemTable::Manifest.table_name()
    }

    /// Get the column family name in RocksDB
    pub fn column_family_name() -> &'static str {
        SystemTable::Manifest.column_family_name()
    }
}
