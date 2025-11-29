//! WebSocket message protocol for KalamDB live query subscriptions
//!
//! This module defines the complete WebSocket message protocol between clients and server
//! for real-time query subscriptions with batched initial data loading.
//!
//! # Protocol Flow
//!
//! ## 1. Client Subscription Request
//! ```json
//! {
//!   "type": "subscribe",
//!   "subscriptions": [{
//!     "id": "sub-1",
//!     "sql": "SELECT * FROM messages WHERE user_id = CURRENT_USER()",
//!     "options": {}
//!   }]
//! }
//! ```
//!
//! ## 2. Server Subscription Acknowledgement
//! ```json
//! {
//!   "type": "subscription_ack",
//!   "subscription_id": "sub-1",
//!   "total_rows": 5000,
//!   "batch_control": {
//!     "batch_num": 0,
//!     "total_batches": 5,
//!     "has_more": true,
//!     "status": "loading"
//!   }
//! }
//! ```
//!
//! ## 3. Server Initial Data Batch (First Batch)
//! ```json
//! {
//!   "type": "initial_data_batch",
//!   "subscription_id": "sub-1",
//!   "rows": [{"id": 1, "message": "Hello"}, ...],
//!   "batch_control": {
//!     "batch_num": 0,
//!     "total_batches": 5,
//!     "has_more": true,
//!     "status": "loading"
//!   }
//! }
//! ```
//!
//! ## 4. Client Next Batch Request
//! ```json
//! {
//!   "type": "next_batch",
//!   "subscription_id": "sub-1",
//!   "batch_num": 1
//! }
//! ```
//!
//! ## 5. Server Subsequent Batch
//! ```json
//! {
//!   "type": "initial_data_batch",
//!   "subscription_id": "sub-1",
//!   "rows": [{"id": 1001, "message": "World"}, ...],
//!   "batch_control": {
//!     "batch_num": 1,
//!     "total_batches": 5,
//!     "has_more": true,
//!     "status": "loading_batch"
//!   }
//! }
//! ```
//!
//! ## 6. Server Final Batch
//! ```json
//! {
//!   "type": "initial_data_batch",
//!   "subscription_id": "sub-1",
//!   "rows": [{"id": 4501, "message": "Done"}, ...],
//!   "batch_control": {
//!     "batch_num": 4,
//!     "total_batches": 5,
//!     "has_more": false,
//!     "status": "ready"
//!   }
//! }
//! ```
//!
//! ## 7. Real-time Change Notifications (After Initial Load)
//! ```json
//! {
//!   "type": "change",
//!   "subscription_id": "sub-1",
//!   "change_type": "insert",
//!   "rows": [{"id": 5001, "message": "New"}]
//! }
//! ```

use crate::ids::SeqId;

// Full Row type for server (with datafusion support)
#[cfg(feature = "full")]
use crate::models::Row;

// Simple Row type for WASM (JSON only)
#[cfg(feature = "wasm")]
pub type Row = serde_json::Map<String, serde_json::Value>;

use serde::{Deserialize, Serialize};

/// Batch size in bytes (8KB) for chunking large initial data payloads
pub const BATCH_SIZE_BYTES: usize = 8 * 1024;

/// Maximum rows per batch to prevent memory spikes (default 1000 rows)
pub const MAX_ROWS_PER_BATCH: usize = 1000;

/// WebSocket message types sent from server to client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WebSocketMessage {
    /// Authentication successful response
    ///
    /// Sent after client sends Authenticate message with valid credentials.
    /// Client can now send Subscribe/Unsubscribe messages.
    AuthSuccess {
        /// Authenticated user ID
        user_id: String,
        /// User role
        role: String,
    },

    /// Authentication failed response
    ///
    /// Sent when authentication fails. Connection will be closed immediately.
    AuthError {
        /// Error message describing why authentication failed
        message: String,
    },

    /// Acknowledgement of successful subscription registration
    ///
    /// Sent immediately after client subscribes, includes total row count
    /// and batch control information for paginated initial data loading.
    SubscriptionAck {
        /// The subscription ID that was registered
        subscription_id: String,
        /// Total number of rows available for initial load
        total_rows: u32,
        /// Batch control information for paginated loading
        batch_control: BatchControl,
    },

    /// Initial data batch sent after subscription or on client request
    ///
    /// Sent automatically for the first batch after subscription acknowledgement,
    /// then sent on-demand when client requests via NextBatchRequest.
    InitialDataBatch {
        /// The subscription ID this data is for
        subscription_id: String,
        /// The rows in this batch
        rows: Vec<Row>,
        /// Batch control information
        batch_control: BatchControl,
    },

    /// Change notification (delegates to Notification enum)
    ///
    /// Sent when data changes (INSERT/UPDATE/DELETE) after initial load completes.
    /// Only sent when batch_control.status == Ready.
    #[serde(untagged)]
    Notification(Notification),
}

/// Authentication credentials for WebSocket connection
///
/// This enum is the **client-facing** authentication model for WebSocket connections.
/// It maps 1:1 with `AuthRequest` variants in `kalamdb-auth` via the `From` impl.
///
/// # Supported Methods
///
/// - `Basic` - Username/password authentication
/// - `Jwt` - JWT token (Bearer) authentication
///
/// # JSON Wire Format
///
/// ```json
/// // Basic Auth
/// {"type": "authenticate", "method": "basic", "username": "alice", "password": "secret"}
///
/// // JWT Auth  
/// {"type": "authenticate", "method": "jwt", "token": "eyJhbGciOiJIUzI1NiIs..."}
/// ```
///
/// # Adding a New Authentication Method
///
/// 1. Add variant here with appropriate fields
/// 2. Add corresponding variant to `AuthRequest` in `kalamdb-auth/unified.rs`
/// 3. Update `From<WsAuthCredentials> for AuthRequest` impl
/// 4. Update client SDKs (TypeScript `Auth` class, WASM `WasmAuthProvider`)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum WsAuthCredentials {
    /// Username and password authentication
    Basic {
        username: String,
        password: String,
    },
    /// JWT token authentication
    Jwt {
        token: String,
    },
    // Future auth methods can be added here:
    // ApiKey { key: String },
    // OAuth { provider: String, token: String },
}

/// Client-to-server request messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Authenticate WebSocket connection
    ///
    /// Client sends this immediately after establishing WebSocket connection.
    /// Server must receive this within 3 seconds or connection will be closed.
    /// Server responds with AuthSuccess or AuthError.
    ///
    /// Supports multiple authentication methods via the `credentials` field.
    Authenticate {
        /// Authentication credentials (basic, jwt, or future methods)
        #[serde(flatten)]
        credentials: WsAuthCredentials,
    },

    /// Subscribe to live query updates
    ///
    /// Client sends this to register a single subscription.
    /// Server responds with SubscriptionAck followed by InitialDataBatch.
    Subscribe {
        /// Subscription to register
        subscription: SubscriptionRequest,
    },

    /// Request next batch of initial data
    ///
    /// Client sends this after processing a batch to request the next batch.
    /// Server responds with InitialDataBatch.
    NextBatch {
        /// The subscription ID to fetch the next batch for
        subscription_id: String,
        /// The SeqId of the last row received (for pagination)
        last_seq_id: Option<SeqId>,
    },

    /// Unsubscribe from live query
    ///
    /// Client sends this to stop receiving updates for a subscription.
    Unsubscribe {
        /// The subscription ID to unsubscribe from
        subscription_id: String,
    },
}

/// Subscription request details
///
/// This is a client-only struct representing the subscription request.
/// Server-side metadata (table_id, filter_expr, projections, etc.) is stored
/// in SubscriptionState within ConnectionState.
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

/// Options for live query subscriptions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubscriptionOptions {
    /// Optional: Configure batch size for initial data streaming
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_size: Option<usize>,

    /// Optional: Number of last rows to fetch for initial data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_rows: Option<u32>,
}

/// Batch control metadata for paginated initial data loading
///
/// Tracks the progress of batched initial data loading to prevent
/// overwhelming clients with large payloads (e.g., 1MB+).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchControl {
    /// Current batch number (0-indexed)
    pub batch_num: u32,

    /// Total number of batches available (optional/estimated)
    pub total_batches: Option<u32>,

    /// Whether more batches are available to fetch
    pub has_more: bool,

    /// Loading status for the subscription
    pub status: BatchStatus,

    /// The SeqId of the last row in this batch (used for next request)
    pub last_seq_id: Option<SeqId>,

    /// The snapshot boundary (max SeqId at start of load)
    pub snapshot_end_seq: Option<SeqId>,
}

/// Status of the initial data loading process
///
/// Transitions: Loading → LoadingBatch → Ready
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BatchStatus {
    /// Initial batch being loaded (batch_num == 0)
    Loading,

    /// Subsequent batches being loaded (batch_num > 0, has_more == true)
    LoadingBatch,

    /// All initial data has been loaded, live updates active (has_more == false)
    Ready,
}

/// Request from client to fetch the next batch of initial data
///
/// DEPRECATED: Use ClientMessage::NextBatch instead
#[deprecated(note = "Use ClientMessage::NextBatch for type-safe requests")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextBatchRequest {
    /// The subscription ID to fetch the next batch for
    pub subscription_id: String,

    /// The batch number to fetch (should be current_batch + 1)
    pub batch_num: u32,
}

/// Notification message sent to clients for live query updates
///
/// # Example Initial Data
/// ```json
/// {
///   "type": "initial_data",
///   "subscription_id": "sub-1",
///   "rows": [
///     {"id": 1, "message": "Hello"},
///     {"id": 2, "message": "World"}
///   ]
/// }
/// ```
///
/// # Example Change Notification (INSERT)
/// ```json
/// {
///   "type": "change",
///   "subscription_id": "sub-1",
///   "change_type": "insert",
///   "rows": [
///     {"id": 3, "message": "New message"}
///   ]
/// }
/// ```
///
/// # Example Change Notification (UPDATE)
/// ```json
/// {
///   "type": "change",
///   "subscription_id": "sub-1",
///   "change_type": "update",
///   "rows": [
///     {"id": 2, "message": "Updated message"}
///   ],
///   "old_values": [
///     {"id": 2, "message": "World"}
///   ]
/// }
/// ```
///
/// # Example Change Notification (DELETE)
/// ```json
/// {
///   "type": "change",
///   "subscription_id": "sub-1",
///   "change_type": "delete",
///   "old_values": [
///     {"id": 1, "message": "Hello"}
///   ]
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Notification {
    /// Change notification for INSERT/UPDATE/DELETE operations
    ///
    /// Sent only after initial data loading is complete (batch_control.status == Ready).
    Change {
        /// The subscription ID this notification is for
        subscription_id: String,

        /// Type of change that occurred
        change_type: ChangeType,

        /// New/current row values (for INSERT and UPDATE)
        #[serde(skip_serializing_if = "Option::is_none")]
        rows: Option<Vec<Row>>,

        /// Previous row values (for UPDATE and DELETE)
        #[serde(skip_serializing_if = "Option::is_none")]
        old_values: Option<Vec<Row>>,
    },

    /// Error notification (e.g., subscription query failed)
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
pub enum ChangeType {
    /// New row(s) inserted
    Insert,

    /// Existing row(s) updated
    Update,

    /// Row(s) deleted
    Delete,
}

impl WebSocketMessage {
    /// Create a subscription acknowledgement message with batch control
    pub fn subscription_ack(
        subscription_id: String,
        total_rows: u32,
        batch_control: BatchControl,
    ) -> Self {
        Self::SubscriptionAck {
            subscription_id,
            total_rows,
            batch_control,
        }
    }

    /// Create an initial data batch message
    pub fn initial_data_batch(
        subscription_id: String,
        rows: Vec<Row>,
        batch_control: BatchControl,
    ) -> Self {
        Self::InitialDataBatch {
            subscription_id,
            rows,
            batch_control,
        }
    }
}

impl ClientMessage {
    /// Create a subscribe message for a single subscription
    pub fn subscribe(subscription: SubscriptionRequest) -> Self {
        Self::Subscribe { subscription }
    }

    /// Create a next batch request message
    pub fn next_batch(subscription_id: String, last_seq_id: Option<SeqId>) -> Self {
        Self::NextBatch {
            subscription_id,
            last_seq_id,
        }
    }

    /// Create an unsubscribe message
    pub fn unsubscribe(subscription_id: String) -> Self {
        Self::Unsubscribe { subscription_id }
    }
}

impl BatchControl {
    /// Create a batch control for the first batch
    pub fn first(total_batches: u32) -> Self {
        Self {
            batch_num: 0,
            total_batches: Some(total_batches),
            has_more: total_batches > 1,
            status: BatchStatus::Loading,
            last_seq_id: None,
            snapshot_end_seq: None,
        }
    }

    /// Create a batch control for a middle batch
    pub fn middle(batch_num: u32, total_batches: u32) -> Self {
        Self {
            batch_num,
            total_batches: Some(total_batches),
            has_more: batch_num + 1 < total_batches,
            status: BatchStatus::LoadingBatch,
            last_seq_id: None,
            snapshot_end_seq: None,
        }
    }

    /// Create a batch control for the last batch
    pub fn last(batch_num: u32, total_batches: u32) -> Self {
        Self {
            batch_num,
            total_batches: Some(total_batches),
            has_more: false,
            status: BatchStatus::Ready,
            last_seq_id: None,
            snapshot_end_seq: None,
        }
    }

    /// Create a batch control for a specific batch number
    pub fn for_batch(batch_num: u32, total_batches: u32) -> Self {
        let has_more = batch_num + 1 < total_batches;
        let status = if batch_num == 0 {
            BatchStatus::Loading
        } else if has_more {
            BatchStatus::LoadingBatch
        } else {
            BatchStatus::Ready
        };

        Self {
            batch_num,
            total_batches: Some(total_batches),
            has_more,
            status,
            last_seq_id: None,
            snapshot_end_seq: None,
        }
    }
}

impl Notification {
    /// Create an INSERT change notification
    pub fn insert(subscription_id: String, rows: Vec<Row>) -> Self {
        Self::Change {
            subscription_id,
            change_type: ChangeType::Insert,
            rows: Some(rows),
            old_values: None,
        }
    }

    /// Create an UPDATE change notification
    pub fn update(subscription_id: String, new_rows: Vec<Row>, old_rows: Vec<Row>) -> Self {
        Self::Change {
            subscription_id,
            change_type: ChangeType::Update,
            rows: Some(new_rows),
            old_values: Some(old_rows),
        }
    }

    /// Create a DELETE change notification
    pub fn delete(subscription_id: String, old_rows: Vec<Row>) -> Self {
        Self::Change {
            subscription_id,
            change_type: ChangeType::Delete,
            rows: None,
            old_values: Some(old_rows),
        }
    }

    /// Create an error notification
    pub fn error(subscription_id: String, code: String, message: String) -> Self {
        Self::Error {
            subscription_id,
            code,
            message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::scalar::ScalarValue;
    use std::collections::BTreeMap;

    fn create_test_row(id: i64, message: &str) -> Row {
        let mut values = BTreeMap::new();
        values.insert("id".to_string(), ScalarValue::Int64(Some(id)));
        values.insert(
            "message".to_string(),
            ScalarValue::Utf8(Some(message.to_string())),
        );
        Row::new(values)
    }

    #[test]
    fn test_client_message_serialization() {
        use crate::websocket::{ClientMessage, SubscriptionOptions, SubscriptionRequest};

        let msg = ClientMessage::subscribe(SubscriptionRequest {
            id: "sub-1".to_string(),
            sql: "SELECT * FROM messages".to_string(),
            options: SubscriptionOptions::default(),
        });

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"subscribe\""));
        assert!(json.contains("sub-1"));
    }

    #[test]
    fn test_next_batch_request() {
        use crate::ids::SeqId;
        use crate::websocket::ClientMessage;

        let msg = ClientMessage::next_batch("sub-1".to_string(), Some(SeqId::new(100)));
        let json = serde_json::to_string(&msg).unwrap();

        assert!(json.contains("\"type\":\"next_batch\""));
        assert!(json.contains("\"last_seq_id\":100"));
    }

    #[test]
    fn test_batch_control_helpers() {
        use crate::websocket::BatchControl;

        let first = BatchControl::first(5);
        assert_eq!(first.batch_num, 0);
        assert_eq!(first.total_batches, Some(5));
        assert!(first.has_more);

        let last = BatchControl::last(4, 5);
        assert_eq!(last.batch_num, 4);
        assert!(!last.has_more);
    }

    #[test]
    fn test_insert_notification() {
        let row = create_test_row(1, "Hello");
        let notification = Notification::insert("sub-1".to_string(), vec![row]);

        let json = serde_json::to_string(&notification).unwrap();
        assert!(json.contains("change"));
        assert!(json.contains("insert"));
        assert!(json.contains("sub-1"));
        assert!(json.contains("Hello"));
    }

    #[test]
    fn test_update_notification() {
        let new_row = create_test_row(1, "Updated");
        let old_row = create_test_row(1, "Original");

        let notification = Notification::update("sub-1".to_string(), vec![new_row], vec![old_row]);

        let json = serde_json::to_string(&notification).unwrap();
        assert!(json.contains("update"));
        assert!(json.contains("Updated"));
        assert!(json.contains("Original"));
    }

    #[test]
    fn test_delete_notification() {
        let old_row = create_test_row(1, "Hello");

        let notification = Notification::delete("sub-1".to_string(), vec![old_row]);

        let json = serde_json::to_string(&notification).unwrap();
        assert!(json.contains("delete"));
        assert!(json.contains("old_values"));
        assert!(!json.contains("\"rows\""));
    }

    #[test]
    fn test_error_notification() {
        let notification = Notification::error(
            "sub-1".to_string(),
            "INVALID_QUERY".to_string(),
            "Query syntax error".to_string(),
        );

        let json = serde_json::to_string(&notification).unwrap();
        assert!(json.contains("error"));
        assert!(json.contains("INVALID_QUERY"));
    }
}
