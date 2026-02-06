use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// A single consumed message from a topic.
///
/// Contains the message payload (decoded from base64), metadata about the
/// source operation, and positioning information for acknowledgment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify_next::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct ConsumeMessage {
    /// Unique message identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,

    /// Source table that produced this message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_table: Option<String>,

    /// Operation type: "insert", "update", or "delete"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub op: Option<String>,

    /// Message timestamp in milliseconds since epoch
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_ms: Option<u64>,

    /// Message offset in the partition (used for acknowledgment)
    pub offset: u64,

    /// Partition this message belongs to
    pub partition_id: u32,

    /// Topic this message belongs to
    pub topic: String,

    /// Consumer group ID
    pub group_id: String,

    /// Decoded message payload as a JSON object
    pub value: JsonValue,
}
