//! Live query manager
//!
//! This module coordinates live query subscriptions, change detection,
//! and real-time notifications to WebSocket clients.

use crate::error::KalamDbError;
use crate::live_query::connection_registry::{
    ConnectionId, LiveId, LiveQuery, LiveQueryOptions, LiveQueryRegistry, NodeId, UserId,
};
use crate::live_query::filter::FilterCache;
use crate::live_query::initial_data::{InitialDataFetcher, InitialDataOptions, InitialDataResult};
use crate::tables::system::LiveQueriesTableProvider;
use crate::tables::{SharedTableStore, StreamTableStore, UserTableStore};
use kalamdb_commons::models::{NamespaceId, TableName};
use kalamdb_commons::schemas::TableType;
use kalamdb_commons::system::LiveQuery as SystemLiveQuery;
use kalamdb_commons::LiveQueryId;
use kalamdb_sql::KalamSql;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Live query manager
pub struct LiveQueryManager {
    registry: Arc<tokio::sync::RwLock<LiveQueryRegistry>>,
    live_queries_provider: Arc<LiveQueriesTableProvider>,
    filter_cache: Arc<tokio::sync::RwLock<FilterCache>>,
    initial_data_fetcher: Arc<InitialDataFetcher>,
    kalam_sql: Arc<KalamSql>,
    node_id: NodeId,
}

impl LiveQueryManager {
    /// Create a new live query manager
    pub fn new(
        kalam_sql: Arc<KalamSql>,
        node_id: NodeId,
        user_table_store: Option<Arc<UserTableStore>>,
        _shared_table_store: Option<Arc<SharedTableStore>>,
        stream_table_store: Option<Arc<StreamTableStore>>,
    ) -> Self {
        let registry = Arc::new(tokio::sync::RwLock::new(LiveQueryRegistry::new(
            node_id.clone(),
        )));
        let live_queries_provider =
            Arc::new(LiveQueriesTableProvider::new(kalam_sql.adapter().backend()));
        let filter_cache = Arc::new(tokio::sync::RwLock::new(FilterCache::new()));
        let initial_data_fetcher = Arc::new(InitialDataFetcher::new(
            user_table_store.clone(),
            stream_table_store.clone(),
        ));

        Self {
            registry,
            live_queries_provider,
            filter_cache,
            initial_data_fetcher,
            kalam_sql,
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
        notification_tx: Option<crate::live_query::connection_registry::NotificationSender>,
    ) -> Result<ConnectionId, KalamDbError> {
        let connection_id = ConnectionId::new(user_id.as_str().to_string(), unique_conn_id);

        let mut registry = self.registry.write().await;
        registry.register_connection(user_id, connection_id.clone(), notification_tx);

        Ok(connection_id)
    }

    /// Register a live query subscription
    ///
    /// This is called when a client subscribes to a query. It:
    /// 1. Generates a LiveId
    /// 2. Registers in system.live_queries table
    /// 3. Adds to in-memory registry
    /// 4. Compiles and caches the WHERE clause filter
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
        // Parse SQL to extract table reference and WHERE clause
        let raw_table = self.extract_table_name_from_query(&query)?;
        let (namespace, table) = raw_table.split_once('.').ok_or_else(|| {
            KalamDbError::InvalidSql(format!(
                "Query must reference table as namespace.table: {}",
                raw_table
            ))
        })?;

        let namespace_id = NamespaceId::from(namespace);
        let table_name = TableName::from(table);
        let table_def = self
            .kalam_sql
            .get_table_definition(&namespace_id, &table_name)
            .map_err(|e| {
                KalamDbError::Other(format!(
                    "Failed to load table definition for {}.{}: {}",
                    namespace, table, e
                ))
            })?
            .ok_or_else(|| {
                KalamDbError::NotFound(format!(
                    "Table {}.{} not found for subscription",
                    namespace, table
                ))
            })?;

        if table_def.table_type == TableType::Shared {
            return Err(KalamDbError::InvalidOperation(
                "Shared table subscriptions are not supported".to_string(),
            ));
        }

        let canonical_table = format!("{}.{}", namespace, table);
        let mut where_clause = self.extract_where_clause(&query);

        // Generate LiveId
        let live_id = LiveId::new(connection_id.clone(), canonical_table.clone(), query_id);

        // Auto-inject user_id filter for user tables (row-level security)
        // Skip injection for admin/system users so they can observe all rows.
        if table_def.table_type == TableType::User {
            let user_id = connection_id.user_id();
            let is_admin_like = user_id.eq_ignore_ascii_case("root")
                || user_id.eq_ignore_ascii_case("system");
            if !is_admin_like {
                let user_filter = format!("user_id = '{}'", user_id);
                where_clause = if let Some(existing_clause) = where_clause {
                    Some(format!("{} AND {}", user_filter, existing_clause))
                } else {
                    Some(user_filter)
                };
            }
        }

        // Compile and cache the filter if WHERE clause exists
        if let Some(clause) = where_clause {
            let resolved_clause =
                Self::resolve_where_clause_placeholders(&clause, connection_id.user_id());
            let mut filter_cache = self.filter_cache.write().await;
            filter_cache.insert(live_id.to_string(), &resolved_clause)?;
        }

        let timestamp = Self::current_timestamp_ms();

        // Serialize options to JSON
        let options_json = serde_json::to_string(&options).map_err(|e| {
            KalamDbError::SerializationError(format!("Failed to serialize options: {}", e))
        })?;

        // Create record for system.live_queries
        let live_query_record = SystemLiveQuery {
            live_id: LiveQueryId::new(live_id.to_string()),
            connection_id: connection_id.to_string(),
            namespace_id: NamespaceId::new(namespace.to_string()),
            table_name: TableName::new(canonical_table.clone()),
            query_id: live_id.query_id().to_string(),
            user_id: kalamdb_commons::UserId::new(connection_id.user_id().to_string()),
            query: query.clone(),
            options: Some(options_json),
            created_at: timestamp,
            last_update: timestamp,
            changes: 0,
            node: self.node_id.as_str().to_string(),
        };

        // Insert into system.live_queries
        self.live_queries_provider
            .insert_live_query(live_query_record)?;

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

    fn resolve_where_clause_placeholders(clause: &str, user_id: &str) -> String {
        let replacement = format!("'{}'", user_id);
        clause
            .replace("CURRENT_USER()", &replacement)
            .replace("current_user()", &replacement)
    }

    /// Register a live query subscription with optional initial data fetch
    ///
    /// This is the enhanced version that can fetch initial data before
    /// starting real-time notifications. Use this when clients need to
    /// populate their state before receiving live updates.
    ///
    /// # Arguments
    /// * `connection_id` - The WebSocket connection ID
    /// * `query_id` - User-chosen identifier for this subscription
    /// * `query` - SQL SELECT query
    /// * `options` - Subscription options (user_id filter, etc.)
    /// * `initial_data_options` - Options for initial data fetch (if Some)
    ///
    /// # Returns
    /// SubscriptionResult with LiveId and optional initial data
    pub async fn register_subscription_with_initial_data(
        &self,
        connection_id: ConnectionId,
        query_id: String,
        query: String,
        options: LiveQueryOptions,
        initial_data_options: Option<InitialDataOptions>,
    ) -> Result<SubscriptionResult, KalamDbError> {
        // First register the subscription (reuse existing method)
        let live_id = self
            .register_subscription(connection_id, query_id, query.clone(), options)
            .await?;

        // Fetch initial data if requested
        let initial_data = if let Some(fetch_options) = initial_data_options {
            // Extract table info for initial data fetch
            let raw_table = self.extract_table_name_from_query(&query)?;
            let (namespace, table) = raw_table.split_once('.').ok_or_else(|| {
                KalamDbError::InvalidSql(format!(
                    "Query must reference table as namespace.table: {}",
                    raw_table
                ))
            })?;

            let namespace_id = NamespaceId::from(namespace);
            let table_name = TableName::from(table);
            let table_def = self
                .kalam_sql
                .get_table_definition(&namespace_id, &table_name)
                .map_err(|e| {
                    KalamDbError::Other(format!(
                        "Failed to load table definition for {}.{}: {}",
                        namespace, table, e
                    ))
                })?
                .ok_or_else(|| {
                    KalamDbError::NotFound(format!(
                        "Table {}.{} not found for subscription",
                        namespace, table
                    ))
                })?;

            let canonical_table = format!("{}.{}", namespace, table);

            let live_id_string = live_id.to_string();
            let filter_predicate = {
                let cache = self.filter_cache.read().await;
                cache.get(&live_id_string)
            };

            Some(
                self.initial_data_fetcher
                    .fetch_initial_data(
                        &live_id,
                        &canonical_table,
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

    /// Determine whether any active subscriptions reference the specified table.
    pub async fn has_active_subscriptions_for(&self, table_ref: &str) -> bool {
        let registry = self.registry.read().await;
        registry.users.values().any(|connections| {
            connections.sockets.values().any(|socket| {
                socket
                    .live_queries
                    .values()
                    .any(|live_query| live_query.live_id.table_name() == table_ref)
            })
        })
    }

    /// Extract table name from SQL query
    ///
    /// This is a simple implementation that looks for "FROM table_name"
    /// TODO: Replace with proper DataFusion SQL parsing
    fn extract_table_name_from_query(&self, query: &str) -> Result<String, KalamDbError> {
        let query_upper = query.to_uppercase();
        let from_pos = query_upper.find(" FROM ").ok_or_else(|| {
            KalamDbError::InvalidSql("Query must contain FROM clause".to_string())
        })?;

        let after_from = &query[(from_pos + 6)..]; // Skip " FROM "
        let table_name = after_from
            .split_whitespace()
            .next()
            .ok_or_else(|| KalamDbError::InvalidSql("Invalid table name after FROM".to_string()))?
            .trim_matches(|c| c == '"' || c == '\'' || c == '`')
            .to_string();

        Ok(table_name)
    }

    /// Extract WHERE clause from SQL query
    ///
    /// Returns None if no WHERE clause exists.
    /// This is a simple implementation that looks for "WHERE ..."
    fn extract_where_clause(&self, query: &str) -> Option<String> {
        let query_upper = query.to_uppercase();
        let where_pos = query_upper.find(" WHERE ")?;

        // Get everything after WHERE, handling potential ORDER BY, LIMIT, etc.
        let after_where = &query[(where_pos + 7)..]; // Skip " WHERE "

        // Find the end of the WHERE clause (before ORDER BY, LIMIT, etc.)
        let end_keywords = [" ORDER BY", " LIMIT", " OFFSET", " GROUP BY"];
        let mut end_pos = after_where.len();

        for keyword in &end_keywords {
            if let Some(pos) = after_where.to_uppercase().find(keyword) {
                if pos < end_pos {
                    end_pos = pos;
                }
            }
        }

        Some(after_where[..end_pos].trim().to_string())
    }

    /// Unregister a WebSocket connection
    ///
    /// This should be called when a WebSocket disconnects. It:
    /// 1. Collects all live_ids for this connection
    /// 2. Deletes from system.live_queries
    /// 3. Removes from in-memory registry
    /// 4. Cleans up cached filters
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

        // Remove cached filters for all live queries
        {
            let mut filter_cache = self.filter_cache.write().await;
            for live_id in &live_ids {
                filter_cache.remove(&live_id.to_string());
            }
        }

        // Delete from system.live_queries
        self.live_queries_provider
            .delete_by_connection_id(&connection_id.to_string())?;

        Ok(live_ids)
    }

    /// Unregister a single live query subscription
    ///
    /// This is used by the KILL LIVE QUERY command. It:
    /// 1. Removes cached filter
    /// 2. Removes from in-memory registry
    /// 3. Deletes from system.live_queries
    pub async fn unregister_subscription(&self, live_id: &LiveId) -> Result<(), KalamDbError> {
        // Remove cached filter first (even before checking if live_id exists)
        {
            let mut filter_cache = self.filter_cache.write().await;
            filter_cache.remove(&live_id.to_string());
        }

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
        self.live_queries_provider
            .delete_live_query_str(&live_id.to_string())?;

        Ok(())
    }

    /// Increment the changes counter for a live query
    ///
    /// This should be called each time a notification is sent.
    pub async fn increment_changes(&self, live_id: &LiveId) -> Result<(), KalamDbError> {
        let timestamp = Self::current_timestamp_ms();
        self.live_queries_provider
            .increment_changes(&live_id.to_string(), timestamp)?;

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
    pub async fn get_user_subscriptions(
        &self,
        user_id: &str,
    ) -> Result<Vec<SystemLiveQuery>, KalamDbError> {
        self.live_queries_provider.get_by_user_id(user_id)
    }

    /// Get all subscriptions for a table
    pub async fn get_table_subscriptions(
        &self,
        table_name: &str,
    ) -> Result<Vec<SystemLiveQuery>, KalamDbError> {
        self.live_queries_provider.get_by_table_name(table_name)
    }

    /// Get a specific live query
    pub async fn get_live_query(
        &self,
        live_id: &str,
    ) -> Result<Option<SystemLiveQuery>, KalamDbError> {
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

    /// Notify subscribers about a change to a table (T154)
    ///
    /// This is called after data changes (INSERT/UPDATE/DELETE) to deliver
    /// real-time notifications to subscribed WebSocket connections.
    ///
    /// # Arguments
    /// * `table_name` - The table that changed
    /// * `change_notification` - The change details to send to subscribers
    ///
    /// # Returns
    /// Number of notifications delivered
    ///
    /// # Note
    /// This is a simplified implementation for Phase 12 T154.
    /// Full filtering and WebSocket delivery will be implemented in Phase 14.
    /// Notify live query subscribers of a table change
    ///
    /// This method is called by change detectors when a row is inserted, updated, or deleted.
    /// It applies filters and only notifies subscribers whose WHERE clause matches the changed row.
    ///
    /// # Arguments
    /// * `table_name` - Name of the table that changed
    /// * `change_notification` - Details about the change (type, row data)
    ///
    /// # Returns
    /// Number of subscribers notified (after filtering)
    pub async fn notify_table_change(
        &self,
        table_name: &str,
        change_notification: ChangeNotification,
    ) -> Result<usize, KalamDbError> {
        log::info!(
            "📢 notify_table_change called for table: '{}', change_type: {:?}",
            table_name,
            change_notification.change_type
        );

        // Get filter cache for matching
        let filter_cache = self.filter_cache.read().await;

        // Collect live_ids that need to be notified
        let live_ids_to_notify: Vec<LiveId> = {
            let registry = self.registry.read().await;
            log::info!("📢 Registry has {} users", registry.users.len());
            let mut ids = Vec::new();

            // Iterate through all users
            for (user_id, user_connections) in registry.users.iter() {
                log::info!(
                    "📢 User {} has {} connections",
                    user_id.as_str(),
                    user_connections.sockets.len()
                );
                // Iterate through all connections for this user
                for (conn_id, socket) in user_connections.sockets.iter() {
                    log::info!(
                        "📢 Connection {} has {} live queries",
                        conn_id,
                        socket.live_queries.len()
                    );
                    // Iterate through all live queries on this connection
                    for (live_id, _live_query) in socket.live_queries.iter() {
                        log::info!(
                            "📢 Checking live_id.table_name='{}' against target table='{}'",
                            live_id.table_name(),
                            table_name
                        );
                        // Check if this subscription is for the changed table
                        if live_id.table_name() == table_name {
                            log::info!(
                                "📢 ✓ Table name MATCHED for live_id={}",
                                live_id
                            );
                            // Check filter if one exists
                            if let Some(filter) = filter_cache.get(&live_id.to_string()) {
                                // Apply filter to row data
                                match filter.matches(&change_notification.row_data) {
                                    Ok(true) => {
                                        // Filter matched, include this subscriber
                                        ids.push(live_id.clone());
                                    }
                                    Ok(false) => {
                                        // Filter didn't match, skip this subscriber
                                        log::trace!("Filter didn't match for live_id={}, skipping notification", live_id);
                                    }
                                    Err(e) => {
                                        // Filter evaluation error, log and skip
                                        log::error!(
                                            "Filter evaluation error for live_id={}: {}",
                                            live_id,
                                            e
                                        );
                                    }
                                }
                            } else {
                                // No filter, notify all subscribers
                                log::info!(
                                    "📢 No filter - adding subscriber live_id={}",
                                    live_id
                                );
                                ids.push(live_id.clone());
                            }
                        } else {
                            log::info!(
                                "📢 ✗ Table name MISMATCH: live_id.table='{}' != target='{}'",
                                live_id.table_name(),
                                table_name
                            );
                        }
                    }
                }
            }

            ids
        }; // registry read lock is dropped here

        log::debug!(
            "📢 Found {} subscribers to notify",
            live_ids_to_notify.len()
        );

        // Drop filter cache read lock before acquiring write locks
        drop(filter_cache);

        // Now send notifications and increment changes for each live_id
        let notification_count = live_ids_to_notify.len();
        for live_id in live_ids_to_notify.iter() {
            // Send notification to WebSocket client
            if let Some(tx) = self.get_notification_sender(live_id).await {
                // Convert row data from serde_json::Value to HashMap
                let row_map = if let Some(obj) = change_notification.row_data.as_object() {
                    obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                } else {
                    std::collections::HashMap::new()
                };

                // Build the typed notification
                let notification = match change_notification.change_type {
                    ChangeType::Insert => {
                        kalamdb_commons::Notification::insert(live_id.to_string(), vec![row_map])
                    }
                    ChangeType::Update => {
                        let old_map = if let Some(old_data) = &change_notification.old_data {
                            if let Some(obj) = old_data.as_object() {
                                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                            } else {
                                std::collections::HashMap::new()
                            }
                        } else {
                            std::collections::HashMap::new()
                        };
                        kalamdb_commons::Notification::update(
                            live_id.to_string(),
                            vec![row_map],
                            vec![old_map],
                        )
                    }
                    ChangeType::Delete => {
                        kalamdb_commons::Notification::delete(live_id.to_string(), vec![row_map])
                    }
                    ChangeType::Flush => {
                        // For flush, we use insert type with flush metadata
                        kalamdb_commons::Notification::insert(live_id.to_string(), vec![row_map])
                    }
                };

                // Send notification through channel (non-blocking)
                if let Err(e) = tx.send((live_id.clone(), notification)) {
                    log::error!(
                        "Failed to send notification to WebSocket client for live_id={}: {}",
                        live_id,
                        e
                    );
                } else {
                    log::debug!(
                        "Notification sent to WebSocket client for live_id={}",
                        live_id
                    );
                }
            } else {
                log::warn!(
                    "No notification sender found for live_id={}",
                    live_id
                );
            }

            self.increment_changes(live_id).await?;

            // Log notification (in production, use proper logging)
            #[cfg(debug_assertions)]
            eprintln!(
                "Notified subscriber: live_id={}, change_type={:?}",
                live_id,
                change_notification.change_type
            );
        }

        Ok(notification_count)
    }

    /// Get the notification sender for a specific live query
    async fn get_notification_sender(
        &self,
        live_id: &LiveId,
    ) -> Option<crate::live_query::connection_registry::NotificationSender> {
        let registry = self.registry.read().await;
        let user_id = UserId::new(live_id.user_id().to_string());

        if let Some(user_connections) = registry.users.get(&user_id) {
            if let Some(socket) = user_connections.get_socket(&live_id.connection_id) {
                return socket.notification_tx.clone();
            }
        }

        None
    }
}

/// Change notification for live query subscribers
#[derive(Debug, Clone)]
pub struct ChangeNotification {
    pub change_type: ChangeType,
    pub table_name: String,
    pub row_data: serde_json::Value,
    pub old_data: Option<serde_json::Value>, // For UPDATE notifications
    pub row_id: Option<String>,              // For DELETE notifications (hard delete)
}

impl ChangeNotification {
    /// Create an INSERT notification
    pub fn insert(table_name: String, row_data: serde_json::Value) -> Self {
        Self {
            change_type: ChangeType::Insert,
            table_name,
            row_data,
            old_data: None,
            row_id: None,
        }
    }

    /// Create an UPDATE notification with old and new values
    pub fn update(
        table_name: String,
        old_data: serde_json::Value,
        new_data: serde_json::Value,
    ) -> Self {
        Self {
            change_type: ChangeType::Update,
            table_name,
            row_data: new_data,
            old_data: Some(old_data),
            row_id: None,
        }
    }

    /// Create a DELETE notification (soft delete with data)
    pub fn delete_soft(table_name: String, row_data: serde_json::Value) -> Self {
        Self {
            change_type: ChangeType::Delete,
            table_name,
            row_data,
            old_data: None,
            row_id: None,
        }
    }

    /// Create a DELETE notification (hard delete, row_id only)
    pub fn delete_hard(table_name: String, row_id: String) -> Self {
        Self {
            change_type: ChangeType::Delete,
            table_name,
            row_data: serde_json::Value::Null,
            old_data: None,
            row_id: Some(row_id),
        }
    }

    /// Create a FLUSH notification (Parquet flush completion)
    pub fn flush(table_name: String, row_count: usize, parquet_files: Vec<String>) -> Self {
        Self {
            change_type: ChangeType::Flush,
            table_name,
            row_data: serde_json::json!({
                "row_count": row_count,
                "parquet_files": parquet_files,
                "flushed_at": chrono::Utc::now().timestamp_millis(),
            }),
            old_data: None,
            row_id: None,
        }
    }
}

/// Result of registering a live query subscription with initial data
#[derive(Debug, Clone)]
pub struct SubscriptionResult {
    /// The generated LiveId for the subscription
    pub live_id: LiveId,

    /// Initial data returned with the subscription (if requested)
    pub initial_data: Option<InitialDataResult>,
}

/// Type of change that occurred
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    Insert,
    Update,
    Delete,
    Flush, // Parquet flush completion
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
    use crate::tables::{new_shared_table_store, new_stream_table_store, new_user_table_store};
    use kalamdb_commons::schemas::{ColumnDefinition, TableDefinition, TableOptions, TableType};
    use kalamdb_commons::types::KalamDataType;
    use kalamdb_commons::{NamespaceId, TableName};
    use kalamdb_store::RocksDbInit;
    use tempfile::TempDir;

    async fn create_test_manager() -> (LiveQueryManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let init = RocksDbInit::new(temp_dir.path().to_str().unwrap());
        let db = Arc::new(init.open().unwrap());
        let backend: Arc<dyn kalamdb_store::StorageBackend> =
            Arc::new(kalamdb_store::RocksDBBackend::new(Arc::clone(&db)));
        let kalam_sql = Arc::new(KalamSql::new(backend.clone()).unwrap());

        // Create table stores for testing (using default namespace and table)
        let test_namespace = NamespaceId::new("user1");
        let test_table = TableName::new("messages");
        let user_table_store = Arc::new(new_user_table_store(
            backend.clone(),
            &test_namespace,
            &test_table,
        ));
        let shared_table_store = Arc::new(new_shared_table_store(
            backend.clone(),
            &test_namespace,
            &test_table,
        ));
        let stream_table_store = Arc::new(new_stream_table_store(&test_namespace, &test_table));

        // Create test tables in information_schema_tables using NEW schema
        let messages_table = TableDefinition::new(
            "user1",
            "messages",
            TableType::User,
            vec![
                ColumnDefinition::new(
                    "id",
                    1,
                    KalamDataType::Int,
                    false, // not nullable
                    true,  // is primary key
                    false,
                    kalamdb_commons::schemas::ColumnDefault::None,
                    None,
                ),
                ColumnDefinition::new(
                    "user_id",
                    2,
                    KalamDataType::Text,
                    true, // nullable
                    false,
                    false,
                    kalamdb_commons::schemas::ColumnDefault::None,
                    None,
                ),
            ],
            TableOptions::user(),
            None,
        ).unwrap();
        kalam_sql.upsert_table_definition(&messages_table).unwrap();

        let notifications_table = TableDefinition::new(
            "user1",
            "notifications",
            TableType::User,
            vec![
                ColumnDefinition::new(
                    "id",
                    1,
                    KalamDataType::Int,
                    false, // not nullable
                    true,  // is primary key
                    false,
                    kalamdb_commons::schemas::ColumnDefault::None,
                    None,
                ),
                ColumnDefinition::new(
                    "user_id",
                    2,
                    KalamDataType::Text,
                    true, // nullable
                    false,
                    false,
                    kalamdb_commons::schemas::ColumnDefault::None,
                    None,
                ),
            ],
            TableOptions::user(),
            None,
        ).unwrap();
        kalam_sql.upsert_table_definition(&messages_table).unwrap();

        let notifications_table = TableDefinition::new(
            "user1",
            "notifications",
            TableType::User,
            vec![
                ColumnDefinition::new(
                    "id",
                    1,
                    KalamDataType::Int,
                    false, // not nullable
                    true,  // is primary key
                    false,
                    kalamdb_commons::schemas::ColumnDefault::None,
                    None,
                ),
                ColumnDefinition::new(
                    "user_id",
                    2,
                    KalamDataType::Text,
                    true, // nullable
                    false,
                    false,
                    kalamdb_commons::schemas::ColumnDefault::None,
                    None,
                ),
            ],
            TableOptions::user(),
            None,
        ).unwrap();
        kalam_sql
            .upsert_table_definition(&notifications_table)
            .unwrap();

        let node_id = NodeId::new("test_node".to_string());
        let manager = LiveQueryManager::new(
            kalam_sql,
            node_id,
            Some(user_table_store),
            Some(shared_table_store),
            Some(stream_table_store),
        );
        (manager, temp_dir)
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
        assert_eq!(live_id.table_name(), "user1.messages");
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

        let messages_subs = manager
            .get_subscriptions_for_table(&user_id, "user1.messages")
            .await;
        assert_eq!(messages_subs.len(), 1);
        assert_eq!(messages_subs[0].table_name(), "user1.messages");

        let notif_subs = manager
            .get_subscriptions_for_table(&user_id, "user1.notifications")
            .await;
        assert_eq!(notif_subs.len(), 1);
        assert_eq!(notif_subs[0].table_name(), "user1.notifications");
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

    #[tokio::test]
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

        manager.unregister_subscription(&live_id).await.unwrap();

        let stats = manager.get_stats().await;
        assert_eq!(stats.total_subscriptions, 0);
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
        let messages_subs = manager
            .get_subscriptions_for_table(&user_id, "user1.messages")
            .await;
        assert_eq!(messages_subs.len(), 2); // messages_query and messages_query2

        let notif_subs = manager
            .get_subscriptions_for_table(&user_id, "user1.notifications")
            .await;
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
            "user1.messages".to_string(),
            serde_json::json!({"user_id": "user1", "text": "Hello"}),
        );

        let notified = manager
            .notify_table_change("user1.messages", matching_change)
            .await
            .unwrap();
        assert_eq!(notified, 1); // Should notify

        // Non-matching notification
        let non_matching_change = ChangeNotification::insert(
            "user1.messages".to_string(),
            serde_json::json!({"user_id": "user2", "text": "Hello"}),
        );

        let notified = manager
            .notify_table_change("user1.messages", non_matching_change)
            .await
            .unwrap();
        assert_eq!(notified, 0); // Should NOT notify (filter didn't match)
    }

    #[tokio::test]
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

        // Try to unregister subscription (will fail due to delete not implemented in kalamdb-sql)
        // But filter cleanup happens first, so we can verify it worked
        let _ = manager.unregister_subscription(&live_id).await;

        // Verify filter was removed (cleanup happens before DB delete)
        {
            let filter_cache = manager.filter_cache.read().await;
            assert!(filter_cache.get(&live_id.to_string()).is_none());
        }
    }
}
