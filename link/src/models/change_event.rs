use serde_json::Value as JsonValue;
use std::collections::HashMap;

use super::batch_control::BatchControl;
use super::schema_field::SchemaField;

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
        /// Schema describing the columns in the subscription result
        schema: Vec<SchemaField>,
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
            Self::Ack { subscription_id, .. }
            | Self::InitialDataBatch { subscription_id, .. }
            | Self::Insert { subscription_id, .. }
            | Self::Update { subscription_id, .. }
            | Self::Delete { subscription_id, .. }
            | Self::Error { subscription_id, .. } => Some(subscription_id.as_str()),
            Self::Unknown { .. } => None,
        }
    }
}
