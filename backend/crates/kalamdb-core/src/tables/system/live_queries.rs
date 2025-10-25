//! System.live_queries table schema
//!
//! This module defines the schema for the system.live_queries table.

use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use std::sync::Arc;

/// System live_queries table schema
pub struct LiveQueriesTable;

impl LiveQueriesTable {
    /// Get the schema for the system.live_queries table
    pub fn schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("live_id", DataType::Utf8, false), // Format: {user_id}-{unique_conn_id}-{table_name}-{query_id}
            Field::new("connection_id", DataType::Utf8, false),
             Field::new("namespace_id", DataType::Utf8, false),
            Field::new("table_name", DataType::Utf8, false),
            Field::new("query_id", DataType::Utf8, false),
            Field::new("user_id", DataType::Utf8, false),
            Field::new("query", DataType::Utf8, false),
            Field::new("options", DataType::Utf8, true), // JSON
            Field::new(
                "created_at",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
            Field::new(
                "last_update",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
            Field::new("changes", DataType::Int64, false),
            Field::new("node", DataType::Utf8, false),
        ]))
    }

    /// Get the table name
    pub fn table_name() -> &'static str {
        "live_queries"
    }

    /// Get the column family name
    pub fn column_family_name() -> String {
        format!("system_{}", Self::table_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_live_queries_table_schema() {
        let schema = LiveQueriesTable::schema();
        assert_eq!(schema.fields().len(), 12);
        assert_eq!(schema.field(0).name(), "live_id");
        assert_eq!(schema.field(1).name(), "connection_id");
        assert_eq!(schema.field(2).name(), "namespace_id");
        assert_eq!(schema.field(3).name(), "table_name");
        assert_eq!(schema.field(4).name(), "query_id");
        assert_eq!(schema.field(5).name(), "user_id");
        assert_eq!(schema.field(6).name(), "query");
        assert_eq!(schema.field(7).name(), "options");
        assert_eq!(schema.field(8).name(), "created_at");
        assert_eq!(schema.field(9).name(), "last_update");
        assert_eq!(schema.field(10).name(), "changes");
        assert_eq!(schema.field(11).name(), "node");
    }

    #[test]
    fn test_live_queries_table_name() {
        assert_eq!(LiveQueriesTable::table_name(), "live_queries");
        assert_eq!(
            LiveQueriesTable::column_family_name(),
            "system_live_queries"
        );
    }
}
