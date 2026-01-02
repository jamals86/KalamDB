//! WebSocket batch handler
//!
//! Handles the NextBatch message for paginated initial data fetching.

use actix_ws::Session;
use kalamdb_commons::ids::SeqId;
use kalamdb_commons::websocket::BatchControl;
use kalamdb_commons::WebSocketMessage;
use kalamdb_core::live::{LiveQueryManager, SharedConnectionState};
use kalamdb_core::providers::arrow_json_conversion::row_to_json_map;
use log::{error, info};
use std::sync::Arc;

use super::{send_error, send_json};

/// Handle next batch request
///
/// Uses subscription metadata from ConnectionState for batch fetching.
/// Tracks batch_num in subscription state and increments it after each batch.
pub async fn handle_next_batch(
    connection_state: &SharedConnectionState,
    subscription_id: &str,
    last_seq_id: Option<SeqId>,
    session: &mut Session,
    live_query_manager: &Arc<LiveQueryManager>,
) -> Result<(), String> {
    // Increment batch number and get the new value
    // This is done BEFORE fetching to get the correct batch_num for this request
    let batch_num = {
        let state = connection_state.read();
        state.increment_batch_num(subscription_id).unwrap_or(0)
    };

    info!(
        "Processing NextBatch request: subscription_id={}, batch_num={}, last_seq_id={:?}",
        subscription_id, batch_num, last_seq_id
    );

    match live_query_manager
        .fetch_initial_data_batch(connection_state, subscription_id, last_seq_id)
        .await
    {
        Ok(result) => {
            // Use BatchControl::new() which handles status based on batch_num and has_more
            let batch_control = BatchControl::new(
                batch_num,
                result.has_more,
                result.last_seq,
                result.snapshot_end_seq,
            );

            info!(
                "Sending batch {}: {} rows, has_more={}",
                batch_num, result.rows.len(), result.has_more
            );

            // Convert Row objects to HashMap
            let mut rows_json = Vec::with_capacity(result.rows.len());
            for row in result.rows {
                match row_to_json_map(&row) {
                    Ok(json) => rows_json.push(json),
                    Err(e) => {
                        error!("Failed to convert row to JSON: {}", e);
                        return send_error(
                            session,
                            subscription_id,
                            "CONVERSION_ERROR",
                            &format!("Failed to convert row data: {}", e),
                        )
                        .await
                        .map_err(|_| "Failed to send error message".to_string());
                    }
                }
            }

            let msg = WebSocketMessage::initial_data_batch(
                subscription_id.to_string(),
                rows_json,
                batch_control,
            );
            let _ = send_json(session, &msg).await;
            Ok(())
        }
        Err(e) => {
            let _ = send_error(session, subscription_id, "BATCH_FETCH_FAILED", &e.to_string()).await;
            Ok(())
        }
    }
}
