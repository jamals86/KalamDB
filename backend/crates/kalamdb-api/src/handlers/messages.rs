// Message handlers
use actix_web::{web, HttpResponse, Responder};
use kalamdb_core::{
    ids::SnowflakeGenerator,
    models::Message,
    storage::MessageStore,
};
use log::{error, info};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// Shared application state
pub struct AppState {
    pub store: Arc<dyn MessageStore + Send + Sync>,
    pub id_generator: Arc<SnowflakeGenerator>,
    pub max_message_size: usize,
}

/// Request body for creating a message
#[derive(Debug, Deserialize)]
pub struct CreateMessageRequest {
    pub conversation_id: String,
    pub from: String,
    pub content: String,
    #[serde(default)]
    pub metadata: Option<JsonValue>,
}

/// Response for message creation
#[derive(Debug, Serialize)]
pub struct CreateMessageResponse {
    pub msg_id: i64,
    pub status: String,
}

/// Error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// POST /api/v1/messages - Create a new message
pub async fn create_message(
    state: web::Data<AppState>,
    req: web::Json<CreateMessageRequest>,
) -> impl Responder {
    info!(
        "Received message: conversation_id={}, from={}, content_len={}",
        req.conversation_id,
        req.from,
        req.content.len()
    );

    // Validation (T022)
    if let Err(err) = validate_message_request(&req, state.max_message_size) {
        error!("Validation error: {}", err);
        return HttpResponse::BadRequest().json(ErrorResponse {
            error: err.to_string(),
        });
    }

    // Generate message ID
    let msg_id = match state.id_generator.next_id() {
        Ok(id) => id,
        Err(err) => {
            error!("Failed to generate message ID: {}", err);
            return HttpResponse::InternalServerError().json(ErrorResponse {
                error: format!("Failed to generate message ID: {}", err),
            });
        }
    };

    // Get current timestamp in microseconds
    let timestamp = chrono::Utc::now().timestamp_micros();

    // Create message
    let message = Message::new(
        msg_id,
        req.conversation_id.clone(),
        req.from.clone(),
        timestamp,
        req.content.clone(),
        req.metadata.clone(),
    );

    // Store message
    if let Err(err) = state.store.insert_message(&message) {
        error!("Failed to store message {}: {}", msg_id, err);
        return HttpResponse::InternalServerError().json(ErrorResponse {
            error: format!("Failed to store message: {}", err),
        });
    }

    info!("Message stored successfully: msg_id={}", msg_id);

    HttpResponse::Ok().json(CreateMessageResponse {
        msg_id,
        status: "stored".to_string(),
    })
}

/// Validate message request (T022)
fn validate_message_request(
    req: &CreateMessageRequest,
    max_size: usize,
) -> Result<(), String> {
    if req.conversation_id.is_empty() {
        return Err("conversation_id cannot be empty".to_string());
    }

    if req.from.is_empty() {
        return Err("from cannot be empty".to_string());
    }

    if req.content.is_empty() {
        return Err("content cannot be empty".to_string());
    }

    if req.content.len() > max_size {
        return Err(format!(
            "content size ({} bytes) exceeds maximum ({} bytes)",
            req.content.len(),
            max_size
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_message_request() {
        let req = CreateMessageRequest {
            conversation_id: "conv_123".to_string(),
            from: "user_alice".to_string(),
            content: "Hello, world!".to_string(),
            metadata: None,
        };

        assert!(validate_message_request(&req, 1048576).is_ok());
    }

    #[test]
    fn test_validate_empty_conversation_id() {
        let req = CreateMessageRequest {
            conversation_id: "".to_string(),
            from: "user_alice".to_string(),
            content: "Hello, world!".to_string(),
            metadata: None,
        };

        assert!(validate_message_request(&req, 1048576).is_err());
    }

    #[test]
    fn test_validate_empty_from() {
        let req = CreateMessageRequest {
            conversation_id: "conv_123".to_string(),
            from: "".to_string(),
            content: "Hello, world!".to_string(),
            metadata: None,
        };

        assert!(validate_message_request(&req, 1048576).is_err());
    }

    #[test]
    fn test_validate_empty_content() {
        let req = CreateMessageRequest {
            conversation_id: "conv_123".to_string(),
            from: "user_alice".to_string(),
            content: "".to_string(),
            metadata: None,
        };

        assert!(validate_message_request(&req, 1048576).is_err());
    }

    #[test]
    fn test_validate_content_too_large() {
        let req = CreateMessageRequest {
            conversation_id: "conv_123".to_string(),
            from: "user_alice".to_string(),
            content: "x".repeat(2_000_000),
            metadata: None,
        };

        assert!(validate_message_request(&req, 1048576).is_err());
    }
}
