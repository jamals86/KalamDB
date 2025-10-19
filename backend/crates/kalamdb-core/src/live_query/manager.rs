//! Live query manager
//!
//! This module coordinates live query subscriptions, change detection,
//! and real-time notifications to WebSocket clients.

use kalamdb_sql::KalamSql;
use crate::error::KalamDbError;
use crate::live_query::connection_registry::{
    ConnectionId, LiveId, LiveQuery, LiveQueryOptions, LiveQueryRegistry, NodeId, UserId,
};
use crate::tables::system::live_queries_provider::{LiveQueryRecord, LiveQueriesTableProvider};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Live query manager
pub struct LiveQueryManager {
    registry: Arc<tokio::sync::RwLock<LiveQueryRegistry>>,
    live_queries_provider: Arc<LiveQueriesTableProvider>,
    node_id: NodeId,
}

impl LiveQueryManager {
    /// Create a new live query manager
    pub fn new(kalam_sql: Arc<KalamSql>, node_id: NodeId) -> Self {
        let registry = Arc::new(tokio::sync::RwLock::new(LiveQueryRegistry::new(node_id.clone())));
        let live_queries_provider = Arc::new(LiveQueriesTableProvider::new(kalam_sql));
        
        Self {
            registry,
            live_queries_provider,
            node_id,
        }
    }

    /// Get current timestamp in milliseconds
    fn current_timestamp_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }

    /// Register a new WebSocket connection
    ///
    /// This should be called when a WebSocket connection is established.
    /// It creates a new connection entry in the in-memory registry.
    pub async fn register_connection(
        &self,
        user_id: UserId,
        unique_conn_id: String,
    ) -> Result<ConnectionId, KalamDbError> {
        let connection_id = ConnectionId::new(user_id.as_str().to_string(), unique_conn_id);
        
        let mut registry = self.registry.write().await;
        registry.register_connection(user_id, connection_id.clone());
        
        Ok(connection_id)
    }

    /// Register a live query subscription
    ///
    /// This is called when a client subscribes to a query. It:
    /// 1. Generates a LiveId
    /// 2. Registers in system.live_queries table
    /// 3. Adds to in-memory registry
    ///
    /// # Arguments
    /// * `connection_id` - The WebSocket connection ID
    /// * `query_id` - User-chosen identifier for this subscription
    /// * `query` - SQL SELECT query
    /// * `options` - Subscription options (e.g., last_rows for initial data)
    pub async fn register_subscription(
        &self,
        connection_id: ConnectionId,
        query_id: String,
        query: String,
        options: LiveQueryOptions,
    ) -> Result<LiveId, KalamDbError> {
        // Parse SQL to extract table_name
        // TODO: Implement proper SQL parsing with DataFusion
        let table_name = self.extract_table_name_from_query(&query)?;
        
        // Generate LiveId
        let live_id = LiveId::new(connection_id.clone(), table_name, query_id);
        
        let timestamp = Self::current_timestamp_ms();
        
        // Serialize options to JSON
        let options_json = serde_json::to_string(&options)
            .map_err(|e| KalamDbError::SerializationError(format!(
                "Failed to serialize options: {}",
                e
            )))?;
        
        // Create record for system.live_queries
        let live_query_record = LiveQueryRecord {
            live_id: live_id.to_string(),
            connection_id: connection_id.to_string(),
            table_name: live_id.table_name().to_string(),
            query_id: live_id.query_id().to_string(),
            user_id: connection_id.user_id().to_string(),
            query: query.clone(),
            options: Some(options_json),
            created_at: timestamp,
            updated_at: timestamp,
            changes: 0,
            node: self.node_id.as_str().to_string(),
        };
        
        // Insert into system.live_queries
        self.live_queries_provider.insert_live_query(live_query_record)?;
        
        // Add to in-memory registry
        let user_id = UserId::new(connection_id.user_id().to_string());
        let live_query = LiveQuery {
            live_id: live_id.clone(),
            query,
            options,
            changes: 0,
        };
        
        let mut registry = self.registry.write().await;
        registry.register_subscription(&user_id, live_query)?;
        
        Ok(live_id)
    }

    /// Extract table name from SQL query
    ///
    /// This is a simple implementation that looks for "FROM table_name"
    /// TODO: Replace with proper DataFusion SQL parsing
    fn extract_table_name_from_query(&self, query: &str) -> Result<String, KalamDbError> {
        let query_upper = query.to_uppercase();
        let from_pos = query_upper.find(" FROM ")
            .ok_or_else(|| KalamDbError::InvalidSql(
                "Query must contain FROM clause".to_string()
            ))?;
        
        let after_from = &query[(from_pos + 6)..];  // Skip " FROM "
        let table_name = after_from
            .split_whitespace()
            .next()
            .ok_or_else(|| KalamDbError::InvalidSql(
                "Invalid table name after FROM".to_string()
            ))?
            .trim_matches(|c| c == '"' || c == '\'' || c == '`')
            .to_string();
        
        Ok(table_name)
    }

    /// Unregister a WebSocket connection
    ///
    /// This should be called when a WebSocket disconnects. It:
    /// 1. Collects all live_ids for this connection
    /// 2. Deletes from system.live_queries
    /// 3. Removes from in-memory registry
    pub async fn unregister_connection(
        &self,
        user_id: &UserId,
        connection_id: &ConnectionId,
    ) -> Result<Vec<LiveId>, KalamDbError> {
        // Remove from in-memory registry and get all live_ids
        let live_ids = {
            let mut registry = self.registry.write().await;
            registry.unregister_connection(user_id, connection_id)
        };
        
        // Delete from system.live_queries
        self.live_queries_provider.delete_by_connection_id(&connection_id.to_string())?;
        
        Ok(live_ids)
    }

    /// Unregister a single live query subscription
    ///
    /// This is used by the KILL LIVE QUERY command. It:
    /// 1. Removes from in-memory registry
    /// 2. Deletes from system.live_queries
    pub async fn unregister_subscription(
        &self,
        live_id: &LiveId,
    ) -> Result<(), KalamDbError> {
        // Remove from in-memory registry
        let connection_id = {
            let mut registry = self.registry.write().await;
            registry.unregister_subscription(live_id)
        };
        
        if connection_id.is_none() {
            return Err(KalamDbError::NotFound(format!(
                "Live query not found: {}",
                live_id
            )));
        }
        
        // Delete from system.live_queries
        self.live_queries_provider.delete_live_query(&live_id.to_string())?;
        
        Ok(())
    }

    /// Increment the changes counter for a live query
    ///
    /// This should be called each time a notification is sent.
    pub async fn increment_changes(&self, live_id: &LiveId) -> Result<(), KalamDbError> {
        let timestamp = Self::current_timestamp_ms();
        self.live_queries_provider.increment_changes(&live_id.to_string(), timestamp)?;
        
        // Also update in-memory counter
        let user_id = UserId::new(live_id.user_id().to_string());
        let mut registry = self.registry.write().await;
        
        if let Some(user_connections) = registry.users.get_mut(&user_id) {
            if let Some(socket) = user_connections.get_socket_mut(&live_id.connection_id) {
                if let Some(live_query) = socket.live_queries.get_mut(live_id) {
                    live_query.changes += 1;
                }
            }
        }
        
        Ok(())
    }

    /// Get all subscriptions for a specific table and user
    ///
    /// This is used during change detection to find which subscriptions
    /// need to be notified when data changes in a table.
    pub async fn get_subscriptions_for_table(
        &self,
        user_id: &UserId,
        table_name: &str,
    ) -> Vec<LiveId> {
        let registry = self.registry.read().await;
        registry
            .get_subscriptions_for_table(user_id, table_name)
            .into_iter()
            .map(|lq| lq.live_id.clone())
            .collect()
    }

    /// Get all subscriptions for a user
    pub async fn get_user_subscriptions(&self, user_id: &str) -> Result<Vec<LiveQueryRecord>, KalamDbError> {
        self.live_queries_provider.get_by_user_id(user_id)
    }

    /// Get all subscriptions for a table
    pub async fn get_table_subscriptions(&self, table_name: &str) -> Result<Vec<LiveQueryRecord>, KalamDbError> {
        self.live_queries_provider.get_by_table_name(table_name)
    }

    /// Get a specific live query
    pub async fn get_live_query(&self, live_id: &str) -> Result<Option<LiveQueryRecord>, KalamDbError> {
        self.live_queries_provider.get_live_query(live_id)
    }

    /// Get registry statistics
    pub async fn get_stats(&self) -> RegistryStats {
        let registry = self.registry.read().await;
        RegistryStats {
            total_connections: registry.total_connections(),
            total_subscriptions: registry.total_subscriptions(),
            node_id: self.node_id.as_str().to_string(),
        }
    }

    /// Get the registry (for advanced use cases)
    pub fn registry(&self) -> Arc<tokio::sync::RwLock<LiveQueryRegistry>> {
        Arc::clone(&self.registry)
    }
}

/// Registry statistics
#[derive(Debug, Clone)]
pub struct RegistryStats {
    pub total_connections: usize,
    pub total_subscriptions: usize,
    pub node_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::RocksDbInit;
    use tempfile::TempDir;

    async fn create_test_manager() -> (LiveQueryManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let init = RocksDbInit::new(temp_dir.path().to_str().unwrap());
        let db = init.open().unwrap();
        let kalam_sql = Arc::new(KalamSql::new(db).unwrap());
        let node_id = NodeId::new("test_node".to_string());
        let manager = LiveQueryManager::new(kalam_sql, node_id);
        (manager, temp_dir)
    }

    #[tokio::test]
    async fn test_register_connection() {
        let (manager, _temp_dir) = create_test_manager().await;
        let user_id = UserId::new("user1".to_string());
        
        let connection_id = manager.register_connection(user_id, "conn1".to_string()).await.unwrap();
        
        assert_eq!(connection_id.user_id(), "user1");
        assert_eq!(connection_id.unique_conn_id(), "conn1");
        
        let stats = manager.get_stats().await;
        assert_eq!(stats.total_connections, 1);
        assert_eq!(stats.total_subscriptions, 0);
    }

    #[tokio::test]
    async fn test_register_subscription() {
        let (manager, _temp_dir) = create_test_manager().await;
        let user_id = UserId::new("user1".to_string());
        
        let connection_id = manager.register_connection(user_id.clone(), "conn1".to_string()).await.unwrap();
        
        let live_id = manager.register_subscription(
            connection_id.clone(),
            "q1".to_string(),
            "SELECT * FROM messages WHERE id > 0".to_string(),
            LiveQueryOptions { last_rows: Some(50) },
        ).await.unwrap();
        
        assert_eq!(live_id.connection_id(), &connection_id);
        assert_eq!(live_id.table_name(), "messages");
        assert_eq!(live_id.query_id(), "q1");
        
        let stats = manager.get_stats().await;
        assert_eq!(stats.total_subscriptions, 1);
    }

    #[tokio::test]
    async fn test_extract_table_name() {
        let (manager, _temp_dir) = create_test_manager().await;
        
        let table_name = manager.extract_table_name_from_query("SELECT * FROM messages WHERE id > 0").unwrap();
        assert_eq!(table_name, "messages");
        
        let table_name = manager.extract_table_name_from_query("select id from users").unwrap();
        assert_eq!(table_name, "users");
        
        let table_name = manager.extract_table_name_from_query("SELECT * FROM \"my_table\" WHERE x = 1").unwrap();
        assert_eq!(table_name, "my_table");
    }

    #[tokio::test]
    async fn test_get_subscriptions_for_table() {
        let (manager, _temp_dir) = create_test_manager().await;
        let user_id = UserId::new("user1".to_string());
        
        let connection_id = manager.register_connection(user_id.clone(), "conn1".to_string()).await.unwrap();
        
        manager.register_subscription(
            connection_id.clone(),
            "q1".to_string(),
            "SELECT * FROM messages".to_string(),
            LiveQueryOptions::default(),
        ).await.unwrap();
        
        manager.register_subscription(
            connection_id.clone(),
            "q2".to_string(),
            "SELECT * FROM notifications".to_string(),
            LiveQueryOptions::default(),
        ).await.unwrap();
        
        let messages_subs = manager.get_subscriptions_for_table(&user_id, "messages").await;
        assert_eq!(messages_subs.len(), 1);
        assert_eq!(messages_subs[0].table_name(), "messages");
        
        let notif_subs = manager.get_subscriptions_for_table(&user_id, "notifications").await;
        assert_eq!(notif_subs.len(), 1);
        assert_eq!(notif_subs[0].table_name(), "notifications");
    }

    #[tokio::test]
    async fn test_unregister_connection() {
        let (manager, _temp_dir) = create_test_manager().await;
        let user_id = UserId::new("user1".to_string());
        
        let connection_id = manager.register_connection(user_id.clone(), "conn1".to_string()).await.unwrap();
        
        manager.register_subscription(
            connection_id.clone(),
            "q1".to_string(),
            "SELECT * FROM messages".to_string(),
            LiveQueryOptions::default(),
        ).await.unwrap();
        
        manager.register_subscription(
            connection_id.clone(),
            "q2".to_string(),
            "SELECT * FROM notifications".to_string(),
            LiveQueryOptions::default(),
        ).await.unwrap();
        
        let removed_live_ids = manager.unregister_connection(&user_id, &connection_id).await.unwrap();
        assert_eq!(removed_live_ids.len(), 2);
        
        let stats = manager.get_stats().await;
        assert_eq!(stats.total_connections, 0);
        assert_eq!(stats.total_subscriptions, 0);
    }

    #[tokio::test]
    async fn test_unregister_subscription() {
        let (manager, _temp_dir) = create_test_manager().await;
        let user_id = UserId::new("user1".to_string());
        
        let connection_id = manager.register_connection(user_id.clone(), "conn1".to_string()).await.unwrap();
        
        let live_id = manager.register_subscription(
            connection_id.clone(),
            "q1".to_string(),
            "SELECT * FROM messages".to_string(),
            LiveQueryOptions::default(),
        ).await.unwrap();
        
        manager.unregister_subscription(&live_id).await.unwrap();
        
        let stats = manager.get_stats().await;
        assert_eq!(stats.total_subscriptions, 0);
    }

    #[tokio::test]
    async fn test_increment_changes() {
        let (manager, _temp_dir) = create_test_manager().await;
        let user_id = UserId::new("user1".to_string());
        
        let connection_id = manager.register_connection(user_id.clone(), "conn1".to_string()).await.unwrap();
        
        let live_id = manager.register_subscription(
            connection_id.clone(),
            "q1".to_string(),
            "SELECT * FROM messages".to_string(),
            LiveQueryOptions::default(),
        ).await.unwrap();
        
        manager.increment_changes(&live_id).await.unwrap();
        manager.increment_changes(&live_id).await.unwrap();
        
        let live_query_record = manager.get_live_query(&live_id.to_string()).await.unwrap().unwrap();
        assert_eq!(live_query_record.changes, 2);
    }

    #[tokio::test]
    async fn test_multi_subscription_support() {
        let (manager, _temp_dir) = create_test_manager().await;
        let user_id = UserId::new("user1".to_string());
        
        let connection_id = manager.register_connection(user_id.clone(), "conn1".to_string()).await.unwrap();
        
        // Multiple subscriptions on same connection
        let live_id1 = manager.register_subscription(
            connection_id.clone(),
            "messages_query".to_string(),
            "SELECT * FROM messages WHERE conversation_id = 'conv1'".to_string(),
            LiveQueryOptions { last_rows: Some(50) },
        ).await.unwrap();
        
        let live_id2 = manager.register_subscription(
            connection_id.clone(),
            "notifications_query".to_string(),
            "SELECT * FROM notifications WHERE user_id = CURRENT_USER()".to_string(),
            LiveQueryOptions { last_rows: Some(10) },
        ).await.unwrap();
        
        let live_id3 = manager.register_subscription(
            connection_id.clone(),
            "messages_query2".to_string(),
            "SELECT * FROM messages WHERE conversation_id = 'conv2'".to_string(),
            LiveQueryOptions { last_rows: Some(20) },
        ).await.unwrap();
        
        let stats = manager.get_stats().await;
        assert_eq!(stats.total_connections, 1);
        assert_eq!(stats.total_subscriptions, 3);
        
        // Verify all subscriptions are tracked
        let messages_subs = manager.get_subscriptions_for_table(&user_id, "messages").await;
        assert_eq!(messages_subs.len(), 2); // messages_query and messages_query2
        
        let notif_subs = manager.get_subscriptions_for_table(&user_id, "notifications").await;
        assert_eq!(notif_subs.len(), 1);
        
        // Verify each has unique live_id
        assert_ne!(live_id1.to_string(), live_id2.to_string());
        assert_ne!(live_id1.to_string(), live_id3.to_string());
        assert_ne!(live_id2.to_string(), live_id3.to_string());
    }
}
