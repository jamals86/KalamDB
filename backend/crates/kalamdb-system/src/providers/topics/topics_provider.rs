//! System.topics table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.topics table.
//! Uses `IndexedEntityStore` for automatic secondary index management.

use super::TopicsTableSchema;
use crate::error::{SystemError, SystemResultExt};
use crate::system_table_trait::SystemTableProviderExt;
use async_trait::async_trait;
use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::Result as DataFusionResult;
use datafusion::logical_expr::Expr;
use datafusion::logical_expr::TableProviderFilterPushDown;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::models::TopicId;
use kalamdb_commons::{RecordBatchBuilder, SystemTable};
use kalamdb_store::entity_store::EntityStore;
use kalamdb_store::{IndexedEntityStore, StorageBackend};
use std::any::Any;
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
            SystemTable::Topics
                .column_family_name()
                .expect("Topics is a table, not a view"),
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
        let all_topics = self.store.scan_all_typed(None, None, None)?;
        Ok(all_topics.iter().find(|(_, t)| t.name == name).map(|(_, t)| t.clone()))
    }

    /// Update an existing topic entry.
    ///
    /// Indexes are automatically maintained via `IndexedEntityStore`.
    pub fn update_topic(&self, topic: Topic) -> Result<(), SystemError> {
        // Check if topic exists
        let old_topic = self.store.get(&topic.topic_id)?;
        if old_topic.is_none() {
            return Err(SystemError::NotFound(format!(
                "Topic not found: {}",
                topic.topic_id
            )));
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
            return Err(SystemError::NotFound(format!(
                "Topic not found: {}",
                topic.topic_id
            )));
        }

        // Use insert for update (IndexedEntityStore handles indexes automatically)
        self.store
            .insert_async(topic.topic_id.clone(), topic)
            .await
            .into_system_error("insert_async topic error")
    }

    /// Delete a topic by ID
    pub fn delete_topic(&self, topic_id: &TopicId) -> Result<(), SystemError> {
        self.store
            .delete(topic_id)
            .into_system_error("delete topic error")
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

    /// Load all topics as a single RecordBatch for DataFusion
    fn load_batch_internal(&self) -> Result<RecordBatch, SystemError> {
        let topics = self.list_topics()?;
        
        let mut topic_ids = Vec::with_capacity(topics.len());
        let mut names = Vec::with_capacity(topics.len());
        let mut aliases = Vec::with_capacity(topics.len());
        let mut partitions = Vec::with_capacity(topics.len());
        let mut retention_seconds = Vec::with_capacity(topics.len());
        let mut retention_max_bytes = Vec::with_capacity(topics.len());
        let mut routes = Vec::with_capacity(topics.len());
        let mut created_ats = Vec::with_capacity(topics.len());
        let mut updated_ats = Vec::with_capacity(topics.len());

        for topic in topics {
            topic_ids.push(Some(topic.topic_id.as_str().to_string()));
            names.push(Some(topic.name));
            aliases.push(topic.alias);
            partitions.push(Some(topic.partitions as i32));
            retention_seconds.push(topic.retention_seconds);
            retention_max_bytes.push(topic.retention_max_bytes);
            let routes_json = serde_json::to_string(&topic.routes).unwrap_or_else(|_| "[]".to_string());
            routes.push(Some(routes_json));
            created_ats.push(Some(topic.created_at));
            updated_ats.push(Some(topic.updated_at));
        }

        let mut builder = RecordBatchBuilder::new(TopicsTableSchema::schema());
        builder
            .add_string_column_owned(topic_ids)
            .add_string_column_owned(names)
            .add_string_column_owned(aliases)
            .add_int32_column(partitions)
            .add_int64_column(retention_seconds)
            .add_int64_column(retention_max_bytes)
            .add_string_column_owned(routes)
            .add_timestamp_micros_column(created_ats)
            .add_timestamp_micros_column(updated_ats);

        let batch = builder.build().into_arrow_error("Failed to create RecordBatch")?;
        Ok(batch)
    }
}

impl SystemTableProviderExt for TopicsTableProvider {
    fn table_name(&self) -> &str {
        TopicsTableSchema::table_name()
    }

    fn schema_ref(&self) -> SchemaRef {
        TopicsTableSchema::schema()
    }

    fn load_batch(&self) -> Result<RecordBatch, SystemError> {
        self.load_batch_internal()
    }
}

#[async_trait]
impl TableProvider for TopicsTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        TopicsTableSchema::schema()
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }

    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> DataFusionResult<Vec<TableProviderFilterPushDown>> {
        Ok(vec![TableProviderFilterPushDown::Inexact; filters.len()])
    }

    async fn scan(
        &self,
        _state: &dyn datafusion::catalog::Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        use datafusion::datasource::MemTable;
        
        let batch = self.load_batch().map_err(|e| {
            datafusion::error::DataFusionError::Execution(format!("Failed to load topics: {}", e))
        })?;
        
        let table = MemTable::try_new(self.schema(), vec![vec![batch]])?;
        table.scan(_state, projection, filters, limit).await
    }
}

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
