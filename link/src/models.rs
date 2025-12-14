//! Data models for kalam-link client library.
//!
//! Defines request and response structures for query execution and
//! WebSocket subscription messages.

use crate::seq_id::SeqId;
use crate::timestamp::{TimestampFormat, TimestampFormatter};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// HTTP protocol version to use for connections.
///
/// HTTP/2 provides benefits like multiplexing multiple requests over a single
/// connection, header compression, and improved performance for multiple
/// concurrent requests.
///
/// # Example
///
/// ```rust
/// use kalam_link::{ConnectionOptions, HttpVersion};
///
/// let options = ConnectionOptions::new()
///     .with_http_version(HttpVersion::Http2);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum HttpVersion {
    /// HTTP/1.1 (default) - widely compatible, one request per connection
    #[default]
    #[serde(rename = "http1", alias = "http/1.1", alias = "1.1")]
    Http1,

    /// HTTP/2 - multiplexed requests, header compression, better performance
    #[serde(rename = "http2", alias = "http/2", alias = "2")]
    Http2,

    /// Automatic - let the client negotiate the best version with the server
    #[serde(rename = "auto")]
    Auto,
}

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

/// Authentication credentials for WebSocket connection
///
/// This enum mirrors `WsAuthCredentials` from the backend (`kalamdb-commons/websocket.rs`).
/// Both enums must stay in sync for proper serialization/deserialization.
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
/// 1. Add variant here (client side)
/// 2. Add matching variant to backend's `WsAuthCredentials`
/// 3. Update `WasmAuthProvider` in `wasm.rs` if WASM support needed
/// 4. Update TypeScript `Auth` class in SDK
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
    /// Supports multiple authentication methods via the credentials field.
    Authenticate {
        /// Authentication credentials (basic, jwt, or future methods)
        #[serde(flatten)]
        credentials: WsAuthCredentials,
    },

    /// Subscribe to live query updates
    Subscribe {
        /// Subscription to register
        subscription: SubscriptionRequest,
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

/// Connection-level options for the WebSocket/HTTP client.
///
/// These options control connection behavior including:
/// - HTTP protocol version (HTTP/1.1 or HTTP/2)
/// - Automatic reconnection on connection loss
/// - Reconnection timing and retry limits
///
/// Separate from SubscriptionOptions which control individual subscriptions.
///
/// # Example
///
/// ```rust
/// use kalam_link::{ConnectionOptions, HttpVersion};
///
/// let options = ConnectionOptions::default()
///     .with_http_version(HttpVersion::Http2)
///     .with_auto_reconnect(true)
///     .with_reconnect_delay_ms(2000)
///     .with_max_reconnect_attempts(Some(10));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionOptions {
    /// HTTP protocol version to use for connections
    /// Default: Http1 (HTTP/1.1) for maximum compatibility
    /// Use Http2 for better performance with multiple concurrent requests
    #[serde(default)]
    pub http_version: HttpVersion,

    /// Enable automatic reconnection on connection loss
    /// Default: true - automatically attempts to reconnect
    #[serde(default = "default_auto_reconnect")]
    pub auto_reconnect: bool,

    /// Initial delay in milliseconds between reconnection attempts
    /// Default: 1000ms (1 second)
    /// Uses exponential backoff up to max_reconnect_delay_ms
    #[serde(default = "default_reconnect_delay_ms")]
    pub reconnect_delay_ms: u64,

    /// Maximum delay between reconnection attempts (for exponential backoff)
    /// Default: 30000ms (30 seconds)
    #[serde(default = "default_max_reconnect_delay_ms")]
    pub max_reconnect_delay_ms: u64,

    /// Maximum number of reconnection attempts before giving up
    /// Default: None (infinite retries)
    /// Set to Some(0) to disable reconnection entirely
    #[serde(default)]
    pub max_reconnect_attempts: Option<u32>,

    /// Timestamp format to use for displaying timestamp columns
    /// Default: Iso8601 (2024-12-14T15:30:45.123Z)
    /// This allows clients to control how timestamps are displayed
    #[serde(default)]
    pub timestamp_format: TimestampFormat,
}

fn default_auto_reconnect() -> bool {
    true
}

fn default_reconnect_delay_ms() -> u64 {
    1000
}

fn default_max_reconnect_delay_ms() -> u64 {
    30000
}

impl Default for ConnectionOptions {
    fn default() -> Self {
        Self {
            http_version: HttpVersion::default(),
            auto_reconnect: true,
            reconnect_delay_ms: 1000,
            max_reconnect_delay_ms: 30000,
            max_reconnect_attempts: None,
            timestamp_format: TimestampFormat::Iso8601,
        }
    }
}

impl ConnectionOptions {
    /// Create new connection options with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the HTTP protocol version to use
    ///
    /// - `HttpVersion::Http1` - HTTP/1.1 (default, maximum compatibility)
    /// - `HttpVersion::Http2` - HTTP/2 (better performance for concurrent requests)
    /// - `HttpVersion::Auto` - Let the client negotiate with the server
    pub fn with_http_version(mut self, version: HttpVersion) -> Self {
        self.http_version = version;
        self
    }

    /// Set whether to automatically reconnect on connection loss
    pub fn with_auto_reconnect(mut self, enabled: bool) -> Self {
        self.auto_reconnect = enabled;
        self
    }

    /// Set the initial delay between reconnection attempts (in milliseconds)
    pub fn with_reconnect_delay_ms(mut self, delay_ms: u64) -> Self {
        self.reconnect_delay_ms = delay_ms;
        self
    }

    /// Set the maximum delay between reconnection attempts (in milliseconds)
    pub fn with_max_reconnect_delay_ms(mut self, max_delay_ms: u64) -> Self {
        self.max_reconnect_delay_ms = max_delay_ms;
        self
    }

    /// Set the maximum number of reconnection attempts
    /// Pass None for infinite retries, Some(0) to disable reconnection
    pub fn with_max_reconnect_attempts(mut self, max_attempts: Option<u32>) -> Self {
        self.max_reconnect_attempts = max_attempts;
        self
    }

    /// Set the timestamp format for displaying timestamp columns
    ///
    /// Controls how timestamp values (milliseconds since epoch) are formatted
    /// in query results and subscription updates.
    ///
    /// # Example
    ///
    /// ```rust
    /// use kalam_link::{ConnectionOptions, TimestampFormat};
    ///
    /// let options = ConnectionOptions::new()
    ///     .with_timestamp_format(TimestampFormat::Iso8601);  // 2024-12-14T15:30:45.123Z
    /// ```
    pub fn with_timestamp_format(mut self, format: TimestampFormat) -> Self {
        self.timestamp_format = format;
        self
    }

    /// Create a timestamp formatter from this configuration
    pub fn create_formatter(&self) -> TimestampFormatter {
        TimestampFormatter::new(self.timestamp_format)
    }
}

/// Subscription options for a live query.
///
/// These options control individual subscription behavior including:
/// - Initial data loading (batch_size, last_rows)
/// - Data resumption after reconnection (from_seq_id)
///
/// Aligned with backend's SubscriptionOptions in kalamdb-commons/websocket.rs.
///
/// # Example
///
/// ```rust
/// use kalam_link::{SeqId, SubscriptionOptions};
///
/// // Fetch last 100 rows with batch size of 50
/// let options = SubscriptionOptions::default()
///     .with_batch_size(50)
///     .with_last_rows(100);
///
/// // Resume from a specific sequence ID after reconnection
/// let some_seq_id = SeqId::new(123);
/// let options = SubscriptionOptions::default()
///     .with_from_seq_id(some_seq_id);
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubscriptionOptions {
    /// Hint for server-side batch sizing during initial data load
    /// Default: server-configured (typically 1000 rows per batch)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_size: Option<usize>,

    /// Number of last (newest) rows to fetch for initial data
    /// Default: None (fetch all matching rows)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_rows: Option<u32>,

    /// Resume subscription from a specific sequence ID
    /// When set, the server will only send changes after this seq_id
    /// Typically set automatically during reconnection to resume from last received event
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_seq_id: Option<SeqId>,
}

impl SubscriptionOptions {
    /// Create new subscription options with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the batch size for initial data loading
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = Some(size);
        self
    }

    /// Set the number of last rows to fetch
    pub fn with_last_rows(mut self, count: u32) -> Self {
        self.last_rows = Some(count);
        self
    }

    /// Resume from a specific sequence ID
    /// Used during reconnection to continue from where we left off
    pub fn with_from_seq_id(mut self, seq_id: SeqId) -> Self {
        self.from_seq_id = Some(seq_id);
        self
    }

    /// Check if this has a resume seq_id set
    pub fn has_resume_seq_id(&self) -> bool {
        self.from_seq_id.is_some()
    }
}

/// WebSocket message types sent from server to client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Authentication successful response (browser clients only)
    AuthSuccess {
        /// Authenticated user ID
        user_id: String,
        /// User role
        role: String,
    },

    /// Authentication failed response (browser clients only)
    AuthError {
        /// Error message
        message: String,
    },

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
///     namespace_id: None,
///     serialization_mode: SerializationMode::Typed,
/// };
///
/// // Parametrized query
/// let request = QueryRequest {
///     sql: "SELECT * FROM users WHERE id = $1".to_string(),
///     params: Some(vec![json!(42)]),
///     namespace_id: None,
///     serialization_mode: SerializationMode::Typed,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRequest {
    /// SQL query string (may contain $1, $2... placeholders)
    pub sql: String,

    /// Optional parameter values for placeholders
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Vec<JsonValue>>,

    /// Optional namespace ID for unqualified table names.
    /// When set, queries like `SELECT * FROM users` resolve to `namespace_id.users`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace_id: Option<String>,

    /// Serialization mode for response data.
    /// 
    /// - `simple`: Plain JSON values, Int64/UInt64 as strings
    /// - `typed`: Values with type wrappers and formatted timestamps (default for kalam-link)
    #[serde(default = "SerializationMode::typed")]
    pub serialization_mode: SerializationMode,
}

/// Serialization mode for converting Arrow data to JSON
///
/// Controls how ScalarValue types are serialized in query responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SerializationMode {
    /// Simple JSON values without type information
    /// Example: `{"id": "123", "name": "Alice"}`
    Simple,

    /// Values with type wrappers and formatted timestamps
    /// Example: `{"id": {"Int64": "123"}, "created_at": {"TimestampMicrosecond": {...}}}`
    Typed,
}

impl Default for SerializationMode {
    fn default() -> Self {
        // kalam-link always uses typed mode by default
        Self::Typed
    }
}

impl SerializationMode {
    /// Create typed mode (used as serde default)
    pub fn typed() -> Self {
        Self::Typed
    }
}

/// Response from SQL query execution.
///
/// Execution status enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ResponseStatus {
    Success,
    Error,
}

impl std::fmt::Display for ResponseStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResponseStatus::Success => write!(f, "success"),
            ResponseStatus::Error => write!(f, "error"),
        }
    }
}

/// Contains query results, execution metadata, and optional error information.
/// Matches the server's SqlResponse structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResponse {
    /// Query execution status ("success" or "error")
    pub status: ResponseStatus,

    /// Array of result sets, one per executed statement
    #[serde(default)]
    pub results: Vec<QueryResult>,

    /// Query execution time in milliseconds (with fractional precision)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub took: Option<f64>,

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
    #[serde(default)]
    pub version: String,

    /// API version (e.g., "v1")
    pub api_version: String,

    /// Server build date
    #[serde(default)]
    pub build_date: Option<String>,
}

/// Configuration for establishing a WebSocket subscription.
#[derive(Debug, Clone)]
pub struct SubscriptionConfig {
    /// Subscription identifier (client-generated, required)
    pub id: String,
    /// SQL query to register for live updates
    pub sql: String,
    /// Optional subscription options (e.g., last_rows)
    pub options: Option<SubscriptionOptions>,
    /// Override WebSocket URL (falls back to base_url conversion when `None`)
    pub ws_url: Option<String>,
}

impl SubscriptionConfig {
    /// Create a new configuration with required ID and SQL.
    ///
    /// By default, includes empty subscription options (batch streaming configured server-side).
    pub fn new(id: impl Into<String>, sql: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            sql: sql.into(),
            options: Some(SubscriptionOptions::default()),
            ws_url: None,
        }
    }

    /// Create a configuration without any initial data fetch
    pub fn without_initial_data(id: impl Into<String>, sql: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            sql: sql.into(),
            options: None,
            ws_url: None,
        }
    }

    /// Set the number of initial rows to fetch (deprecated - batch streaming configured server-side)
    #[deprecated(note = "Batch streaming is now configured server-side, this method is a no-op")]
    pub fn with_last_rows(self, _count: usize) -> Self {
        // No-op: batch streaming configured server-side
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ==================== ConnectionOptions Tests ====================

    #[test]
    fn test_connection_options_default() {
        let opts = ConnectionOptions::default();

        assert!(opts.auto_reconnect, "auto_reconnect should default to true");
        assert_eq!(opts.reconnect_delay_ms, 1000, "reconnect_delay_ms should default to 1000");
        assert_eq!(
            opts.max_reconnect_delay_ms, 30000,
            "max_reconnect_delay_ms should default to 30000"
        );
        assert!(
            opts.max_reconnect_attempts.is_none(),
            "max_reconnect_attempts should default to None (infinite)"
        );
    }

    #[test]
    fn test_connection_options_new() {
        let opts = ConnectionOptions::new();

        // new() should be equivalent to default()
        assert!(opts.auto_reconnect);
        assert_eq!(opts.reconnect_delay_ms, 1000);
        assert_eq!(opts.max_reconnect_delay_ms, 30000);
        assert!(opts.max_reconnect_attempts.is_none());
    }

    #[test]
    fn test_connection_options_builder_pattern() {
        let opts = ConnectionOptions::new()
            .with_auto_reconnect(false)
            .with_reconnect_delay_ms(2000)
            .with_max_reconnect_delay_ms(60000)
            .with_max_reconnect_attempts(Some(5));

        assert!(!opts.auto_reconnect);
        assert_eq!(opts.reconnect_delay_ms, 2000);
        assert_eq!(opts.max_reconnect_delay_ms, 60000);
        assert_eq!(opts.max_reconnect_attempts, Some(5));
    }

    #[test]
    fn test_connection_options_disable_reconnect() {
        // Setting max_reconnect_attempts to Some(0) should disable reconnection
        let opts = ConnectionOptions::new().with_max_reconnect_attempts(Some(0));

        assert_eq!(opts.max_reconnect_attempts, Some(0));
    }

    #[test]
    fn test_connection_options_infinite_retries() {
        // None means infinite retries
        let opts = ConnectionOptions::new().with_max_reconnect_attempts(None);

        assert!(opts.max_reconnect_attempts.is_none());
    }

    #[test]
    fn test_connection_options_serialization() {
        let opts = ConnectionOptions::new()
            .with_auto_reconnect(true)
            .with_reconnect_delay_ms(500)
            .with_max_reconnect_delay_ms(10000)
            .with_max_reconnect_attempts(Some(3));

        let json = serde_json::to_string(&opts).unwrap();
        let parsed: ConnectionOptions = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.auto_reconnect, opts.auto_reconnect);
        assert_eq!(parsed.reconnect_delay_ms, opts.reconnect_delay_ms);
        assert_eq!(parsed.max_reconnect_delay_ms, opts.max_reconnect_delay_ms);
        assert_eq!(parsed.max_reconnect_attempts, opts.max_reconnect_attempts);
    }

    #[test]
    fn test_connection_options_deserialization_with_defaults() {
        // Test that missing fields get proper defaults
        let json = r#"{"auto_reconnect": false}"#;
        let opts: ConnectionOptions = serde_json::from_str(json).unwrap();

        assert!(!opts.auto_reconnect);
        assert_eq!(opts.reconnect_delay_ms, 1000); // default
        assert_eq!(opts.max_reconnect_delay_ms, 30000); // default
        assert!(opts.max_reconnect_attempts.is_none()); // default
    }

    // ==================== SubscriptionOptions Tests ====================

    #[test]
    fn test_subscription_options_default() {
        let opts = SubscriptionOptions::default();

        assert!(opts.batch_size.is_none(), "batch_size should default to None");
        assert!(opts.last_rows.is_none(), "last_rows should default to None");
        assert!(opts.from_seq_id.is_none(), "from_seq_id should default to None");
    }

    #[test]
    fn test_subscription_options_new() {
        let opts = SubscriptionOptions::new();

        // new() should be equivalent to default()
        assert!(opts.batch_size.is_none());
        assert!(opts.last_rows.is_none());
        assert!(opts.from_seq_id.is_none());
    }

    #[test]
    fn test_subscription_options_builder_pattern() {
        let seq_id = SeqId::from(12345i64);
        let opts = SubscriptionOptions::new()
            .with_batch_size(100)
            .with_last_rows(50)
            .with_from_seq_id(seq_id);

        assert_eq!(opts.batch_size, Some(100));
        assert_eq!(opts.last_rows, Some(50));
        assert!(opts.from_seq_id.is_some());
        assert_eq!(opts.from_seq_id.unwrap(), seq_id);
    }

    #[test]
    fn test_subscription_options_with_batch_size_only() {
        let opts = SubscriptionOptions::new().with_batch_size(500);

        assert_eq!(opts.batch_size, Some(500));
        assert!(opts.last_rows.is_none());
        assert!(opts.from_seq_id.is_none());
    }

    #[test]
    fn test_subscription_options_with_last_rows_only() {
        let opts = SubscriptionOptions::new().with_last_rows(25);

        assert!(opts.batch_size.is_none());
        assert_eq!(opts.last_rows, Some(25));
        assert!(opts.from_seq_id.is_none());
    }

    #[test]
    fn test_subscription_options_with_from_seq_id_only() {
        let seq_id = SeqId::from(99999i64);
        let opts = SubscriptionOptions::new().with_from_seq_id(seq_id);

        assert!(opts.batch_size.is_none());
        assert!(opts.last_rows.is_none());
        assert_eq!(opts.from_seq_id, Some(seq_id));
    }

    #[test]
    fn test_subscription_options_has_resume_seq_id() {
        let opts_without = SubscriptionOptions::new();
        assert!(!opts_without.has_resume_seq_id());

        let opts_with = SubscriptionOptions::new().with_from_seq_id(SeqId::from(123i64));
        assert!(opts_with.has_resume_seq_id());
    }

    #[test]
    fn test_subscription_options_serialization() {
        let opts = SubscriptionOptions::new()
            .with_batch_size(200)
            .with_last_rows(100);

        let json = serde_json::to_string(&opts).unwrap();

        // Verify JSON structure
        assert!(json.contains("\"batch_size\":200"));
        assert!(json.contains("\"last_rows\":100"));
        // from_seq_id should be skipped when None
        assert!(!json.contains("from_seq_id"));
    }

    #[test]
    fn test_subscription_options_serialization_with_seq_id() {
        let seq_id = SeqId::from(42i64);
        let opts = SubscriptionOptions::new()
            .with_batch_size(50)
            .with_from_seq_id(seq_id);

        let json = serde_json::to_string(&opts).unwrap();

        assert!(json.contains("\"batch_size\":50"));
        assert!(json.contains("from_seq_id"));
    }

    #[test]
    fn test_subscription_options_deserialization() {
        let json = r#"{"batch_size": 100, "last_rows": 50}"#;
        let opts: SubscriptionOptions = serde_json::from_str(json).unwrap();

        assert_eq!(opts.batch_size, Some(100));
        assert_eq!(opts.last_rows, Some(50));
        assert!(opts.from_seq_id.is_none());
    }

    #[test]
    fn test_subscription_options_deserialization_empty() {
        let json = r#"{}"#;
        let opts: SubscriptionOptions = serde_json::from_str(json).unwrap();

        assert!(opts.batch_size.is_none());
        assert!(opts.last_rows.is_none());
        assert!(opts.from_seq_id.is_none());
    }

    // ==================== Options Separation Tests ====================

    #[test]
    fn test_connection_and_subscription_options_are_independent() {
        // Ensure the two option types don't overlap in their fields
        let conn_opts = ConnectionOptions::new()
            .with_auto_reconnect(true)
            .with_reconnect_delay_ms(1000);

        let sub_opts = SubscriptionOptions::new()
            .with_batch_size(100)
            .with_last_rows(50);

        // Connection options should NOT have subscription fields
        let conn_json = serde_json::to_string(&conn_opts).unwrap();
        assert!(!conn_json.contains("batch_size"));
        assert!(!conn_json.contains("last_rows"));
        assert!(!conn_json.contains("from_seq_id"));

        // Subscription options should NOT have connection fields
        let sub_json = serde_json::to_string(&sub_opts).unwrap();
        assert!(!sub_json.contains("auto_reconnect"));
        assert!(!sub_json.contains("reconnect_delay"));
        assert!(!sub_json.contains("max_reconnect"));
    }

    // ==================== SubscriptionRequest Tests ====================

    #[test]
    fn test_subscription_request_with_options() {
        let opts = SubscriptionOptions::new()
            .with_batch_size(100)
            .with_last_rows(25);

        let request = SubscriptionRequest {
            id: "sub-123".to_string(),
            sql: "SELECT * FROM messages".to_string(),
            options: opts,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"id\":\"sub-123\""));
        assert!(json.contains("SELECT * FROM messages"));
        assert!(json.contains("\"batch_size\":100"));
        assert!(json.contains("\"last_rows\":25"));
    }

    #[test]
    fn test_subscription_request_with_default_options() {
        let request = SubscriptionRequest {
            id: "sub-456".to_string(),
            sql: "SELECT * FROM users".to_string(),
            options: SubscriptionOptions::default(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"id\":\"sub-456\""));
        // Default options with all None should serialize as empty object or with no optional fields
    }

    // ==================== ClientMessage Tests ====================

    #[test]
    fn test_client_message_authenticate_basic_serialization() {
        let msg = ClientMessage::Authenticate {
            credentials: WsAuthCredentials::Basic {
                username: "alice".to_string(),
                password: "secret123".to_string(),
            },
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"authenticate\""));
        assert!(json.contains("\"method\":\"basic\""));
        assert!(json.contains("\"username\":\"alice\""));
        assert!(json.contains("\"password\":\"secret123\""));
    }

    #[test]
    fn test_client_message_authenticate_jwt_serialization() {
        let msg = ClientMessage::Authenticate {
            credentials: WsAuthCredentials::Jwt {
                token: "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.test".to_string(),
            },
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"authenticate\""));
        assert!(json.contains("\"method\":\"jwt\""));
        assert!(json.contains("\"token\":\"eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.test\""));
    }

    #[test]
    fn test_client_message_subscribe_serialization() {
        let msg = ClientMessage::Subscribe {
            subscription: SubscriptionRequest {
                id: "test-sub".to_string(),
                sql: "SELECT * FROM chat.messages".to_string(),
                options: SubscriptionOptions::new().with_batch_size(50),
            },
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"subscribe\""));
        assert!(json.contains("\"id\":\"test-sub\""));
        assert!(json.contains("SELECT * FROM chat.messages"));
        assert!(json.contains("\"batch_size\":50"));
    }

    #[test]
    fn test_client_message_subscribe_with_resume() {
        let seq_id = SeqId::from(12345i64);
        let msg = ClientMessage::Subscribe {
            subscription: SubscriptionRequest {
                id: "resume-sub".to_string(),
                sql: "SELECT * FROM events".to_string(),
                options: SubscriptionOptions::new().with_from_seq_id(seq_id),
            },
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"subscribe\""));
        assert!(json.contains("from_seq_id"));
    }

    // ==================== BatchControl Tests ====================

    #[test]
    fn test_batch_control_with_seq_id() {
        let seq_id = SeqId::from(999i64);
        let batch_control = BatchControl {
            batch_num: 0,
            total_batches: Some(5),
            has_more: true,
            status: BatchStatus::Loading,
            last_seq_id: Some(seq_id),
            snapshot_end_seq: Some(SeqId::from(1000i64)),
        };

        let json = serde_json::to_string(&batch_control).unwrap();
        assert!(json.contains("\"batch_num\":0"));
        assert!(json.contains("\"has_more\":true"));
        assert!(json.contains("\"status\":\"loading\""));
        assert!(json.contains("last_seq_id"));
        assert!(json.contains("snapshot_end_seq"));
    }

    #[test]
    fn test_batch_control_ready_status() {
        let batch_control = BatchControl {
            batch_num: 5,
            total_batches: Some(5),
            has_more: false,
            status: BatchStatus::Ready,
            last_seq_id: Some(SeqId::from(1000i64)),
            snapshot_end_seq: Some(SeqId::from(1000i64)),
        };

        let json = serde_json::to_string(&batch_control).unwrap();
        assert!(json.contains("\"status\":\"ready\""));
        assert!(json.contains("\"has_more\":false"));
    }

    #[test]
    fn test_batch_status_serialization() {
        // Test all BatchStatus variants
        let loading = BatchStatus::Loading;
        let loading_batch = BatchStatus::LoadingBatch;
        let ready = BatchStatus::Ready;

        assert_eq!(
            serde_json::to_string(&loading).unwrap(),
            "\"loading\""
        );
        assert_eq!(
            serde_json::to_string(&loading_batch).unwrap(),
            "\"loading_batch\""
        );
        assert_eq!(
            serde_json::to_string(&ready).unwrap(),
            "\"ready\""
        );
    }

    // ==================== ServerMessage Parsing Tests ====================

    #[test]
    fn test_server_message_initial_data_batch_parsing() {
        let json = r#"{
            "type": "initial_data_batch",
            "subscription_id": "sub-123",
            "rows": [{"id": 1, "name": "test"}],
            "batch_control": {
                "batch_num": 0,
                "has_more": true,
                "status": "loading",
                "last_seq_id": 12345
            }
        }"#;

        let msg: ServerMessage = serde_json::from_str(json).unwrap();
        match msg {
            ServerMessage::InitialDataBatch {
                subscription_id,
                rows,
                batch_control,
            } => {
                assert_eq!(subscription_id, "sub-123");
                assert_eq!(rows.len(), 1);
                assert!(batch_control.has_more);
                assert_eq!(batch_control.last_seq_id, Some(SeqId::from(12345i64)));
            }
            _ => panic!("Expected InitialDataBatch"),
        }
    }

    #[test]
    fn test_server_message_change_parsing() {
        let json = r#"{
            "type": "change",
            "subscription_id": "sub-456",
            "change_type": "insert",
            "rows": [{"id": 2, "content": "new message"}]
        }"#;

        let msg: ServerMessage = serde_json::from_str(json).unwrap();
        match msg {
            ServerMessage::Change {
                subscription_id,
                change_type,
                rows,
                old_values,
            } => {
                assert_eq!(subscription_id, "sub-456");
                assert_eq!(change_type, ChangeTypeRaw::Insert);
                assert!(rows.is_some());
                assert!(old_values.is_none());
            }
            _ => panic!("Expected Change"),
        }
    }

    // ==================== Reconnection Scenario Tests ====================

    #[test]
    fn test_subscription_options_for_reconnection() {
        // Simulate what happens during reconnection:
        // 1. Start with default options
        let initial_opts = SubscriptionOptions::new().with_batch_size(100);
        assert!(!initial_opts.has_resume_seq_id());

        // 2. After receiving data, we have a last_seq_id
        let last_received_seq = SeqId::from(54321i64);

        // 3. On reconnect, we create new options with the resume seq_id
        let reconnect_opts = SubscriptionOptions::new()
            .with_batch_size(initial_opts.batch_size.unwrap_or(100))
            .with_from_seq_id(last_received_seq);

        assert!(reconnect_opts.has_resume_seq_id());
        assert_eq!(reconnect_opts.from_seq_id, Some(last_received_seq));
        assert_eq!(reconnect_opts.batch_size, Some(100));
    }

    #[test]
    fn test_connection_options_exponential_backoff_calculation() {
        let opts = ConnectionOptions::new()
            .with_reconnect_delay_ms(1000)
            .with_max_reconnect_delay_ms(30000);

        // Simulate exponential backoff calculation
        let base_delay = opts.reconnect_delay_ms;
        let max_delay = opts.max_reconnect_delay_ms;

        // Attempt 0: 1000ms
        let delay_0 = std::cmp::min(base_delay * 2u64.pow(0), max_delay);
        assert_eq!(delay_0, 1000);

        // Attempt 1: 2000ms
        let delay_1 = std::cmp::min(base_delay * 2u64.pow(1), max_delay);
        assert_eq!(delay_1, 2000);

        // Attempt 2: 4000ms
        let delay_2 = std::cmp::min(base_delay * 2u64.pow(2), max_delay);
        assert_eq!(delay_2, 4000);

        // Attempt 5: 32000ms -> capped at 30000ms
        let delay_5 = std::cmp::min(base_delay * 2u64.pow(5), max_delay);
        assert_eq!(delay_5, 30000);
    }

    // ==================== Original Tests ====================

    #[test]
    fn test_query_request_serialization() {
        let request = QueryRequest {
            sql: "SELECT * FROM users WHERE id = $1".to_string(),
            params: Some(vec![json!(42)]),
            namespace_id: None,
            serialization_mode: SerializationMode::Typed,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("SELECT * FROM users"));
        assert!(json.contains("params"));
        assert!(json.contains("typed")); // serialization_mode is typed by default
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
