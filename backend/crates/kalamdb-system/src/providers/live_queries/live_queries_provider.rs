//! System.live_queries table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.live_queries table.
//! Uses the new EntityStore architecture with LiveQueryId keys.

use super::{new_live_queries_store, LiveQueriesStore, LiveQueriesTableSchema};
use crate::error::SystemError;
use crate::system_table_trait::SystemTableProviderExt;
use async_trait::async_trait;
use datafusion::arrow::array::{
    ArrayRef, Int64Array, RecordBatch, StringBuilder, TimestampMicrosecondArray,
};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::models::ConnectionId;
use kalamdb_commons::system::LiveQuery;
use kalamdb_commons::{LiveQueryId, StorageKey, TableId, UserId};
use kalamdb_store::entity_store::{EntityStore, EntityStoreAsync};
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
    pub fn create_live_query(&self, live_query: LiveQuery) -> Result<(), SystemError> {
        self.store.put(&live_query.live_id, &live_query)?;
        Ok(())
    }

    /// Async version of `create_live_query()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn create_live_query_async(&self, live_query: LiveQuery) -> Result<(), SystemError> {
        self.store
            .put_async(&live_query.live_id, &live_query)
            .await
            .map_err(|e| SystemError::Other(format!("put_async error: {}", e)))?;
        Ok(())
    }

    /// Alias for create_live_query (for backward compatibility)
    pub fn insert_live_query(&self, live_query: LiveQuery) -> Result<(), SystemError> {
        self.create_live_query(live_query)
    }

    /// Async version of `insert_live_query()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn insert_live_query_async(&self, live_query: LiveQuery) -> Result<(), SystemError> {
        self.create_live_query_async(live_query).await
    }

    /// Get a live query by ID
    pub fn get_live_query_by_id(
        &self,
        live_id: &LiveQueryId,
    ) -> Result<Option<LiveQuery>, SystemError> {
        Ok(self.store.get(live_id)?)
    }

    /// Alias for get_live_query_by_id (for backward compatibility)
    pub fn get_live_query(&self, live_id: &str) -> Result<Option<LiveQuery>, SystemError> {
        let live_query_id = LiveQueryId::from_string(live_id)
            .map_err(|e| SystemError::InvalidOperation(format!("Invalid LiveQueryId: {}", e)))?;
        self.get_live_query_by_id(&live_query_id)
    }

    /// Async version of `get_live_query()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn get_live_query_async(
        &self,
        live_id: &str,
    ) -> Result<Option<LiveQuery>, SystemError> {
        let live_query_id = LiveQueryId::from_string(live_id)
            .map_err(|e| SystemError::InvalidOperation(format!("Invalid LiveQueryId: {}", e)))?;
        self.store
            .get_async(&live_query_id)
            .await
            .map_err(|e| SystemError::Other(format!("get_async error: {}", e)))
    }

    /// Update an existing live query entry
    pub fn update_live_query(&self, live_query: LiveQuery) -> Result<(), SystemError> {
        // Check if live query exists
        if self.store.get(&live_query.live_id)?.is_none() {
            return Err(SystemError::NotFound(format!(
                "Live query not found: {}",
                live_query.live_id
            )));
        }

        self.store.put(&live_query.live_id, &live_query)?;
        Ok(())
    }

    /// Async version of `update_live_query()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn update_live_query_async(&self, live_query: LiveQuery) -> Result<(), SystemError> {
        // Check if live query exists
        if self.store.get_async(&live_query.live_id).await?.is_none() {
            return Err(SystemError::NotFound(format!(
                "Live query not found: {}",
                live_query.live_id
            )));
        }

        self.store
            .put_async(&live_query.live_id, &live_query)
            .await
            .map_err(|e| SystemError::Other(format!("put_async error: {}", e)))?;
        Ok(())
    }

    /// Delete a live query entry
    pub fn delete_live_query(&self, live_id: &LiveQueryId) -> Result<(), SystemError> {
        self.store.delete(live_id)?;
        Ok(())
    }

    /// Async version of `delete_live_query()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn delete_live_query_async(&self, live_id: &LiveQueryId) -> Result<(), SystemError> {
        self.store
            .delete_async(live_id)
            .await
            .map_err(|e| SystemError::Other(format!("delete_async error: {}", e)))?;
        Ok(())
    }

    /// Delete a live query entry (string version for backward compatibility)
    pub fn delete_live_query_str(&self, live_id: &str) -> Result<(), SystemError> {
        let live_query_id = LiveQueryId::from_string(live_id)
            .map_err(|e| SystemError::InvalidOperation(format!("Invalid LiveQueryId: {}", e)))?;
        self.delete_live_query(&live_query_id)
    }

    /// List all live queries
    pub fn list_live_queries(&self) -> Result<Vec<LiveQuery>, SystemError> {
        let live_queries = self.store.scan_all(None, None, None)?;
        Ok(live_queries.into_iter().map(|(_, lq)| lq).collect())
    }

    /// Async version of `list_live_queries()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn list_live_queries_async(&self) -> Result<Vec<LiveQuery>, SystemError> {
        let results: Vec<(Vec<u8>, LiveQuery)> = self
            .store
            .scan_all_async(None, None, None)
            .await
            .map_err(|e| SystemError::Other(format!("scan_all_async error: {}", e)))?;
        Ok(results.into_iter().map(|(_, lq)| lq).collect())
    }

    /// Get live queries by user ID
    pub fn get_by_user_id(&self, user_id: &UserId) -> Result<Vec<LiveQuery>, SystemError> {
        let all_queries = self.list_live_queries()?;
        Ok(all_queries
            .into_iter()
            .filter(|lq| lq.user_id == *user_id)
            .collect())
    }

    /// Async version of `get_by_user_id()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn get_by_user_id_async(
        &self,
        user_id: &UserId,
    ) -> Result<Vec<LiveQuery>, SystemError> {
        let user_id = user_id.clone();
        let all_queries: Vec<(Vec<u8>, LiveQuery)> = self
            .store
            .scan_all_async(None, None, None)
            .await
            .map_err(|e| SystemError::Other(format!("scan_all_async error: {}", e)))?;
        Ok(all_queries
            .into_iter()
            .map(|(_, lq)| lq)
            .filter(|lq| lq.user_id == user_id)
            .collect())
    }

    /// Get live queries by table ID
    pub fn get_by_table_id(&self, table_id: &TableId) -> Result<Vec<LiveQuery>, SystemError> {
        let all_queries = self.list_live_queries()?;
        Ok(all_queries
            .into_iter()
            .filter(|lq| {
                lq.table_name == *table_id.table_name()
                    && lq.namespace_id == *table_id.namespace_id()
            })
            .collect())
    }

    /// Delete live queries by user ID and connection ID using efficient prefix scan.
    ///
    /// PERFORMANCE OPTIMIZATION: Uses prefix scan on the storage key format
    /// `{user_id}-{connection_id}-{subscription_id}` to find all live queries
    /// for a user+connection without scanning the entire table.
    ///
    /// This reduces RocksDB operations from O(n) to O(m) where n is total
    /// queries and m is queries for this user+connection.
    pub fn delete_by_connection_id(
        &self,
        user_id: &UserId,
        connection_id: &ConnectionId,
    ) -> Result<(), SystemError> {
        // Create prefix key for scanning: "user_id-connection_id-"
        let prefix = LiveQueryId::user_connection_prefix(
            user_id,
            &kalamdb_commons::models::ConnectionId::new(connection_id.to_string()),
        );

        // Scan all keys with this prefix
        let prefix_bytes = prefix.as_bytes();
        let results = self
            .store
            .scan_limited_with_prefix_and_start(Some(prefix_bytes), None, 10000)?;

        // Delete each matching key
        for (key_bytes, _) in results {
            let key = LiveQueryId::from_storage_key(&key_bytes)
                .map_err(|e| SystemError::InvalidOperation(format!("Invalid key: {}", e)))?;
            self.store.delete(&key)?;
        }
        Ok(())
    }

    /// Async version of `delete_by_connection_id()` - offloads to blocking thread pool.
    ///
    /// Uses efficient prefix scan on the storage key format to find and delete
    /// all live queries for a user+connection without scanning the entire table.
    pub async fn delete_by_connection_id_async(
        &self,
        user_id: &UserId,
        connection_id: &ConnectionId,
    ) -> Result<(), SystemError> {
        // Create prefix key for scanning: "user_id-connection_id-"
        let prefix = LiveQueryId::user_connection_prefix(
            user_id,
            connection_id,
        );

        // Scan all keys with this prefix (async)
        let prefix_bytes = prefix.as_bytes().to_vec();
        let results: Vec<(Vec<u8>, LiveQuery)> = {
            let store = self.store.clone();
            tokio::task::spawn_blocking(move || {
                store.scan_limited_with_prefix_and_start(Some(&prefix_bytes), None, 10000)
            })
            .await
            .map_err(|e| SystemError::Other(format!("Join error: {}", e)))??
        };

        // Delete each matching key asynchronously
        for (key_bytes, _) in results {
            let key = LiveQueryId::from_storage_key(&key_bytes)
                .map_err(|e| SystemError::InvalidOperation(format!("Invalid key: {}", e)))?;
            self.delete_live_query_async(&key).await?;
        }
        Ok(())
    }

    /// Increment the changes counter for a live query
    pub fn increment_changes(&self, live_id: &str, timestamp: i64) -> Result<(), SystemError> {
        let mut live_query = self
            .get_live_query(live_id)?
            .ok_or_else(|| SystemError::NotFound(format!("Live query not found: {}", live_id)))?;

        live_query.changes += 1;
        live_query.last_update = timestamp;

        self.update_live_query(live_query)?;
        Ok(())
    }

    /// Async version of `increment_changes()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn increment_changes_async(
        &self,
        live_id: &str,
        timestamp: i64,
    ) -> Result<(), SystemError> {
        let mut live_query = self
            .get_live_query_async(live_id)
            .await?
            .ok_or_else(|| SystemError::NotFound(format!("Live query not found: {}", live_id)))?;

        live_query.changes += 1;
        live_query.last_update = timestamp;

        self.update_live_query_async(live_query).await?;
        Ok(())
    }

    /// Scan all live queries and return as RecordBatch
    pub fn scan_all_live_queries(&self) -> Result<RecordBatch, SystemError> {
        let live_queries = self.store.scan_all(None, None, None)?;
        let row_count = live_queries.len();

        // Pre-allocate builders for optimal performance
        let mut live_ids = StringBuilder::with_capacity(row_count, row_count * 48);
        let mut connection_ids = StringBuilder::with_capacity(row_count, row_count * 16);
        let mut subscription_ids = StringBuilder::with_capacity(row_count, row_count * 16);
        let mut namespace_ids = StringBuilder::with_capacity(row_count, row_count * 16);
        let mut table_names = StringBuilder::with_capacity(row_count, row_count * 16);
        let mut user_ids = StringBuilder::with_capacity(row_count, row_count * 16);
        let mut queries = StringBuilder::with_capacity(row_count, row_count * 128);
        let mut options = StringBuilder::with_capacity(row_count, row_count * 64);
        let mut statuses = StringBuilder::with_capacity(row_count, row_count * 16);
        let mut created_ats = Vec::with_capacity(row_count);
        let mut last_updates = Vec::with_capacity(row_count);
        let mut changes = Vec::with_capacity(row_count);
        let mut nodes = StringBuilder::with_capacity(row_count, row_count * 16);

        for (_key, lq) in live_queries {
            live_ids.append_value(lq.live_id.as_str());
            connection_ids.append_value(&lq.connection_id);
            subscription_ids.append_value(&lq.subscription_id);
            namespace_ids.append_value(lq.namespace_id.as_str());
            table_names.append_value(lq.table_name.as_str());
            user_ids.append_value(lq.user_id.as_str());
            queries.append_value(&lq.query);
            options.append_option(lq.options.as_deref());
            statuses.append_value(lq.status.as_str());
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
                Arc::new(subscription_ids.finish()) as ArrayRef,
                Arc::new(namespace_ids.finish()) as ArrayRef,
                Arc::new(table_names.finish()) as ArrayRef,
                Arc::new(user_ids.finish()) as ArrayRef,
                Arc::new(queries.finish()) as ArrayRef,
                Arc::new(options.finish()) as ArrayRef,
                Arc::new(statuses.finish()) as ArrayRef,
                Arc::new(TimestampMicrosecondArray::from(
                    created_ats
                        .into_iter()
                        .map(|ts| ts.map(|ms| ms * 1000))
                        .collect::<Vec<_>>(),
                )) as ArrayRef,
                Arc::new(TimestampMicrosecondArray::from(
                    last_updates
                        .into_iter()
                        .map(|ts| ts.map(|ms| ms * 1000))
                        .collect::<Vec<_>>(),
                )) as ArrayRef,
                Arc::new(Int64Array::from(changes)) as ArrayRef,
                Arc::new(nodes.finish()) as ArrayRef,
            ],
        )
        .map_err(|e| SystemError::Other(format!("Arrow error: {}", e)))?;

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
    fn table_name(&self) -> &str {
        "system.live_queries"
    }

    fn schema_ref(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn load_batch(&self) -> Result<RecordBatch, SystemError> {
        self.scan_all_live_queries()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::{NamespaceId, TableName, UserId};
    use kalamdb_store::test_utils::InMemoryBackend;

    fn create_test_provider() -> LiveQueriesTableProvider {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        LiveQueriesTableProvider::new(backend)
    }

    fn create_test_live_query(live_id: &str, user_id: &str, table_name: &str) -> LiveQuery {
        LiveQuery {
            live_id: LiveQueryId::from_string(live_id).expect("Invalid LiveQueryId format"),
            connection_id: "conn123".to_string(),
            subscription_id: "sub123".to_string(),
            namespace_id: NamespaceId::new("default"),
            table_name: TableName::new(table_name),
            user_id: UserId::new(user_id),
            query: "SELECT * FROM test".to_string(),
            options: Some("{}".to_string()),
            status: kalamdb_commons::types::LiveQueryStatus::Active,
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

        let retrieved = provider
            .get_live_query(&live_query.live_id.to_string())
            .unwrap();
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
            .get_live_query(&live_query.live_id.to_string())
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.changes, 5);
    }

    #[test]
    fn test_delete_live_query() {
        let provider = create_test_provider();
        let live_query = create_test_live_query("user1-conn1-test-q1", "user1", "test");

        provider.create_live_query(live_query.clone()).unwrap();
        provider
            .delete_live_query_str(&live_query.live_id.to_string())
            .unwrap();

        let retrieved = provider
            .get_live_query(&live_query.live_id.to_string())
            .unwrap();
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
        assert_eq!(batch.num_columns(), 13); // Schema has 13 columns (see live_queries_table_definition)
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
