//! Topic offset models - ID and entity definitions
//!
//! This module defines the data structures for consumer group offset tracking:
//! - TopicOffsetId: Composite key for offset identification
//! - TopicOffset: Offset tracking with committed/pending positions

use kalamdb_commons::models::{ConsumerGroupId, TopicId};
use kalamdb_commons::{encode_key, decode_key, KSerializable, StorageKey};
use serde::{Deserialize, Serialize};

/// Composite key for topic offsets: topic_id + group_id + partition_id
///
/// Uses order-preserving composite key encoding for efficient range scans.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TopicOffsetId {
    pub topic_id: TopicId,
    pub group_id: ConsumerGroupId,
    pub partition_id: u32,
}

impl TopicOffsetId {
    /// Create a new topic offset ID
    pub fn new(topic_id: TopicId, group_id: ConsumerGroupId, partition_id: u32) -> Self {
        Self {
            topic_id,
            group_id,
            partition_id,
        }
    }
}

impl StorageKey for TopicOffsetId {
    fn storage_key(&self) -> Vec<u8> {
        // Use composite key encoding: (topic_id, group_id, partition_id)
        // This enables efficient prefix scans by topic or topic+group
        encode_key(&(
            self.topic_id.as_str(),
            self.group_id.as_str(),
            self.partition_id,
        ))
    }

    fn from_storage_key(bytes: &[u8]) -> Result<Self, String> {
        let (topic_str, group_str, partition_id): (String, String, u32) = decode_key(bytes)?;
        Ok(Self::new(
            TopicId::from(topic_str.as_str()),
            ConsumerGroupId::from(group_str.as_str()),
            partition_id,
        ))
    }
}

/// Topic offset tracking for consumer groups
///
/// **Offset Semantics**:
/// - committed_offset: Last successfully processed message offset
/// - pending_offset: Next offset to fetch (may be ahead of committed)
/// - Consumers commit offsets after processing messages
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[derive(bincode::Encode, bincode::Decode)]
pub struct TopicOffset {
    /// Topic identifier
    pub topic_id: TopicId,
    /// Consumer group identifier
    pub group_id: ConsumerGroupId,
    /// Partition ID (0-based)
    pub partition_id: u32,
    /// Last committed offset (successfully processed)
    pub committed_offset: u64,
    /// Pending offset to fetch next (may be ahead of committed)
    pub pending_offset: u64,
    /// Timestamp of last commit (milliseconds since epoch)
    pub last_commit_ms: i64,
}

impl TopicOffset {
    /// Create a new topic offset
    pub fn new(
        topic_id: TopicId,
        group_id: ConsumerGroupId,
        partition_id: u32,
        committed_offset: u64,
        pending_offset: u64,
        last_commit_ms: i64,
    ) -> Self {
        Self {
            topic_id,
            group_id,
            partition_id,
            committed_offset,
            pending_offset,
            last_commit_ms,
        }
    }

    /// Get the composite ID for this offset
    pub fn id(&self) -> TopicOffsetId {
        TopicOffsetId::new(
            self.topic_id.clone(),
            self.group_id.clone(),
            self.partition_id,
        )
    }

    /// Acknowledge (commit) an offset after processing
    pub fn ack(&mut self, offset: u64, timestamp_ms: i64) {
        self.committed_offset = offset;
        self.last_commit_ms = timestamp_ms;
        // Advance pending if needed
        if self.pending_offset <= offset {
            self.pending_offset = offset + 1;
        }
    }

    /// Get the next offset to fetch (pending_offset)
    pub fn next_offset(&self) -> u64 {
        self.pending_offset
    }
}

impl KSerializable for TopicOffset {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_offset_id_storage_key() {
        let offset_id = TopicOffsetId::new(
            TopicId::from("my_topic"),
            ConsumerGroupId::from("my_group"),
            3,
        );
        let key = offset_id.storage_key();
        
        // Should be able to decode it back
        let decoded = TopicOffsetId::from_storage_key(&key).unwrap();
        assert_eq!(decoded, offset_id);
    }

    #[test]
    fn test_offset_id_ordering() {
        // Keys should be ordered by topic, then group, then partition
        let key1 = TopicOffsetId::new(
            TopicId::from("topic_a"),
            ConsumerGroupId::from("group1"),
            0,
        ).storage_key();
        let key2 = TopicOffsetId::new(
            TopicId::from("topic_a"),
            ConsumerGroupId::from("group1"),
            1,
        ).storage_key();
        let key3 = TopicOffsetId::new(
            TopicId::from("topic_a"),
            ConsumerGroupId::from("group2"),
            0,
        ).storage_key();
        let key4 = TopicOffsetId::new(
            TopicId::from("topic_b"),
            ConsumerGroupId::from("group1"),
            0,
        ).storage_key();
        
        assert!(key1 < key2, "Partition ordering within same topic+group");
        assert!(key2 < key3, "Group ordering within same topic");
        assert!(key3 < key4, "Topic ordering");
    }

    #[test]
    fn test_topic_offset_creation() {
        let offset = TopicOffset::new(
            TopicId::from("test_topic"),
            ConsumerGroupId::from("group1"),
            0,
            10,
            11,
            1706745600000,
        );
        
        assert_eq!(offset.topic_id, TopicId::from("test_topic"));
        assert_eq!(offset.group_id, ConsumerGroupId::from("group1"));
        assert_eq!(offset.partition_id, 0);
        assert_eq!(offset.committed_offset, 10);
        assert_eq!(offset.pending_offset, 11);
    }

    #[test]
    fn test_offset_ack_logic() {
        let mut offset = TopicOffset::new(
            TopicId::from("test"),
            ConsumerGroupId::from("group1"),
            0,
            0,
            0,
            1000,
        );

        // Ack offset 5
        offset.ack(5, 2000);
        assert_eq!(offset.committed_offset, 5);
        assert_eq!(offset.pending_offset, 6);
        assert_eq!(offset.next_offset(), 6);

        // Ack offset 3 (older) - pending should not go backwards
        offset.ack(3, 3000);
        assert_eq!(offset.committed_offset, 3);
        assert_eq!(offset.pending_offset, 6); // Stays at 6
    }

    #[test]
    fn test_offset_id_extraction() {
        let offset = TopicOffset::new(
            TopicId::from("test"),
            ConsumerGroupId::from("group1"),
            5,
            10,
            11,
            0,
        );
        
        let id = offset.id();
        assert_eq!(id.topic_id, TopicId::from("test"));
        assert_eq!(id.group_id, ConsumerGroupId::from("group1"));
        assert_eq!(id.partition_id, 5);
    }
}
