//! Subscription service for live queries
//!
//! Handles registration and unregistration of live query subscriptions,
//! including permission checks, filter compilation, and system table updates.
//!
//! All SQL parsing is done inside register_subscription - no intermediate ParsedSubscription.

use super::connections_manager::{ConnectionsManager, SharedConnectionState, SubscriptionHandle, SubscriptionState};
use crate::error::KalamDbError;
use crate::error_extensions::KalamDbResultExt;
use log::debug;
use datafusion::sql::sqlparser::ast::Expr;
use kalamdb_commons::ids::SeqId;
use kalamdb_commons::models::{ConnectionId, LiveQueryId, TableId, UserId};
use kalamdb_commons::system::LiveQuery as SystemLiveQuery;
use kalamdb_commons::websocket::SubscriptionRequest;
use kalamdb_commons::NodeId;
use kalamdb_system::LiveQueriesTableProvider;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Service for managing subscriptions
///
/// Uses Arc<ConnectionsManager> directly since ConnectionsManager internally
/// uses DashMap for lock-free concurrent access - no RwLock wrapper needed.
pub struct SubscriptionService {
    /// Manager uses DashMap internally for lock-free access
    registry: Arc<ConnectionsManager>,
    live_queries_provider: Arc<LiveQueriesTableProvider>,
    node_id: NodeId,
}

impl SubscriptionService {
    pub fn new(
        registry: Arc<ConnectionsManager>,
        live_queries_provider: Arc<LiveQueriesTableProvider>,
        node_id: NodeId,
    ) -> Self {
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

    /// Register a live query subscription
    ///
    /// This method performs all SQL parsing internally and creates the SubscriptionState.
    /// The ws_handler passes the SharedConnectionState directly.
    ///
    /// Parameters:
    /// - connection_state: Shared reference to the connection state
    /// - request: Client subscription request containing SQL and options
    /// - table_id: Pre-validated table identifier (validated in ws_handler)
    /// - filter_expr: Optional parsed WHERE clause expression
    /// - projections: Optional column projections (None = SELECT *, i.e., all columns)
    /// - batch_size: Batch size for initial data fetching
    pub async fn register_subscription(
        &self,
        connection_state: &SharedConnectionState,
        request: &SubscriptionRequest,
        table_id: TableId,
        filter_expr: Option<Expr>,
        projections: Option<Vec<String>>,
        batch_size: usize,
    ) -> Result<LiveQueryId, KalamDbError> {
        // Read connection info from state and check subscription limit
        let (connection_id, user_id, notification_tx) = {
            let state = connection_state.read();
            let user_id = state.user_id.clone().ok_or_else(|| {
                KalamDbError::InvalidOperation("Connection not authenticated".to_string())
            })?;
            
            // Prevent DoS via excessive subscriptions per connection
            const MAX_SUBSCRIPTIONS_PER_CONNECTION: usize = 100;
            if state.subscriptions.len() >= MAX_SUBSCRIPTIONS_PER_CONNECTION {
                return Err(KalamDbError::InvalidOperation(format!(
                    "Maximum subscriptions ({}) per connection exceeded",
                    MAX_SUBSCRIPTIONS_PER_CONNECTION
                )));
            }
            
            (state.connection_id.clone(), user_id, state.notification_tx.clone())
        };

        // Generate LiveQueryId
        let live_id = LiveQueryId::new(
            user_id.clone(),
            connection_id.clone(),
            request.id.clone(),
        );

        let timestamp = Self::current_timestamp_ms();

        // Clone subscription options directly
        let options = request.options.clone();

        // Serialize options to JSON
        let options_json = serde_json::to_string(&options)
            .into_serialization_error("Failed to serialize options")?;

        // Create record for system.live_queries
        let live_query_record = SystemLiveQuery {
            live_id: live_id.clone(),
            connection_id: connection_id.to_string(),
            namespace_id: table_id.namespace_id().clone(),
            table_name: table_id.table_name().clone(),
            user_id: user_id.clone(),
            query: request.sql.clone(),
            options: Some(options_json),
            created_at: timestamp,
            last_update: timestamp,
            changes: 0,
            node_id: self.node_id.clone(),
            subscription_id: request.id.clone(),
            status: kalamdb_commons::types::LiveQueryStatus::Active,
            last_ping_at: timestamp,
        };

        // Insert into system.live_queries (async - uses EntityStoreAsync internally)
        self.live_queries_provider
            .insert_live_query_async(live_query_record)
            .await
            .into_kalamdb_error("Failed to insert live query")?;

        // Wrap filter_expr and projections in Arc for zero-copy sharing between state and handle
        let filter_expr_arc = filter_expr.map(Arc::new);
        let projections_arc = projections.map(Arc::new);

        // Create SubscriptionState with all necessary data (stored in ConnectionState)
        let subscription_state = SubscriptionState {
            live_id: live_id.clone(),
            table_id: table_id.clone(),
            sql: request.sql.as_str().into(),  // Arc<str> for zero-copy
            filter_expr: filter_expr_arc.clone(),
            projections: projections_arc.clone(),
            batch_size,
            snapshot_end_seq: None,
            current_batch_num: 0,  // Start at batch 0
            notification_tx: notification_tx.clone(),
        };

        // Create lightweight handle for the index (~48 bytes vs ~800+ bytes)
        let subscription_handle = SubscriptionHandle {
            live_id: live_id.clone(),
            filter_expr: filter_expr_arc,
            projections: projections_arc,
            notification_tx,
        };

        // Add subscription to connection state
        {
            let state = connection_state.read();
            state.subscriptions.insert(request.id.clone(), subscription_state);
        }

        // Add lightweight handle to registry's table index for efficient lookups
        self.registry.index_subscription(
            &user_id,
            &connection_id,
            live_id.clone(),
            table_id.clone(),
            subscription_handle,
        );

        debug!(
            "Registered subscription {} for connection {} on {}",
            request.id, connection_id, table_id
        );

        Ok(live_id)
    }

    /// Update the snapshot_end_seq for a subscription after initial data fetch
    pub fn update_snapshot_end_seq(
        &self,
        connection_state: &SharedConnectionState,
        subscription_id: &str,
        snapshot_end_seq: SeqId,
    ) {
        let state = connection_state.read();
        state.update_snapshot_end_seq(subscription_id, Some(snapshot_end_seq));
    }

    /// Unregister a single live query subscription
    pub async fn unregister_subscription(
        &self,
        connection_state: &SharedConnectionState,
        subscription_id: &str,
        live_id: &LiveQueryId,
    ) -> Result<(), KalamDbError> {
        // Get user_id and subscription details, then remove from connection state
        let (connection_id, user_id, table_id) = {
            let state = connection_state.read();
            let user_id = state.user_id.clone().ok_or_else(|| {
                KalamDbError::InvalidOperation("Connection not authenticated".to_string())
            })?;
            let subscription = state.subscriptions.remove(subscription_id);
            match subscription {
                Some((_, sub)) => (state.connection_id.clone(), user_id, sub.table_id),
                None => {
                    return Err(KalamDbError::NotFound(format!(
                        "Subscription not found: {}",
                        subscription_id
                    )));
                }
            }
        };

        // Remove from registry's table index
        self.registry.unindex_subscription(&user_id, live_id, &table_id);

        // Delete from system.live_queries (async)
        self.live_queries_provider
            .delete_live_query_async(live_id)
            .await
            .into_kalamdb_error("Failed to delete live query")?;

        debug!(
            "Unregistered subscription {} for connection {}",
            subscription_id, connection_id
        );

        Ok(())
    }

    /// Unregister a WebSocket connection and all its subscriptions
    ///
    /// Cleans up both the system.live_queries table entries and the
    /// ConnectionsManager registry.
    pub async fn unregister_connection(
        &self,
        user_id: &UserId,
        connection_id: &ConnectionId,
    ) -> Result<Vec<LiveQueryId>, KalamDbError> {
        // Delete from system.live_queries (async - uses prefix scan for efficiency)
        self.live_queries_provider
            .delete_by_connection_id_async(user_id, connection_id)
            .await
            .map_err(|e| {
                KalamDbError::Other(format!("Failed to delete live queries by connection: {}", e))
            })?;

        // Unregister from connections manager (removes connection and returns live_ids)
        let live_ids = self.registry.unregister_connection(connection_id);

        Ok(live_ids)
    }

    /// Get subscription state from a connection
    pub fn get_subscription(
        &self,
        connection_state: &SharedConnectionState,
        subscription_id: &str,
    ) -> Option<SubscriptionState> {
        let state = connection_state.read();
        state.subscriptions.get(subscription_id).map(|s| s.clone())
    }
}

/// Result of registering a subscription (used for ws_handler response)
#[derive(Debug, Clone)]
pub struct RegisteredSubscription {
    pub live_id: LiveQueryId,
    pub subscription_id: String,
    pub table_id: TableId,
    pub batch_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp() {
        let ts = SubscriptionService::current_timestamp_ms();
        assert!(ts > 0);
    }
}
