//! Topic offset entity for system.topic_offsets table.
//!
//! Tracks consumer group progress through topic partitions.

use bincode::{Decode, Encode};
use kalamdb_commons::datatypes::KalamDataType;
use kalamdb_commons::models::{ConsumerGroupId, TopicId};
use kalamdb_commons::KSerializable;
use kalamdb_macros::table;
use serde::{Deserialize, Serialize};

/// Topic offset entity for system.topic_offsets table.
///
/// Tracks consumer group progress through topic partitions, enabling
/// at-least-once delivery semantics and replay capabilities.
///
/// ## Fields
/// - `topic_id`: Topic identifier (part of composite primary key)
/// - `group_id`: Consumer group identifier (part of composite primary key)
/// - `partition_id`: Partition identifier (part of composite primary key)
/// - `last_acked_offset`: Last successfully acknowledged offset
/// - `updated_at`: Unix timestamp in milliseconds when offset was last updated
///
/// ## Primary Key
/// Composite key of (`topic_id`, `group_id`, `partition_id`)
///
/// ## Semantics
/// - Messages with offset <= `last_acked_offset` are considered consumed
/// - Next CONSUME will start from `last_acked_offset + 1`
/// - Consumer groups maintain independent progress (isolation)
/// - Partition-level offsets enable parallel consumption within a group
///
/// ## Example
/// ```rust,ignore
/// use kalamdb_system::providers::topic_offsets::models::TopicOffset;
/// use kalamdb_commons::models::{TopicId, ConsumerGroupId};
///
/// let offset = TopicOffset {
///     topic_id: TopicId::new("topic_123"),
///     group_id: ConsumerGroupId::new("ai-service"),
///     partition_id: 0,
///     last_acked_offset: 42,
///     updated_at: 1730000000000,
/// };
///
/// // After consuming and acknowledging through offset 50
/// let updated = offset.ack(50);
/// assert_eq!(updated.last_acked_offset, 50);
/// ```
#[table(
    name = "topic_offsets",
    comment = "Consumer group offset tracking for topics"
)]
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug, PartialEq, Eq)]
pub struct TopicOffset {
    // Composite primary key fields first
    #[column(
        id = 1,
        ordinal = 1,
        data_type(KalamDataType::Text),
        nullable = false,
        primary_key = true,
        default = "None",
        comment = "Topic identifier"
    )]
    pub topic_id: TopicId,

    #[column(
        id = 2,
        ordinal = 2,
        data_type(KalamDataType::Text),
        nullable = false,
        primary_key = true,
        default = "None",
        comment = "Consumer group identifier"
    )]
    pub group_id: ConsumerGroupId,

    #[column(
        id = 3,
        ordinal = 3,
        data_type(KalamDataType::Int),
        nullable = false,
        primary_key = true,
        default = "None",
        comment = "Partition identifier"
    )]
    pub partition_id: u32,

    // Data fields (8-byte aligned first)
    #[column(
        id = 4,
        ordinal = 4,
        data_type(KalamDataType::BigInt),
        nullable = false,
        primary_key = false,
        default = "None",
        comment = "Last successfully acknowledged offset"
    )]
    pub last_acked_offset: u64,

    #[column(
        id = 5,
        ordinal = 5,
        data_type(KalamDataType::Timestamp),
        nullable = false,
        primary_key = false,
        default = "None",
        comment = "Unix timestamp in milliseconds when offset was last updated"
    )]
    pub updated_at: i64,
}

impl TopicOffset {
    /// Creates a new topic offset at position 0 (beginning of partition).
    pub fn new(topic_id: TopicId, group_id: ConsumerGroupId, partition_id: u32) -> Self {
        Self {
            topic_id,
            group_id,
            partition_id,
            last_acked_offset: 0,
            updated_at: chrono::Utc::now().timestamp_millis(),
        }
    }

    /// Creates a TopicOffset starting at a specific offset.
    pub fn at_offset(
        topic_id: TopicId,
        group_id: ConsumerGroupId,
        partition_id: u32,
        offset: u64,
    ) -> Self {
        Self {
            topic_id,
            group_id,
            partition_id,
            last_acked_offset: offset,
            updated_at: chrono::Utc::now().timestamp_millis(),
        }
    }

    /// Acknowledges consumption through the specified offset.
    ///
    /// Updates `last_acked_offset` to the provided value and refreshes timestamp.
    /// The next CONSUME will start from `offset + 1`.
    pub fn ack(&mut self, offset: u64) {
        self.last_acked_offset = offset;
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }

    /// Returns the next offset to consume (last_acked_offset + 1).
    pub fn next_offset(&self) -> u64 {
        self.last_acked_offset + 1
    }

    /// Returns whether this offset has progressed beyond the initial position.
    pub fn has_consumed(&self) -> bool {
        self.last_acked_offset > 0
    }

    /// Returns a composite key string for display/debugging.
    pub fn composite_key(&self) -> String {
        format!("{}:{}:{}", self.topic_id, self.group_id, self.partition_id)
    }
}

impl KSerializable for TopicOffset {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topic_offset_creation() {
        let offset = TopicOffset::new(
            TopicId::new("topic_123"),
            ConsumerGroupId::new("ai-service"),
            0,
        );

        assert_eq!(offset.last_acked_offset, 0);
        assert_eq!(offset.next_offset(), 1);
        assert!(!offset.has_consumed());
    }

    #[test]
    fn test_topic_offset_at_offset() {
        let offset = TopicOffset::at_offset(
            TopicId::new("topic_123"),
            ConsumerGroupId::new("analytics"),
            0,
            42,
        );

        assert_eq!(offset.last_acked_offset, 42);
        assert_eq!(offset.next_offset(), 43);
        assert!(offset.has_consumed());
    }

    #[test]
    fn test_topic_offset_ack() {
        let mut offset = TopicOffset::new(
            TopicId::new("topic_123"),
            ConsumerGroupId::new("service"),
            0,
        );

        offset.ack(10);
        assert_eq!(offset.last_acked_offset, 10);
        assert_eq!(offset.next_offset(), 11);

        offset.ack(20);
        assert_eq!(offset.last_acked_offset, 20);
        assert_eq!(offset.next_offset(), 21);
    }

    #[test]
    fn test_topic_offset_composite_key() {
        let offset = TopicOffset::new(
            TopicId::new("app.events"),
            ConsumerGroupId::new("worker-1"),
            3,
        );

        assert_eq!(offset.composite_key(), "app.events:worker-1:3");
    }

    #[test]
    fn test_topic_offset_serialization() {
        let offset = TopicOffset::at_offset(
            TopicId::new("topic_789"),
            ConsumerGroupId::new("test-group"),
            2,
            100,
        );

        // Test bincode round-trip
        let encoded = bincode::encode_to_vec(&offset, bincode::config::standard()).unwrap();
        let decoded: TopicOffset = bincode::decode_from_slice(&encoded, bincode::config::standard())
            .unwrap()
            .0;
        assert_eq!(offset, decoded);
    }
}
