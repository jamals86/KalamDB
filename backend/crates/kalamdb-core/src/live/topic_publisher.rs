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
use datafusion::arrow::record_batch::RecordBatch;
use kalamdb_commons::{
    errors::{CommonError, Result},
    models::{ConsumerGroupId, PayloadMode, TableId, TopicId, TopicOp},
};
use kalamdb_store::StorageBackend;
use kalamdb_system::providers::topics::{Topic, TopicRoute};
use kalamdb_tables::{TopicMessage, TopicMessageStore, TopicOffset, TopicOffsetStore};
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
    /// Store for consumer group offsets (owned by this service)
    offset_store: Arc<TopicOffsetStore>,
    
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
        let message_store = Arc::new(TopicMessageStore::new(
            storage_backend.clone(),
            "topic_messages".to_string(), //TODO: Use partition constant
        ));
        let offset_store = Arc::new(TopicOffsetStore::new(
            storage_backend,
            "topic_offsets".to_string(), //TODO: Use partition constant
        ));

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

    /// Route a table operation to matching topics and publish messages
    ///
    /// # Arguments
    /// * `table_id` - ID of the table (namespace + table name)
    /// * `operation` - Type of operation (Insert/Update/Delete)
    /// * `batch` - Record batch containing the affected rows
    ///
    /// # Returns
    /// Number of messages published across all matching topics
    pub fn route_and_publish(
        &self,
        table_id: &TableId,
        operation: TopicOp,
        batch: &RecordBatch,
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
            let count = self.publish_to_topic(&entry, batch)?;
            total_published += count;
        }

        Ok(total_published)
    }

    /// Publish messages from a RecordBatch to a single topic
    fn publish_to_topic(&self, entry: &RouteEntry, batch: &RecordBatch) -> Result<usize> {
        let num_rows = batch.num_rows();
        if num_rows == 0 {
            return Ok(0);
        }

        let partition_count = entry.topic_partitions.max(1) as usize;
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        for row_idx in 0..num_rows {
            // Select partition (round-robin for now)
            let partition_id = (row_idx % partition_count) as u32;

            // Get next offset for this partition
            let offset = self.next_offset(&entry.topic_id, partition_id);

            // Extract payload
            let payload = self.extract_payload(&entry.route, batch, row_idx)?;

            // Extract message key (optional)
            let key = self.extract_message_key(&entry.route, batch, row_idx)?;

            // Publish message
            self.message_store
                .publish(
                    &entry.topic_id,
                    partition_id,
                    offset,
                    payload,
                    key,
                    timestamp_ms,
                )
                .map_err(|e| CommonError::Internal(format!("Failed to publish message: {}", e)))?;
        }

        Ok(num_rows)
    }

    /// Get the next offset for a topic partition and increment counter
    fn next_offset(&self, topic_id: &TopicId, partition_id: u32) -> u64 {
        let key = format!("{}:{}", topic_id.as_str(), partition_id);
        let mut entry = self.offset_counters.entry(key).or_insert(0);
        let offset = *entry;
        *entry += 1;
        offset
    }

    /// Extract payload from a row based on PayloadMode
    fn extract_payload(
        &self,
        route: &TopicRoute,
        batch: &RecordBatch,
        row_idx: usize,
    ) -> Result<Vec<u8>> {
        match route.payload_mode {
            PayloadMode::Key => self.extract_key_columns(batch, row_idx),
            PayloadMode::Full => self.extract_full_row(batch, row_idx),
            PayloadMode::Diff => {
                // For now, extract full row (diff requires before/after state)
                self.extract_full_row(batch, row_idx)
            }
        }
    }

    /// Extract primary key columns from a row
    fn extract_key_columns(&self, batch: &RecordBatch, row_idx: usize) -> Result<Vec<u8>> {
        if batch.num_columns() == 0 {
            return Err(CommonError::InvalidInput(
                "Cannot extract key from empty batch".to_string(),
            ));
        }

        // Placeholder: serialize first column value
        let col = batch.column(0);
        let value_str = format!("{:?}", col.slice(row_idx, 1));
        Ok(value_str.into_bytes())
    }

    /// Extract full row as serialized bytes
    fn extract_full_row(&self, batch: &RecordBatch, row_idx: usize) -> Result<Vec<u8>> {
        let schema = batch.schema();
        let mut row_data = Vec::new();
        
        for col_idx in 0..batch.num_columns() {
            let col = batch.column(col_idx);
            let field = schema.field(col_idx);
            let value_str = format!("{}={:?}", field.name(), col.slice(row_idx, 1));
            row_data.push(value_str);
        }

        Ok(row_data.join(", ").into_bytes())
    }

    /// Extract message key from a row (for keyed topics)
    fn extract_message_key(
        &self,
        _route: &TopicRoute,
        _batch: &RecordBatch,
        _row_idx: usize,
    ) -> Result<Option<String>> {
        // TODO: Add partition_key_expr support
        Ok(None)
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
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        self.offset_store
            .ack_offset(topic_id, group_id, partition_id, offset, timestamp_ms)
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
    pub fn offset_store(&self) -> Arc<TopicOffsetStore> {
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
    use datafusion::arrow::{
        array::{Int32Array, StringArray},
        datatypes::{DataType, Field, Schema},
    };
    use kalamdb_commons::models::{NamespaceId, TableName};
    use kalamdb_store::test_utils::InMemoryBackend;

    fn create_test_batch() -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int32, false),
            Field::new("name", DataType::Utf8, false),
        ]));

        let ids = Arc::new(Int32Array::from(vec![1, 2, 3]));
        let names = Arc::new(StringArray::from(vec!["Alice", "Bob", "Charlie"]));

        RecordBatch::try_new(schema, vec![ids, names]).unwrap()
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

        let batch = create_test_batch();
        let count = service
            .route_and_publish(&table_id, TopicOp::Insert, &batch)
            .unwrap();

        assert_eq!(count, 3);

        // Verify messages were published
        let messages = service.fetch_messages(&topic_id, 0, 0, 10).unwrap();
        assert!(!messages.is_empty());
    }

    #[test]
    fn test_no_routes_returns_zero() {
        let backend = Arc::new(InMemoryBackend::new());
        let service = TopicPublisherService::new(backend);

        let ns = NamespaceId::new("test_ns");
        let table_id = TableId::new(ns.clone(), TableName::from("no_routes"));

        let batch = create_test_batch();
        let count = service
            .route_and_publish(&table_id, TopicOp::Insert, &batch)
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
        assert_eq!(offsets[0].committed_offset, 42);
    }
}
