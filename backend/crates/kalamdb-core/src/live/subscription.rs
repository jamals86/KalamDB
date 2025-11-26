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

        // Generate LiveQueryId
        let live_id = LiveQueryId::new(
            user_id.clone(),
            connection_id.clone(),
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
            connection_id: connection_id.to_string(),
            namespace_id: table_id.namespace_id().clone(),
            table_name: table_id.table_name().clone(),
            user_id: user_id.clone(),
            query: subscription.sql.clone(),
            options: Some(options_json),
            created_at: timestamp,
            last_update: timestamp,
            changes: 0,
            node: self.node_id.as_str().to_string(),
            subscription_id: subscription.id.clone(),
            status: kalamdb_commons::types::LiveQueryStatus::Active,
        };

        // Insert into system.live_queries (async - uses EntityStoreAsync internally)
        self.live_queries_provider
            .insert_live_query_async(live_query_record)
            .await
            .map_err(|e| KalamDbError::Other(format!("Failed to insert live query: {}", e)))?;

        // Register subscription in in-memory registry (DashMap - lock-free)
        self.registry.register_subscription(
            user_id,
            table_id,
            live_id.clone(),
            connection_id,
            options,
        )?;

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

        // Delete from system.live_queries (async - uses EntityStoreAsync internally)
        self.live_queries_provider
            .delete_live_query_async(live_id)
            .await
            .map_err(|e| KalamDbError::Other(format!("Failed to delete live query: {}", e)))?;

        Ok(())
    }

    /// Unregister a WebSocket connection
    pub async fn unregister_connection(
        &self,
        user_id: &UserId,
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

        // Delete from system.live_queries (async - uses prefix scan for efficiency)
        let connection_key = connection_id.to_string();
        self.live_queries_provider
            .delete_by_connection_id_async(user_id, &connection_key)
            .await
            .map_err(|e| {
                KalamDbError::Other(format!("Failed to delete live queries by connection: {}", e))
            })?;

        Ok(live_ids)
    }
}
