// Query handler
use actix_web::{web, HttpResponse, Responder};
use kalamdb_core::storage::QueryParams;
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use std::time::Instant;

use super::messages::AppState;

/// Response for query endpoint
#[derive(Debug, Serialize, Deserialize)]
pub struct QueryResponse {
    /// List of messages matching the query
    pub messages: Vec<serde_json::Value>,
    /// Number of messages returned
    pub count: usize,
    /// Whether there are more messages available
    pub has_more: bool,
}

/// Handler for POST /api/v1/query endpoint
pub async fn query_messages(
    query_params: web::Json<QueryParams>,
    state: web::Data<AppState>,
) -> impl Responder {
    let start_time = Instant::now();
    
    // Log the incoming query
    debug!(
        "Received query request: conversation_id={:?}, since_msg_id={:?}, limit={:?}, order={:?}",
        query_params.conversation_id,
        query_params.since_msg_id,
        query_params.limit,
        query_params.order
    );

    // Validate query parameters
    if let Err(validation_error) = query_params.validate() {
        error!("Query validation failed: {}", validation_error);
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Invalid query parameters",
            "message": validation_error
        }));
    }

    // Get effective limit for has_more calculation
    let requested_limit = query_params.effective_limit();
    
    // Create a modified query params to fetch one extra message to determine has_more
    let mut fetch_params = query_params.clone();
    fetch_params.limit = Some(requested_limit + 1);

    // Execute the query
    match state.store.query_messages(&fetch_params) {
        Ok(mut messages) => {
            let total_fetched = messages.len();
            let has_more = total_fetched > requested_limit;
            
            // Truncate to requested limit
            if has_more {
                messages.truncate(requested_limit);
            }

            let count = messages.len();
            
            // Convert messages to JSON
            let message_values: Vec<serde_json::Value> = messages
                .into_iter()
                .map(|msg| serde_json::json!({
                    "msg_id": msg.msg_id,
                    "conversation_id": msg.conversation_id,
                    "from": msg.from,
                    "timestamp": msg.timestamp,
                    "content": msg.content,
                    "metadata": msg.metadata
                }))
                .collect();

            let elapsed = start_time.elapsed();
            
            info!(
                "Query completed: returned {} messages, has_more={}, took {:?}",
                count, has_more, elapsed
            );

            let response = QueryResponse {
                messages: message_values,
                count,
                has_more,
            };

            HttpResponse::Ok().json(response)
        }
        Err(e) => {
            let elapsed = start_time.elapsed();
            error!("Query failed after {:?}: {}", elapsed, e);
            
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Query failed",
                "message": e.to_string()
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test;
    use kalamdb_core::models::Message;
    use kalamdb_core::storage::MessageStore;
    use serde_json::json;

    #[actix_web::test]
    async fn test_query_messages_handler() {
        let test_dir = format!("./data/test_query_handler_{}", std::process::id());
        let store = Arc::new(RocksDbStore::open(&test_dir).expect("Failed to create store"));

        // Insert test message
        let msg = Message {
            msg_id: 100,
            conversation_id: "conv_test".to_string(),
            from: "alice".to_string(),
            timestamp: 1000,
            content: "Hello".to_string(),
            metadata: None,
        };
        store.insert_message(&msg).expect("Failed to insert message");

        let query = QueryParams {
            conversation_id: Some("conv_test".to_string()),
            since_msg_id: None,
            limit: None,
            order: None,
        };

        let req = test::TestRequest::post()
            .set_json(&query)
            .to_http_request();

        let resp = query_messages(
            web::Json(query),
            web::Data::new(store.clone()),
        )
        .await
        .respond_to(&req);

        assert!(resp.status().is_success());

        // Cleanup
        drop(store);
        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[actix_web::test]
    async fn test_query_invalid_params() {
        let test_dir = format!("./data/test_query_invalid_{}", std::process::id());
        let store = Arc::new(RocksDbStore::open(&test_dir).expect("Failed to create store"));

        let query = QueryParams {
            conversation_id: None,
            since_msg_id: None,
            limit: Some(0), // Invalid: limit must be > 0
            order: None,
        };

        let req = test::TestRequest::post()
            .set_json(&query)
            .to_http_request();

        let resp = query_messages(
            web::Json(query),
            web::Data::new(store.clone()),
        )
        .await
        .respond_to(&req);

        assert_eq!(resp.status(), 400);

        // Cleanup
        drop(store);
        let _ = std::fs::remove_dir_all(&test_dir);
    }
}
