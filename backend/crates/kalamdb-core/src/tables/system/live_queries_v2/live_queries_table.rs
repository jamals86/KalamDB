//! System.live_queries table schema
//!
//! Defines the Arrow schema for the system.live_queries table.
//!
//! Phase 4 (Column Ordering): Uses live_queries_table_definition().to_arrow_schema()
//! to ensure consistent column ordering via ordinal_position field.

use crate::tables::system::system_table_definitions::live_queries_table_definition;
use datafusion::arrow::datatypes::SchemaRef;
use std::sync::OnceLock;

/// Cached schema for system.live_queries table
static LIVE_QUERIES_SCHEMA: OnceLock<SchemaRef> = OnceLock::new();

/// Schema provider for system.live_queries table
pub struct LiveQueriesTableSchema;

impl LiveQueriesTableSchema {
    /// Get the Arrow schema for system.live_queries table
    ///
    /// Schema includes (in ordinal_position order):
    /// - live_id: Utf8 (PK) - Format: {user_id}-{unique_conn_id}-{table_name}-{query_id}
    /// - connection_id: Utf8
    /// - namespace_id: Utf8
    /// - table_name: Utf8
    /// - query_id: Utf8
    /// - user_id: Utf8
    /// - query: Utf8
    /// - options: Utf8 (nullable) - JSON
    /// - created_at: TimestampMillisecond
    /// - last_update: TimestampMillisecond
    /// - changes: Int64
    /// - node: Utf8
    pub fn schema() -> SchemaRef {
        LIVE_QUERIES_SCHEMA
            .get_or_init(|| {
                live_queries_table_definition()
                    .to_arrow_schema()
                    .expect("Failed to convert live_queries TableDefinition to Arrow schema")
            })
            .clone()
    }

    /// Get the table name
    pub fn table_name() -> &'static str {
        kalamdb_commons::SystemTable::LiveQueries.table_name()
    }

    /// Get the column family name in RocksDB
    pub fn column_family_name() -> &'static str {
        kalamdb_commons::SystemTable::LiveQueries.column_family_name()
    }

    /// Get the partition key for storage
    pub fn partition() -> &'static str {
        kalamdb_commons::SystemTable::LiveQueries.column_family_name()
    }
}
