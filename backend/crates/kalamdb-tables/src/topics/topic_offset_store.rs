//! Topic offset store implementation using EntityStore pattern
//!
//! This module provides EntityStore-based storage for consumer group offsets.
//! Offsets track the last consumed message position for each consumer group.
//!
//! **Storage Architecture**:
//! - TopicOffsetId: Composite key (topic_id, group_id, partition_id)
//! - TopicOffset: Offset position with committed/pending offsets
//! - Storage key format: composite encoding for efficient filtering
//! - Efficient per-group and per-partition queries

use crate::common::{ensure_partition, partition_name};
use crate::topics::topic_offset_models::{TopicOffset, TopicOffsetId};
use kalamdb_commons::models::{ConsumerGroupId, TopicId};
use kalamdb_commons::storage::Partition;
use kalamdb_store::{EntityStore, StorageBackend};
use std::sync::Arc;

/// Store for topic offsets (consumer group offset tracking)
///
/// Uses composite TopicOffsetId keys for efficient group/partition queries.
/// Offsets are mutable (updated as consumers process messages).
#[derive(Clone)]
pub struct TopicOffsetStore {
    backend: Arc<dyn StorageBackend>,
    partition: Partition,
}

impl TopicOffsetStore {
    /// Create a new topic offset store
    ///
    /// # Arguments
    /// * `backend` - Storage backend (RocksDB or mock)
    /// * `partition` - Partition name (e.g., "topic_offsets")
    pub fn new(backend: Arc<dyn StorageBackend>, partition: impl Into<Partition>) -> Self {
        Self {
            backend,
            partition: partition.into(),
        }
    }

    /// Upsert (insert or update) an offset for a consumer group
    pub fn upsert_offset(
        &self,
        offset: TopicOffset,
    ) -> kalamdb_store::storage_trait::Result<()> {
        let offset_id = offset.id();
        self.put(&offset_id, &offset)
    }

    /// Acknowledge (commit) an offset after processing
    pub fn ack_offset(
        &self,
        topic_id: &TopicId,
        group_id: &ConsumerGroupId,
        partition_id: u32,
        offset: u64,
        timestamp_ms: i64,
    ) -> kalamdb_store::storage_trait::Result<()> {
        let offset_id = TopicOffsetId::new(topic_id.clone(), group_id.clone(), partition_id);
        
        // Get existing offset or create new one
        let mut topic_offset = match self.get(&offset_id)? {
            Some(off) => off,
            None => TopicOffset::new(
                topic_id.clone(),
                group_id.clone(),
                partition_id,
                0,
                0,
                timestamp_ms,
            ),
        };

        // Acknowledge
        topic_offset.ack(offset, timestamp_ms);
        
        // Update
        self.put(&offset_id, &topic_offset)
    }

    /// Get all offsets for a topic and consumer group across all partitions
    pub fn get_group_offsets(
        &self,
        topic_id: &TopicId,
        group_id: &ConsumerGroupId,
    ) -> kalamdb_store::storage_trait::Result<Vec<TopicOffset>> {
        let all_offsets = self.scan_all_typed(None, None, None)?;
        let filtered: Vec<TopicOffset> = all_offsets
            .into_iter()
            .filter(|(id, _)| id.topic_id == *topic_id && id.group_id == *group_id)
            .map(|(_, offset)| offset)
            .collect();
        Ok(filtered)
    }

    /// Get all offsets for a topic across all consumer groups
    pub fn get_topic_offsets(
        &self,
        topic_id: &TopicId,
    ) -> kalamdb_store::storage_trait::Result<Vec<TopicOffset>> {
        let all_offsets = self.scan_all_typed(None, None, None)?;
        let filtered: Vec<TopicOffset> = all_offsets
            .into_iter()
            .filter(|(id, _)| id.topic_id == *topic_id)
            .map(|(_, offset)| offset)
            .collect();
        Ok(filtered)
    }
}

/// Implement EntityStore trait for typed CRUD operations
impl EntityStore<TopicOffsetId, TopicOffset> for TopicOffsetStore {
    fn backend(&self) -> &Arc<dyn StorageBackend> {
        &self.backend
    }

    fn partition(&self) -> Partition {
        self.partition.clone()
    }
}

/// Helper function to create a new topic offset store with partition initialization
pub fn new_topic_offset_store(
    backend: Arc<dyn StorageBackend>,
    table_id: &kalamdb_commons::TableId,
) -> TopicOffsetStore {
    let partition_name = partition_name("topic_offsets", table_id);
    let partition = Partition::new(partition_name.clone());
    ensure_partition(&backend, partition_name);
    TopicOffsetStore::new(backend, partition)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::StorageKey;
    use kalamdb_store::test_utils::InMemoryBackend;

    fn setup_test_store() -> TopicOffsetStore {
        let backend = Arc::new(InMemoryBackend::new());
        let partition = Partition::new("test_topic_offsets");
        TopicOffsetStore::new(backend, partition)
    }

    #[test]
    fn test_upsert_and_get() {
        let store = setup_test_store();
        let topic_id = TopicId::from("test_topic");
        let group_id = ConsumerGroupId::from("group1");
        let partition_id = 0;

        let offset = TopicOffset::new(topic_id.clone(), group_id.clone(), partition_id, 10, 11, 1000);
        store.upsert_offset(offset.clone()).unwrap();

        let offset_id = TopicOffsetId::new(topic_id, group_id, partition_id);
        let retrieved = store.get(&offset_id).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), offset);
    }

    #[test]
    fn test_ack_offset() {
        let store = setup_test_store();
        let topic_id = TopicId::from("test_topic");
        let group_id = ConsumerGroupId::from("group1");
        let partition_id = 0;

        // Initial offset
        let offset = TopicOffset::new(topic_id.clone(), group_id.clone(), partition_id, 0, 0, 1000);
        store.upsert_offset(offset).unwrap();

        // Acknowledge offset 5
        store
            .ack_offset(&topic_id, &group_id, partition_id, 5, 2000)
            .unwrap();

        // Verify
        let offset_id = TopicOffsetId::new(topic_id, group_id, partition_id);
        let retrieved = store.get(&offset_id).unwrap().unwrap();
        assert_eq!(retrieved.committed_offset, 5);
        assert_eq!(retrieved.pending_offset, 6);
        assert_eq!(retrieved.last_commit_ms, 2000);
    }

    #[test]
    fn test_get_group_offsets() {
        let store = setup_test_store();
        let topic_id = TopicId::from("test_topic");
        let group_id = ConsumerGroupId::from("group1");

        // Insert offsets for multiple partitions
        for partition_id in 0..3 {
            let offset = TopicOffset::new(
                topic_id.clone(),
                group_id.clone(),
                partition_id,
                partition_id as u64 * 10,
                partition_id as u64 * 10 + 1,
                1000,
            );
            store.upsert_offset(offset).unwrap();
        }

        // Get all offsets for this group
        let offsets = store.get_group_offsets(&topic_id, &group_id).unwrap();
        assert_eq!(offsets.len(), 3);
    }

    #[test]
    fn test_get_topic_offsets() {
        let store = setup_test_store();
        let topic_id = TopicId::from("test_topic");

        // Insert offsets for multiple groups
        for i in 0..2 {
            let group_id = ConsumerGroupId::from(format!("group{}", i));
            let offset = TopicOffset::new(topic_id.clone(), group_id, 0, i as u64 * 10, i as u64 * 10 + 1, 1000);
            store.upsert_offset(offset).unwrap();
        }

        // Get all offsets for this topic
        let offsets = store.get_topic_offsets(&topic_id).unwrap();
        assert_eq!(offsets.len(), 2);
    }

    #[test]
    fn test_storage_key_roundtrip() {
        let offset_id = TopicOffsetId::new(
            TopicId::from("my_topic"),
            ConsumerGroupId::from("my_group"),
            3,
        );
        let key = offset_id.storage_key();
        let decoded = TopicOffsetId::from_storage_key(&key).unwrap();
        assert_eq!(decoded, offset_id);
    }
}
