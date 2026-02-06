//! Topic Publisher Service for Pub/Sub Infrastructure
//!
//! Unified service that manages all topic-related operations:
//! - Topic registry (in-memory cache of topics and routes)
//! - Message publishing to topics
//! - Consumer offset tracking
//! - TableId → Topics routing
//!
//! This service owns the topic stores and provides a clean API for:
//! - NotificationService to publish messages on table changes
//! - SQL handlers to consume messages
//! - AddTopicSourceHandler to update routes

use dashmap::DashMap;
use kalamdb_commons::{
    conversions::arrow_json_conversion::row_to_json_map,
    errors::{CommonError, Result},
    models::{rows::Row, ConsumerGroupId, PayloadMode, TableId, TopicId, TopicOp, UserId},
};
use kalamdb_store::{EntityStore, StorageBackend};
use kalamdb_system::providers::topic_offsets::{TopicOffset, TopicOffsetsTableProvider};
use kalamdb_system::providers::topics::{Topic, TopicRoute};
use kalamdb_tables::{TopicMessage, TopicMessageStore};
use std::sync::Arc;

/// Cached route entry combining topic ID with route configuration
#[derive(Clone, Debug)]
struct RouteEntry {
    topic_id: TopicId,
    topic_partitions: u32,
    route: TopicRoute,
}

/// Topic Publisher Service - unified service for all topic operations
///
/// Responsibilities:
/// - Maintain in-memory registry of topics and their routes
/// - Route table mutations to matching topics
/// - Publish messages to topic message store
/// - Track consumer group offsets
/// - Provide fast TableId → Topics lookup
pub struct TopicPublisherService {
    /// Store for topic messages (owned by this service)
    message_store: Arc<TopicMessageStore>,
    /// Store for consumer group offsets (system table provider)
    offset_store: Arc<TopicOffsetsTableProvider>,
    
    /// In-memory cache: TableId → Vec<RouteEntry>
    /// Enables O(1) lookup to check if a table has any topic routes
    table_routes: DashMap<TableId, Vec<RouteEntry>>,
    
    /// In-memory cache: TopicId → Topic
    /// Full topic metadata for quick access
    topics: DashMap<TopicId, Topic>,
    
    /// Per-topic-partition offset counters for message ordering
    /// Key: "topic_id:partition_id", Value: next_offset
    offset_counters: DashMap<String, u64>, //TODO: Use type-safe keys
}

impl TopicPublisherService {
    /// Create a new TopicPublisherService with stores backed by the given storage
    pub fn new(storage_backend: Arc<dyn StorageBackend>) -> Self {
        // Ensure global partitions for topic messages and offsets exist
        // These are shared across all topics
        let messages_partition = kalamdb_commons::storage::Partition::new("topic_messages");
        let offsets_partition = kalamdb_commons::storage::Partition::new("topic_offsets");
        let _ = storage_backend.create_partition(&messages_partition);
        let _ = storage_backend.create_partition(&offsets_partition);

        let message_store = Arc::new(TopicMessageStore::new(
            storage_backend.clone(),
            messages_partition.clone()
        ));
        let offset_store = Arc::new(TopicOffsetsTableProvider::new(storage_backend));

        Self {
            message_store,
            offset_store,
            table_routes: DashMap::new(),
            topics: DashMap::new(),
            offset_counters: DashMap::new(),
        }
    }

    // ===== Registry Methods =====

    /// Check if any topics are configured for a given table
    #[inline]
    pub fn has_topics_for_table(&self, table_id: &TableId) -> bool {
        self.table_routes.contains_key(table_id)
    }

    /// Check if any topics are configured for a table with a specific operation
    #[inline]
    pub fn has_topics_for_table_op(&self, table_id: &TableId, operation: &TopicOp) -> bool {
        if let Some(routes) = self.table_routes.get(table_id) {
            routes.iter().any(|entry| &entry.route.op == operation)
        } else {
            false
        }
    }

    /// Check if a topic exists
    pub fn topic_exists(&self, topic_id: &TopicId) -> bool {
        self.topics.contains_key(topic_id)
    }

    /// Get a topic by ID
    pub fn get_topic(&self, topic_id: &TopicId) -> Option<Topic> {
        self.topics.get(topic_id).map(|r| r.clone())
    }

    /// Get all topic IDs for a table
    pub fn get_topic_ids_for_table(&self, table_id: &TableId) -> Vec<TopicId> {
        self.table_routes
            .get(table_id)
            .map(|routes| routes.iter().map(|e| e.topic_id.clone()).collect())
            .unwrap_or_default()
    }

    /// Refresh the topics cache from a list of topics
    pub fn refresh_topics_cache(&self, topics: Vec<Topic>) {
        // Clear existing caches
        self.topics.clear();
        self.table_routes.clear();

        // Rebuild caches
        for topic in topics {
            // Store topic metadata
            self.topics.insert(topic.topic_id.clone(), topic.clone());

            // Build table → routes mapping
            for route in &topic.routes {
                let entry = RouteEntry {
                    topic_id: topic.topic_id.clone(),
                    topic_partitions: topic.partitions,
                    route: route.clone(),
                };

                self.table_routes
                    .entry(route.table_id.clone())
                    .or_insert_with(Vec::new)
                    .push(entry);
            }
        }
    }

    /// Add a single topic to the cache
    pub fn add_topic(&self, topic: Topic) {
        let topic_id = topic.topic_id.clone();
        
        // Add routes to table mapping
        for route in &topic.routes {
            let entry = RouteEntry {
                topic_id: topic_id.clone(),
                topic_partitions: topic.partitions,
                route: route.clone(),
            };

            self.table_routes
                .entry(route.table_id.clone())
                .or_insert_with(Vec::new)
                .push(entry);
        }

        // Store topic metadata
        self.topics.insert(topic_id, topic);
    }

    /// Remove a topic from the cache
    pub fn remove_topic(&self, topic_id: &TopicId) {
        // Remove from topics cache
        if let Some((_, topic)) = self.topics.remove(topic_id) {
            // Remove routes from table mapping
            for route in topic.routes {
                if let Some(mut routes) = self.table_routes.get_mut(&route.table_id) {
                    routes.retain(|e| &e.topic_id != topic_id);
                }
            }
        }

        // Clean up empty table entries
        self.table_routes.retain(|_, routes| !routes.is_empty());
    }

    /// Update a topic in the cache (removes old routes, adds new ones)
    pub fn update_topic(&self, topic: Topic) {
        self.remove_topic(&topic.topic_id);
        self.add_topic(topic);
    }

    /// Clear all caches
    pub fn clear_cache(&self) {
        self.topics.clear();
        self.table_routes.clear();
        self.offset_counters.clear();
    }

    // ===== Publishing Methods =====

    /// Publish a single row change to matching topics
    ///
    /// Message-centric design: one Row = one message (like Kafka).
    /// No batching overhead - directly serializes Row to JSON payload.
    ///
    /// # Arguments
    /// * `table_id` - ID of the table (namespace + table name)
    /// * `operation` - Type of operation (Insert/Update/Delete)
    /// * `row` - Row containing the affected data
    /// * `user_id` - Optional user who triggered the change
    ///
    /// # Returns
    /// Number of messages published across all matching topics
    pub fn publish_message(
        &self,
        table_id: &TableId,
        operation: TopicOp,
        row: &Row,
        user_id: Option<&UserId>,
    ) -> Result<usize> {
        // Fast path: no routes for this table
        let routes = match self.table_routes.get(table_id) {
            Some(r) => r.clone(),
            None => return Ok(0),
        };

        // Filter routes by operation
        let matching: Vec<_> = routes
            .iter()
            .filter(|entry| entry.route.op == operation)
            .cloned()
            .collect();

        if matching.is_empty() {
            return Ok(0);
        }

        let mut total_published = 0;

        for entry in matching {
            // Extract payload based on route's payload mode
            let payload = self.extract_payload_from_row(&entry.route, row)?;

            // Extract message key (optional)
            let key = self.extract_key_from_row(&entry.route, row)?;

            // Select partition (consistent hashing if no key, hash key otherwise)
            let partition_id = if let Some(ref k) = key {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                k.hash(&mut hasher);
                (hasher.finish() % entry.topic_partitions as u64) as u32
            } else {
                (self.hash_row(row) % entry.topic_partitions as u64) as u32
            };

            // Get next offset
            let offset = self.next_offset(&entry.topic_id, partition_id);

            // Create message
            let timestamp_ms = chrono::Utc::now().timestamp_millis();
            let message = TopicMessage::new_with_user(
                entry.topic_id.clone(),
                partition_id,
                offset,
                payload,
                key,
                timestamp_ms,
                user_id.cloned(),
            );

            // Store message (TODO: integrate with actual persistence layer)
            // For now, just using message_store directly
            self.message_store
                .put(&message.id(), &message)
                .map_err(|e| CommonError::Internal(format!("Failed to store topic message: {}", e)))?;

            total_published += 1;
        }

        Ok(total_published)
    }

    /// Get the next offset for a topic partition and increment counter
    fn next_offset(&self, topic_id: &TopicId, partition_id: u32) -> u64 {
        let key = format!("{}:{}", topic_id.as_str(), partition_id);
        let mut entry = self.offset_counters.entry(key).or_insert(0);
        let offset = *entry;
        *entry += 1;
        offset
    }

    /// Extract payload from a Row based on PayloadMode
    fn extract_payload_from_row(&self, route: &TopicRoute, row: &Row) -> Result<Vec<u8>> {
        match route.payload_mode {
            PayloadMode::Key => self.extract_key_columns_from_row(row),
            PayloadMode::Full => self.extract_full_row_payload(row),
            PayloadMode::Diff => {
                // For now, extract full row (diff requires before/after state)
                self.extract_full_row_payload(row)
            }
        }
    }

    /// Extract primary key columns from a Row
    fn extract_key_columns_from_row(&self, row: &Row) -> Result<Vec<u8>> {
        if row.values.is_empty() {
            return Err(CommonError::InvalidInput(
                "Cannot extract key from empty row".to_string(),
            ));
        }

        // Convert ScalarValue to JSON using proper conversion (faster and cleaner than Debug)
        let json_map = row_to_json_map(row)
            .map_err(|e| CommonError::Internal(format!("Failed to convert row to JSON: {}", e)))?;
        
        serde_json::to_vec(&json_map)
            .map_err(|e| CommonError::Internal(format!("Failed to serialize keys: {}", e)))
    }

    /// Extract full row as serialized JSON bytes
    fn extract_full_row_payload(&self, row: &Row) -> Result<Vec<u8>> {
        // Convert ScalarValue to JSON using proper conversion (faster and cleaner than Debug)
        let json_map = row_to_json_map(row)
            .map_err(|e| CommonError::Internal(format!("Failed to convert row to JSON: {}", e)))?;
        
        serde_json::to_vec(&json_map)
            .map_err(|e| CommonError::Internal(format!("Failed to serialize row: {}", e)))
    }

    /// Extract message key from a Row (for keyed topics)
    fn extract_key_from_row(&self, _route: &TopicRoute, row: &Row) -> Result<Option<String>> {
        // TODO: Add partition_key_expr support
        // For now, serialize entire row as JSON key (consistent with key columns)
        if row.values.is_empty() {
            return Ok(None);
        }
        
        let json_map = row_to_json_map(row)
            .map_err(|e| CommonError::Internal(format!("Failed to convert row to JSON: {}", e)))?;
        
        let json_str = serde_json::to_string(&json_map)
            .map_err(|e| CommonError::Internal(format!("Failed to serialize key: {}", e)))?;
        
        Ok(Some(json_str))
    }

    /// Hash a row for consistent partition selection
    fn hash_row(&self, row: &Row) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        
        // Hash the JSON representation for consistency (faster than Debug)
        if let Ok(json_map) = row_to_json_map(row) {
            if let Ok(json_str) = serde_json::to_string(&json_map) {
                json_str.hash(&mut hasher);
                return hasher.finish();
            }
        }
        
        // Fallback: hash column names only
        for key in row.values.keys() {
            key.hash(&mut hasher);
        }
        
        hasher.finish()
    }

    // ===== Message Consumption Methods =====

    /// Fetch messages from a topic partition
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

    /// Acknowledge (commit) an offset for a consumer group
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

    /// Get all committed offsets for a consumer group on a topic
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

    /// Get the underlying message store (for advanced use cases)
    pub fn message_store(&self) -> Arc<TopicMessageStore> {
        self.message_store.clone()
    }

    /// Get the underlying offset store (for advanced use cases)
    pub fn offset_store(&self) -> Arc<TopicOffsetsTableProvider> {
        self.offset_store.clone()
    }

    /// Get statistics about the topic cache
    pub fn cache_stats(&self) -> TopicCacheStats {
        TopicCacheStats {
            topic_count: self.topics.len(),
            table_route_count: self.table_routes.len(),
            total_routes: self.table_routes.iter().map(|r| r.len()).sum(),
        }
    }

    /// Restore in-memory offset counters by scanning persisted messages.
    ///
    /// After a server restart the `offset_counters` DashMap is empty, which
    /// would cause new messages to be published starting at offset 0 —
    /// potentially overwriting previously committed data.  This method scans
    /// each cached topic/partition for the highest existing offset and seeds
    /// the counter to `max_offset + 1`.
    pub fn restore_offset_counters(&self) {
        for entry in self.topics.iter() {
            let topic = entry.value();
            for partition_id in 0..topic.partitions {
                // Fetch a large batch from offset 0 to find the latest message.
                // We scan with a high limit; the last returned message carries
                // the highest offset.
                match self.message_store.fetch_messages(
                    &topic.topic_id,
                    partition_id,
                    0,
                    usize::MAX,  // scan all
                ) {
                    Ok(msgs) => {
                        if let Some(last) = msgs.last() {
                            let next = last.offset + 1;
                            let key = format!("{}:{}", topic.topic_id.as_str(), partition_id);
                            self.offset_counters.insert(key, next);
                            log::info!(
                                "Restored offset counter for topic={} partition={}: next_offset={}",
                                topic.topic_id.as_str(),
                                partition_id,
                                next,
                            );
                        }
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to restore offset for topic={} partition={}: {}",
                            topic.topic_id.as_str(),
                            partition_id,
                            e,
                        );
                    }
                }
            }
        }
    }
}

/// Statistics about the topic cache
#[derive(Debug, Clone)]
pub struct TopicCacheStats {
    pub topic_count: usize,
    pub table_route_count: usize,
    pub total_routes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::scalar::ScalarValue;
    use kalamdb_commons::models::{NamespaceId, TableName};
    use kalamdb_store::test_utils::InMemoryBackend;

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

        assert_eq!(service.topics.len(), 0);
        assert_eq!(service.table_routes.len(), 0);
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

        // Publish 3 rows individually (message-centric design)
        let rows = vec![
            create_test_row(1, "Alice"),
            create_test_row(2, "Bob"),
            create_test_row(3, "Charlie"),
        ];
        
        let mut total_count = 0;
        for row in &rows {
            let count = service
                .publish_message(&table_id, TopicOp::Insert, row, None)
                .unwrap();
            total_count += count;
        }

        assert_eq!(total_count, 3);

        // Verify messages were published (check all partitions)
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
        let count = service
            .publish_message(&table_id, TopicOp::Insert, &row, None)
            .unwrap();

        assert_eq!(count, 0);
    }

    #[test]
    fn test_offset_tracking() {
        let backend = Arc::new(InMemoryBackend::new());
        let service = TopicPublisherService::new(backend);

        let topic_id = TopicId::new("test_topic");
        let group_id = ConsumerGroupId::new("test_group");

        // Initially no offsets
        let offsets = service.get_group_offsets(&topic_id, &group_id).unwrap();
        assert!(offsets.is_empty());

        // Ack an offset
        service.ack_offset(&topic_id, &group_id, 0, 42).unwrap();

        // Verify offset was saved
        let offsets = service.get_group_offsets(&topic_id, &group_id).unwrap();
        assert_eq!(offsets.len(), 1);
        assert_eq!(offsets[0].last_acked_offset, 42);
    }
}
