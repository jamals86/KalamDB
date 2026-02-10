//! System.topics table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.topics table.
//! Uses `IndexedEntityStore` for automatic secondary index management.

use super::TopicsTableSchema;
use crate::error::{SystemError, SystemResultExt};
use crate::providers::base::IndexedProviderDefinition;
use datafusion::arrow::array::RecordBatch;
use datafusion::error::Result as DataFusionResult;
use datafusion::logical_expr::Expr;
use datafusion::logical_expr::TableProviderFilterPushDown;
use kalamdb_commons::models::TopicId;
use kalamdb_commons::SystemTable;
use kalamdb_store::entity_store::EntityStore;
use kalamdb_store::{IndexedEntityStore, StorageBackend};
use std::sync::Arc;

use super::models::Topic;

/// Type alias for the indexed topics store
pub type TopicsStore = IndexedEntityStore<TopicId, Topic>;

/// System.topics table provider using IndexedEntityStore for automatic index management.
///
/// All insert/update/delete operations automatically maintain secondary indexes
/// using RocksDB's atomic WriteBatch - no manual index management needed.
pub struct TopicsTableProvider {
    store: TopicsStore,
}

impl std::fmt::Debug for TopicsTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TopicsTableProvider").finish()
    }
}

impl TopicsTableProvider {
    /// Create a new topics table provider with automatic index management.
    ///
    /// # Arguments
    /// * `backend` - Storage backend (RocksDB or mock)
    ///
    /// # Returns
    /// A new TopicsTableProvider instance with indexes configured
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        let store = IndexedEntityStore::new(
            backend,
            SystemTable::Topics.column_family_name().expect("Topics is a table, not a view"),
            Vec::new(), // No secondary indexes for MVP
        );
        Self { store }
    }

    /// Create a new topic entry.
    ///
    /// # Arguments
    /// * `topic` - Topic entity to create
    ///
    /// # Returns
    /// Success message or error
    pub fn create_topic(&self, topic: Topic) -> Result<String, SystemError> {
        self.store
            .insert(&topic.topic_id, &topic)
            .into_system_error("insert topic error")?;
        Ok(format!("Topic {} created", topic.name))
    }

    /// Async version of `create_topic()`.
    pub async fn create_topic_async(&self, topic: Topic) -> Result<(), SystemError> {
        let topic_id = topic.topic_id.clone();
        self.store
            .insert_async(topic_id, topic)
            .await
            .into_system_error("insert_async topic error")
    }

    /// Get a topic by ID
    pub fn get_topic_by_id(&self, topic_id: &TopicId) -> Result<Option<Topic>, SystemError> {
        Ok(self.store.get(topic_id)?)
    }

    /// Async version of `get_topic_by_id()`.
    pub async fn get_topic_by_id_async(
        &self,
        topic_id: &TopicId,
    ) -> Result<Option<Topic>, SystemError> {
        self.store
            .get_async(topic_id.clone())
            .await
            .into_system_error("get_async error")
    }

    /// Get a topic by name (requires full scan for MVP)
    ///
    /// TODO: Add name index for efficient lookups
    pub fn get_topic_by_name(&self, name: &str) -> Result<Option<Topic>, SystemError> {
        let iter = self.store.scan_iterator(None, None)?;
        for item in iter {
            let (_, topic) = item?;
            if topic.name == name {
                return Ok(Some(topic));
            }
        }
        Ok(None)
    }

    /// Update an existing topic entry.
    ///
    /// Indexes are automatically maintained via `IndexedEntityStore`.
    pub fn update_topic(&self, topic: Topic) -> Result<(), SystemError> {
        // Check if topic exists
        let old_topic = self.store.get(&topic.topic_id)?;
        if old_topic.is_none() {
            return Err(SystemError::NotFound(format!("Topic not found: {}", topic.topic_id)));
        }
        let old_topic = old_topic.unwrap();

        // Use update_with_old for efficiency
        self.store
            .update_with_old(&topic.topic_id, Some(&old_topic), &topic)
            .into_system_error("update topic error")
    }

    /// Async version of `update_topic()`.
    pub async fn update_topic_async(&self, topic: Topic) -> Result<(), SystemError> {
        // Check if topic exists
        let old_topic = self
            .store
            .get_async(topic.topic_id.clone())
            .await
            .into_system_error("get_async error")?;

        if old_topic.is_none() {
            return Err(SystemError::NotFound(format!("Topic not found: {}", topic.topic_id)));
        }

        // Use insert for update (IndexedEntityStore handles indexes automatically)
        self.store
            .insert_async(topic.topic_id.clone(), topic)
            .await
            .into_system_error("insert_async topic error")
    }

    /// Delete a topic by ID
    pub fn delete_topic(&self, topic_id: &TopicId) -> Result<(), SystemError> {
        self.store.delete(topic_id).into_system_error("delete topic error")
    }

    /// Async version of `delete_topic()`.
    pub async fn delete_topic_async(&self, topic_id: &TopicId) -> Result<(), SystemError> {
        self.store
            .delete_async(topic_id.clone())
            .await
            .into_system_error("delete_async topic error")
    }

    /// List all topics
    pub fn list_topics(&self) -> Result<Vec<Topic>, SystemError> {
        let topics = self.store.scan_all_typed(None, None, None)?;
        Ok(topics.iter().map(|(_, t)| t.clone()).collect())
    }

    /// Get reference to the underlying store for advanced operations
    pub fn store(&self) -> &TopicsStore {
        &self.store
    }

    fn build_batch_from_pairs(
        &self,
        pairs: Vec<(TopicId, Topic)>,
    ) -> Result<RecordBatch, SystemError> {
        crate::build_record_batch!(
            schema: TopicsTableSchema::schema(),
            entries: pairs,
            columns: [
                topic_ids => OptionalString(|entry| Some(entry.0.as_str())),
                names => OptionalString(|entry| Some(entry.1.name.as_str())),
                aliases => OptionalString(|entry| entry.1.alias.as_deref()),
                partitions => OptionalInt32(|entry| Some(entry.1.partitions as i32)),
                retention_seconds => OptionalInt64(|entry| entry.1.retention_seconds),
                retention_max_bytes => OptionalInt64(|entry| entry.1.retention_max_bytes),
                routes => OptionalString(|entry| Some(
                    serde_json::to_string(&entry.1.routes).unwrap_or_else(|_| "[]".to_string())
                )),
                created_ats => Timestamp(|entry| Some(entry.1.created_at)),
                updated_ats => Timestamp(|entry| Some(entry.1.updated_at))
            ]
        )
        .into_arrow_error("Failed to create RecordBatch")
    }
    fn scan_all_topics_batch(&self) -> Result<RecordBatch, SystemError> {
        let pairs = self
            .store
            .scan_all_typed(None, None, None)
            .into_system_error("scan_all_typed error")?;
        self.build_batch_from_pairs(pairs)
    }

    fn provider_definition() -> IndexedProviderDefinition<TopicId> {
        IndexedProviderDefinition {
            table_name: TopicsTableSchema::table_name(),
            primary_key_column: "topic_id",
            schema: TopicsTableSchema::schema,
            parse_key: |value| Some(TopicId::new(value)),
        }
    }

    fn filter_pushdown(filters: &[&Expr]) -> DataFusionResult<Vec<TableProviderFilterPushDown>> {
        Ok(vec![TableProviderFilterPushDown::Inexact; filters.len()])
    }
}

crate::impl_indexed_system_table_provider!(
    provider = TopicsTableProvider,
    key = TopicId,
    value = Topic,
    store = store,
    definition = provider_definition,
    build_batch = build_batch_from_pairs,
    load_batch = scan_all_topics_batch,
    pushdown = TopicsTableProvider::filter_pushdown
);

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_store::test_utils::InMemoryBackend;

    #[test]
    fn test_topics_provider_creation() {
        let backend = Arc::new(InMemoryBackend::new());
        let _provider = TopicsTableProvider::new(backend);
        // Provider created successfully
    }

    #[test]
    fn test_create_and_get_topic() {
        let backend = Arc::new(InMemoryBackend::new());
        let provider = TopicsTableProvider::new(backend);

        let topic = Topic::new(TopicId::new("topic_123"), "app.notifications".to_string());

        // Create topic
        let result = provider.create_topic(topic.clone());
        assert!(result.is_ok());

        // Get topic
        let retrieved = provider.get_topic_by_id(&topic.topic_id).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "app.notifications");
    }

    #[test]
    fn test_update_topic() {
        let backend = Arc::new(InMemoryBackend::new());
        let provider = TopicsTableProvider::new(backend);

        let mut topic = Topic::new(TopicId::new("topic_456"), "test.topic".to_string());
        provider.create_topic(topic.clone()).unwrap();

        // Update topic
        topic.partitions = 4;
        let result = provider.update_topic(topic.clone());
        assert!(result.is_ok());

        // Verify update
        let retrieved = provider.get_topic_by_id(&topic.topic_id).unwrap().unwrap();
        assert_eq!(retrieved.partitions, 4);
    }
}
