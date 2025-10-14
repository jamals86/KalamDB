// Message model
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Represents a single message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    /// Unique message ID (Snowflake ID for time-ordered uniqueness)
    pub msg_id: i64,
    
    /// Conversation this message belongs to
    pub conversation_id: String,
    
    /// Sender user ID (can be AI, user's own ID, or another user)
    pub from: String,
    
    /// Message timestamp in microseconds since Unix epoch (UTC)
    pub timestamp: i64,
    
    /// Message content/text
    pub content: String,
    
    /// Flexible metadata as JSON (role, model, tokens, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<JsonValue>,
}

impl Message {
    /// Create a new message with the given fields
    pub fn new(
        msg_id: i64,
        conversation_id: String,
        from: String,
        timestamp: i64,
        content: String,
        metadata: Option<JsonValue>,
    ) -> Self {
        Self {
            msg_id,
            conversation_id,
            from,
            timestamp,
            content,
            metadata,
        }
    }

    /// Create a new message with current timestamp
    pub fn new_now(
        msg_id: i64,
        conversation_id: String,
        from: String,
        content: String,
        metadata: Option<JsonValue>,
    ) -> Self {
        let timestamp = Utc::now().timestamp_micros();
        Self::new(msg_id, conversation_id, from, timestamp, content, metadata)
    }

    /// Get timestamp as DateTime
    pub fn timestamp_as_datetime(&self) -> DateTime<Utc> {
        DateTime::from_timestamp_micros(self.timestamp)
            .unwrap_or_else(|| DateTime::UNIX_EPOCH)
    }

    /// Validate message fields
    pub fn validate(&self, max_size: usize) -> Result<(), String> {
        if self.conversation_id.is_empty() {
            return Err("conversation_id cannot be empty".to_string());
        }

        if self.from.is_empty() {
            return Err("from cannot be empty".to_string());
        }

        if self.content.len() > max_size {
            return Err(format!(
                "content size ({} bytes) exceeds maximum ({} bytes)",
                self.content.len(),
                max_size
            ));
        }

        Ok(())
    }

    /// Serialize to JSON bytes
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    /// Deserialize from JSON bytes
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_message_creation() {
        let msg = Message::new(
            123456789,
            "conv_test".to_string(),
            "user_alice".to_string(),
            1699999999000000,
            "Hello, world!".to_string(),
            Some(json!({"role": "user"})),
        );

        assert_eq!(msg.msg_id, 123456789);
        assert_eq!(msg.conversation_id, "conv_test");
        assert_eq!(msg.from, "user_alice");
        assert_eq!(msg.content, "Hello, world!");
        assert!(msg.metadata.is_some());
    }

    #[test]
    fn test_message_serialization() {
        let msg = Message::new(
            123456789,
            "conv_test".to_string(),
            "user_alice".to_string(),
            1699999999000000,
            "Hello, world!".to_string(),
            Some(json!({"role": "user"})),
        );

        let json_bytes = msg.to_json_bytes().unwrap();
        let deserialized = Message::from_json_bytes(&json_bytes).unwrap();

        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_message_validation() {
        let msg = Message::new(
            123456789,
            "conv_test".to_string(),
            "user_alice".to_string(),
            1699999999000000,
            "Hello, world!".to_string(),
            None,
        );

        assert!(msg.validate(1048576).is_ok());
    }

    #[test]
    fn test_message_validation_empty_conversation_id() {
        let msg = Message::new(
            123456789,
            "".to_string(),
            "user_alice".to_string(),
            1699999999000000,
            "Hello, world!".to_string(),
            None,
        );

        assert!(msg.validate(1048576).is_err());
    }

    #[test]
    fn test_message_validation_empty_from() {
        let msg = Message::new(
            123456789,
            "conv_test".to_string(),
            "".to_string(),
            1699999999000000,
            "Hello, world!".to_string(),
            None,
        );

        assert!(msg.validate(1048576).is_err());
    }

    #[test]
    fn test_message_validation_content_too_large() {
        let large_content = "x".repeat(2_000_000);
        let msg = Message::new(
            123456789,
            "conv_test".to_string(),
            "user_alice".to_string(),
            1699999999000000,
            large_content,
            None,
        );

        assert!(msg.validate(1048576).is_err());
    }

    #[test]
    fn test_message_without_metadata() {
        let msg = Message::new(
            123456789,
            "conv_test".to_string(),
            "user_alice".to_string(),
            1699999999000000,
            "Hello, world!".to_string(),
            None,
        );

        let json_bytes = msg.to_json_bytes().unwrap();
        let json_str = String::from_utf8(json_bytes).unwrap();
        
        // metadata should not be in JSON when None
        assert!(!json_str.contains("metadata"));
    }
}
