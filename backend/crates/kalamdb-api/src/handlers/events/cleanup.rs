//! WebSocket connection cleanup handler
//!
//! Handles cleanup when a WebSocket connection is closed.

use kalamdb_core::live::{ConnectionsManager, LiveQueryManager, SharedConnectionState};
use log::{debug, info};
use std::sync::Arc;

use crate::rate_limiter::RateLimiter;

/// Cleanup connection on close
///
/// ConnectionsManager.unregister_connection handles all subscription cleanup.
///
/// Uses connection_id from SharedConnectionState, no separate parameter needed.
pub async fn cleanup_connection(
    connection_state: &SharedConnectionState,
    registry: &Arc<ConnectionsManager>,
    rate_limiter: &Arc<RateLimiter>,
    live_query_manager: &Arc<LiveQueryManager>,
) {
    // Get connection_id, user ID and subscription count before unregistering
    let (connection_id, user_id, subscription_count) = {
        let state = connection_state.read();
        (
            state.connection_id().clone(),
            state.user_id.clone(),
            state.subscriptions.len(),
        )
    };

    info!("Cleaning up connection: {}", connection_id);

    // Unregister from unified registry (handles subscription cleanup)
    let removed_live_ids = registry.unregister_connection(&connection_id);

    // Unregister from live query manager
    if let Some(ref uid) = user_id {
        let _ = live_query_manager
            .unregister_connection(uid, &connection_id)
            .await;

        // Update rate limiter
        rate_limiter.cleanup_connection(&connection_id);
        for _ in 0..subscription_count {
            rate_limiter.decrement_subscription(uid);
        }
    }

    debug!(
        "Connection cleanup complete: {} (removed {} subscriptions)",
        connection_id,
        removed_live_ids.len()
    );
}
