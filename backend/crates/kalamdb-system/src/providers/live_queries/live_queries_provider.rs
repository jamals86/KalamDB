//! System.live_queries table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.live_queries table.
//! Uses IndexedEntityStore with TableIdIndex for efficient table-based lookups.

use crate::error::{SystemError, SystemResultExt};
use crate::providers::base::{system_rows_to_batch, IndexedProviderDefinition};
use crate::providers::live_queries::models::{LiveQuery, LiveQueryStatus};
use crate::system_row_mapper::{model_to_system_row, system_row_to_model};
use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::error::Result as DataFusionResult;
use datafusion::logical_expr::{Expr, TableProviderFilterPushDown};
use kalamdb_commons::models::ConnectionId;
use kalamdb_commons::models::rows::SystemTableRow;
use kalamdb_commons::{LiveQueryId, NodeId, SystemTable, TableId, UserId};
use kalamdb_store::entity_store::EntityStore;
use kalamdb_store::{IndexedEntityStore, StorageBackend};
use std::sync::{Arc, OnceLock};

/// System.live_queries table provider using EntityStore architecture
pub struct LiveQueriesTableProvider {
    store: IndexedEntityStore<LiveQueryId, SystemTableRow>,
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
        let store = IndexedEntityStore::new(
            backend,
            crate::SystemTable::LiveQueries
                .column_family_name()
                .expect("LiveQueries is a table"),
            super::create_live_queries_indexes(),
        );
        Self {
            store,
        }
    }

    /// Create a new live query entry
    pub fn create_live_query(&self, live_query: LiveQuery) -> Result<(), SystemError> {
        let row = Self::encode_live_query_row(&live_query)?;
        self.store.insert(&live_query.live_id, &row)?;
        Ok(())
    }

    /// Async version of `create_live_query()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn create_live_query_async(&self, live_query: LiveQuery) -> Result<(), SystemError> {
        let row = Self::encode_live_query_row(&live_query)?;
        self.store
            .insert_async(live_query.live_id.clone(), row)
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
        let row = self.store.get(live_id)?;
        row.map(|value| Self::decode_live_query_row(&value)).transpose()
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
        let row =
            self.store.get_async(live_query_id).await.into_system_error("get_async error")?;
        row.map(|value| Self::decode_live_query_row(&value)).transpose()
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

        let row = Self::encode_live_query_row(&live_query)?;
        self.store.insert(&live_query.live_id, &row)?;
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

        let row = Self::encode_live_query_row(&live_query)?;
        self.store
            .insert_async(live_query.live_id.clone(), row)
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
        let rows = self.store.scan_all_typed(None, None, None)?;
        rows.into_iter()
            .map(|(_, row)| Self::decode_live_query_row(&row))
            .collect()
    }

    /// Async version of `list_live_queries()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn list_live_queries_async(&self) -> Result<Vec<LiveQuery>, SystemError> {
        let results: Vec<(Vec<u8>, SystemTableRow)> = self
            .store
            .scan_all_async(None, None, None)
            .await
            .into_system_error("scan_all_async error")?;
        results
            .into_iter()
            .map(|(_, row)| Self::decode_live_query_row(&row))
            .collect()
    }

    /// Get live queries by user ID
    pub fn get_by_user_id(&self, user_id: &UserId) -> Result<Vec<LiveQuery>, SystemError> {
        let all_queries = self.list_live_queries()?;
        Ok(all_queries.into_iter().filter(|lq| lq.user_id == *user_id).collect())
    }

    /// Async version of `get_by_user_id()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn get_by_user_id_async(
        &self,
        user_id: &UserId,
    ) -> Result<Vec<LiveQuery>, SystemError> {
        let user_id = user_id.clone();
        let all_queries: Vec<(Vec<u8>, SystemTableRow)> = self
            .store
            .scan_all_async(None, None, None)
            .await
            .into_system_error("scan_all_async error")?;
        let mut filtered = Vec::new();
        for (_, row) in all_queries {
            let lq = Self::decode_live_query_row(&row)?;
            if lq.user_id == user_id {
                filtered.push(lq);
            }
        }
        Ok(filtered)
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
    /// PERFORMANCE: Uses storekey prefix scan on (user_id, connection_id).
    /// With max ~10 live queries per user, this is effectively O(1).
    /// No secondary index needed - saves write overhead on every insert/update/delete.
    pub fn delete_by_connection_id(
        &self,
        user_id: &UserId,
        connection_id: &ConnectionId,
    ) -> Result<(), SystemError> {
        // Create prefix key for scanning
        let prefix_bytes = LiveQueryId::user_connection_prefix(user_id, connection_id);

        // Scan all keys with this prefix (max ~10 per user) - using raw bytes for prefix
        let results = self.store.scan_with_raw_prefix(&prefix_bytes, None, 100)?;

        // Delete each matching key (uses atomic WriteBatch internally)
        for (key, _) in results {
            self.store.delete(&key)?;
        }
        Ok(())
    }

    /// Async version of `delete_by_connection_id()` - offloads to blocking thread pool.
    ///
    /// Uses efficient primary key prefix scan on (user_id, connection_id).
    pub async fn delete_by_connection_id_async(
        &self,
        user_id: &UserId,
        connection_id: &ConnectionId,
    ) -> Result<(), SystemError> {
        // Create prefix key for scanning
        let prefix_bytes = LiveQueryId::user_connection_prefix(user_id, connection_id);

        // Scan all keys with this prefix (async) - using raw bytes for prefix
        let results: Vec<(LiveQueryId, SystemTableRow)> = {
            let store = self.store.clone();
            tokio::task::spawn_blocking(move || {
                store.scan_with_raw_prefix(&prefix_bytes, None, 100)
            })
            .await
            .into_system_error("Join error")??
        };

        // Delete each matching key asynchronously
        for (key, _) in results {
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
        let all = self.store.scan_all_typed(None, None, None)?;
        let count = all.len();
        for (key, _) in all {
            self.store.delete(&key)?;
        }
        Ok(count)
    }

    /// Async version of `clear_all()`.
    pub async fn clear_all_async(&self) -> Result<usize, SystemError> {
        let store = self.store.clone();
        let all: Vec<(LiveQueryId, SystemTableRow)> =
            tokio::task::spawn_blocking(move || store.scan_all_typed(None, None, None))
                .await
                .into_system_error("Join error")??;

        let count = all.len();
        for (key, _) in all {
            self.delete_live_query_async(&key).await?;
        }
        Ok(count)
    }

    /// Scan all live queries and return as RecordBatch
    pub fn scan_all_live_queries(&self) -> Result<RecordBatch, SystemError> {
        let live_queries = self.store.scan_all_typed(None, None, None)?;
        self.create_batch(live_queries)
    }

    /// Helper to create RecordBatch from live queries
    fn create_batch(
        &self,
        live_queries: Vec<(LiveQueryId, SystemTableRow)>,
    ) -> Result<RecordBatch, SystemError> {
        let rows = live_queries.into_iter().map(|(_, row)| row).collect();
        system_rows_to_batch(&Self::schema(), rows)
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
        Ok(all_queries.into_iter().filter(|lq| lq.node_id == node_id).collect())
    }

    /// Update subscription status
    ///
    /// Used for marking subscriptions as completed during failover cleanup.
    pub fn update_status(
        &self,
        live_id: &LiveQueryId,
        status: LiveQueryStatus,
    ) -> Result<(), SystemError> {
        let mut live_query = self
            .get_live_query_by_id(live_id)?
            .ok_or_else(|| SystemError::NotFound(format!("Live query not found: {}", live_id)))?;

        live_query.status = status;
        let row = Self::encode_live_query_row(&live_query)?;
        self.store.insert(live_id, &row)?;
        Ok(())
    }

    /// Update last ping timestamp
    ///
    /// Called periodically by WebSocket handlers to keep subscriptions alive.
    pub fn update_last_ping(
        &self,
        live_id: &LiveQueryId,
        timestamp_ms: i64,
    ) -> Result<(), SystemError> {
        let mut live_query = self
            .get_live_query_by_id(live_id)?
            .ok_or_else(|| SystemError::NotFound(format!("Live query not found: {}", live_id)))?;

        live_query.last_ping_at = timestamp_ms;
        let row = Self::encode_live_query_row(&live_query)?;
        self.store.insert(live_id, &row)?;
        Ok(())
    }
}

impl LiveQueriesTableProvider {
    fn provider_definition() -> IndexedProviderDefinition<LiveQueryId> {
        IndexedProviderDefinition {
            table_name: SystemTable::LiveQueries.table_name(),
            primary_key_column: "live_id",
            schema: Self::schema,
            parse_key: |value| LiveQueryId::from_string(value).ok(),
        }
    }

    fn filter_pushdown(filters: &[&Expr]) -> DataFusionResult<Vec<TableProviderFilterPushDown>> {
        // Inexact pushdown: we may use filters for index/prefix scans,
        // but DataFusion must still apply them for correctness.
        Ok(vec![TableProviderFilterPushDown::Inexact; filters.len()])
    }

    fn encode_live_query_row(live_query: &LiveQuery) -> Result<SystemTableRow, SystemError> {
        model_to_system_row(live_query, &LiveQuery::definition())
    }

    fn decode_live_query_row(row: &SystemTableRow) -> Result<LiveQuery, SystemError> {
        system_row_to_model(row, &LiveQuery::definition())
    }

    fn schema() -> SchemaRef {
        static SCHEMA: OnceLock<SchemaRef> = OnceLock::new();
        SCHEMA
            .get_or_init(|| {
                LiveQuery::definition()
                    .to_arrow_schema()
                    .expect("failed to build live_queries schema")
            })
            .clone()
    }
}

crate::impl_indexed_system_table_provider!(
    provider = LiveQueriesTableProvider,
    key = LiveQueryId,
    value = SystemTableRow,
    store = store,
    definition = provider_definition,
    build_batch = create_batch,
    load_batch = scan_all_live_queries,
    pushdown = LiveQueriesTableProvider::filter_pushdown
);

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::datasource::TableProvider;
    use kalamdb_commons::{NamespaceId, TableName, UserId};
    use kalamdb_store::test_utils::InMemoryBackend;

    fn create_test_provider() -> LiveQueriesTableProvider {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        LiveQueriesTableProvider::new(backend)
    }

    fn create_test_live_query(live_id: &str, user_id: &UserId, table_name: &str) -> LiveQuery {
        LiveQuery {
            live_id: LiveQueryId::from_string(live_id).expect("Invalid LiveQueryId format"),
            connection_id: "conn123".to_string(),
            subscription_id: "sub123".to_string(),
            namespace_id: NamespaceId::default(),
            table_name: TableName::new(table_name),
            user_id: user_id.clone(),
            query: "SELECT * FROM test".to_string(),
            options: Some("{}".to_string()),
            status: crate::LiveQueryStatus::Active,
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
        let live_query =
            create_test_live_query("user1-conn1-test-q1", &UserId::new("user1"), "test");

        provider.create_live_query(live_query.clone()).unwrap();

        let retrieved = provider.get_live_query(live_query.live_id.as_ref()).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.live_id, live_query.live_id);
        assert_eq!(retrieved.user_id.as_str(), "user1");
    }

    #[test]
    fn test_update_live_query() {
        let provider = create_test_provider();
        let mut live_query =
            create_test_live_query("user1-conn1-test-q1", &UserId::new("user1"), "test");
        provider.create_live_query(live_query.clone()).unwrap();

        // Update
        live_query.changes = 5;
        provider.update_live_query(live_query.clone()).unwrap();

        // Verify
        let retrieved = provider.get_live_query(live_query.live_id.as_ref()).unwrap().unwrap();
        assert_eq!(retrieved.changes, 5);
    }

    #[test]
    fn test_delete_live_query() {
        let provider = create_test_provider();
        let live_query =
            create_test_live_query("user1-conn1-test-q1", &UserId::new("user1"), "test");

        provider.create_live_query(live_query.clone()).unwrap();
        provider.delete_live_query_str(live_query.live_id.as_ref()).unwrap();

        let retrieved = provider.get_live_query(live_query.live_id.as_ref()).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_list_live_queries() {
        let provider = create_test_provider();

        // Insert multiple live queries
        for i in 1..=3 {
            let lq = create_test_live_query(
                &format!("user1-conn{}-test-q{}", i, i),
                &UserId::new("user1"),
                "test",
            );
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
        let lq = create_test_live_query("user1-conn1-test-q1", &UserId::new("user1"), "test");
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
        let lq = create_test_live_query("user1-conn1-test-q1", &UserId::new("user1"), "test");
        provider.create_live_query(lq).unwrap();

        // Create DataFusion session
        let ctx = datafusion::execution::context::SessionContext::new();
        let state = ctx.state();

        // Scan via DataFusion
        let plan = provider.scan(&state, None, &[], None).await.unwrap();
        assert!(!plan.schema().fields().is_empty());
    }
}
