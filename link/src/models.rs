//! Data models for kalam-link client library.
//!
//! Defines request and response structures for query execution and
//! WebSocket subscription messages.

use crate::seq_id::SeqId;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// Batch control metadata for paginated initial data loading
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchControl {
    /// Current batch number (0-indexed)
    pub batch_num: u32,

    /// Total number of batches available (optional/estimated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_batches: Option<u32>,

    /// Whether more batches are available to fetch
    pub has_more: bool,

    /// Loading status for the subscription
    pub status: BatchStatus,

    /// The SeqId of the last row in this batch (used for subsequent requests)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seq_id: Option<SeqId>,

    /// Snapshot boundary SeqId captured at subscription time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_end_seq: Option<SeqId>,
}

/// Status of the initial data loading process
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BatchStatus {
    /// Initial batch being loaded
    Loading,

    /// Subsequent batches being loaded
    LoadingBatch,

    /// All initial data has been loaded, live updates active
    Ready,
}

/// Client-to-server request messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Subscribe to live query updates
    Subscribe {
        /// List of subscriptions to register
        subscriptions: Vec<SubscriptionRequest>,
    },

    /// Request next batch of initial data
    NextBatch {
        /// The subscription ID to fetch the next batch for
        subscription_id: String,
        /// The SeqId of the last row received (used for pagination)
        #[serde(skip_serializing_if = "Option::is_none")]
        last_seq_id: Option<SeqId>,
    },

    /// Unsubscribe from live query
    Unsubscribe {
        /// The subscription ID to unsubscribe from
        subscription_id: String,
    },
}

/// Subscription request details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionRequest {
    /// Unique subscription identifier (client-generated)
    pub id: String,
    /// SQL query for live updates (must be a SELECT statement)
    pub sql: String,
    /// Optional subscription options
    #[serde(default)]
    pub options: SubscriptionOptions,
}

/// Subscription options for a live query
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubscriptionOptions {
    /// Reserved for future use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _reserved: Option<String>,

    /// Optional hint for server-side batch sizing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_size: Option<usize>,
}

/// WebSocket message types sent from server to client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Acknowledgement of successful subscription registration
    SubscriptionAck {
        /// The subscription ID that was registered
        subscription_id: String,
        /// Total number of rows available for initial load
        total_rows: u32,
        /// Batch control information
        batch_control: BatchControl,
    },

    /// Initial data batch sent after subscription or on client request
    InitialDataBatch {
        /// The subscription ID this data is for
        subscription_id: String,
        /// The rows in this batch
        rows: Vec<HashMap<String, JsonValue>>,
        /// Batch control information
        batch_control: BatchControl,
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
#[derive(Debug, Clone)]
pub enum ChangeEvent {
    /// Acknowledgement of subscription registration with batch info
    Ack {
        /// Subscription ID
        subscription_id: String,
        /// Total rows available for initial load
        total_rows: u32,
        /// Batch control information
        batch_control: BatchControl,
    },

    /// Initial data batch (paginated loading)
    InitialDataBatch {
        /// Subscription ID the batch belongs to
        subscription_id: String,
        /// Rows in this batch
        rows: Vec<HashMap<String, JsonValue>>,
        /// Batch control information
        batch_control: BatchControl,
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
            }
            | Self::InitialDataBatch {
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
            subscription_id: "sub-1".to_string(),
            total_rows: 0,
            batch_control: BatchControl {
                batch_num: 0,
                total_batches: Some(0),
                has_more: false,
                status: BatchStatus::Ready,
                last_seq_id: None,
                snapshot_end_seq: None,
            },
        };
        assert_eq!(ack.subscription_id(), Some("sub-1"));
        assert!(!ack.is_error());
    }
}
