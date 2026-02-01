//! Topic message store implementation using EntityStore pattern
//!
//! This module provides EntityStore-based storage for pub/sub topic messages.
//! Messages are stored with composite keys for efficient range queries.
//!
//! **Storage Architecture**:
//! - TopicMessageId: Composite key (topic_id, partition_id, offset)
//! - TopicMessage: Message envelope with payload, key, timestamp
//! - Storage key format: composite encoding for efficient filtering
//! - Efficient range scans for message fetching

use crate::common::{ensure_partition, partition_name};
use crate::topics::topic_message_models::{TopicMessage, TopicMessageId};
use kalamdb_commons::models::TopicId;
use kalamdb_commons::storage::Partition;
use kalamdb_store::{EntityStore, StorageBackend};
use std::sync::Arc;

/// Store for topic messages (append-only message log)
///
/// Uses composite TopicMessageId keys for efficient range queries.
/// Messages are immutable once written.
#[derive(Clone)]
pub struct TopicMessageStore {
    backend: Arc<dyn StorageBackend>,
    partition: Partition,
}

impl TopicMessageStore {
    /// Create a new topic message store
    ///
    /// # Arguments
    /// * `backend` - Storage backend (RocksDB or mock)
    /// * `partition` - Partition name (e.g., "topic_messages")
    pub fn new(backend: Arc<dyn StorageBackend>, partition: impl Into<Partition>) -> Self {
        Self {
            backend,
            partition: partition.into(),
        }
    }

    /// Publish a message to a topic partition, returning the assigned offset
    ///
    /// This method:
    /// 1. Gets the next offset from counter
    /// 2. Creates the message with assigned offset
    /// 3. Stores the message
    ///
    /// Note: Offset management should be coordinated externally
    pub fn publish(
        &self,
        topic_id: &TopicId,
        partition_id: u32,
        offset: u64,
        payload: Vec<u8>,
        key: Option<String>,
        timestamp_ms: i64,
    ) -> kalamdb_store::storage_trait::Result<()> {
        let message = TopicMessage::new(
            topic_id.clone(),
            partition_id,
            offset,
            payload,
            key,
            timestamp_ms,
        );
        let msg_id = message.id();
        self.put(&msg_id, &message)
    }

    /// Fetch messages from a partition starting at `offset`, up to `limit` messages
    ///
    /// Returns messages in order: [offset, offset+1, offset+2, ...]
    pub fn fetch_messages(
        &self,
        topic_id: &TopicId,
        partition_id: u32,
        offset: u64,
        limit: usize,
    ) -> kalamdb_store::storage_trait::Result<Vec<TopicMessage>> {
        // Create start key
        let start_id = TopicMessageId::new(topic_id.clone(), partition_id, offset);
        
        // Scan with prefix matching topic and partition
        let results = self.scan_all_typed(Some(limit), None, Some(&start_id))?;
        
        // Filter to only messages from this topic+partition
        let messages: Vec<TopicMessage> = results
            .into_iter()
            .filter(|(id, _)| {
                id.topic_id == *topic_id && id.partition_id == partition_id
            })
            .map(|(_, msg)| msg)
            .collect();
        
        Ok(messages)
    }
}

/// Implement EntityStore trait for typed CRUD operations
impl EntityStore<TopicMessageId, TopicMessage> for TopicMessageStore {
    fn backend(&self) -> &Arc<dyn StorageBackend> {
        &self.backend
    }

    fn partition(&self) -> Partition {
        self.partition.clone()
    }
}

/// Helper function to create a new topic message store with partition initialization
pub fn new_topic_message_store(
    backend: Arc<dyn StorageBackend>,
    table_id: &kalamdb_commons::TableId,
) -> TopicMessageStore {
    let partition_name = partition_name("topic_messages", table_id);
    let partition = Partition::new(partition_name.clone());
    ensure_partition(&backend, partition_name);
    TopicMessageStore::new(backend, partition)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::StorageKey;
    use kalamdb_store::test_utils::InMemoryBackend;

    fn setup_test_store() -> TopicMessageStore {
        let backend = Arc::new(InMemoryBackend::new());
        let partition = Partition::new("test_topic_messages");
        TopicMessageStore::new(backend, partition)
    }

    #[test]
    fn test_publish_and_fetch() {
        let store = setup_test_store();
        let topic_id = TopicId::from("test_topic");
        let partition_id = 0;

        // Publish messages
        store
            .publish(&topic_id, partition_id, 0, b"message1".to_vec(), None, 1000)
            .unwrap();
        store
            .publish(
                &topic_id,
                partition_id,
                1,
                b"message2".to_vec(),
                Some("key1".to_string()),
                2000,
            )
            .unwrap();

        // Fetch messages
        let messages = store.fetch_messages(&topic_id, partition_id, 0, 10).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].payload, b"message1");
        assert_eq!(messages[1].payload, b"message2");
        assert_eq!(messages[1].key, Some("key1".to_string()));
    }

    #[test]
    fn test_get_message() {
        let store = setup_test_store();
        let topic_id = TopicId::from("test_topic");
        let partition_id = 0;

        store
            .publish(&topic_id, partition_id, 0, b"test".to_vec(), None, 1000)
            .unwrap();

        let msg_id = TopicMessageId::new(topic_id.clone(), partition_id, 0);
        let message = store.get(&msg_id).unwrap();
        assert!(message.is_some());
        assert_eq!(message.unwrap().payload, b"test");

        // Non-existent message
        let missing_id = TopicMessageId::new(topic_id, partition_id, 999);
        let missing = store.get(&missing_id).unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn test_multiple_partitions() {
        let store = setup_test_store();
        let topic_id = TopicId::from("test_topic");

        // Publish to partition 0
        store
            .publish(&topic_id, 0, 0, b"p0_msg1".to_vec(), None, 1000)
            .unwrap();

        // Publish to partition 1
        store
            .publish(&topic_id, 1, 0, b"p1_msg1".to_vec(), None, 2000)
            .unwrap();

        // Each partition has independent messages
        let messages0 = store.fetch_messages(&topic_id, 0, 0, 10).unwrap();
        let messages1 = store.fetch_messages(&topic_id, 1, 0, 10).unwrap();

        assert_eq!(messages0.len(), 1);
        assert_eq!(messages1.len(), 1);
        assert_eq!(messages0[0].payload, b"p0_msg1");
        assert_eq!(messages1[0].payload, b"p1_msg1");
    }

    #[test]
    fn test_fetch_with_limit() {
        let store = setup_test_store();
        let topic_id = TopicId::from("test_topic");
        let partition_id = 0;

        // Publish 5 messages
        for i in 0..5 {
            store
                .publish(
                    &topic_id,
                    partition_id,
                    i,
                    format!("message{}", i).into_bytes(),
                    None,
                    (1000 + i * 100) as i64,
                )
                .unwrap();
        }

        // Fetch with limit
        let messages = store.fetch_messages(&topic_id, partition_id, 0, 3).unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].payload, b"message0");
        assert_eq!(messages[2].payload, b"message2");

        // Fetch from middle
        let messages = store.fetch_messages(&topic_id, partition_id, 2, 10).unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].payload, b"message2");
        assert_eq!(messages[2].payload, b"message4");
    }

    #[test]
    fn test_storage_key_roundtrip() {
        let msg_id = TopicMessageId::new(TopicId::from("my_topic"), 3, 42);
        let key = msg_id.storage_key();
        let decoded = TopicMessageId::from_storage_key(&key).unwrap();
        assert_eq!(decoded, msg_id);
    }
}
