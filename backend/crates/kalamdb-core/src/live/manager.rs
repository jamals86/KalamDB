//! Live query manager
//!
//! This module coordinates live query subscriptions, change detection,
//! and real-time notifications to WebSocket clients.

use super::connection_registry::{
    ConnectionId, LiveId, LiveQueryOptions, LiveQueryRegistry, NodeId,
};
use super::filter::{FilterCache, FilterPredicate};
use super::initial_data::{InitialDataFetcher, InitialDataOptions, InitialDataResult};
use super::notification::NotificationService;
use super::query_parser::QueryParser;
use super::subscription::SubscriptionService;
use super::types::{ChangeNotification, RegistryStats, SubscriptionResult};
use crate::error::KalamDbError;
use crate::sql::executor::SqlExecutor;
use datafusion::execution::context::SessionContext;
use kalamdb_commons::models::{NamespaceId, TableId, TableName, UserId};
use kalamdb_commons::system::LiveQuery as SystemLiveQuery;
use kalamdb_system::LiveQueriesTableProvider;
use std::sync::Arc;

/// Live query manager
pub struct LiveQueryManager {
    registry: Arc<tokio::sync::RwLock<LiveQueryRegistry>>,
    live_queries_provider: Arc<LiveQueriesTableProvider>,
    filter_cache: Arc<tokio::sync::RwLock<FilterCache>>,
    initial_data_fetcher: Arc<InitialDataFetcher>,
    schema_registry: Arc<crate::schema_registry::SchemaRegistry>,
    node_id: NodeId,

    // Delegated services
    subscription_service: Arc<SubscriptionService>,
    notification_service: Arc<NotificationService>,
}

impl LiveQueryManager {
    /// Create a new live query manager
    pub fn new(
        live_queries_provider: Arc<LiveQueriesTableProvider>,
        schema_registry: Arc<crate::schema_registry::SchemaRegistry>,
        node_id: NodeId,
        base_session_context: Arc<SessionContext>,
    ) -> Self {
        let registry = Arc::new(tokio::sync::RwLock::new(LiveQueryRegistry::new(
            node_id.clone(),
        )));
        let filter_cache = Arc::new(tokio::sync::RwLock::new(FilterCache::new()));
        let initial_data_fetcher = Arc::new(InitialDataFetcher::new(
            base_session_context,
            schema_registry.clone(),
        ));

        let subscription_service = Arc::new(SubscriptionService::new(
            registry.clone(),
            filter_cache.clone(),
            live_queries_provider.clone(),
            schema_registry.clone(),
            node_id.clone(),
        ));

        let notification_service = Arc::new(NotificationService::new(
            registry.clone(),
            filter_cache.clone(),
            live_queries_provider.clone(),
        ));

        Self {
            registry,
            live_queries_provider,
            filter_cache,
            initial_data_fetcher,
            schema_registry,
            node_id,
            subscription_service,
            notification_service,
        }
    }

    /// Get the node_id for this manager
    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    /// Provide shared SqlExecutor so initial data fetches reuse common execution path
    pub fn set_sql_executor(&self, executor: Arc<SqlExecutor>) {
        self.initial_data_fetcher.set_sql_executor(executor);
    }

    /// Register a new WebSocket connection
    pub async fn register_connection(
        &self,
        user_id: UserId,
        unique_conn_id: String,
        notification_tx: Option<super::connection_registry::NotificationSender>,
    ) -> Result<ConnectionId, KalamDbError> {
        let connection_id = ConnectionId::new(user_id.as_str().to_string(), unique_conn_id);

        let tx = if let Some(tx) = notification_tx {
            tx
        } else {
            let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
            tx
        };
        let registry = self.registry.read().await;
        registry.register_connection(connection_id.clone(), tx);

        Ok(connection_id)
    }

    /// Register a live query subscription
    pub async fn register_subscription(
        &self,
        connection_id: ConnectionId,
        query_id: String,
        query: String,
        options: LiveQueryOptions,
    ) -> Result<LiveId, KalamDbError> {
        self.subscription_service
            .register_subscription(connection_id, query_id, query, options)
            .await
    }

    /// Register a live query subscription with optional initial data fetch
    pub async fn register_subscription_with_initial_data(
        &self,
        connection_id: ConnectionId,
        query_id: String,
        query: String,
        options: LiveQueryOptions,
        initial_data_options: Option<InitialDataOptions>,
    ) -> Result<SubscriptionResult, KalamDbError> {
        // First register the subscription
        let live_id = self
            .register_subscription(connection_id, query_id, query.clone(), options)
            .await?;

        // Fetch initial data if requested
        let initial_data = if let Some(fetch_options) = initial_data_options {
            // Extract table info for initial data fetch
            let raw_table = QueryParser::extract_table_name(&query)?;
            let (namespace, table) = raw_table.split_once('.').ok_or_else(|| {
                KalamDbError::InvalidSql(format!(
                    "Query must reference table as namespace.table: {}",
                    raw_table
                ))
            })?;

            let namespace_id = NamespaceId::from(namespace);
            let table_name = TableName::from(table);
            let table_id = TableId::new(namespace_id.clone(), table_name.clone());
            let table_def = self
                .schema_registry
                .get_table_definition(&table_id)?
                .ok_or_else(|| {
                    KalamDbError::NotFound(format!(
                        "Table {}.{} not found for subscription",
                        namespace, table
                    ))
                })?;

            let live_id_string = live_id.to_string();
            let filter_predicate = {
                let cache = self.filter_cache.read().await;
                cache.get(&live_id_string)
            };

            Some(
                self.initial_data_fetcher
                    .fetch_initial_data(
                        &live_id,
                        &table_id,
                        table_def.table_type,
                        fetch_options,
                        filter_predicate,
                    )
                    .await?,
            )
        } else {
            None
        };

        Ok(SubscriptionResult {
            live_id,
            initial_data,
        })
    }

    /// Fetch a batch of initial data for an existing subscription
    pub async fn fetch_initial_data_batch(
        &self,
        sql: &str,
        user_id: &UserId,
        initial_data_options: Option<InitialDataOptions>,
    ) -> Result<InitialDataResult, KalamDbError> {
        let fetch_options = initial_data_options.ok_or_else(|| {
            KalamDbError::InvalidOperation("Batch options are required".to_string())
        })?;

        // Extract table info from SQL
        let raw_table = QueryParser::extract_table_name(sql)?;
        let (namespace, table) = raw_table.split_once('.').ok_or_else(|| {
            KalamDbError::InvalidSql(format!(
                "Query must reference table as namespace.table: {}",
                raw_table
            ))
        })?;

        let namespace_id = NamespaceId::from(namespace);
        let table_name = TableName::from(table);
        let table_id = TableId::new(namespace_id.clone(), table_name.clone());

        let table_def = self
            .schema_registry
            .get_table_definition(&table_id)?
            .ok_or_else(|| {
                KalamDbError::NotFound(format!(
                    "Table {}.{} not found for batch fetch",
                    namespace, table
                ))
            })?;

        // Create a temporary LiveId for fetching (not actually used by fetcher for filtering)
        let temp_conn_id = ConnectionId::new(
            user_id.as_str().to_string(),
            format!("batch-{}", uuid::Uuid::new_v4()),
        );
        let temp_live_id = LiveId::new(
            temp_conn_id,
            table_id.clone(),
            format!("batch-query-{}", uuid::Uuid::new_v4()),
        );

        // Extract filter predicate from SQL WHERE clause if present
        // For now, we pass None as filter (TODO: parse WHERE clause into FilterPredicate)
        let filter_predicate: Option<Arc<FilterPredicate>> = None;

        self.initial_data_fetcher
            .fetch_initial_data(
                &temp_live_id,
                &table_id,
                table_def.table_type,
                fetch_options,
                filter_predicate,
            )
            .await
    }

    /// Extract table name from SQL query
    pub fn extract_table_name_from_query(&self, query: &str) -> Result<String, KalamDbError> {
        QueryParser::extract_table_name(query)
    }

    /// Unregister a WebSocket connection
    pub async fn unregister_connection(
        &self,
        user_id: &UserId,
        connection_id: &ConnectionId,
    ) -> Result<Vec<LiveId>, KalamDbError> {
        self.subscription_service
            .unregister_connection(user_id, connection_id)
            .await
    }

    /// Unregister a single live query subscription
    pub async fn unregister_subscription(&self, live_id: &LiveId) -> Result<(), KalamDbError> {
        self.subscription_service
            .unregister_subscription(live_id)
            .await
    }

    /// Increment the changes counter for a live query
    pub async fn increment_changes(&self, live_id: &LiveId) -> Result<(), KalamDbError> {
        self.notification_service.increment_changes(live_id).await
    }

    /// Get all subscriptions for a user
    pub async fn get_user_subscriptions(
        &self,
        user_id: &UserId,
    ) -> Result<Vec<SystemLiveQuery>, KalamDbError> {
        self.live_queries_provider
            .get_by_user_id(user_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to get user subscriptions: {}", e)))
    }

    /// Get a specific live query
    pub async fn get_live_query(
        &self,
        live_id: &str,
    ) -> Result<Option<SystemLiveQuery>, KalamDbError> {
        self.live_queries_provider
            .get_live_query(live_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to get live query: {}", e)))
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

    /// Notify subscribers about a table change (fire-and-forget async)
    pub fn notify_table_change_async(
        self: &Arc<Self>,
        user_id: UserId,
        table_id: TableId,
        notification: ChangeNotification,
    ) {
        self.notification_service
            .notify_async(user_id, table_id, notification)
    }

    /// Notify live query subscribers of a table change
    pub async fn notify_table_change(
        &self,
        user_id: &UserId,
        table_id: &TableId,
        change_notification: ChangeNotification,
    ) -> Result<usize, KalamDbError> {
        self.notification_service
            .notify_table_change(user_id, table_id, change_notification)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_context::AppContext;
    use crate::schema_registry::SchemaRegistry;
    use crate::sql::executor::SqlExecutor;
    use crate::test_helpers::{create_test_session, init_test_app_context};
    use kalamdb_commons::datatypes::KalamDataType;
    use kalamdb_commons::models::TableId;
    use kalamdb_commons::schemas::{ColumnDefinition, TableDefinition, TableOptions, TableType};
    use kalamdb_commons::{NamespaceId, TableName};
    use kalamdb_store::RocksDbInit;
    use kalamdb_system::providers::live_queries::LiveQueriesTableProvider;
    use kalamdb_tables::{new_shared_table_store, new_stream_table_store, new_user_table_store};
    // use crate::providers::arrow_json_conversion::json_to_row;
    use crate::providers::arrow_json_conversion::json_to_row;
    use kalamdb_commons::models::Row;
    use tempfile::TempDir;

    fn to_row(v: serde_json::Value) -> Row {
        json_to_row(&v).unwrap()
    }

    async fn create_test_manager() -> (LiveQueryManager, TempDir) {
        init_test_app_context();
        let temp_dir = TempDir::new().unwrap();
        let init = RocksDbInit::with_defaults(temp_dir.path().to_str().unwrap());
        let db = Arc::new(init.open().unwrap());
        let backend: Arc<dyn kalamdb_store::StorageBackend> =
            Arc::new(kalamdb_store::RocksDBBackend::new(Arc::clone(&db)));

        let live_queries_provider = Arc::new(LiveQueriesTableProvider::new(backend.clone()));
        let schema_registry = Arc::new(SchemaRegistry::new(128));
        let base_session_context = create_test_session();
        schema_registry.set_base_session_context(Arc::clone(&base_session_context));

        // Create table stores for testing (using default namespace and table)
        let test_namespace = NamespaceId::new("user1");
        let test_table = TableName::new("messages");
        let _user_table_store = Arc::new(new_user_table_store(
            backend.clone(),
            &test_namespace,
            &test_table,
        ));
        let _shared_table_store = Arc::new(new_shared_table_store(
            backend.clone(),
            &test_namespace,
            &test_table,
        ));
        let _stream_table_store = Arc::new(new_stream_table_store(&test_namespace, &test_table));

        // Create test table definitions via SchemaRegistry
        let messages_table = TableDefinition::new(
            NamespaceId::new("user1"),
            TableName::new("messages"),
            TableType::User,
            vec![
                ColumnDefinition::new(
                    "id",
                    1,
                    KalamDataType::Int,
                    false,
                    true,
                    false,
                    kalamdb_commons::schemas::ColumnDefault::None,
                    None,
                ),
                ColumnDefinition::new(
                    "user_id",
                    2,
                    KalamDataType::Text,
                    true,
                    false,
                    false,
                    kalamdb_commons::schemas::ColumnDefault::None,
                    None,
                ),
            ],
            TableOptions::user(),
            None,
        )
        .unwrap();
        let messages_table_id = TableId::new(
            messages_table.namespace_id.clone(),
            messages_table.table_name.clone(),
        );
        schema_registry
            .put_table_definition(&messages_table_id, &messages_table)
            .unwrap();

        let notifications_table = TableDefinition::new(
            NamespaceId::new("user1"),
            TableName::new("notifications"),
            TableType::User,
            vec![
                ColumnDefinition::new(
                    "id",
                    1,
                    KalamDataType::Int,
                    false,
                    true,
                    false,
                    kalamdb_commons::schemas::ColumnDefault::None,
                    None,
                ),
                ColumnDefinition::new(
                    "user_id",
                    2,
                    KalamDataType::Text,
                    true,
                    false,
                    false,
                    kalamdb_commons::schemas::ColumnDefault::None,
                    None,
                ),
            ],
            TableOptions::user(),
            None,
        )
        .unwrap();
        let notifications_table_id = TableId::new(
            notifications_table.namespace_id.clone(),
            notifications_table.table_name.clone(),
        );
        schema_registry
            .put_table_definition(&notifications_table_id, &notifications_table)
            .unwrap();

        let manager = LiveQueryManager::new(
            live_queries_provider,
            schema_registry,
            NodeId::from("test_node"),
            base_session_context,
        );
        let app_ctx = AppContext::get();
        let sql_executor = Arc::new(SqlExecutor::new(app_ctx, false));
        manager.set_sql_executor(sql_executor);
        (manager, temp_dir)
    }

    #[test]
    fn test_admin_detection_matches_sys_root() {
        assert!(LiveQueryManager::is_admin_user("root"));
        assert!(LiveQueryManager::is_admin_user("ROOT"));
        assert!(LiveQueryManager::is_admin_user("system"));
        assert!(LiveQueryManager::is_admin_user("sys_root"));
        assert!(!LiveQueryManager::is_admin_user("sys_user"));
        assert!(!LiveQueryManager::is_admin_user("user1"));
    }

    #[tokio::test]
    async fn test_register_connection() {
        let (manager, _temp_dir) = create_test_manager().await;
        let user_id = UserId::new("user1".to_string());

        let connection_id = manager
            .register_connection(user_id, "conn1".to_string(), None)
            .await
            .unwrap();

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

        let connection_id = manager
            .register_connection(user_id.clone(), "conn1".to_string(), None)
            .await
            .unwrap();

        let live_id = manager
            .register_subscription(
                connection_id.clone(),
                "q1".to_string(),
                "SELECT * FROM user1.messages WHERE id > 0".to_string(),
                LiveQueryOptions {
                    last_rows: Some(50),
                },
            )
            .await
            .unwrap();

        assert_eq!(live_id.connection_id(), &connection_id);
        assert_eq!(live_id.table_id().to_string(), "user1:messages");
        assert_eq!(live_id.query_id(), "q1");

        let stats = manager.get_stats().await;
        assert_eq!(stats.total_subscriptions, 1);
    }

    #[tokio::test]
    async fn test_extract_table_name() {
        let (manager, _temp_dir) = create_test_manager().await;

        let table_name = manager
            .extract_table_name_from_query("SELECT * FROM user1.messages WHERE id > 0")
            .unwrap();
        assert_eq!(table_name, "user1.messages");

        let table_name = manager
            .extract_table_name_from_query("select id from test.users")
            .unwrap();
        assert_eq!(table_name, "test.users");

        let table_name = manager
            .extract_table_name_from_query("SELECT * FROM \"ns.my_table\" WHERE x = 1")
            .unwrap();
        assert_eq!(table_name, "ns.my_table");
    }

    #[tokio::test]
    async fn test_get_subscriptions_for_table() {
        let (manager, _temp_dir) = create_test_manager().await;
        let user_id = UserId::new("user1".to_string());

        let connection_id = manager
            .register_connection(user_id.clone(), "conn1".to_string(), None)
            .await
            .unwrap();

        manager
            .register_subscription(
                connection_id.clone(),
                "q1".to_string(),
                "SELECT * FROM user1.messages".to_string(),
                LiveQueryOptions::default(),
            )
            .await
            .unwrap();

        manager
            .register_subscription(
                connection_id.clone(),
                "q2".to_string(),
                "SELECT * FROM user1.notifications".to_string(),
                LiveQueryOptions::default(),
            )
            .await
            .unwrap();

        // Get subscriptions from registry
        let registry = manager.registry.read().await;
        let table_id1 = TableId::from_strings("user1", "messages");
        let messages_subs = registry.get_subscriptions_for_table(&user_id, &table_id1);
        assert_eq!(messages_subs.len(), 1);
        assert_eq!(
            messages_subs[0].live_id.table_id().table_name().as_str(),
            "messages"
        );

        let table_id2 = TableId::from_strings("user1", "notifications");
        let notif_subs = registry.get_subscriptions_for_table(&user_id, &table_id2);
        assert_eq!(notif_subs.len(), 1);
        assert_eq!(
            notif_subs[0].live_id.table_id().table_name().as_str(),
            "notifications"
        );
    }

    #[tokio::test]
    async fn test_unregister_connection() {
        let (manager, _temp_dir) = create_test_manager().await;
        let user_id = UserId::new("user1".to_string());

        let connection_id = manager
            .register_connection(user_id.clone(), "conn1".to_string(), None)
            .await
            .unwrap();

        manager
            .register_subscription(
                connection_id.clone(),
                "q1".to_string(),
                "SELECT * FROM user1.messages".to_string(),
                LiveQueryOptions::default(),
            )
            .await
            .unwrap();

        manager
            .register_subscription(
                connection_id.clone(),
                "q2".to_string(),
                "SELECT * FROM user1.notifications".to_string(),
                LiveQueryOptions::default(),
            )
            .await
            .unwrap();

        let removed_live_ids = manager
            .unregister_connection(&user_id, &connection_id)
            .await
            .unwrap();
        assert_eq!(removed_live_ids.len(), 2);

        let stats = manager.get_stats().await;
        assert_eq!(stats.total_connections, 0);
        assert_eq!(stats.total_subscriptions, 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[cfg_attr(not(feature = "expensive_tests"), ignore)]
    async fn test_unregister_subscription() {
        let (manager, _temp_dir) = create_test_manager().await;
        let user_id = UserId::new("user1".to_string());

        let connection_id = manager
            .register_connection(user_id.clone(), "conn1".to_string(), None)
            .await
            .unwrap();

        let live_id = manager
            .register_subscription(
                connection_id.clone(),
                "q1".to_string(),
                "SELECT * FROM user1.messages".to_string(),
                LiveQueryOptions::default(),
            )
            .await
            .unwrap();

        // Add timeout to prevent hanging
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            manager.unregister_subscription(&live_id),
        )
        .await;

        match result {
            Ok(Ok(())) => {
                let stats = manager.get_stats().await;
                assert_eq!(stats.total_subscriptions, 0);
            }
            Ok(Err(e)) => panic!("Unregister failed: {}", e),
            Err(_) => panic!("Test timed out after 5 seconds"),
        }
    }

    #[tokio::test]
    async fn test_increment_changes() {
        let (manager, _temp_dir) = create_test_manager().await;
        let user_id = UserId::new("user1".to_string());

        let connection_id = manager
            .register_connection(user_id.clone(), "conn1".to_string(), None)
            .await
            .unwrap();

        let live_id = manager
            .register_subscription(
                connection_id.clone(),
                "q1".to_string(),
                "SELECT * FROM user1.messages".to_string(),
                LiveQueryOptions::default(),
            )
            .await
            .unwrap();

        manager.increment_changes(&live_id).await.unwrap();
        manager.increment_changes(&live_id).await.unwrap();

        let live_query_record = manager
            .get_live_query(&live_id.to_string())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(live_query_record.changes, 2);
    }

    #[tokio::test]
    async fn test_multi_subscription_support() {
        let (manager, _temp_dir) = create_test_manager().await;
        let user_id = UserId::new("user1".to_string());

        let connection_id = manager
            .register_connection(user_id.clone(), "conn1".to_string(), None)
            .await
            .unwrap();

        // Multiple subscriptions on same connection
        let live_id1 = manager
            .register_subscription(
                connection_id.clone(),
                "messages_query".to_string(),
                "SELECT * FROM user1.messages WHERE conversation_id = 'conv1'".to_string(),
                LiveQueryOptions {
                    last_rows: Some(50),
                },
            )
            .await
            .unwrap();

        let live_id2 = manager
            .register_subscription(
                connection_id.clone(),
                "notifications_query".to_string(),
                "SELECT * FROM user1.notifications WHERE user_id = CURRENT_USER()".to_string(),
                LiveQueryOptions {
                    last_rows: Some(10),
                },
            )
            .await
            .unwrap();

        let live_id3 = manager
            .register_subscription(
                connection_id.clone(),
                "messages_query2".to_string(),
                "SELECT * FROM user1.messages WHERE conversation_id = 'conv2'".to_string(),
                LiveQueryOptions {
                    last_rows: Some(20),
                },
            )
            .await
            .unwrap();

        let stats = manager.get_stats().await;
        assert_eq!(stats.total_connections, 1);
        assert_eq!(stats.total_subscriptions, 3);

        // Verify all subscriptions are tracked
        let registry = manager.registry.read().await;
        let table_id1 = TableId::from_strings("user1", "messages");
        let messages_subs = registry.get_subscriptions_for_table(&user_id, &table_id1);
        assert_eq!(messages_subs.len(), 2); // messages_query and messages_query2

        let table_id2 = TableId::from_strings("user1", "notifications");
        let notif_subs = registry.get_subscriptions_for_table(&user_id, &table_id2);
        assert_eq!(notif_subs.len(), 1);

        // Verify each has unique live_id
        assert_ne!(live_id1.to_string(), live_id2.to_string());
        assert_ne!(live_id1.to_string(), live_id3.to_string());
        assert_ne!(live_id2.to_string(), live_id3.to_string());
    }

    #[tokio::test]
    async fn test_filter_compilation_and_caching() {
        let (manager, _temp_dir) = create_test_manager().await;
        let user_id = UserId::new("user1".to_string());

        let connection_id = manager
            .register_connection(user_id.clone(), "conn1".to_string(), None)
            .await
            .unwrap();

        // Register subscription with WHERE clause
        let live_id = manager
            .register_subscription(
                connection_id.clone(),
                "filtered_messages".to_string(),
                "SELECT * FROM user1.messages WHERE user_id = 'user1' AND read = false".to_string(),
                LiveQueryOptions::default(),
            )
            .await
            .unwrap();

        // Verify filter was compiled and cached
        let filter_cache = manager.filter_cache.read().await;
        let filter = filter_cache.get(&live_id.to_string());
        assert!(filter.is_some());

        // Verify filter SQL is correct (includes auto-injected user_id filter for USER tables)
        let filter = filter.unwrap();
        assert_eq!(
            filter.sql(),
            "user_id = 'user1' AND user_id = 'user1' AND read = false"
        );
    }

    #[tokio::test]
    async fn test_notification_filtering() {
        let (manager, _temp_dir) = create_test_manager().await;
        let user_id = UserId::new("user1".to_string());

        let connection_id = manager
            .register_connection(user_id.clone(), "conn1".to_string(), None)
            .await
            .unwrap();

        // Register subscription with filter
        manager
            .register_subscription(
                connection_id.clone(),
                "q1".to_string(),
                "SELECT * FROM user1.messages WHERE user_id = 'user1'".to_string(),
                LiveQueryOptions::default(),
            )
            .await
            .unwrap();

        // Matching notification
        let matching_change = ChangeNotification::insert(
            TableId::from_strings("user1", "messages"),
            to_row(serde_json::json!({"user_id": "user1", "text": "Hello"})),
        );

        let table_id = TableId::from_strings("user1", "messages");
        let notified = manager
            .notify_table_change(&user_id, &table_id, matching_change)
            .await
            .unwrap();
        assert_eq!(notified, 1); // Should notify

        // Non-matching notification
        let non_matching_change = ChangeNotification::insert(
            TableId::from_strings("user1", "messages"),
            to_row(serde_json::json!({"user_id": "user2", "text": "Hello"})),
        );

        let notified = manager
            .notify_table_change(&user_id, &table_id, non_matching_change)
            .await
            .unwrap();
        assert_eq!(notified, 0); // Should NOT notify (filter didn't match)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[cfg_attr(not(feature = "expensive_tests"), ignore)]
    async fn test_filter_cleanup_on_unsubscribe() {
        let (manager, _temp_dir) = create_test_manager().await;
        let user_id = UserId::new("user1".to_string());

        let connection_id = manager
            .register_connection(user_id.clone(), "conn1".to_string(), None)
            .await
            .unwrap();

        let live_id = manager
            .register_subscription(
                connection_id.clone(),
                "q1".to_string(),
                "SELECT * FROM user1.messages WHERE user_id = 'user1'".to_string(),
                LiveQueryOptions::default(),
            )
            .await
            .unwrap();

        // Verify filter exists
        {
            let filter_cache = manager.filter_cache.read().await;
            assert!(filter_cache.get(&live_id.to_string()).is_some());
        }

        // Try to unregister subscription with timeout
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            manager.unregister_subscription(&live_id),
        )
        .await;

        // Filter cleanup happens first regardless of final success/failure
        match result {
            Ok(_) => {
                // Verify filter was removed (cleanup happens before DB delete)
                let filter_cache = manager.filter_cache.read().await;
                assert!(filter_cache.get(&live_id.to_string()).is_none());
            }
            Err(_) => panic!("Test timed out after 5 seconds"),
        }
    }
}
