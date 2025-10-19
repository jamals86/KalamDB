//! System.live_queries table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.live_queries table,
//! backed by RocksDB column family system_live_queries.

use kalamdb_sql::KalamSql;
use crate::error::KalamDbError;
use crate::tables::system::live_queries::LiveQueriesTable;
use async_trait::async_trait;
use datafusion::arrow::array::{ArrayRef, Int64Array, RecordBatch, StringBuilder, TimestampMillisecondArray};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::execution::context::SessionState;
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::sync::Arc;

/// Live query record structure stored in RocksDB
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveQueryRecord {
    pub live_id: String,        // Format: {user_id}-{unique_conn_id}-{table_name}-{query_id}
    pub connection_id: String,  // Format: {user_id}-{unique_conn_id}
    pub table_name: String,     // Target table name from SQL SELECT
    pub query_id: String,       // User-chosen query identifier
    pub user_id: String,        // Owner of the subscription
    pub query: String,          // SQL query text
    pub options: Option<String>, // JSON: {"last_rows": 50} for initial data fetch
    pub created_at: i64,        // timestamp in milliseconds
    pub updated_at: i64,        // timestamp in milliseconds
    pub changes: i64,           // Total notifications sent
    pub node: String,           // Which node owns the WebSocket
}

/// System.live_queries table provider backed by RocksDB
pub struct LiveQueriesTableProvider {
    kalam_sql: Arc<KalamSql>,
    schema: SchemaRef,
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
    pub fn insert_live_query(&self, live_query: LiveQueryRecord) -> Result<(), KalamDbError> {
        // Convert to kalamdb_sql model
        let sql_live_query = kalamdb_sql::LiveQuery {
            live_id: live_query.live_id,
            connection_id: live_query.connection_id,
            table_name: live_query.table_name,
            query_id: live_query.query_id,
            user_id: live_query.user_id,
            query: live_query.query,
            options: live_query.options.unwrap_or_default(),
            created_at: live_query.created_at,
            updated_at: live_query.updated_at,
            changes: live_query.changes,
            node: live_query.node,
        };
        
        self.kalam_sql
            .insert_live_query(&sql_live_query)
            .map_err(|e| KalamDbError::Other(format!("Failed to insert live query: {}", e)))
    }

    /// Update an existing live query subscription
    pub fn update_live_query(&self, live_query: LiveQueryRecord) -> Result<(), KalamDbError> {
        // Check if live query exists
        let existing = self.get_live_query(&live_query.live_id)?;
        if existing.is_none() {
            return Err(KalamDbError::NotFound(format!(
                "Live query not found: {}",
                live_query.live_id
            )));
        }
        
        // Convert to kalamdb_sql model with updated timestamp
        let sql_live_query = kalamdb_sql::LiveQuery {
            live_id: live_query.live_id,
            connection_id: live_query.connection_id,
            table_name: live_query.table_name,
            query_id: live_query.query_id,
            user_id: live_query.user_id,
            query: live_query.query,
            options: live_query.options.unwrap_or_default(),
            created_at: live_query.created_at,
            updated_at: chrono::Utc::now().timestamp_millis(),
            changes: live_query.changes,
            node: live_query.node,
        };
        
        self.kalam_sql
            .insert_live_query(&sql_live_query) // kalamdb-sql uses insert for both insert and update
            .map_err(|e| KalamDbError::Other(format!("Failed to update live query: {}", e)))
    }

    /// Delete a live query subscription
    pub fn delete_live_query(&self, live_id: &str) -> Result<(), KalamDbError> {
        // TODO: Delete not yet implemented in kalamdb-sql adapter
        Err(KalamDbError::Other(
            "Delete operation not yet implemented in kalamdb-sql adapter".to_string()
        ))
    }

    /// Get a live query by ID
    pub fn get_live_query(&self, live_id: &str) -> Result<Option<LiveQueryRecord>, KalamDbError> {
        let sql_live_query = self.kalam_sql
            .get_live_query(live_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to get live query: {}", e)))?;
        
        match sql_live_query {
            Some(lq) => {
                let live_query = LiveQueryRecord {
                    live_id: lq.live_id,
                    connection_id: lq.connection_id,
                    table_name: lq.table_name,
                    query_id: lq.query_id,
                    user_id: lq.user_id,
                    query: lq.query,
                    options: if lq.options.is_empty() { None } else { Some(lq.options) },
                    created_at: lq.created_at,
                    updated_at: lq.updated_at,
                    changes: lq.changes,
                    node: lq.node,
                };
                Ok(Some(live_query))
            }
            None => Ok(None),
        }
    }

    /// Delete all live queries for a connection_id
    pub fn delete_by_connection_id(&self, connection_id: &str) -> Result<Vec<String>, KalamDbError> {
        // TODO: Delete not yet implemented in kalamdb-sql adapter
        // For now, return error
        Err(KalamDbError::Other(
            "Delete operation not yet implemented in kalamdb-sql adapter".to_string()
        ))
    }

    /// Get all live queries for a user_id
    pub fn get_by_user_id(&self, user_id: &str) -> Result<Vec<LiveQueryRecord>, KalamDbError> {
        let live_queries = self.kalam_sql
            .scan_all_live_queries()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan live queries: {}", e)))?;
        
        let filtered: Vec<LiveQueryRecord> = live_queries
            .into_iter()
            .filter(|lq| lq.user_id == user_id)
            .map(|lq| LiveQueryRecord {
                live_id: lq.live_id,
                connection_id: lq.connection_id,
                table_name: lq.table_name,
                query_id: lq.query_id,
                user_id: lq.user_id,
                query: lq.query,
                options: if lq.options.is_empty() { None } else { Some(lq.options) },
                created_at: lq.created_at,
                updated_at: lq.updated_at,
                changes: lq.changes,
                node: lq.node,
            })
            .collect();
        
        Ok(filtered)
    }

    /// Get all live queries for a table_name
    pub fn get_by_table_name(&self, table_name: &str) -> Result<Vec<LiveQueryRecord>, KalamDbError> {
        let live_queries = self.kalam_sql
            .scan_all_live_queries()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan live queries: {}", e)))?;
        
        let filtered: Vec<LiveQueryRecord> = live_queries
            .into_iter()
            .filter(|lq| lq.table_name == table_name)
            .map(|lq| LiveQueryRecord {
                live_id: lq.live_id,
                connection_id: lq.connection_id,
                table_name: lq.table_name,
                query_id: lq.query_id,
                user_id: lq.user_id,
                query: lq.query,
                options: if lq.options.is_empty() { None } else { Some(lq.options) },
                created_at: lq.created_at,
                updated_at: lq.updated_at,
                changes: lq.changes,
                node: lq.node,
            })
            .collect();
        
        Ok(filtered)
    }

    /// Scan all live queries and return as RecordBatch
    pub fn scan_all_live_queries(&self) -> Result<RecordBatch, KalamDbError> {
        let live_queries = self.kalam_sql
            .scan_all_live_queries()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan live queries: {}", e)))?;
        
        let mut live_ids = StringBuilder::new();
        let mut connection_ids = StringBuilder::new();
        let mut table_names = StringBuilder::new();
        let mut query_ids = StringBuilder::new();
        let mut user_ids = StringBuilder::new();
        let mut queries = StringBuilder::new();
        let mut options_list = StringBuilder::new();
        let mut created_ats = Vec::new();
        let mut updated_ats = Vec::new();
        let mut changes_list = Vec::new();
        let mut nodes = StringBuilder::new();
        
        for live_query in live_queries {
            live_ids.append_value(&live_query.live_id);
            connection_ids.append_value(&live_query.connection_id);
            table_names.append_value(&live_query.table_name);
            query_ids.append_value(&live_query.query_id);
            user_ids.append_value(&live_query.user_id);
            queries.append_value(&live_query.query);
            
            if !live_query.options.is_empty() {
                options_list.append_value(&live_query.options);
            } else {
                options_list.append_null();
            }
            
            created_ats.push(Some(live_query.created_at));
            updated_ats.push(Some(live_query.updated_at));
            changes_list.push(Some(live_query.changes));
            nodes.append_value(&live_query.node);
        }
        
        let batch = RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(live_ids.finish()) as ArrayRef,
                Arc::new(connection_ids.finish()) as ArrayRef,
                Arc::new(table_names.finish()) as ArrayRef,
                Arc::new(query_ids.finish()) as ArrayRef,
                Arc::new(user_ids.finish()) as ArrayRef,
                Arc::new(queries.finish()) as ArrayRef,
                Arc::new(options_list.finish()) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(created_ats)) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(updated_ats)) as ArrayRef,
                Arc::new(Int64Array::from(changes_list)) as ArrayRef,
                Arc::new(nodes.finish()) as ArrayRef,
            ],
        )
        .map_err(|e| KalamDbError::Other(format!("Failed to create RecordBatch: {}", e)))?;
        
        Ok(batch)
    }

    /// Increment changes counter for a live query
    pub fn increment_changes(&self, live_id: &str, timestamp: i64) -> Result<(), KalamDbError> {
        let mut live_query = self.get_live_query(live_id)?
            .ok_or_else(|| KalamDbError::NotFound(format!("Live query not found: {}", live_id)))?;
        
        live_query.changes += 1;
        live_query.updated_at = timestamp;
        
        self.update_live_query(live_query)
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
        _state: &SessionState,
        _projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        // For now, we'll return an error indicating that scanning is not yet implemented
        // This will be implemented in a future task with proper ExecutionPlan
        Err(DataFusionError::NotImplemented(
            "System.live_queries table scanning not yet implemented. Use get_live_query() or scan_all_live_queries() methods instead.".to_string()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::RocksDbInit;
    use tempfile::TempDir;

    fn create_test_provider() -> (LiveQueriesTableProvider, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let init = RocksDbInit::new(temp_dir.path().to_str().unwrap());
        let db = init.open().unwrap();
        let kalam_sql = Arc::new(KalamSql::new(db).unwrap());
        let provider = LiveQueriesTableProvider::new(kalam_sql);
        (provider, temp_dir)
    }

    #[test]
    fn test_insert_and_get_live_query() {
        let (provider, _temp_dir) = create_test_provider();
        
        let live_query = LiveQueryRecord {
            live_id: "user1-conn1-messages-q1".to_string(),
            connection_id: "user1-conn1".to_string(),
            table_name: "messages".to_string(),
            query_id: "q1".to_string(),
            user_id: "user1".to_string(),
            query: "SELECT * FROM messages WHERE _updated > NOW() - INTERVAL '1 hour'".to_string(),
            options: Some(r#"{"last_rows": 50}"#.to_string()),
            created_at: 1000,
            updated_at: 1000,
            changes: 0,
            node: "node1".to_string(),
        };
        
        provider.insert_live_query(live_query.clone()).unwrap();
        
        let retrieved = provider.get_live_query("user1-conn1-messages-q1").unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.live_id, "user1-conn1-messages-q1");
        assert_eq!(retrieved.connection_id, "user1-conn1");
        assert_eq!(retrieved.table_name, "messages");
        assert_eq!(retrieved.query_id, "q1");
    }

    #[test]
    fn test_update_live_query() {
        let (provider, _temp_dir) = create_test_provider();
        
        let mut live_query = LiveQueryRecord {
            live_id: "user1-conn1-messages-q1".to_string(),
            connection_id: "user1-conn1".to_string(),
            table_name: "messages".to_string(),
            query_id: "q1".to_string(),
            user_id: "user1".to_string(),
            query: "SELECT * FROM messages".to_string(),
            options: None,
            created_at: 1000,
            updated_at: 1000,
            changes: 0,
            node: "node1".to_string(),
        };
        
        provider.insert_live_query(live_query.clone()).unwrap();
        
        live_query.changes = 5;
        live_query.updated_at = 2000;
        provider.update_live_query(live_query.clone()).unwrap();
        
        let retrieved = provider.get_live_query("user1-conn1-messages-q1").unwrap().unwrap();
        assert_eq!(retrieved.changes, 5);
        assert_eq!(retrieved.updated_at, 2000);
    }

    #[test]
    #[ignore] // Delete not yet implemented in kalamdb-sql adapter
    fn test_delete_live_query() {
        let (provider, _temp_dir) = create_test_provider();
        
        let live_query = LiveQueryRecord {
            live_id: "user1-conn1-messages-q1".to_string(),
            connection_id: "user1-conn1".to_string(),
            table_name: "messages".to_string(),
            query_id: "q1".to_string(),
            user_id: "user1".to_string(),
            query: "SELECT * FROM messages".to_string(),
            options: None,
            created_at: 1000,
            updated_at: 1000,
            changes: 0,
            node: "node1".to_string(),
        };
        
        provider.insert_live_query(live_query.clone()).unwrap();
        provider.delete_live_query("user1-conn1-messages-q1").unwrap();
        
        let retrieved = provider.get_live_query("user1-conn1-messages-q1").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    #[ignore] // Delete not yet implemented in kalamdb-sql adapter
    fn test_delete_by_connection_id() {
        let (provider, _temp_dir) = create_test_provider();
        
        let live_query1 = LiveQueryRecord {
            live_id: "user1-conn1-messages-q1".to_string(),
            connection_id: "user1-conn1".to_string(),
            table_name: "messages".to_string(),
            query_id: "q1".to_string(),
            user_id: "user1".to_string(),
            query: "SELECT * FROM messages".to_string(),
            options: None,
            created_at: 1000,
            updated_at: 1000,
            changes: 0,
            node: "node1".to_string(),
        };
        
        let live_query2 = LiveQueryRecord {
            live_id: "user1-conn1-notifications-q2".to_string(),
            connection_id: "user1-conn1".to_string(),
            table_name: "notifications".to_string(),
            query_id: "q2".to_string(),
            user_id: "user1".to_string(),
            query: "SELECT * FROM notifications".to_string(),
            options: None,
            created_at: 1000,
            updated_at: 1000,
            changes: 0,
            node: "node1".to_string(),
        };
        
        provider.insert_live_query(live_query1).unwrap();
        provider.insert_live_query(live_query2).unwrap();
        
        let deleted = provider.delete_by_connection_id("user1-conn1").unwrap();
        assert_eq!(deleted.len(), 2);
        
        let retrieved1 = provider.get_live_query("user1-conn1-messages-q1").unwrap();
        let retrieved2 = provider.get_live_query("user1-conn1-notifications-q2").unwrap();
        assert!(retrieved1.is_none());
        assert!(retrieved2.is_none());
    }

    #[test]
    fn test_get_by_user_id() {
        let (provider, _temp_dir) = create_test_provider();
        
        let live_query1 = LiveQueryRecord {
            live_id: "user1-conn1-messages-q1".to_string(),
            connection_id: "user1-conn1".to_string(),
            table_name: "messages".to_string(),
            query_id: "q1".to_string(),
            user_id: "user1".to_string(),
            query: "SELECT * FROM messages".to_string(),
            options: None,
            created_at: 1000,
            updated_at: 1000,
            changes: 0,
            node: "node1".to_string(),
        };
        
        let live_query2 = LiveQueryRecord {
            live_id: "user2-conn2-messages-q1".to_string(),
            connection_id: "user2-conn2".to_string(),
            table_name: "messages".to_string(),
            query_id: "q1".to_string(),
            user_id: "user2".to_string(),
            query: "SELECT * FROM messages".to_string(),
            options: None,
            created_at: 1000,
            updated_at: 1000,
            changes: 0,
            node: "node1".to_string(),
        };
        
        provider.insert_live_query(live_query1).unwrap();
        provider.insert_live_query(live_query2).unwrap();
        
        let user1_queries = provider.get_by_user_id("user1").unwrap();
        assert_eq!(user1_queries.len(), 1);
        assert_eq!(user1_queries[0].user_id, "user1");
    }

    #[test]
    fn test_get_by_table_name() {
        let (provider, _temp_dir) = create_test_provider();
        
        let live_query1 = LiveQueryRecord {
            live_id: "user1-conn1-messages-q1".to_string(),
            connection_id: "user1-conn1".to_string(),
            table_name: "messages".to_string(),
            query_id: "q1".to_string(),
            user_id: "user1".to_string(),
            query: "SELECT * FROM messages".to_string(),
            options: None,
            created_at: 1000,
            updated_at: 1000,
            changes: 0,
            node: "node1".to_string(),
        };
        
        let live_query2 = LiveQueryRecord {
            live_id: "user1-conn1-notifications-q2".to_string(),
            connection_id: "user1-conn1".to_string(),
            table_name: "notifications".to_string(),
            query_id: "q2".to_string(),
            user_id: "user1".to_string(),
            query: "SELECT * FROM notifications".to_string(),
            options: None,
            created_at: 1000,
            updated_at: 1000,
            changes: 0,
            node: "node1".to_string(),
        };
        
        provider.insert_live_query(live_query1).unwrap();
        provider.insert_live_query(live_query2).unwrap();
        
        let messages_queries = provider.get_by_table_name("messages").unwrap();
        assert_eq!(messages_queries.len(), 1);
        assert_eq!(messages_queries[0].table_name, "messages");
    }

    #[test]
    fn test_increment_changes() {
        let (provider, _temp_dir) = create_test_provider();
        
        let live_query = LiveQueryRecord {
            live_id: "user1-conn1-messages-q1".to_string(),
            connection_id: "user1-conn1".to_string(),
            table_name: "messages".to_string(),
            query_id: "q1".to_string(),
            user_id: "user1".to_string(),
            query: "SELECT * FROM messages".to_string(),
            options: None,
            created_at: 1000,
            updated_at: 1000,
            changes: 0,
            node: "node1".to_string(),
        };
        
        provider.insert_live_query(live_query).unwrap();
        
        provider.increment_changes("user1-conn1-messages-q1", 2000).unwrap();
        provider.increment_changes("user1-conn1-messages-q1", 3000).unwrap();
        
        let retrieved = provider.get_live_query("user1-conn1-messages-q1").unwrap().unwrap();
        assert_eq!(retrieved.changes, 2);
        assert_eq!(retrieved.updated_at, 3000);
    }

    #[test]
    fn test_scan_all_live_queries() {
        let (provider, _temp_dir) = create_test_provider();
        
        let live_query1 = LiveQueryRecord {
            live_id: "user1-conn1-messages-q1".to_string(),
            connection_id: "user1-conn1".to_string(),
            table_name: "messages".to_string(),
            query_id: "q1".to_string(),
            user_id: "user1".to_string(),
            query: "SELECT * FROM messages".to_string(),
            options: Some(r#"{"last_rows": 50}"#.to_string()),
            created_at: 1000,
            updated_at: 1000,
            changes: 0,
            node: "node1".to_string(),
        };
        
        let live_query2 = LiveQueryRecord {
            live_id: "user2-conn2-notifications-q2".to_string(),
            connection_id: "user2-conn2".to_string(),
            table_name: "notifications".to_string(),
            query_id: "q2".to_string(),
            user_id: "user2".to_string(),
            query: "SELECT * FROM notifications".to_string(),
            options: None,
            created_at: 2000,
            updated_at: 2000,
            changes: 5,
            node: "node2".to_string(),
        };
        
        provider.insert_live_query(live_query1).unwrap();
        provider.insert_live_query(live_query2).unwrap();
        
        let batch = provider.scan_all_live_queries().unwrap();
        assert_eq!(batch.num_rows(), 2);
        assert_eq!(batch.num_columns(), 11);
    }
}
