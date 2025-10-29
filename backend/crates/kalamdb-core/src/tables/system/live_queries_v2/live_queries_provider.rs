//! System.live_queries table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.live_queries table.
//! Uses the new EntityStore architecture with LiveQueryId keys.

use super::super::SystemTableProviderExt;
use super::{new_live_queries_store, LiveQueriesStore, LiveQueriesTableSchema};
use crate::error::KalamDbError;
use async_trait::async_trait;
use datafusion::arrow::array::{
    ArrayRef, Int64Array, RecordBatch, StringBuilder, TimestampMillisecondArray,
};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::system::LiveQuery;
use kalamdb_commons::LiveQueryId;
use kalamdb_store::EntityStoreV2;
use kalamdb_store::StorageBackend;
use std::any::Any;
use std::sync::Arc;

/// System.live_queries table provider using EntityStore architecture
pub struct LiveQueriesTableProvider {
    store: LiveQueriesStore,
    schema: SchemaRef,
}

impl std::fmt::Debug for LiveQueriesTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveQueriesTableProvider").finish()
    }
}

impl LiveQueriesTableProvider {
    /// Create a new live queries table provider
    ///
    /// # Arguments
    /// * `backend` - Storage backend (RocksDB or mock)
    ///
    /// # Returns
    /// A new LiveQueriesTableProvider instance
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self {
            store: new_live_queries_store(backend),
            schema: LiveQueriesTableSchema::schema(),
        }
    }

    /// Create a new live query entry
    pub fn create_live_query(&self, live_query: LiveQuery) -> Result<(), KalamDbError> {
        self.store.put(&live_query.live_id, &live_query)?;
        Ok(())
    }

    /// Alias for create_live_query (for backward compatibility)
    pub fn insert_live_query(&self, live_query: LiveQuery) -> Result<(), KalamDbError> {
        self.create_live_query(live_query)
    }

    /// Get a live query by ID
    pub fn get_live_query_by_id(
        &self,
        live_id: &LiveQueryId,
    ) -> Result<Option<LiveQuery>, KalamDbError> {
        Ok(self.store.get(live_id)?)
    }

    /// Alias for get_live_query_by_id (for backward compatibility)
    pub fn get_live_query(&self, live_id: &LiveQueryId) -> Result<Option<LiveQuery>, KalamDbError> {
        self.get_live_query_by_id(live_id)
    }

    /// Update an existing live query entry
    pub fn update_live_query(&self, live_query: LiveQuery) -> Result<(), KalamDbError> {
        // Check if live query exists
        if self.store.get(&live_query.live_id)?.is_none() {
            return Err(KalamDbError::NotFound(format!(
                "Live query not found: {}",
                live_query.live_id
            )));
        }

        self.store.put(&live_query.live_id, &live_query)?;
        Ok(())
    }

    /// Delete a live query entry
    pub fn delete_live_query(&self, live_id: &LiveQueryId) -> Result<(), KalamDbError> {
        self.store.delete(live_id)?;
        Ok(())
    }

    /// List all live queries
    pub fn list_live_queries(&self) -> Result<Vec<LiveQuery>, KalamDbError> {
        let live_queries = self.store.scan_all()?;
        Ok(live_queries.into_iter().map(|(_, lq)| lq).collect())
    }

    /// Scan all live queries and return as RecordBatch
    pub fn scan_all_live_queries(&self) -> Result<RecordBatch, KalamDbError> {
        let live_queries = self.store.scan_all()?;

        let mut live_ids = StringBuilder::new();
        let mut connection_ids = StringBuilder::new();
        let mut namespace_ids = StringBuilder::new();
        let mut table_names = StringBuilder::new();
        let mut query_ids = StringBuilder::new();
        let mut user_ids = StringBuilder::new();
        let mut queries = StringBuilder::new();
        let mut options = StringBuilder::new();
        let mut created_ats = Vec::new();
        let mut last_updates = Vec::new();
        let mut changes = Vec::new();
        let mut nodes = StringBuilder::new();

        for (_key, lq) in live_queries {
            live_ids.append_value(lq.live_id.as_str());
            connection_ids.append_value(&lq.connection_id);
            namespace_ids.append_value(lq.namespace_id.as_str());
            table_names.append_value(lq.table_name.as_str());
            query_ids.append_value(&lq.query_id);
            user_ids.append_value(lq.user_id.as_str());
            queries.append_value(&lq.query);
            options.append_option(lq.options.as_deref());
            created_ats.push(Some(lq.created_at));
            last_updates.push(Some(lq.last_update));
            changes.push(Some(lq.changes));
            nodes.append_value(&lq.node);
        }

        let batch = RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(live_ids.finish()) as ArrayRef,
                Arc::new(connection_ids.finish()) as ArrayRef,
                Arc::new(namespace_ids.finish()) as ArrayRef,
                Arc::new(table_names.finish()) as ArrayRef,
                Arc::new(query_ids.finish()) as ArrayRef,
                Arc::new(user_ids.finish()) as ArrayRef,
                Arc::new(queries.finish()) as ArrayRef,
                Arc::new(options.finish()) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(created_ats)) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(last_updates)) as ArrayRef,
                Arc::new(Int64Array::from(changes)) as ArrayRef,
                Arc::new(nodes.finish()) as ArrayRef,
            ],
        )
        .map_err(|e| KalamDbError::Other(format!("Arrow error: {}", e)))?;

        Ok(batch)
    }
}

#[async_trait]
impl TableProvider for LiveQueriesTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }

    async fn scan(
        &self,
        _state: &dyn datafusion::catalog::Session,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        use datafusion::datasource::MemTable;
        let schema = self.schema.clone();
        let batch = self.scan_all_live_queries().map_err(|e| {
            DataFusionError::Execution(format!("Failed to build live_queries batch: {}", e))
        })?;
        let partitions = vec![vec![batch]];
        let table = MemTable::try_new(schema, partitions)
            .map_err(|e| DataFusionError::Execution(format!("Failed to create MemTable: {}", e)))?;
        table.scan(_state, projection, &[], _limit).await
    }
}

impl SystemTableProviderExt for LiveQueriesTableProvider {
    fn table_name(&self) -> &'static str {
        "system.live_queries"
    }

    fn schema_ref(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn load_batch(&self) -> Result<RecordBatch, KalamDbError> {
        self.scan_all_live_queries()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::{NamespaceId, TableName, UserId};
    use kalamdb_store::InMemoryBackend;

    fn create_test_provider() -> LiveQueriesTableProvider {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        LiveQueriesTableProvider::new(backend)
    }

    fn create_test_live_query(live_id: &str, user_id: &str, table_name: &str) -> LiveQuery {
        LiveQuery {
            live_id: LiveQueryId::new(live_id),
            connection_id: "conn123".to_string(),
            namespace_id: NamespaceId::new("default"),
            table_name: TableName::new(table_name),
            query_id: "query123".to_string(),
            user_id: UserId::new(user_id),
            query: "SELECT * FROM test".to_string(),
            options: Some("{}".to_string()),
            created_at: 1000,
            last_update: 1000,
            changes: 0,
            node: "node1".to_string(),
        }
    }

    #[test]
    fn test_create_and_get_live_query() {
        let provider = create_test_provider();
        let live_query = create_test_live_query("user1-conn1-test-q1", "user1", "test");

        provider.create_live_query(live_query.clone()).unwrap();

        let retrieved = provider.get_live_query(&live_query.live_id).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.live_id, live_query.live_id);
        assert_eq!(retrieved.user_id.as_str(), "user1");
    }

    #[test]
    fn test_update_live_query() {
        let provider = create_test_provider();
        let mut live_query = create_test_live_query("user1-conn1-test-q1", "user1", "test");
        provider.create_live_query(live_query.clone()).unwrap();

        // Update
        live_query.changes = 5;
        provider.update_live_query(live_query.clone()).unwrap();

        // Verify
        let retrieved = provider
            .get_live_query(&live_query.live_id)
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.changes, 5);
    }

    #[test]
    fn test_delete_live_query() {
        let provider = create_test_provider();
        let live_query = create_test_live_query("user1-conn1-test-q1", "user1", "test");

        provider.create_live_query(live_query.clone()).unwrap();
        provider.delete_live_query(&live_query.live_id).unwrap();

        let retrieved = provider.get_live_query(&live_query.live_id).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_list_live_queries() {
        let provider = create_test_provider();

        // Insert multiple live queries
        for i in 1..=3 {
            let lq =
                create_test_live_query(&format!("user1-conn{}-test-q{}", i, i), "user1", "test");
            provider.create_live_query(lq).unwrap();
        }

        // List
        let live_queries = provider.list_live_queries().unwrap();
        assert_eq!(live_queries.len(), 3);
    }

    #[test]
    fn test_scan_all_live_queries() {
        let provider = create_test_provider();

        // Insert test data
        let lq = create_test_live_query("user1-conn1-test-q1", "user1", "test");
        provider.create_live_query(lq).unwrap();

        // Scan
        let batch = provider.scan_all_live_queries().unwrap();
        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), 12);
    }

    #[tokio::test]
    async fn test_datafusion_scan() {
        let provider = create_test_provider();

        // Insert test data
        let lq = create_test_live_query("user1-conn1-test-q1", "user1", "test");
        provider.create_live_query(lq).unwrap();

        // Create DataFusion session
        let ctx = datafusion::execution::context::SessionContext::new();
        let state = ctx.state();

        // Scan via DataFusion
        let plan = provider.scan(&state, None, &[], None).await.unwrap();
        assert!(plan.schema().fields().len() > 0);
    }
}
