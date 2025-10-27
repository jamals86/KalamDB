//! KalamDB metrics collection
//!
//! This module provides metrics collection for:
//! - Query latency (histogram by table_name and query_type)
//! - Flush job duration (histogram by table_name)
//! - WebSocket message throughput (counter per connection_id)
//! - Column family sizes (gauge per CF)

use crate::catalog::TableName;
use metrics::{counter, gauge, histogram};
use std::time::Duration;

/// Query types for metrics labeling
#[derive(Debug, Clone, Copy)]
pub enum QueryType {
    Select,
    Insert,
    Update,
    Delete,
    CreateTable,
    AlterTable,
    DescribeTable,
}

impl QueryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            QueryType::Select => "select",
            QueryType::Insert => "insert",
            QueryType::Update => "update",
            QueryType::Delete => "delete",
            QueryType::CreateTable => "create_table",
            QueryType::AlterTable => "alter_table",
            QueryType::DescribeTable => "describe_table",
        }
    }
}

/// Column family types for metrics labeling
#[derive(Debug, Clone, Copy)]
pub enum ColumnFamilyType {
    UserTable,
    SharedTable,
    SystemTable,
}

impl ColumnFamilyType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ColumnFamilyType::UserTable => "user_table",
            ColumnFamilyType::SharedTable => "shared_table",
            ColumnFamilyType::SystemTable => "system_table",
        }
    }
}


/// Record query latency
///
/// # Example
/// ```
/// use kalamdb_core::metrics::{record_query_latency, QueryType};
/// use kalamdb_core::catalog::TableName;
/// use std::time::Duration;
///
/// let table_name = TableName::new("users");
/// record_query_latency(&table_name, QueryType::Select, Duration::from_millis(50));
/// ```
pub fn record_query_latency(table_name: &TableName, query_type: QueryType, duration: Duration) {
    histogram!(
        "kalamdb_query_latency_seconds",
        "table_name" => table_name.as_str().to_string(),
        "query_type" => query_type.as_str()
    )
    .record(duration.as_secs_f64());
}

/// Record flush job duration
///
/// # Example
/// ```
/// use kalamdb_core::metrics::record_flush_duration;
/// use kalamdb_core::catalog::TableName;
/// use std::time::Duration;
///
/// let table_name = TableName::new("users");
/// record_flush_duration(&table_name, Duration::from_millis(500));
/// ```
pub fn record_flush_duration(table_name: &TableName, duration: Duration) {
    histogram!(
        "kalamdb_flush_duration_seconds",
        "table_name" => table_name.as_str().to_string()
    )
    .record(duration.as_secs_f64());
}

/// Increment WebSocket message count
///
/// # Example
/// ```
/// use kalamdb_core::metrics::increment_websocket_messages;
///
/// increment_websocket_messages("conn_123");
/// ```
pub fn increment_websocket_messages(connection_id: &str) {
    counter!(
        "kalamdb_websocket_messages_total",
        "connection_id" => connection_id.to_string()
    )
    .increment(1);
}

/// Update column family size
///
/// # Example
/// ```
/// use kalamdb_core::metrics::{update_column_family_size, ColumnFamilyType};
///
/// update_column_family_size(ColumnFamilyType::UserTable, "users", 1024 * 1024 * 10); // 10 MB
/// ```
pub fn update_column_family_size(cf_type: ColumnFamilyType, name: &str, size_bytes: u64) {
    gauge!(
        "kalamdb_column_family_size_bytes",
        "cf_type" => cf_type.as_str(),
        "name" => name.to_string()
    )
    .set(size_bytes as f64);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_type_as_str() {
        assert_eq!(QueryType::Select.as_str(), "select");
        assert_eq!(QueryType::Insert.as_str(), "insert");
        assert_eq!(QueryType::Update.as_str(), "update");
        assert_eq!(QueryType::Delete.as_str(), "delete");
        assert_eq!(QueryType::CreateTable.as_str(), "create_table");
        assert_eq!(QueryType::AlterTable.as_str(), "alter_table");
        assert_eq!(QueryType::DescribeTable.as_str(), "describe_table");
    }

    #[test]
    fn test_column_family_type_as_str() {
        assert_eq!(ColumnFamilyType::UserTable.as_str(), "user_table");
        assert_eq!(ColumnFamilyType::SharedTable.as_str(), "shared_table");
        assert_eq!(ColumnFamilyType::StreamTable.as_str(), "stream_table");
        assert_eq!(ColumnFamilyType::SystemTables.as_str(), "system_tables");
        assert_eq!(
            ColumnFamilyType::SystemNamespaces.as_str(),
            "system_namespaces"
        );
        assert_eq!(
            ColumnFamilyType::SystemLiveQueries.as_str(),
            "system_live_queries"
        );
        assert_eq!(
            ColumnFamilyType::SystemFlushJobs.as_str(),
            "system_flush_jobs"
        );
    }

    #[test]
    fn test_record_query_latency() {
        // This test just verifies the function doesn't panic
        let table_name = TableName::new("test_table");
        record_query_latency(&table_name, QueryType::Select, Duration::from_millis(10));
        record_query_latency(&table_name, QueryType::Insert, Duration::from_millis(20));
    }

    #[test]
    fn test_record_flush_duration() {
        let table_name = TableName::new("test_table");
        record_flush_duration(&table_name, Duration::from_millis(100));
    }

    #[test]
    fn test_increment_websocket_messages() {
        increment_websocket_messages("test_connection");
        increment_websocket_messages("test_connection");
    }

    #[test]
    fn test_update_column_family_size() {
        update_column_family_size(ColumnFamilyType::UserTable, "test_table", 1024);
        update_column_family_size(ColumnFamilyType::SharedTable, "shared_table", 2048);
    }

    #[test]
    fn test_multiple_metrics_for_same_table() {
        let table_name = TableName::new("multi_test");

        // Record multiple query types
        record_query_latency(&table_name, QueryType::Select, Duration::from_millis(5));
        record_query_latency(&table_name, QueryType::Insert, Duration::from_millis(10));
        record_query_latency(&table_name, QueryType::Update, Duration::from_millis(15));

        // Record flush
        record_flush_duration(&table_name, Duration::from_millis(200));
    }

    #[test]
    fn test_metrics_with_different_connections() {
        increment_websocket_messages("conn_1");
        increment_websocket_messages("conn_2");
        increment_websocket_messages("conn_1");

        // Should track per connection
    }

    #[test]
    fn test_duration_conversion() {
        let table_name = TableName::new("duration_test");

        // Test microseconds
        record_query_latency(&table_name, QueryType::Select, Duration::from_micros(500));

        // Test milliseconds
        record_query_latency(&table_name, QueryType::Insert, Duration::from_millis(50));

        // Test seconds
        record_query_latency(&table_name, QueryType::Update, Duration::from_secs(1));
    }
}
