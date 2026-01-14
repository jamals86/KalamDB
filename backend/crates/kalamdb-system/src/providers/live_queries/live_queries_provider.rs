//! System.live_queries table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.live_queries table.
//! Uses IndexedEntityStore with TableIdIndex for efficient table-based lookups.

use super::{new_live_queries_store, LiveQueriesStore, LiveQueriesTableSchema};
use crate::error::{SystemError, SystemResultExt};
use crate::system_table_trait::SystemTableProviderExt;
use async_trait::async_trait;
use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::datatypes::SchemaRef;
use kalamdb_commons::RecordBatchBuilder;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::{Expr, TableProviderFilterPushDown};
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::models::ConnectionId;
use kalamdb_commons::system::LiveQuery;
use kalamdb_commons::types::LiveQueryStatus;
use kalamdb_commons::{LiveQueryId, NodeId, StorageKey, TableId, UserId};
use kalamdb_store::entity_store::EntityStore;
use kalamdb_store::StorageBackend;
use std::any::Any;
use std::sync::Arc;

/// System.live_queries table provider using EntityStore architecture
pub struct LiveQueriesTableProvider {
    store: LiveQueriesStore,
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
        }
    }

    /// Create a new live query entry
    pub fn create_live_query(&self, live_query: LiveQuery) -> Result<(), SystemError> {
        self.store.insert(&live_query.live_id, &live_query)?;
        Ok(())
    }

    /// Async version of `create_live_query()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn create_live_query_async(&self, live_query: LiveQuery) -> Result<(), SystemError> {
        self.store
            .insert_async(live_query.live_id.clone(), live_query)
            .await
            .into_system_error("insert_async error")?;
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
            .get_async(live_query_id)
            .await
            .into_system_error("get_async error")
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

        self.store.insert(&live_query.live_id, &live_query)?;
        Ok(())
    }

    /// Async version of `update_live_query()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn update_live_query_async(&self, live_query: LiveQuery) -> Result<(), SystemError> {
        // Check if live query exists
        if self.store.get_async(live_query.live_id.clone()).await?.is_none() {
            return Err(SystemError::NotFound(format!(
                "Live query not found: {}",
                live_query.live_id
            )));
        }

        self.store
            .insert_async(live_query.live_id.clone(), live_query)
            .await
            .into_system_error("insert_async error")?;
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
            .delete_async(live_id.clone())
            .await
            .into_system_error("delete_async error")?;
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
            .into_system_error("scan_all_async error")?;
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
            .into_system_error("scan_all_async error")?;
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

    /// Delete live queries by user ID and connection ID using efficient primary key prefix scan.
    ///
    /// PERFORMANCE: Uses prefix scan on primary key format `{user_id}-{connection_id}-`.
    /// With max ~10 live queries per user, this is effectively O(1).
    /// No secondary index needed - saves write overhead on every insert/update/delete.
    pub fn delete_by_connection_id(
        &self,
        user_id: &UserId,
        connection_id: &ConnectionId,
    ) -> Result<(), SystemError> {
        // Create prefix key for scanning: "user_id-connection_id-"
        let prefix = LiveQueryId::user_connection_prefix(user_id, connection_id);
        let prefix_bytes = prefix.as_bytes();

        // Scan all keys with this prefix (max ~10 per user)
        let results = self
            .store
            .scan_limited_with_prefix_and_start(Some(prefix_bytes), None, 100)?;

        // Delete each matching key (uses atomic WriteBatch internally)
        for (key_bytes, _) in results {
            let key = LiveQueryId::from_storage_key(&key_bytes)
                .map_err(|e| SystemError::InvalidOperation(format!("Invalid key: {}", e)))?;
            self.store.delete(&key)?;
        }
        Ok(())
    }

    /// Async version of `delete_by_connection_id()` - offloads to blocking thread pool.
    ///
    /// Uses efficient primary key prefix scan on `{user_id}-{connection_id}-`.
    pub async fn delete_by_connection_id_async(
        &self,
        user_id: &UserId,
        connection_id: &ConnectionId,
    ) -> Result<(), SystemError> {
        // Create prefix key for scanning: "user_id-connection_id-"
        let prefix = LiveQueryId::user_connection_prefix(user_id, connection_id);
        let prefix_bytes = prefix.as_bytes().to_vec();

        // Scan all keys with this prefix (async)
        let results: Vec<(Vec<u8>, LiveQuery)> = {
            let store = self.store.clone();
            tokio::task::spawn_blocking(move || {
                store.scan_limited_with_prefix_and_start(Some(&prefix_bytes), None, 100)
            })
            .await
            .into_system_error("Join error")??
        };

        // Delete each matching key asynchronously
        for (key_bytes, _) in results {
            let key = LiveQueryId::from_storage_key(&key_bytes)
                .map_err(|e| SystemError::InvalidOperation(format!("Invalid key: {}", e)))?;
            self.delete_live_query_async(&key).await?;
        }
        Ok(())
    }

    /// Clear all live queries from storage.
    ///
    /// Used during server startup to clean up orphan entries from previous runs.
    /// Live queries don't persist across server restarts since WebSocket connections
    /// are lost on restart.
    pub fn clear_all(&self) -> Result<usize, SystemError> {
        let all = self.store.scan_all(None, None, None)?;
        let count = all.len();
        for (key_bytes, _) in all {
            let key = LiveQueryId::from_storage_key(&key_bytes)
                .map_err(|e| SystemError::InvalidOperation(format!("Invalid key: {}", e)))?;
            self.store.delete(&key)?;
        }
        Ok(count)
    }

    /// Async version of `clear_all()`.
    pub async fn clear_all_async(&self) -> Result<usize, SystemError> {
        let store = self.store.clone();
        let all: Vec<(Vec<u8>, LiveQuery)> = tokio::task::spawn_blocking(move || {
            store.scan_all(None, None, None)
        })
        .await
        .into_system_error("Join error")??;

        let count = all.len();
        for (key_bytes, _) in all {
            let key = LiveQueryId::from_storage_key(&key_bytes)
                .map_err(|e| SystemError::InvalidOperation(format!("Invalid key: {}", e)))?;
            self.delete_live_query_async(&key).await?;
        }
        Ok(count)
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
        self.create_batch(live_queries)
    }

    /// Helper to create RecordBatch from live queries
    fn create_batch(&self, live_queries: Vec<(Vec<u8>, LiveQuery)>) -> Result<RecordBatch, SystemError> {
        // Extract data into vectors
        let mut live_ids = Vec::with_capacity(live_queries.len());
        let mut connection_ids = Vec::with_capacity(live_queries.len());
        let mut subscription_ids = Vec::with_capacity(live_queries.len());
        let mut namespace_ids = Vec::with_capacity(live_queries.len());
        let mut table_names = Vec::with_capacity(live_queries.len());
        let mut user_ids = Vec::with_capacity(live_queries.len());
        let mut queries = Vec::with_capacity(live_queries.len());
        let mut options = Vec::with_capacity(live_queries.len());
        let mut statuses = Vec::with_capacity(live_queries.len());
        let mut created_ats = Vec::with_capacity(live_queries.len());
        let mut last_updates = Vec::with_capacity(live_queries.len());
        let mut last_ping_ats = Vec::with_capacity(live_queries.len());
        let mut changes = Vec::with_capacity(live_queries.len());
        let mut node_ids = Vec::with_capacity(live_queries.len());

        for (_key, lq) in live_queries {
            live_ids.push(Some(lq.live_id.as_str().to_string()));
            connection_ids.push(Some(lq.connection_id));
            subscription_ids.push(Some(lq.subscription_id));
            namespace_ids.push(Some(lq.namespace_id.as_str().to_string()));
            table_names.push(Some(lq.table_name.as_str().to_string()));
            user_ids.push(Some(lq.user_id.as_str().to_string()));
            queries.push(Some(lq.query));
            options.push(lq.options);
            statuses.push(Some(lq.status.as_str().to_string()));
            created_ats.push(Some(lq.created_at));
            last_updates.push(Some(lq.last_update));
            last_ping_ats.push(Some(lq.last_ping_at));
            changes.push(Some(lq.changes));
            node_ids.push(Some(lq.node_id.as_u64() as i64));
        }

        // Build batch using RecordBatchBuilder
        // Column order MUST match the schema definition in live_queries_table_definition:
        // 1-9: String columns
        // 10: created_at (Timestamp)
        // 11: last_update (Timestamp)
        // 12: changes (Int64)
        // 13: node_id (Int64)
        // 14: last_ping_at (Timestamp)
        let mut builder = RecordBatchBuilder::new(LiveQueriesTableSchema::schema());
        builder
            .add_string_column_owned(live_ids)
            .add_string_column_owned(connection_ids)
            .add_string_column_owned(subscription_ids)
            .add_string_column_owned(namespace_ids)
            .add_string_column_owned(table_names)
            .add_string_column_owned(user_ids)
            .add_string_column_owned(queries)
            .add_string_column_owned(options)
            .add_string_column_owned(statuses)
            .add_timestamp_micros_column(created_ats)
            .add_timestamp_micros_column(last_updates)
            .add_int64_column(changes)
            .add_int64_column(node_ids)
            .add_timestamp_micros_column(last_ping_ats);

        let batch = builder.build().into_arrow_error("Failed to create RecordBatch")?;

        Ok(batch)
    }

    // =========================================================================
    // Phase 5: Live Query Sharding - Failover Support Methods
    // =========================================================================

    /// List all active subscriptions
    ///
    /// Returns subscriptions with Active status.
    pub fn list_all_active(&self) -> Result<Vec<LiveQuery>, SystemError> {
        let all_queries = self.list_live_queries()?;
        Ok(all_queries
            .into_iter()
            .filter(|lq| lq.status == LiveQueryStatus::Active)
            .collect())
    }

    /// List subscriptions by node ID
    ///
    /// Returns all subscriptions owned by the specified node.
    pub fn list_by_node(&self, node_id: NodeId) -> Result<Vec<LiveQuery>, SystemError> {
        let all_queries = self.list_live_queries()?;
        Ok(all_queries
            .into_iter()
            .filter(|lq| lq.node_id == node_id)
            .collect())
    }

    /// Update subscription status
    ///
    /// Used for marking subscriptions as completed during failover cleanup.
    pub fn update_status(&self, live_id: &LiveQueryId, status: LiveQueryStatus) -> Result<(), SystemError> {
        let mut live_query = self.get_live_query_by_id(live_id)?
            .ok_or_else(|| SystemError::NotFound(format!("Live query not found: {}", live_id)))?;

        live_query.status = status;
        self.store.insert(live_id, &live_query)?;
        Ok(())
    }

    /// Update last ping timestamp
    ///
    /// Called periodically by WebSocket handlers to keep subscriptions alive.
    pub fn update_last_ping(&self, live_id: &LiveQueryId, timestamp_ms: i64) -> Result<(), SystemError> {
        let mut live_query = self.get_live_query_by_id(live_id)?
            .ok_or_else(|| SystemError::NotFound(format!("Live query not found: {}", live_id)))?;

        live_query.last_ping_at = timestamp_ms;
        self.store.insert(live_id, &live_query)?;
        Ok(())
    }
}

#[async_trait]
impl TableProvider for LiveQueriesTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        LiveQueriesTableSchema::schema()
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }

    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> DataFusionResult<Vec<TableProviderFilterPushDown>> {
        // Inexact pushdown: we may use filters for index/prefix scans,
        // but DataFusion must still apply them for correctness.
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

        let schema = LiveQueriesTableSchema::schema();

        // Prefer secondary index scans when possible (auto-picks from store.indexes()).
        // Falls back to scan_all if no index matches.
        let live_queries: Vec<(Vec<u8>, LiveQuery)> = if let Some((index_idx, index_prefix)) =
            self.store.find_best_index_for_filters(filters)
        {
            log::info!(
                "[system.live_queries] Using secondary index {} for filters: {:?}",
                index_idx,
                filters
            );
            self.store
                .scan_by_index(index_idx, Some(&index_prefix), limit)
                .map_err(|e| {
                    DataFusionError::Execution(format!(
                        "Failed to scan live_queries by index: {}",
                        e
                    ))
                })?
                .into_iter()
                .map(|(id, lq)| (id.storage_key(), lq))
                .collect()
        } else {
            log::info!(
                "[system.live_queries] Full table scan (no index match) for filters: {:?}",
                filters
            );
            self.store
                .scan_all(limit, None, None)
                .map_err(|e| {
                    DataFusionError::Execution(format!("Failed to scan live_queries: {}", e))
                })?
        };

        let batch = self.create_batch(live_queries).map_err(|e| {
            DataFusionError::Execution(format!("Failed to build live_queries batch: {}", e))
        })?;

        let partitions = vec![vec![batch]];
        let table = MemTable::try_new(schema, partitions)
            .map_err(|e| DataFusionError::Execution(format!("Failed to create MemTable: {}", e)))?;

        // Always pass through projection and filters to MemTable - it will handle them
        table.scan(_state, projection, filters, limit).await
    }
}

impl SystemTableProviderExt for LiveQueriesTableProvider {
    fn table_name(&self) -> &str {
        "system.live_queries"
    }

    fn schema_ref(&self) -> SchemaRef {
        LiveQueriesTableSchema::schema()
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
            last_ping_at: 1000,
            changes: 0,
            node_id: NodeId::from(1u64),
        }
    }

    #[test]
    fn test_create_and_get_live_query() {
        let provider = create_test_provider();
        let live_query = create_test_live_query("user1-conn1-test-q1", "user1", "test");

        provider.create_live_query(live_query.clone()).unwrap();

        let retrieved = provider
            .get_live_query(live_query.live_id.as_ref())
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
            .get_live_query(live_query.live_id.as_ref())
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
            .delete_live_query_str(live_query.live_id.as_ref())
            .unwrap();

        let retrieved = provider
            .get_live_query(live_query.live_id.as_ref())
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
        assert_eq!(batch.num_columns(), 14); // Schema has 14 columns (see live_queries_table_definition)
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
        assert!(!plan.schema().fields().is_empty());
    }
}
