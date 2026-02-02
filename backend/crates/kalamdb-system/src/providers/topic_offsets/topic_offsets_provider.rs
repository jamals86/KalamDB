//! System.topic_offsets table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.topic_offsets table.
//! Uses `IndexedEntityStore` with composite primary key (topic_id, group_id, partition_id).

use super::TopicOffsetsTableSchema;
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
use kalamdb_commons::models::{ConsumerGroupId, TopicId};
use kalamdb_commons::{RecordBatchBuilder, SystemTable};
use kalamdb_store::entity_store::EntityStore;
use kalamdb_store::{IndexedEntityStore, StorageBackend};
use std::any::Any;
use std::sync::Arc;

use super::models::TopicOffset;

/// Composite key for topic offsets as a String: "topic_id:group_id:partition_id"
pub type TopicOffsetKey = String;

/// Type alias for the indexed topic offsets store
pub type TopicOffsetsStore = IndexedEntityStore<TopicOffsetKey, TopicOffset>;

/// System.topic_offsets table provider using IndexedEntityStore.
///
/// Uses a composite primary key (topic_id, group_id, partition_id) for
/// efficient offset tracking per consumer group and partition.
pub struct TopicOffsetsTableProvider {
    store: TopicOffsetsStore,
}

impl std::fmt::Debug for TopicOffsetsTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TopicOffsetsTableProvider").finish()
    }
}

impl TopicOffsetsTableProvider {
    /// Create a new topic offsets table provider.
    ///
    /// # Arguments
    /// * `backend` - Storage backend (RocksDB or mock)
    ///
    /// # Returns
    /// A new TopicOffsetsTableProvider instance
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        let store = IndexedEntityStore::new(
            backend,
            SystemTable::TopicOffsets
                .column_family_name()
                .expect("TopicOffsets is a table, not a view"),
            Vec::new(), // No secondary indexes for MVP
        );
        Self { store }
    }

    /// Create a composite key from components
    fn make_key(
        topic_id: &TopicId,
        group_id: &ConsumerGroupId,
        partition_id: u32,
    ) -> TopicOffsetKey {
        format!("{}:{}:{}", topic_id.as_str(), group_id.as_str(), partition_id)
    }

    /// Create or update a topic offset entry.
    ///
    /// # Arguments
    /// * `offset` - TopicOffset entity to upsert
    pub fn upsert_offset(&self, offset: TopicOffset) -> Result<(), SystemError> {
        let key = Self::make_key(&offset.topic_id, &offset.group_id, offset.partition_id);
        self.store
            .insert(&key, &offset)
            .into_system_error("insert topic offset error")
    }

    /// Async version of `upsert_offset()`.
    pub async fn upsert_offset_async(&self, offset: TopicOffset) -> Result<(), SystemError> {
        let key = Self::make_key(&offset.topic_id, &offset.group_id, offset.partition_id);
        self.store
            .insert_async(key, offset)
            .await
            .into_system_error("insert_async topic offset error")
    }

    /// Get a topic offset by composite key
    pub fn get_offset(
        &self,
        topic_id: &TopicId,
        group_id: &ConsumerGroupId,
        partition_id: u32,
    ) -> Result<Option<TopicOffset>, SystemError> {
        let key = Self::make_key(topic_id, group_id, partition_id);
        Ok(self.store.get(&key)?)
    }

    /// Async version of `get_offset()`.
    pub async fn get_offset_async(
        &self,
        topic_id: &TopicId,
        group_id: &ConsumerGroupId,
        partition_id: u32,
    ) -> Result<Option<TopicOffset>, SystemError> {
        let key = Self::make_key(topic_id, group_id, partition_id);
        self.store
            .get_async(key)
            .await
            .into_system_error("get_async error")
    }

    /// Get all offsets for a topic and consumer group across all partitions
    pub fn get_group_offsets(
        &self,
        topic_id: &TopicId,
        group_id: &ConsumerGroupId,
    ) -> Result<Vec<TopicOffset>, SystemError> {
        let all_offsets = self.store.scan_all_typed(None, None, None)?;
        Ok(all_offsets
            .iter()
            .filter_map(|(_, offset)| {
                if offset.topic_id == *topic_id && offset.group_id == *group_id {
                    Some(offset.clone())
                } else {
                    None
                }
            })
            .collect())
    }

    /// Get all offsets for a topic across all consumer groups
    pub fn get_topic_offsets(&self, topic_id: &TopicId) -> Result<Vec<TopicOffset>, SystemError> {
        let all_offsets = self.store.scan_all_typed(None, None, None)?;
        Ok(all_offsets
            .iter()
            .filter_map(|(_, offset)| {
                if offset.topic_id == *topic_id {
                    Some(offset.clone())
                } else {
                    None
                }
            })
            .collect())
    }

    /// Acknowledge consumption through a specific offset
    ///
    /// Updates last_acked_offset for the given topic, group, and partition.
    pub fn ack_offset(
        &self,
        topic_id: &TopicId,
        group_id: &ConsumerGroupId,
        partition_id: u32,
        offset: u64,
    ) -> Result<(), SystemError> {
        let key = Self::make_key(topic_id, group_id, partition_id);

        // Get existing offset or create new one
        let mut topic_offset = self
            .store
            .get(&key)?
            .unwrap_or_else(|| TopicOffset::new(topic_id.clone(), group_id.clone(), partition_id));

        // Update offset
        topic_offset.ack(offset);

        // Save back
        self.store
            .insert(&key, &topic_offset)
            .into_system_error("update offset error")
    }

    /// Async version of `ack_offset()`.
    pub async fn ack_offset_async(
        &self,
        topic_id: &TopicId,
        group_id: &ConsumerGroupId,
        partition_id: u32,
        offset: u64,
    ) -> Result<(), SystemError> {
        let key = Self::make_key(topic_id, group_id, partition_id);

        // Get existing offset or create new one
        let mut topic_offset = self
            .store
            .get_async(key.clone())
            .await
            .into_system_error("get_async error")?
            .unwrap_or_else(|| TopicOffset::new(topic_id.clone(), group_id.clone(), partition_id));

        // Update offset
        topic_offset.ack(offset);

        // Save back
        self.store
            .insert_async(key, topic_offset)
            .await
            .into_system_error("insert_async offset error")
    }

    /// Delete all offsets for a consumer group
    pub fn delete_group_offsets(
        &self,
        topic_id: &TopicId,
        group_id: &ConsumerGroupId,
    ) -> Result<usize, SystemError> {
        let offsets = self.get_group_offsets(topic_id, group_id)?;
        let count = offsets.len();

        for offset in offsets {
            let key = Self::make_key(&offset.topic_id, &offset.group_id, offset.partition_id);
            self.store.delete(&key).into_system_error("delete offset error")?;
        }

        Ok(count)
    }

    /// Delete all offsets for a topic (all consumer groups and partitions)
    pub fn delete_topic_offsets(&self, topic_id: &TopicId) -> Result<usize, SystemError> {
        let offsets = self.get_topic_offsets(topic_id)?;
        let count = offsets.len();

        for offset in offsets {
            let key = Self::make_key(&offset.topic_id, &offset.group_id, offset.partition_id);
            self.store.delete(&key).into_system_error("delete offset error")?;
        }

        Ok(count)
    }

    /// List all topic offsets
    pub fn list_offsets(&self) -> Result<Vec<TopicOffset>, SystemError> {
        let offsets = self.store.scan_all_typed(None, None, None)?;
        Ok(offsets.iter().map(|(_, o)| o.clone()).collect())
    }

    /// Get reference to the underlying store for advanced operations
    pub fn store(&self) -> &TopicOffsetsStore {
        &self.store
    }

    /// Load all topic offsets as a single RecordBatch for DataFusion
    fn load_batch_internal(&self) -> Result<RecordBatch, SystemError> {
        let offsets = self.list_offsets()?;
        
        let mut topic_ids = Vec::with_capacity(offsets.len());
        let mut group_ids = Vec::with_capacity(offsets.len());
        let mut partition_ids = Vec::with_capacity(offsets.len());
        let mut last_acked_offsets = Vec::with_capacity(offsets.len());
        let mut updated_ats = Vec::with_capacity(offsets.len());

        for offset in offsets {
            topic_ids.push(Some(offset.topic_id.as_str().to_string()));
            group_ids.push(Some(offset.group_id.as_str().to_string()));
            partition_ids.push(Some(offset.partition_id as i32));
            last_acked_offsets.push(Some(offset.last_acked_offset as i64));
            updated_ats.push(Some(offset.updated_at));
        }

        let mut builder = RecordBatchBuilder::new(TopicOffsetsTableSchema::schema());
        builder
            .add_string_column_owned(topic_ids)
            .add_string_column_owned(group_ids)
            .add_int32_column(partition_ids)
            .add_int64_column(last_acked_offsets)
            .add_timestamp_micros_column(updated_ats);

        let batch = builder.build().into_arrow_error("Failed to create RecordBatch")?;
        Ok(batch)
    }
}

impl SystemTableProviderExt for TopicOffsetsTableProvider {
    fn table_name(&self) -> &str {
        TopicOffsetsTableSchema::table_name()
    }

    fn schema_ref(&self) -> SchemaRef {
        TopicOffsetsTableSchema::schema()
    }

    fn load_batch(&self) -> Result<RecordBatch, SystemError> {
        self.load_batch_internal()
    }
}

#[async_trait]
impl TableProvider for TopicOffsetsTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        TopicOffsetsTableSchema::schema()
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
        
        let batch = self.load_batch_internal().map_err(|e| {
            datafusion::error::DataFusionError::Execution(format!("Failed to load topic offsets: {}", e))
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
    fn test_topic_offsets_provider_creation() {
        let backend = Arc::new(InMemoryBackend::new());
        let _provider = TopicOffsetsTableProvider::new(backend);
        // Provider created successfully
    }

    #[test]
    fn test_upsert_and_get_offset() {
        let backend = Arc::new(InMemoryBackend::new());
        let provider = TopicOffsetsTableProvider::new(backend);

        let offset = TopicOffset::new(
            TopicId::new("topic_123"),
            ConsumerGroupId::new("group_1"),
            0,
        );

        // Upsert offset
        provider.upsert_offset(offset.clone()).unwrap();

        // Get offset
        let retrieved = provider
            .get_offset(&offset.topic_id, &offset.group_id, offset.partition_id)
            .unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().last_acked_offset, 0);
    }

    #[test]
    fn test_ack_offset() {
        let backend = Arc::new(InMemoryBackend::new());
        let provider = TopicOffsetsTableProvider::new(backend);

        let topic_id = TopicId::new("topic_456");
        let group_id = ConsumerGroupId::new("service_1");

        // Ack offset (creates entry if not exists)
        provider.ack_offset(&topic_id, &group_id, 0, 42).unwrap();

        // Verify
        let offset = provider.get_offset(&topic_id, &group_id, 0).unwrap().unwrap();
        assert_eq!(offset.last_acked_offset, 42);
        assert_eq!(offset.next_offset(), 43);
    }

    #[test]
    fn test_get_group_offsets() {
        let backend = Arc::new(InMemoryBackend::new());
        let provider = TopicOffsetsTableProvider::new(backend);

        let topic_id = TopicId::new("topic_789");
        let group_id = ConsumerGroupId::new("analytics");

        // Create offsets for multiple partitions
        provider.ack_offset(&topic_id, &group_id, 0, 10).unwrap();
        provider.ack_offset(&topic_id, &group_id, 1, 20).unwrap();
        provider.ack_offset(&topic_id, &group_id, 2, 30).unwrap();

        // Get all offsets for the group
        let offsets = provider.get_group_offsets(&topic_id, &group_id).unwrap();
        assert_eq!(offsets.len(), 3);
    }
}
