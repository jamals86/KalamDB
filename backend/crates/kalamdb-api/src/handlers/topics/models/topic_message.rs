//! Topic message model

use kalamdb_commons::models::TopicId;
use serde::Serialize;

/// Message in consume response
#[derive(Debug, Serialize)]
pub struct TopicMessage {
    /// Topic identifier (type-safe)
    pub topic_id: TopicId,
    /// Partition ID
    pub partition_id: u32,
    /// Message offset
    pub offset: u64,
    /// Base64-encoded payload bytes
    pub payload: String,
    /// Optional message key
    pub key: Option<String>,
    /// Timestamp in milliseconds since epoch
    pub timestamp_ms: i64,
}
