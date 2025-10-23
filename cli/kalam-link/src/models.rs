//! Data models for kalam-link client library.
//!
//! Defines request and response structures for query execution and
//! WebSocket subscription messages.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Request payload for SQL query execution.
///
/// # Examples
///
/// ```rust
/// use kalam_link::QueryRequest;
/// use serde_json::json;
///
/// // Simple query without parameters
/// let request = QueryRequest {
///     sql: "SELECT * FROM users".to_string(),
///     params: None,
/// };
///
/// // Parametrized query
/// let request = QueryRequest {
///     sql: "SELECT * FROM users WHERE id = $1".to_string(),
///     params: Some(vec![json!(42)]),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRequest {
    /// SQL query string (may contain $1, $2... placeholders)
    pub sql: String,

    /// Optional parameter values for placeholders
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Vec<JsonValue>>,
}

/// Response from SQL query execution.
///
/// Contains query results, execution metadata, and optional error information.
/// Matches the server's SqlResponse structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResponse {
    /// Query execution status ("success" or "error")
    pub status: String,

    /// Array of result sets, one per executed statement
    #[serde(default)]
    pub results: Vec<QueryResult>,

    /// Query execution time in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_time_ms: Option<u64>,

    /// Error details if status is "error"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorDetail>,
}

/// Individual query result within a SQL response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// The result rows as JSON objects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<Vec<std::collections::HashMap<String, JsonValue>>>,

    /// Number of rows affected or returned
    pub row_count: usize,

    /// Column names in the result set
    pub columns: Vec<String>,

    /// Optional message for non-query statements
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Error details for failed SQL execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetail {
    /// Error code
    pub code: String,

    /// Human-readable error message
    pub message: String,

    /// Optional additional details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// Change event received via WebSocket subscription.
///
/// Represents an INSERT, UPDATE, or DELETE operation on subscribed data.
///
/// # Examples
///
/// ```rust
/// use kalam_link::ChangeEvent;
/// use serde_json::json;
///
/// // INSERT event
/// let event = ChangeEvent::Insert {
///     table: "messages".to_string(),
///     row: json!({
///         "id": 1,
///         "content": "Hello",
///         "_updated": "2025-01-15T10:30:00Z"
///     }),
/// };
///
/// // UPDATE event with old and new values
/// let event = ChangeEvent::Update {
///     table: "messages".to_string(),
///     old_row: json!({"id": 1, "content": "Hello"}),
///     new_row: json!({"id": 1, "content": "Hello World"}),
/// };
///
/// // DELETE event
/// let event = ChangeEvent::Delete {
///     table: "messages".to_string(),
///     row: json!({"id": 1, "_deleted": true}),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "UPPERCASE")]
pub enum ChangeEvent {
    /// New row inserted
    #[serde(rename = "INSERT")]
    Insert {
        /// Table name
        table: String,
        /// Inserted row data
        row: JsonValue,
    },

    /// Existing row updated
    #[serde(rename = "UPDATE")]
    Update {
        /// Table name
        table: String,
        /// Previous row state
        old_row: JsonValue,
        /// New row state
        new_row: JsonValue,
    },

    /// Row deleted (soft or hard delete)
    #[serde(rename = "DELETE")]
    Delete {
        /// Table name
        table: String,
        /// Deleted row data (with _deleted flag if soft delete)
        row: JsonValue,
    },

    /// Subscription initialization message with initial data
    #[serde(rename = "SNAPSHOT")]
    Snapshot {
        /// Table name
        table: String,
        /// Initial rows matching subscription query
        rows: Vec<JsonValue>,
    },

    /// Subscription acknowledgment
    #[serde(rename = "ACK")]
    Ack {
        /// Subscription ID assigned by server
        subscription_id: String,
        /// Subscription query
        query: String,
    },

    /// Error from server
    #[serde(rename = "ERROR")]
    Error {
        /// Error message
        message: String,
    },
}

impl ChangeEvent {
    /// Returns true if this is an error event
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error { .. })
    }

    /// Returns the table name for data change events
    pub fn table_name(&self) -> Option<&str> {
        match self {
            Self::Insert { table, .. }
            | Self::Update { table, .. }
            | Self::Delete { table, .. }
            | Self::Snapshot { table, .. } => Some(table),
            Self::Ack { .. } | Self::Error { .. } => None,
        }
    }
}

/// Health check response from the server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResponse {
    /// Health status (e.g., "healthy")
    pub status: String,

    /// Server version
    pub version: String,

    /// API version (e.g., "v1")
    pub api_version: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_query_request_serialization() {
        let request = QueryRequest {
            sql: "SELECT * FROM users WHERE id = $1".to_string(),
            params: Some(vec![json!(42)]),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("SELECT * FROM users"));
        assert!(json.contains("params"));
    }

    #[test]
    fn test_change_event_deserialization() {
        let json = r#"{"type":"INSERT","table":"users","row":{"id":1,"name":"Alice"}}"#;
        let event: ChangeEvent = serde_json::from_str(json).unwrap();

        match event {
            ChangeEvent::Insert { table, .. } => assert_eq!(table, "users"),
            _ => panic!("Expected Insert event"),
        }
    }

    #[test]
    fn test_change_event_helpers() {
        let insert = ChangeEvent::Insert {
            table: "test".to_string(),
            row: json!({}),
        };
        assert_eq!(insert.table_name(), Some("test"));
        assert!(!insert.is_error());

        let error = ChangeEvent::Error {
            message: "test error".to_string(),
        };
        assert!(error.is_error());
        assert_eq!(error.table_name(), None);
    }
}
