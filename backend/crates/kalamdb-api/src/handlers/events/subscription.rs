//! WebSocket subscription handler
//!
//! Handles the Subscribe message for live query subscriptions.

use actix_ws::Session;
use kalamdb_commons::websocket::{BatchControl, SubscriptionRequest, MAX_ROWS_PER_BATCH};
use kalamdb_commons::WebSocketMessage;
use kalamdb_core::live::{InitialDataOptions, LiveQueryManager, SharedConnectionState};
use kalamdb_core::providers::arrow_json_conversion::row_to_json_map;
use log::{error, info};
use std::sync::Arc;

use crate::limiter::RateLimiter;

use super::{send_error, send_json};

/// Handle subscription request
///
/// Validates subscription ID and rate limits, then delegates to LiveQueryManager
/// which handles all SQL parsing, permission checks, and registration.
///
/// Uses connection_id from SharedConnectionState, no separate parameter needed.
pub async fn handle_subscribe(
    connection_state: &SharedConnectionState,
    subscription: SubscriptionRequest,
    session: &mut Session,
    rate_limiter: &Arc<RateLimiter>,
    live_query_manager: &Arc<LiveQueryManager>,
) -> Result<(), String> {
    let user_id = connection_state.read().user_id().cloned().ok_or("Not authenticated")?;

    // Validate subscription ID
    if subscription.id.trim().is_empty() {
        let _ = send_error(
            session,
            "invalid_subscription",
            "INVALID_SUBSCRIPTION_ID",
            "Subscription ID cannot be empty",
        )
        .await;
        return Ok(());
    }

    // Rate limit check
    if !rate_limiter.check_subscription_limit(&user_id) {
        let _ = send_error(
            session,
            &subscription.id,
            "SUBSCRIPTION_LIMIT_EXCEEDED",
            "Maximum subscriptions reached",
        )
        .await;
        return Ok(());
    }

    let subscription_id = subscription.id.clone();

    // info!(
    //     "Processing subscription request: id={}, sql='{}', user_id={}, options={:?}",
    //     subscription_id,
    //     subscription.sql,
    //     user_id.as_str(),
    //     subscription.options
    // );

    // Determine batch size for initial data options
    let batch_size = subscription.options.batch_size.unwrap_or(MAX_ROWS_PER_BATCH);

    // Create initial data options respecting all three options:
    // - from_seq_id: Resume from a specific sequence ID
    // - last_rows: Fetch the last N rows
    // - batch_size: Hint for server-side batch sizing
    let initial_opts = if let Some(from_seq) = subscription.options.from_seq_id {
        // Resume from specific sequence ID - use since_seq for filtering
        // info!("Using from_seq_id={} for initial data (resuming)", from_seq.as_i64());
        InitialDataOptions::batch(Some(from_seq), None, batch_size)
    } else if let Some(n) = subscription.options.last_rows {
        // Fetch last N rows
        // info!("Using last_rows={} for initial data", n);
        InitialDataOptions::last(n as usize)
    } else {
        // Default batch fetch
        // info!("Using default batch size={} for initial data", batch_size);
        InitialDataOptions::batch(None, None, batch_size)
    };

    // Register subscription with initial data fetch
    // LiveQueryManager handles all SQL parsing, permission checks, and registration internally
    match live_query_manager
        .register_subscription_with_initial_data(
            connection_state,
            &subscription,
            Some(initial_opts),
        )
        .await
    {
        Ok(result) => {
            info!(
                "Subscription registered: id={}, user_id={}, has_initial_data={}",
                subscription_id,
                user_id.as_str(),
                result.initial_data.is_some()
            );
            if let Some(ref initial) = result.initial_data {
                info!("Initial data: {} rows, has_more={}", initial.rows.len(), initial.has_more);
            }

            // Update rate limiter
            rate_limiter.increment_subscription(&user_id);

            // Send response
            // Use BatchControl::new() which handles status based on batch_num and has_more
            let batch_control = if let Some(ref initial) = result.initial_data {
                BatchControl::new(
                    0, // batch_num
                    initial.has_more,
                    initial.last_seq,
                    initial.snapshot_end_seq,
                )
            } else {
                // No initial data - empty result, ready immediately
                BatchControl::new(0, false, None, None)
            };

            let ack = WebSocketMessage::subscription_ack(
                subscription_id.clone(),
                0,
                batch_control.clone(),
                result.schema.clone(),
            );
            // info!("Sending subscription_ack for {} with {} schema fields", subscription_id, result.schema.len());
            let _ = send_json(session, &ack).await;

            if let Some(initial) = result.initial_data {
                // info!(
                //     "Sending initial_data_batch for {} with {} rows",
                //     subscription_id,
                //     initial.rows.len()
                // );

                // Convert Row objects to HashMap (always using simple JSON format)
                let mut rows_json = Vec::with_capacity(initial.rows.len());
                for row in initial.rows {
                    match row_to_json_map(&row) {
                        Ok(json) => rows_json.push(json),
                        Err(e) => {
                            error!("Failed to convert row to JSON: {}", e);
                            return send_error(
                                session,
                                &subscription_id,
                                "CONVERSION_ERROR",
                                &format!("Failed to convert row data: {}", e),
                            )
                            .await
                            .map_err(|_| "Failed to send error message".to_string());
                        },
                    }
                }

                let batch_msg =
                    WebSocketMessage::initial_data_batch(subscription_id.clone(), rows_json, batch_control);
                let _ = send_json(session, &batch_msg).await;

                if !initial.has_more {
                    let flushed = connection_state.read().complete_initial_load(&subscription_id);
                    if flushed > 0 {
                        info!(
                            "Flushed {} buffered notifications after initial load for {}",
                            flushed, subscription_id
                        );
                    }
                }
            } else {
                info!("No initial data to send for {}", subscription_id);
                let flushed = connection_state.read().complete_initial_load(&subscription_id.clone());
                if flushed > 0 {
                    info!(
                        "Flushed {} buffered notifications after initial load for {}",
                        flushed, subscription_id
                    );
                }
            }

            Ok(())
        },
        Err(e) => {
            // Map error types to appropriate WebSocket error codes
            let (code, message) = match &e {
                kalamdb_core::error::KalamDbError::PermissionDenied(msg) => {
                    ("UNAUTHORIZED", msg.as_str())
                },
                kalamdb_core::error::KalamDbError::NotFound(msg) => ("NOT_FOUND", msg.as_str()),
                kalamdb_core::error::KalamDbError::InvalidSql(msg) => ("INVALID_SQL", msg.as_str()),
                kalamdb_core::error::KalamDbError::InvalidOperation(msg) => {
                    ("UNSUPPORTED", msg.as_str())
                },
                _ => ("SUBSCRIPTION_FAILED", "Subscription registration failed"),
            };
            error!(
                "Failed to register subscription {}: {} (sql: '{}')",
                subscription_id, e, subscription.sql
            );
            let _ = send_error(session, &subscription_id, code, message).await;
            Ok(())
        },
    }
}
