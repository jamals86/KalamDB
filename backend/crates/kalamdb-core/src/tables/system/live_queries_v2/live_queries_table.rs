//! System.live_queries table schema
//!
//! Defines the Arrow schema for the system.live_queries table.

use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use std::sync::{Arc, OnceLock};

/// Cached schema for system.live_queries table
static LIVE_QUERIES_SCHEMA: OnceLock<SchemaRef> = OnceLock::new();

/// Schema provider for system.live_queries table
pub struct LiveQueriesTableSchema;

impl LiveQueriesTableSchema {
    /// Get the Arrow schema for system.live_queries table
    ///
    /// Schema includes:
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
                Arc::new(Schema::new(vec![
                    Field::new("live_id", DataType::Utf8, false),
                    Field::new("connection_id", DataType::Utf8, false),
                    Field::new("namespace_id", DataType::Utf8, false),
                    Field::new("table_name", DataType::Utf8, false),
                    Field::new("query_id", DataType::Utf8, false),
                    Field::new("user_id", DataType::Utf8, false),
                    Field::new("query", DataType::Utf8, false),
                    Field::new("options", DataType::Utf8, true),
                    Field::new(
                        "created_at",
                        DataType::Timestamp(
                            datafusion::arrow::datatypes::TimeUnit::Millisecond,
                            None,
                        ),
                        false,
                    ),
                    Field::new(
                        "last_update",
                        DataType::Timestamp(
                            datafusion::arrow::datatypes::TimeUnit::Millisecond,
                            None,
                        ),
                        false,
                    ),
                    Field::new("changes", DataType::Int64, false),
                    Field::new("node", DataType::Utf8, false),
                ]))
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
