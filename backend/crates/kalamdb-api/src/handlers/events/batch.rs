//! WebSocket batch handler
//!
//! Handles the NextBatch message for paginated initial data fetching.

use actix_ws::Session;
use kalamdb_commons::ids::SeqId;
use kalamdb_commons::websocket::{BatchControl, BatchStatus, SerializationMode};
use kalamdb_commons::WebSocketMessage;
use kalamdb_core::live::{LiveQueryManager, SharedConnectionState};
use kalamdb_core::providers::arrow_json_conversion::row_to_json_map;
use log::error;
use std::sync::Arc;

use super::{send_error, send_json};

/// Handle next batch request
///
/// Uses subscription metadata from ConnectionState for batch fetching.
pub async fn handle_next_batch(
    connection_state: &SharedConnectionState,
    subscription_id: &str,
    last_seq_id: Option<SeqId>,
    session: &mut Session,
    live_query_manager: &Arc<LiveQueryManager>,
) -> Result<(), String> {
    match live_query_manager
        .fetch_initial_data_batch(connection_state, subscription_id, last_seq_id)
        .await
    {
        Ok(result) => {
            let batch_control = BatchControl {
                batch_num: 0,
                total_batches: None,
                has_more: result.has_more,
                status: if result.has_more {
                    BatchStatus::LoadingBatch
                } else {
                    BatchStatus::Ready
                },
                last_seq_id: result.last_seq,
                snapshot_end_seq: result.snapshot_end_seq,
            };

            // Get serialization mode from subscription state
            let serialization_mode = {
                let state = connection_state.read();
                state
                    .subscriptions
                    .get(subscription_id)
                    .map(|sub| sub.serialization_mode)
                    .unwrap_or(SerializationMode::Typed)
            };
            
            // Convert Row objects to HashMap using the subscription's serialization mode
            let mut rows_json = Vec::with_capacity(result.rows.len());
            for row in result.rows {
                match row_to_json_map(&row, serialization_mode) {
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
