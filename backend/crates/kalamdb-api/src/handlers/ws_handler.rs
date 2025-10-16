//! WebSocket handler for live query subscriptions
//!
//! This module provides the HTTP endpoint for establishing WebSocket connections
//! and managing live query subscriptions.

use actix_web::{get, web, HttpRequest, HttpResponse, Error};
use actix_web_actors::ws;
use log::info;
use uuid::Uuid;

use crate::actors::WebSocketSession;

/// GET /ws - Establish WebSocket connection
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
pub async fn websocket_handler(
    req: HttpRequest,
    stream: web::Payload,
) -> Result<HttpResponse, Error> {
    // Generate unique connection ID
    let connection_id = Uuid::new_v4().to_string();
    
    info!("New WebSocket connection request: {}", connection_id);
    
    // Create WebSocket session actor
    let session = WebSocketSession::new(connection_id);
    
    // Start WebSocket handshake
    ws::start(session, &req, stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};
    
    #[actix_rt::test]
    async fn test_websocket_endpoint() {
        let app = test::init_service(
            App::new().service(websocket_handler)
        ).await;
        
        let req = test::TestRequest::get()
            .uri("/ws")
            .insert_header(("upgrade", "websocket"))
            .insert_header(("connection", "upgrade"))
            .insert_header(("sec-websocket-version", "13"))
            .insert_header(("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ=="))
            .to_request();
        
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success() || resp.status().as_u16() == 101);
    }
}
