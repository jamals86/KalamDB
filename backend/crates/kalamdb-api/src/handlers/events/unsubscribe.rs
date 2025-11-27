//! WebSocket unsubscribe handler
//!
//! Handles the Unsubscribe message for live query subscriptions.

use kalamdb_commons::models::LiveQueryId;
use kalamdb_core::live::{LiveQueryManager, SharedConnectionState};
use log::{error, info};
use std::sync::Arc;

use crate::rate_limiter::RateLimiter;

/// Handle unsubscribe request
///
/// Uses connection_id from SharedConnectionState, no separate parameter needed.
pub async fn handle_unsubscribe(
    connection_state: &SharedConnectionState,
    subscription_id: &str,
    rate_limiter: &Arc<RateLimiter>,
    live_query_manager: &Arc<LiveQueryManager>,
) -> Result<(), String> {
    let (user_id, connection_id) = {
        let state = connection_state.read();
        (state.user_id.clone(), state.connection_id().clone())
    };

    let user_id = match user_id {
        Some(uid) => uid,
        None => return Ok(()),
    };

    let live_id = LiveQueryId::new(user_id.clone(), connection_id, subscription_id.to_string());

    if let Err(e) = live_query_manager
        .unregister_subscription(connection_state, subscription_id, &live_id)
        .await
    {
        error!("Failed to unsubscribe {}: {}", subscription_id, e);
    }

    // Update rate limiter
    rate_limiter.decrement_subscription(&user_id);

    info!("Unsubscribed: {}", subscription_id);
    Ok(())
}
