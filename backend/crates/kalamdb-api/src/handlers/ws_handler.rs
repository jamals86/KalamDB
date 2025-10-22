//! WebSocket handler for live query subscriptions
//!
//! This module provides the HTTP endpoint for establishing WebSocket connections
//! and managing live query subscriptions.

use actix_web::{get, web, Error, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use log::{error, info, warn};
use std::sync::Arc;
use uuid::Uuid;

use crate::actors::WebSocketSession;
use crate::auth::{JwtAuth, JwtError};
use crate::rate_limiter::RateLimiter;

/// GET /v1/ws - Establish WebSocket connection
///
/// This endpoint upgrades an HTTP request to a WebSocket connection.
/// Clients can then send subscription requests to receive real-time updates.
///
/// # WebSocket Protocol
///
/// ## Client → Server (Subscription Request)
/// ```json
/// {
///   "subscriptions": [
///     {
///       "id": "sub-1",
///       "sql": "SELECT * FROM messages WHERE user_id = CURRENT_USER()",
///       "options": {"last_rows": 10}
///     }
///   ]
/// }
/// ```
///
/// ## Server → Client (Initial Data)
/// ```json
/// {
///   "type": "initial_data",
///   "subscription_id": "sub-1",
///   "rows": [...]
/// }
/// ```
///
/// ## Server → Client (Change Notification)
/// ```json
/// {
///   "type": "change",
///   "subscription_id": "sub-1",
///   "change_type": "insert",
///   "rows": [...]
/// }
/// ```
#[get("/ws")]
pub async fn websocket_handler_v1(
    req: HttpRequest,
    stream: web::Payload,
    jwt_auth: web::Data<JwtAuth>,
    rate_limiter: web::Data<Arc<RateLimiter>>,
) -> Result<HttpResponse, Error> {
    // Generate unique connection ID
    let connection_id = Uuid::new_v4().to_string();

    info!("New WebSocket connection request: {}", connection_id);

    // Extract and validate JWT token from Authorization header
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok());

    let user_id = match auth_header {
        Some(header) => match JwtAuth::extract_token(header) {
            Ok(token) => match jwt_auth.validate_token(token) {
                Ok(claims) => {
                    info!(
                        "WebSocket connection authenticated: connection_id={}, user_id={}",
                        connection_id, claims.user_id
                    );
                    Some(claims.user_id())
                }
                Err(JwtError::Expired) => {
                    warn!(
                        "WebSocket connection rejected: token expired (connection_id={})",
                        connection_id
                    );
                    return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                        "error": "TOKEN_EXPIRED",
                        "message": "JWT token has expired"
                    })));
                }
                Err(e) => {
                    error!(
                        "WebSocket connection rejected: invalid token (connection_id={}, error={})",
                        connection_id, e
                    );
                    return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                        "error": "INVALID_TOKEN",
                        "message": format!("JWT validation failed: {}", e)
                    })));
                }
            },
            Err(e) => {
                error!(
                    "WebSocket connection rejected: {} (connection_id={})",
                    e, connection_id
                );
                return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                    "error": "INVALID_AUTHORIZATION",
                    "message": format!("{}", e)
                })));
            }
        },
        None => {
            warn!(
                "WebSocket connection rejected: missing Authorization header (connection_id={})",
                connection_id
            );
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "MISSING_AUTHORIZATION",
                "message": "Authorization header is required"
            })));
        }
    };

    // Create WebSocket session actor with authenticated user_id and rate limiter
    let session =
        WebSocketSession::new(connection_id, user_id, Some(rate_limiter.get_ref().clone()));

    // Start WebSocket handshake
    ws::start(session, &req, stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::JwtAuth;
    use crate::rate_limiter::RateLimiter;
    use actix_web::{test, App};
    use jsonwebtoken::Algorithm;

    #[actix_rt::test]
    async fn test_websocket_endpoint() {
        let jwt_auth = JwtAuth::new("test-secret".to_string(), Algorithm::HS256);
        let rate_limiter = Arc::new(RateLimiter::new());

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(jwt_auth))
                .app_data(web::Data::new(rate_limiter))
                .service(websocket_handler_v1),
        )
        .await;

        // Test without Authorization header - should return 401
        let req = test::TestRequest::get()
            .uri("/ws")
            .insert_header(("upgrade", "websocket"))
            .insert_header(("connection", "upgrade"))
            .insert_header(("sec-websocket-version", "13"))
            .insert_header(("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ=="))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401); // Unauthorized without JWT token
    }
}
