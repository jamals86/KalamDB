//! Topic Publisher Service — unified service for all topic operations.
//!
//! Responsibilities:
//! - Maintain in-memory registry of topics and their routes
//! - Route table mutations to matching topics
//! - Publish messages to topic message store
//! - Track consumer group offsets
//! - Provide fast TableId → Topics lookup

use std::sync::Arc;

use kalamdb_commons::{
    errors::{CommonError, Result},
    models::{rows::Row, ConsumerGroupId, TableId, TopicId, TopicOp, UserId},
    storage::Partition,
};
use kalamdb_store::{EntityStore, StorageBackend};
use kalamdb_system::providers::{
    topic_offsets::{TopicOffset, TopicOffsetsTableProvider},
    topics::Topic,
};
use kalamdb_tables::{TopicMessage, TopicMessageStore};

use crate::models::TopicCacheStats;
use crate::offset::OffsetAllocator;
use crate::payload;
use crate::routing::RouteCache;

/// Topic Publisher Service — unified service for all topic operations.
///
/// Thread-safe. Wrap in `Arc` for shared ownership.
pub struct TopicPublisherService {
    /// Persistent storage for topic messages.
    message_store: Arc<TopicMessageStore>,
    /// System table provider for consumer group offsets.
    offset_store: Arc<TopicOffsetsTableProvider>,
    /// In-memory route cache: TableId → routes.
    route_cache: RouteCache,
    /// Atomic per-topic-partition offset counters.
    offset_allocator: OffsetAllocator,
}

impl TopicPublisherService {
    /// Create a new TopicPublisherService with stores backed by the given storage.
    pub fn new(storage_backend: Arc<dyn StorageBackend>) -> Self {
        // Ensure global partitions for topic messages and offsets exist.
        let messages_partition = Partition::new("topic_messages");
        let offsets_partition = Partition::new("topic_offsets");
        let _ = storage_backend.create_partition(&messages_partition);
        let _ = storage_backend.create_partition(&offsets_partition);

        let message_store =
            Arc::new(TopicMessageStore::new(storage_backend.clone(), messages_partition));
        let offset_store = Arc::new(TopicOffsetsTableProvider::new(storage_backend));

        Self {
            message_store,
            offset_store,
            route_cache: RouteCache::new(),
            offset_allocator: OffsetAllocator::new(),
        }
    }

    // ===== Registry Methods =====

    /// Check if any topics are configured for a given table.
    #[inline]
    pub fn has_topics_for_table(&self, table_id: &TableId) -> bool {
        self.route_cache.has_topics_for_table(table_id)
    }

    /// Check if any topics are configured for a table with a specific operation.
    #[inline]
    pub fn has_topics_for_table_op(&self, table_id: &TableId, operation: &TopicOp) -> bool {
        self.route_cache.has_topics_for_table_op(table_id, operation)
    }

    /// Check if a topic exists.
    pub fn topic_exists(&self, topic_id: &TopicId) -> bool {
        self.route_cache.topic_exists(topic_id)
    }

    /// Get a topic by ID.
    pub fn get_topic(&self, topic_id: &TopicId) -> Option<Topic> {
        self.route_cache.get_topic(topic_id)
    }

    /// Get all topic IDs for a table.
    pub fn get_topic_ids_for_table(&self, table_id: &TableId) -> Vec<TopicId> {
        self.route_cache.get_topic_ids_for_table(table_id)
    }

    /// Refresh the topics cache from a list of topics.
    pub fn refresh_topics_cache(&self, topics: Vec<Topic>) {
        self.route_cache.refresh(topics);
    }

    /// Add a single topic to the cache.
    pub fn add_topic(&self, topic: Topic) {
        self.route_cache.add_topic(topic);
    }

    /// Remove a topic from the cache.
    pub fn remove_topic(&self, topic_id: &TopicId) {
        self.route_cache.remove_topic(topic_id);
    }

    /// Update a topic in the cache (removes old routes, adds new ones).
    pub fn update_topic(&self, topic: Topic) {
        self.route_cache.update_topic(topic);
    }

    /// Clear all caches.
    pub fn clear_cache(&self) {
        self.route_cache.clear();
        self.offset_allocator.clear();
    }

    // ===== Publishing Methods =====

    /// Publish a single row change to matching topics.
    ///
    /// Message-centric design: one Row = one message.
    /// Called synchronously from table providers.
    ///
    /// # Returns
    /// Number of messages published across all matching topics.
    pub fn publish_message(
        &self,
        table_id: &TableId,
        operation: TopicOp,
        row: &Row,
        user_id: Option<&UserId>,
    ) -> Result<usize> {
        let span = tracing::debug_span!(
            "topic.publish",
            table_id = %table_id,
            operation = ?operation,
            has_user_id = user_id.is_some(),
            row_value_count = row.values.len(),
            published_count = tracing::field::Empty
        );
        let _span_guard = span.entered();

        // Fast path: get matching routes for this table + operation.
        let matching = self.route_cache.get_matching_routes(table_id, &operation);
        if matching.is_empty() {
            return Ok(0);
        }

        let mut total_published = 0;

        for entry in matching {
            let topic_span = tracing::debug_span!(
                "publish_to_topic",
                topic_name = entry.topic_id.as_str(),
                topic_partitions = entry.topic_partitions,
                operation = ?entry.route.op
            );
            let _topic_span_guard = topic_span.entered();

            // Extract payload based on route's payload mode.
            let payload_bytes = payload::extract_payload(&entry.route, row, table_id)?;

            // Extract message key (optional).
            let key = payload::extract_key(row)?;

            // Select partition.
            let partition_id = if let Some(ref k) = key {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                k.hash(&mut hasher);
                (hasher.finish() % entry.topic_partitions as u64) as u32
            } else {
                (payload::hash_row(row) % entry.topic_partitions as u64) as u32
            };

            // Allocate offset atomically.
            let offset = self.offset_allocator.next_offset(entry.topic_id.as_str(), partition_id);

            // Create and persist message.
            let timestamp_ms = chrono::Utc::now().timestamp_millis();
            let message = TopicMessage::new_with_user(
                entry.topic_id.clone(),
                partition_id,
                offset,
                payload_bytes,
                key,
                timestamp_ms,
                user_id.cloned(),
                operation.clone(),
            );

            self.message_store.put(&message.id(), &message).map_err(|e| {
                CommonError::Internal(format!("Failed to store topic message: {}", e))
            })?;

            tracing::debug!(
                topic_name = entry.topic_id.as_str(),
                partition_id = partition_id,
                offset = offset,
                payload_bytes = message.payload.len(),
                "Published message to topic"
            );

            total_published += 1;
        }

        tracing::Span::current().record("published_count", total_published);
        Ok(total_published)
    }

    // ===== Message Consumption Methods =====

    /// Fetch messages from a topic partition.
    pub fn fetch_messages(
        &self,
        topic_id: &TopicId,
        partition_id: u32,
        offset: u64,
        limit: usize,
    ) -> Result<Vec<TopicMessage>> {
        self.message_store
            .fetch_messages(topic_id, partition_id, offset, limit)
            .map_err(|e| CommonError::Internal(format!("Failed to fetch messages: {}", e)))
    }

    // ===== Offset Management Methods =====

    /// Acknowledge (commit) an offset for a consumer group.
    pub fn ack_offset(
        &self,
        topic_id: &TopicId,
        group_id: &ConsumerGroupId,
        partition_id: u32,
        offset: u64,
    ) -> Result<()> {
        self.offset_store
            .ack_offset(topic_id, group_id, partition_id, offset)
            .map_err(|e| CommonError::Internal(format!("Failed to ack offset: {}", e)))
    }

    /// Get all committed offsets for a consumer group on a topic.
    pub fn get_group_offsets(
        &self,
        topic_id: &TopicId,
        group_id: &ConsumerGroupId,
    ) -> Result<Vec<TopicOffset>> {
        self.offset_store
            .get_group_offsets(topic_id, group_id)
            .map_err(|e| CommonError::Internal(format!("Failed to get offsets: {}", e)))
    }

    // ===== Accessors =====

    /// Get the underlying message store.
    pub fn message_store(&self) -> Arc<TopicMessageStore> {
        self.message_store.clone()
    }

    /// Get the underlying offset store.
    pub fn offset_store(&self) -> Arc<TopicOffsetsTableProvider> {
        self.offset_store.clone()
    }

    /// Get statistics about the topic cache.
    pub fn cache_stats(&self) -> TopicCacheStats {
        TopicCacheStats {
            topic_count: self.route_cache.topic_count(),
            table_route_count: self.route_cache.table_route_count(),
            total_routes: self.route_cache.total_routes(),
        }
    }

    /// Restore in-memory offset counters by scanning persisted messages.
    ///
    /// After a server restart the counters are empty, which would cause new
    /// messages to start at offset 0 — potentially overwriting data. This
    /// method scans each cached topic/partition for the highest existing offset
    /// and seeds the counter to `max_offset + 1`.
    pub fn restore_offset_counters(&self) {
        for entry in self.route_cache.iter_topics() {
            let topic = entry.value();
            for partition_id in 0..topic.partitions {
                match self.message_store.fetch_messages(
                    &topic.topic_id,
                    partition_id,
                    0,
                    usize::MAX,
                ) {
                    Ok(msgs) => {
                        if let Some(last) = msgs.last() {
                            let next = last.offset + 1;
                            self.offset_allocator.seed(
                                topic.topic_id.as_str(),
                                partition_id,
                                next,
                            );
                            log::info!(
                                "Restored offset counter for topic={} partition={}: next_offset={}",
                                topic.topic_id.as_str(),
                                partition_id,
                                next,
                            );
                        }
                    },
                    Err(e) => {
                        log::warn!(
                            "Failed to restore offset for topic={} partition={}: {}",
                            topic.topic_id.as_str(),
                            partition_id,
                            e,
                        );
                    },
                }
            }
        }
    }
}

// ===== TopicPublisher trait implementation =====

impl kalamdb_system::TopicPublisher for TopicPublisherService {
    fn has_topics_for_table(&self, table_id: &TableId) -> bool {
        self.route_cache.has_topics_for_table(table_id)
    }

    fn publish_for_table(
        &self,
        table_id: &TableId,
        operation: TopicOp,
        row: &Row,
        user_id: Option<&UserId>,
    ) -> std::result::Result<usize, String> {
        self.publish_message(table_id, operation, row, user_id)
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::scalar::ScalarValue;
    use kalamdb_commons::models::{NamespaceId, PayloadMode, TableName};
    use kalamdb_store::test_utils::InMemoryBackend;
    use kalamdb_system::providers::topics::TopicRoute;

    fn create_test_row(id: i32, name: &str) -> Row {
        let mut values = std::collections::BTreeMap::new();
        values.insert("id".to_string(), ScalarValue::Int32(Some(id)));
        values.insert("name".to_string(), ScalarValue::Utf8(Some(name.to_string())));
        Row { values }
    }

    fn create_test_topic(topic_id: TopicId, table_id: TableId, op: TopicOp) -> Topic {
        Topic {
            topic_id: topic_id.clone(),
            name: format!("topic_{}", topic_id.as_str()),
            alias: None,
            partitions: 2,
            retention_seconds: None,
            retention_max_bytes: None,
            routes: vec![TopicRoute {
                table_id,
                op,
                payload_mode: PayloadMode::Full,
                filter_expr: None,
                partition_key_expr: None,
            }],
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn test_service_creation() {
        let backend = Arc::new(InMemoryBackend::new());
        let service = TopicPublisherService::new(backend);
        assert_eq!(service.cache_stats().topic_count, 0);
        assert_eq!(service.cache_stats().table_route_count, 0);
    }

    #[test]
    fn test_has_topics_for_table() {
        let backend = Arc::new(InMemoryBackend::new());
        let service = TopicPublisherService::new(backend);

        let ns = NamespaceId::new("test_ns");
        let table_id = TableId::new(ns.clone(), TableName::from("users"));
        let topic_id = TopicId::new("user_events");

        assert!(!service.has_topics_for_table(&table_id));

        let topic = create_test_topic(topic_id, table_id.clone(), TopicOp::Insert);
        service.add_topic(topic);

        assert!(service.has_topics_for_table(&table_id));
        assert!(service.has_topics_for_table_op(&table_id, &TopicOp::Insert));
        assert!(!service.has_topics_for_table_op(&table_id, &TopicOp::Delete));
    }

    #[test]
    fn test_add_and_remove_topic() {
        let backend = Arc::new(InMemoryBackend::new());
        let service = TopicPublisherService::new(backend);

        let ns = NamespaceId::new("test_ns");
        let table_id = TableId::new(ns.clone(), TableName::from("users"));
        let topic_id = TopicId::new("user_events");

        let topic = create_test_topic(topic_id.clone(), table_id.clone(), TopicOp::Insert);
        service.add_topic(topic);

        assert!(service.topic_exists(&topic_id));
        assert_eq!(service.cache_stats().topic_count, 1);

        service.remove_topic(&topic_id);

        assert!(!service.topic_exists(&topic_id));
        assert!(!service.has_topics_for_table(&table_id));
        assert_eq!(service.cache_stats().topic_count, 0);
    }

    #[test]
    fn test_route_and_publish() {
        let backend = Arc::new(InMemoryBackend::new());
        let service = TopicPublisherService::new(backend);

        let ns = NamespaceId::new("test_ns");
        let table_id = TableId::new(ns.clone(), TableName::from("users"));
        let topic_id = TopicId::new("user_events");

        let topic = create_test_topic(topic_id.clone(), table_id.clone(), TopicOp::Insert);
        service.add_topic(topic);

        let rows = vec![
            create_test_row(1, "Alice"),
            create_test_row(2, "Bob"),
            create_test_row(3, "Charlie"),
        ];

        let mut total_count = 0;
        for row in &rows {
            let count = service.publish_message(&table_id, TopicOp::Insert, row, None).unwrap();
            total_count += count;
        }

        assert_eq!(total_count, 3);

        let mut all_messages = Vec::new();
        for partition_id in 0..2 {
            let messages = service.fetch_messages(&topic_id, partition_id, 0, 10).unwrap();
            all_messages.extend(messages);
        }
        assert_eq!(all_messages.len(), 3);
    }

    #[test]
    fn test_no_routes_returns_zero() {
        let backend = Arc::new(InMemoryBackend::new());
        let service = TopicPublisherService::new(backend);

        let ns = NamespaceId::new("test_ns");
        let table_id = TableId::new(ns.clone(), TableName::from("no_routes"));

        let row = create_test_row(1, "Test");
        let count = service.publish_message(&table_id, TopicOp::Insert, &row, None).unwrap();

        assert_eq!(count, 0);
    }

    #[test]
    fn test_offset_tracking() {
        let backend = Arc::new(InMemoryBackend::new());
        let service = TopicPublisherService::new(backend);

        let topic_id = TopicId::new("test_topic");
        let group_id = ConsumerGroupId::new("test_group");

        let offsets = service.get_group_offsets(&topic_id, &group_id).unwrap();
        assert!(offsets.is_empty());

        service.ack_offset(&topic_id, &group_id, 0, 42).unwrap();

        let offsets = service.get_group_offsets(&topic_id, &group_id).unwrap();
        assert_eq!(offsets.len(), 1);
        assert_eq!(offsets[0].last_acked_offset, 42);
    }
}
