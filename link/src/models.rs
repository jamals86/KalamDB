//! Data models for kalam-link client library.
//!
//! Defines request and response structures for query execution and
//! WebSocket subscription messages.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// WebSocket message types sent from server to client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Acknowledgement of successful subscription registration
    SubscriptionAck {
        /// The subscription ID that was registered
        subscription_id: String,
        /// Number of rows in the initial data snapshot
        last_rows: u32,
    },

    /// Initial data snapshot sent after subscription
    InitialData {
        /// The subscription ID this data is for
        subscription_id: String,
        /// The initial rows matching the query
        rows: Vec<HashMap<String, JsonValue>>,
        /// Number of rows in the snapshot
        count: usize,
    },

    /// Change notification for INSERT/UPDATE/DELETE operations
    Change {
        /// The subscription ID this notification is for
        subscription_id: String,

        /// Type of change: "insert", "update", or "delete"
        change_type: ChangeTypeRaw,

        /// New/current row values (for INSERT and UPDATE)
        #[serde(skip_serializing_if = "Option::is_none")]
        rows: Option<Vec<HashMap<String, JsonValue>>>,

        /// Previous row values (for UPDATE and DELETE)
        #[serde(skip_serializing_if = "Option::is_none")]
        old_values: Option<Vec<HashMap<String, JsonValue>>>,
    },

    /// Error notification
    Error {
        /// The subscription ID this error is for
        subscription_id: String,

        /// Error code
        code: String,

        /// Error message
        message: String,
    },
}

/// Type of change that occurred in the database
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChangeTypeRaw {
    /// New row(s) inserted
    Insert,

    /// Existing row(s) updated
    Update,

    /// Row(s) deleted
    Delete,
}

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
    pub took_ms: Option<u64>,

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
/// // Initial snapshot
/// let snapshot = ChangeEvent::InitialData {
///     subscription_id: "sub-1".to_string(),
///     rows: vec![json!({"id": 1, "message": "Hello"})],
/// };
///
/// // INSERT notification
/// let insert = ChangeEvent::Insert {
///     subscription_id: "sub-1".to_string(),
///     rows: vec![json!({"id": 2, "message": "New message"})],
/// };
///
/// // UPDATE notification
/// let update = ChangeEvent::Update {
///     subscription_id: "sub-1".to_string(),
///     rows: vec![json!({"id": 1, "message": "Updated"})],
///     old_rows: vec![json!({"id": 1, "message": "Hello"})],
/// };
///
/// // DELETE notification
/// let delete = ChangeEvent::Delete {
///     subscription_id: "sub-1".to_string(),
///     old_rows: vec![json!({"id": 2, "message": "New message"})],
/// };
/// ```
/// Subscription options returned by the server for a live query.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubscriptionOptions {
    /// Number of most recent rows to include in the initial snapshot
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_rows: Option<usize>,
}

/// Change notification received via WebSocket subscription.
#[derive(Debug, Clone)]
pub enum ChangeEvent {
    /// Acknowledgement of subscription registration
    Ack {
        /// Subscription ID if provided by the server
        subscription_id: Option<String>,
        /// Optional server message
        message: Option<String>,
    },

    /// Initial snapshot of rows matching the subscription query
    InitialData {
        /// Subscription ID the snapshot belongs to
        subscription_id: String,
        /// Snapshot rows
        rows: Vec<JsonValue>,
    },

    /// Insert notification
    Insert {
        /// Subscription ID the change belongs to
        subscription_id: String,
        /// Inserted rows
        rows: Vec<JsonValue>,
    },

    /// Update notification
    Update {
        /// Subscription ID the change belongs to
        subscription_id: String,
        /// Updated rows (current values)
        rows: Vec<JsonValue>,
        /// Previous row values
        old_rows: Vec<JsonValue>,
    },

    /// Delete notification
    Delete {
        /// Subscription ID the change belongs to
        subscription_id: String,
        /// Deleted rows
        old_rows: Vec<JsonValue>,
    },

    /// Error notification from the server
    Error {
        /// Subscription ID related to the error
        subscription_id: String,
        /// Error code
        code: String,
        /// Human-readable error message
        message: String,
    },

    /// Unknown payload (kept for logging/diagnostics)
    Unknown {
        /// Raw JSON payload
        raw: JsonValue,
    },
}

impl ChangeEvent {
    /// Returns true if this is an error event
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error { .. })
    }

    /// Returns the subscription ID for this event, if any
    pub fn subscription_id(&self) -> Option<&str> {
        match self {
            Self::Ack {
                subscription_id, ..
            } => subscription_id.as_deref(),
            Self::InitialData {
                subscription_id, ..
            }
            | Self::Insert {
                subscription_id, ..
            }
            | Self::Update {
                subscription_id, ..
            }
            | Self::Delete {
                subscription_id, ..
            }
            | Self::Error {
                subscription_id, ..
            } => Some(subscription_id.as_str()),
            Self::Unknown { .. } => None,
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

    /// Server build date
    #[serde(default)]
    pub build_date: Option<String>,
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
    fn test_change_event_helpers() {
        let insert = ChangeEvent::Insert {
            subscription_id: "sub-1".to_string(),
            rows: vec![json!({"id": 1})],
        };
        assert_eq!(insert.subscription_id(), Some("sub-1"));
        assert!(!insert.is_error());

        let error = ChangeEvent::Error {
            subscription_id: "sub-2".to_string(),
            code: "ERR".to_string(),
            message: "test error".to_string(),
        };
        assert!(error.is_error());
        assert_eq!(error.subscription_id(), Some("sub-2"));

        let ack = ChangeEvent::Ack {
            subscription_id: None,
            message: Some("registered".to_string()),
        };
        assert_eq!(ack.subscription_id(), None);
        assert!(!ack.is_error());
    }
}
