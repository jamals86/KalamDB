//! System.live_queries table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.live_queries table,
//! backed by RocksDB column family system_live_queries.

use crate::error::KalamDbError;
use crate::tables::system::live_queries::LiveQueriesTable;
use crate::tables::system::SystemTableProviderExt;
use async_trait::async_trait;
use datafusion::arrow::array::{
    ArrayRef, Int64Array, RecordBatch, StringBuilder, TimestampMillisecondArray,
};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::models::{NamespaceId, TableName, UserId};
use kalamdb_commons::system::LiveQuery;
use kalamdb_sql::KalamSql;
use std::any::Any;
use std::sync::Arc;

/// System.live_queries table provider backed by RocksDB
pub struct LiveQueriesTableProvider {
    kalam_sql: Arc<KalamSql>,
    schema: SchemaRef,
}

impl std::fmt::Debug for LiveQueriesTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveQueriesTableProvider").finish()
    }
}

impl LiveQueriesTableProvider {
    /// Create a new live_queries table provider
    pub fn new(kalam_sql: Arc<KalamSql>) -> Self {
        Self {
            kalam_sql,
            schema: LiveQueriesTable::schema(),
        }
    }

    /// Insert a new live query subscription
    pub fn insert_live_query(&self, live_query: LiveQuery) -> Result<(), KalamDbError> {
        // kalamdb_sql::LiveQuery is now the same as kalamdb_commons::system::LiveQuery
        self.kalam_sql
            .insert_live_query(&live_query)
            .map_err(|e| KalamDbError::Other(format!("Failed to insert live query: {}", e)))
    }

    /// Update an existing live query subscription
    pub fn update_live_query(&self, live_query: LiveQuery) -> Result<(), KalamDbError> {
        // Check if live query exists
        let existing = self.get_live_query(&live_query.live_id)?;
        if existing.is_none() {
            return Err(KalamDbError::NotFound(format!(
                "Live query not found: {}",
                live_query.live_id
            )));
        }

        // kalamdb_sql::LiveQuery is now the same as kalamdb_commons::system::LiveQuery
        self.kalam_sql
            .insert_live_query(&live_query) // kalamdb-sql uses insert for both insert and update
            .map_err(|e| KalamDbError::Other(format!("Failed to update live query: {}", e)))
    }

    /// Delete a live query subscription
    pub fn delete_live_query(&self, live_id: &str) -> Result<(), KalamDbError> {
        self.kalam_sql
            .delete_live_query(live_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to delete live query: {}", e)))
    }

    /// Get a live query by ID
    pub fn get_live_query(&self, live_id: &str) -> Result<Option<LiveQuery>, KalamDbError> {
        self.kalam_sql
            .get_live_query(live_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to get live query: {}", e)))
    }

    /// Delete all live queries for a connection_id
    pub fn delete_by_connection_id(
        &self,
        connection_id: &str,
    ) -> Result<Vec<String>, KalamDbError> {
        let live_queries = self
            .kalam_sql
            .scan_all_live_queries()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan live queries: {}", e)))?;

        let mut deleted_ids = Vec::new();
        for lq in live_queries
            .into_iter()
            .filter(|lq| lq.connection_id == connection_id)
        {
            self.kalam_sql
                .delete_live_query(&lq.live_id)
                .map_err(|e| KalamDbError::Other(format!("Failed to delete live query: {}", e)))?;
            deleted_ids.push(lq.live_id);
        }

        Ok(deleted_ids)
    }

    /// Get all live queries for a user_id
    pub fn get_by_user_id(&self, user_id: &str) -> Result<Vec<LiveQuery>, KalamDbError> {
        let live_queries = self
            .kalam_sql
            .scan_all_live_queries()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan live queries: {}", e)))?;

        let filtered: Vec<LiveQuery> = live_queries
            .into_iter()
            .filter(|lq| lq.user_id.as_str() == user_id)
            .collect();

        Ok(filtered)
    }

    /// Get all live queries for a table_name
    pub fn get_by_table_name(&self, table_name: &str) -> Result<Vec<LiveQuery>, KalamDbError> {
        let live_queries = self
            .kalam_sql
            .scan_all_live_queries()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan live queries: {}", e)))?;

        let filtered: Vec<LiveQuery> = live_queries
            .into_iter()
            .filter(|lq| lq.table_name.as_str() == table_name)
            .collect();

        Ok(filtered)
    }

    /// Scan all live queries and return as RecordBatch
    pub fn scan_all_live_queries(&self) -> Result<RecordBatch, KalamDbError> {
        let live_queries = self
            .kalam_sql
            .scan_all_live_queries()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan live queries: {}", e)))?;

        let mut live_ids = StringBuilder::new();
        let mut connection_ids = StringBuilder::new();
        let mut namespaces = StringBuilder::new();
        let mut table_names = StringBuilder::new();
        let mut query_ids = StringBuilder::new();
        let mut user_ids = StringBuilder::new();
        let mut queries = StringBuilder::new();
        let mut options_list = StringBuilder::new();
        let mut created_ats = Vec::new();
        let mut last_updates = Vec::new();
        let mut changes_list = Vec::new();
        let mut nodes = StringBuilder::new();

        for live_query in live_queries {
            live_ids.append_value(&live_query.live_id);
            connection_ids.append_value(&live_query.connection_id);
            namespaces.append_value(live_query.namespace_id.as_str());
            table_names.append_value(live_query.table_name.as_str());
            query_ids.append_value(&live_query.query_id);
            user_ids.append_value(live_query.user_id.as_str());
            queries.append_value(&live_query.query);

            if let Some(ref options) = live_query.options {
                options_list.append_value(options);
            } else {
                options_list.append_null();
            }

            created_ats.push(Some(live_query.created_at));
            last_updates.push(Some(live_query.last_update));
            changes_list.push(Some(live_query.changes));
            nodes.append_value(&live_query.node);
        }

        let batch = RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(live_ids.finish()) as ArrayRef,
                Arc::new(connection_ids.finish()) as ArrayRef,
                Arc::new(namespaces.finish()) as ArrayRef,
                Arc::new(table_names.finish()) as ArrayRef,
                Arc::new(query_ids.finish()) as ArrayRef,
                Arc::new(user_ids.finish()) as ArrayRef,
                Arc::new(queries.finish()) as ArrayRef,
                Arc::new(options_list.finish()) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(created_ats)) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(last_updates)) as ArrayRef,
                Arc::new(Int64Array::from(changes_list)) as ArrayRef,
                Arc::new(nodes.finish()) as ArrayRef,
            ],
        )
        .map_err(|e| KalamDbError::Other(format!("Failed to create RecordBatch: {}", e)))?;

        Ok(batch)
    }

    /// Increment changes counter for a live query
    pub fn increment_changes(&self, live_id: &str, timestamp: i64) -> Result<(), KalamDbError> {
        let mut live_query = self
            .get_live_query(live_id)?
            .ok_or_else(|| KalamDbError::NotFound(format!("Live query not found: {}", live_id)))?;

        live_query.changes += 1;
        live_query.last_update = timestamp;

        self.update_live_query(live_query)
    }
}

impl SystemTableProviderExt for LiveQueriesTableProvider {
    fn table_name(&self) -> &'static str {
        kalamdb_commons::constants::SystemTableNames::LIVE_QUERIES
    }

    fn schema_ref(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn load_batch(&self) -> Result<RecordBatch, KalamDbError> {
        self.scan_all_live_queries()
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

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_store::RocksDbInit;
    use tempfile::TempDir;

    fn create_test_provider() -> (LiveQueriesTableProvider, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let init = RocksDbInit::new(temp_dir.path().to_str().unwrap());
        let db = init.open().unwrap();
        let backend: Arc<dyn kalamdb_commons::storage::StorageBackend> =
            Arc::new(kalamdb_store::RocksDBBackend::new(db.clone()));
        let kalam_sql = Arc::new(KalamSql::new(backend).unwrap());
        let provider = LiveQueriesTableProvider::new(kalam_sql);
        (provider, temp_dir)
    }

    #[test]
    fn test_insert_and_get_live_query() {
        let (provider, _temp_dir) = create_test_provider();

        let live_query = LiveQuery {
            live_id: "user1-conn1-messages-q1".to_string(),
            connection_id: "user1-conn1".to_string(),
            namespace_id: NamespaceId::new("chat".to_string()),
            table_name: TableName::new("messages".to_string()),
            query_id: "q1".to_string(),
            user_id: UserId::new("user1".to_string()),
            query: "SELECT * FROM messages WHERE _updated > NOW() - INTERVAL '1 hour'".to_string(),
            options: Some(r#"{"last_rows": 50}"#.to_string()),
            created_at: 1000,
            last_update: 1000,
            changes: 0,
            node: "node1".to_string(),
        };

        provider.insert_live_query(live_query.clone()).unwrap();

        let retrieved = provider.get_live_query("user1-conn1-messages-q1").unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.live_id, "user1-conn1-messages-q1");
        assert_eq!(retrieved.connection_id, "user1-conn1");
        assert_eq!(retrieved.table_name.as_str(), "messages");
        assert_eq!(retrieved.query_id, "q1");
    }

    #[test]
    fn test_update_live_query() {
        let (provider, _temp_dir) = create_test_provider();

        let mut live_query = LiveQuery {
            live_id: "user1-conn1-messages-q1".to_string(),
            connection_id: "user1-conn1".to_string(),
            namespace_id: NamespaceId::new("chat".to_string()),
            table_name: TableName::new("messages".to_string()),
            query_id: "q1".to_string(),
            user_id: UserId::new("user1".to_string()),
            query: "SELECT * FROM messages".to_string(),
            options: None,
            created_at: 1000,
            last_update: 1000,
            changes: 0,
            node: "node1".to_string(),
        };

        provider.insert_live_query(live_query.clone()).unwrap();

        live_query.changes = 5;
        live_query.last_update = 2000;
        provider.update_live_query(live_query.clone()).unwrap();

        let retrieved = provider
            .get_live_query("user1-conn1-messages-q1")
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.changes, 5);
        assert_eq!(retrieved.last_update, 2000);
    }

    #[test]
    #[ignore] // Delete not yet implemented in kalamdb-sql adapter
    fn test_delete_live_query() {
        let (provider, _temp_dir) = create_test_provider();

        let live_query = LiveQuery {
            live_id: "user1-conn1-messages-q1".to_string(),
            connection_id: "user1-conn1".to_string(),
            namespace_id: NamespaceId::new("chat".to_string()),
            table_name: TableName::new("messages".to_string()),
            query_id: "q1".to_string(),
            user_id: UserId::new("user1".to_string()),
            query: "SELECT * FROM messages".to_string(),
            options: None,
            created_at: 1000,
            last_update: 1000,
            changes: 0,
            node: "node1".to_string(),
        };

        provider.insert_live_query(live_query.clone()).unwrap();
        provider
            .delete_live_query("user1-conn1-messages-q1")
            .unwrap();

        let retrieved = provider.get_live_query("user1-conn1-messages-q1").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    #[ignore] // Delete not yet implemented in kalamdb-sql adapter
    fn test_delete_by_connection_id() {
        let (provider, _temp_dir) = create_test_provider();

        let live_query1 = LiveQuery {
            live_id: "user1-conn1-messages-q1".to_string(),
            connection_id: "user1-conn1".to_string(),
            namespace_id: NamespaceId::new("chat".to_string()),
            table_name: TableName::new("messages".to_string()),
            query_id: "q1".to_string(),
            user_id: UserId::new("user1".to_string()),
            query: "SELECT * FROM messages".to_string(),
            options: None,
            created_at: 1000,
            last_update: 1000,
            changes: 0,
            node: "node1".to_string(),
        };

        let live_query2 = LiveQuery {
            live_id: "user1-conn1-notifications-q2".to_string(),
            connection_id: "user1-conn1".to_string(),
            namespace_id: NamespaceId::new("chat".to_string()),
            table_name: TableName::new("notifications".to_string()),
            query_id: "q2".to_string(),
            user_id: UserId::new("user1".to_string()),
            query: "SELECT * FROM notifications".to_string(),
            options: None,
            created_at: 1000,
            last_update: 1000,
            changes: 0,
            node: "node1".to_string(),
        };

        provider.insert_live_query(live_query1).unwrap();
        provider.insert_live_query(live_query2).unwrap();

        let deleted = provider.delete_by_connection_id("user1-conn1").unwrap();
        assert_eq!(deleted.len(), 2);

        let retrieved1 = provider.get_live_query("user1-conn1-messages-q1").unwrap();
        let retrieved2 = provider
            .get_live_query("user1-conn1-notifications-q2")
            .unwrap();
        assert!(retrieved1.is_none());
        assert!(retrieved2.is_none());
    }

    #[test]
    fn test_get_by_user_id() {
        let (provider, _temp_dir) = create_test_provider();

        let live_query1 = LiveQuery {
            live_id: "user1-conn1-messages-q1".to_string(),
            connection_id: "user1-conn1".to_string(),
            namespace_id: NamespaceId::new("chat".to_string()),
            table_name: TableName::new("messages".to_string()),
            query_id: "q1".to_string(),
            user_id: UserId::new("user1".to_string()),
            query: "SELECT * FROM messages".to_string(),
            options: None,
            created_at: 1000,
            last_update: 1000,
            changes: 0,
            node: "node1".to_string(),
        };

        let live_query2 = LiveQuery {
            live_id: "user2-conn2-messages-q1".to_string(),
            connection_id: "user2-conn2".to_string(),
            namespace_id: NamespaceId::new("chat".to_string()),
            table_name: TableName::new("messages".to_string()),
            query_id: "q1".to_string(),
            user_id: UserId::new("user2".to_string()),
            query: "SELECT * FROM messages".to_string(),
            options: None,
            created_at: 1000,
            last_update: 1000,
            changes: 0,
            node: "node1".to_string(),
        };

        provider.insert_live_query(live_query1).unwrap();
        provider.insert_live_query(live_query2).unwrap();

        let user1_queries = provider.get_by_user_id("user1").unwrap();
        assert_eq!(user1_queries.len(), 1);
        assert_eq!(user1_queries[0].user_id.as_ref(), "user1");
    }

    #[test]
    fn test_get_by_table_name() {
        let (provider, _temp_dir) = create_test_provider();

        let live_query1 = LiveQuery {
            live_id: "user1-conn1-messages-q1".to_string(),
            connection_id: "user1-conn1".to_string(),
            namespace_id: NamespaceId::new("chat".to_string()),
            table_name: TableName::new("messages".to_string()),
            query_id: "q1".to_string(),
            user_id: UserId::new("user1".to_string()),
            query: "SELECT * FROM messages".to_string(),
            options: None,
            created_at: 1000,
            last_update: 1000,
            changes: 0,
            node: "node1".to_string(),
        };

        let live_query2 = LiveQuery {
            live_id: "user1-conn1-notifications-q2".to_string(),
            connection_id: "user1-conn1".to_string(),
            namespace_id: NamespaceId::new("chat".to_string()),
            table_name: TableName::new("notifications".to_string()),
            query_id: "q2".to_string(),
            user_id: UserId::new("user1".to_string()),
            query: "SELECT * FROM notifications".to_string(),
            options: None,
            created_at: 1000,
            last_update: 1000,
            changes: 0,
            node: "node1".to_string(),
        };

        provider.insert_live_query(live_query1).unwrap();
        provider.insert_live_query(live_query2).unwrap();

        let messages_queries = provider.get_by_table_name("messages").unwrap();
        assert_eq!(messages_queries.len(), 1);
        assert_eq!(messages_queries[0].table_name.as_ref(), "messages");
    }

    #[test]
    fn test_increment_changes() {
        let (provider, _temp_dir) = create_test_provider();

        let live_query = LiveQuery {
            live_id: "user1-conn1-messages-q1".to_string(),
            connection_id: "user1-conn1".to_string(),
            namespace_id: NamespaceId::new("chat".to_string()),
            table_name: TableName::new("messages".to_string()),
            query_id: "q1".to_string(),
            user_id: UserId::new("user1".to_string()),
            query: "SELECT * FROM messages".to_string(),
            options: None,
            created_at: 1000,
            last_update: 1000,
            changes: 0,
            node: "node1".to_string(),
        };

        provider.insert_live_query(live_query).unwrap();

        provider
            .increment_changes("user1-conn1-messages-q1", 2000)
            .unwrap();
        provider
            .increment_changes("user1-conn1-messages-q1", 3000)
            .unwrap();

        let retrieved = provider
            .get_live_query("user1-conn1-messages-q1")
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.changes, 2);
        assert_eq!(retrieved.last_update, 3000);
    }

    #[test]
    fn test_scan_all_live_queries() {
        let (provider, _temp_dir) = create_test_provider();

        let live_query1 = LiveQuery {
            live_id: "user1-conn1-messages-q1".to_string(),
            connection_id: "user1-conn1".to_string(),
            namespace_id: NamespaceId::new("chat".to_string()),
            table_name: TableName::new("messages".to_string()),
            query_id: "q1".to_string(),
            user_id: UserId::new("user1".to_string()),
            query: "SELECT * FROM messages".to_string(),
            options: Some(r#"{"last_rows": 50}"#.to_string()),
            created_at: 1000,
            last_update: 1000,
            changes: 0,
            node: "node1".to_string(),
        };

        let live_query2 = LiveQuery {
            live_id: "user2-conn2-notifications-q2".to_string(),
            connection_id: "user2-conn2".to_string(),
            namespace_id: NamespaceId::new("chat".to_string()),
            table_name: TableName::new("notifications".to_string()),
            query_id: "q2".to_string(),
            user_id: UserId::new("user2".to_string()),
            query: "SELECT * FROM notifications".to_string(),
            options: None,
            created_at: 2000,
            last_update: 2000,
            changes: 5,
            node: "node2".to_string(),
        };

        provider.insert_live_query(live_query1).unwrap();
        provider.insert_live_query(live_query2).unwrap();

        let batch = provider.scan_all_live_queries().unwrap();
        assert_eq!(batch.num_rows(), 2);
        assert_eq!(batch.num_columns(), 12);
    }
}
