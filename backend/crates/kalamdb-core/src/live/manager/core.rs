//! Live query manager core implementation

use crate::live::connection_registry::{
    ConnectionId, LiveId, LiveQueryOptions, LiveQueryRegistry, NodeId, NotificationSender,
};
use crate::live::filter::{FilterCache, FilterPredicate};
use crate::live::initial_data::{InitialDataFetcher, InitialDataOptions, InitialDataResult};
use crate::live::notification::NotificationService;
use crate::live::query_parser::QueryParser;
use crate::live::subscription::SubscriptionService;
use crate::live::types::{ChangeNotification, RegistryStats, SubscriptionResult};
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
        notification_tx: Option<NotificationSender>,
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

    /// Get the filter cache (for testing)
    #[cfg(test)]
    pub fn filter_cache(&self) -> Arc<tokio::sync::RwLock<FilterCache>> {
        Arc::clone(&self.filter_cache)
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
