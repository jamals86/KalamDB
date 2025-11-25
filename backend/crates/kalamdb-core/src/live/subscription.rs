//! Subscription service for live queries
//!
//! Handles registration and unregistration of live query subscriptions,
//! including permission checks, filter compilation, and system table updates.

use super::connection_registry::{
    ConnectionId, LiveQueryId, LiveQueryOptions, LiveQueryRegistry, NodeId,
};
use super::filter::FilterCache;
use crate::error::KalamDbError;
use kalamdb_commons::models::UserId;
use kalamdb_commons::system::LiveQuery as SystemLiveQuery;
use kalamdb_system::LiveQueriesTableProvider;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::task::spawn_blocking;

/// Service for managing subscriptions
///
/// Uses Arc<LiveQueryRegistry> directly since LiveQueryRegistry internally
/// uses DashMap for lock-free concurrent access - no RwLock wrapper needed.
pub struct SubscriptionService {
    /// Registry uses DashMap internally for lock-free access
    registry: Arc<LiveQueryRegistry>,
    filter_cache: Arc<tokio::sync::RwLock<FilterCache>>,
    live_queries_provider: Arc<LiveQueriesTableProvider>,
    node_id: NodeId,
}

impl SubscriptionService {
    pub fn new(
        registry: Arc<LiveQueryRegistry>,
        filter_cache: Arc<tokio::sync::RwLock<FilterCache>>,
        live_queries_provider: Arc<LiveQueriesTableProvider>,
        node_id: NodeId,
    ) -> Self {
        Self {
            registry,
            filter_cache,
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

    /// Register a live query subscription
    ///
    /// Takes SubscriptionRequest which contains pre-parsed table_id, where_clause,
    /// and projections from handle_subscription() to avoid duplicate parsing.
    /// Permissions have already been checked, but we still need to handle
    /// auto-injection of user_id filter for user tables.
    pub async fn register_subscription(
        &self,
        connection_id: ConnectionId,
        subscription: kalamdb_commons::websocket::SubscriptionRequest,
    ) -> Result<LiveQueryId, KalamDbError> {
        // Extract table_id from pre-parsed subscription request
        let table_id = subscription.table_id.clone().ok_or_else(|| {
            KalamDbError::InvalidOperation("table_id not parsed in SubscriptionRequest".to_string())
        })?;

        // Get user_id from connection registry (DashMap - lock-free)
        let user_id = self.registry.get_user_id(&connection_id).ok_or_else(|| {
            KalamDbError::NotFound(format!("Connection not found: {}", connection_id))
        })?;

        let registry = Arc::clone(&self.registry);
        let provider = Arc::clone(&self.live_queries_provider);
        let node_id = self.node_id.clone();
        let conn_for_record = connection_id.clone();
        let table_for_record = table_id.clone();
        let user_for_record = user_id.clone();

        let live_id = spawn_blocking(move || {
            // Generate LiveQueryId
            let live_id = LiveQueryId::new(
                user_for_record.clone(),
                conn_for_record.clone(),
                subscription.id.clone(),
            );

            let timestamp = Self::current_timestamp_ms();

            // Convert SubscriptionOptions to LiveQueryOptions
            let options = LiveQueryOptions {
                last_rows: subscription.options.last_rows,
            };

            // Serialize options to JSON
            let options_json = serde_json::to_string(&options).map_err(|e| {
                KalamDbError::SerializationError(format!("Failed to serialize options: {}", e))
            })?;

            // Create record for system.live_queries
            let live_query_record = SystemLiveQuery {
                live_id: live_id.clone(),
                connection_id: conn_for_record.to_string(),
                namespace_id: table_for_record.namespace_id().clone(),
                table_name: table_for_record.table_name().clone(),
                user_id: user_for_record.clone(),
                query: subscription.sql.clone(),
                options: Some(options_json),
                created_at: timestamp,
                last_update: timestamp,
                changes: 0,
                node: node_id.as_str().to_string(),
                subscription_id: subscription.id.clone(),
                status: kalamdb_commons::types::LiveQueryStatus::Active,
            };

            // Insert into system.live_queries (blocking)
            provider.insert_live_query(live_query_record)?;

            // Register subscription in in-memory registry (DashMap - lock-free)
            registry.register_subscription(
                user_for_record,
                table_for_record,
                live_id.clone(),
                conn_for_record,
                options,
            )?;

            Ok::<_, KalamDbError>(live_id)
        })
        .await
        .map_err(|e| KalamDbError::Other(format!("Join error registering subscription: {}", e)))??;

        Ok(live_id)
    }

    /// Unregister a single live query subscription
    pub async fn unregister_subscription(&self, live_id: &LiveQueryId) -> Result<(), KalamDbError> {
        // Remove cached filter first (needs RwLock write)
        {
            let mut filter_cache = self.filter_cache.write().await;
            filter_cache.remove(&live_id.to_string());
        }

        // Remove from in-memory registry (DashMap - lock-free)
        let connection_id = self.registry.unregister_subscription(live_id);

        if connection_id.is_none() {
            return Err(KalamDbError::NotFound(format!(
                "Live query not found: {}",
                live_id
            )));
        }

        let provider = Arc::clone(&self.live_queries_provider);
        let live_id_for_delete = live_id.clone();
        spawn_blocking(move || {
            provider.delete_live_query(&live_id_for_delete)?;
            Ok::<_, KalamDbError>(())
        })
        .await
        .map_err(|e| KalamDbError::Other(format!("Join error deleting live query: {}", e)))??;

        Ok(())
    }

    /// Unregister a WebSocket connection
    pub async fn unregister_connection(
        &self,
        _user_id: &UserId, //TODO: Remove this completely
        connection_id: &ConnectionId,
    ) -> Result<Vec<LiveQueryId>, KalamDbError> {
        // Remove from in-memory registry and get all live_ids (DashMap - lock-free)
        let live_ids = self.registry.unregister_connection(connection_id);

        // Remove cached filters for all live queries
        {
            let mut filter_cache = self.filter_cache.write().await;
            for live_id in &live_ids {
                filter_cache.remove(&live_id.to_string());
            }
        }

        let provider = Arc::clone(&self.live_queries_provider);
        let connection_key = connection_id.to_string();
        spawn_blocking(move || {
            provider.delete_by_connection_id(&connection_key)?;
            Ok::<_, KalamDbError>(())
        })
        .await
        .map_err(|e| KalamDbError::Other(format!("Join error deleting live queries by connection: {}", e)))??;

        Ok(live_ids)
    }
}
