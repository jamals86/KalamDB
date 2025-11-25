//! WebSocket handler for live query subscriptions
//!
//! This module provides the HTTP endpoint for establishing WebSocket connections
//! and managing live query subscriptions.

use actix_web::{get, web, Error, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use log::info;
use std::sync::Arc;
use uuid::Uuid;

use crate::actors::WebSocketSession;
use crate::rate_limiter::RateLimiter;
use kalamdb_auth::UserRepository;
use kalamdb_core::live_query::LiveQueryManager;

/// GET /v1/ws - Establish WebSocket connection
///
/// Accepts unauthenticated WebSocket connections.
/// Authentication happens via post-connection Authenticate message (3-second timeout enforced by WebSocketSession).
///
/// No query parameters or headers required for initial connection.
/// The WebSocketSession actor will handle authentication after connection is established.
/// This endpoint upgrades an HTTP request to a WebSocket connection.
/// Clients can then send subscription requests to receive real-time updates.
///
/// # Authentication
///
/// Requires HTTP Basic Auth or JWT Bearer token in Authorization header.
/// Authentication is handled by the centralized `extract_auth` function from kalamdb-auth.
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
    app_context: web::Data<Arc<kalamdb_core::app_context::AppContext>>,
    rate_limiter: web::Data<Arc<RateLimiter>>,
    live_query_manager: web::Data<Arc<LiveQueryManager>>,
    user_repo: web::Data<Arc<dyn UserRepository>>,
) -> Result<HttpResponse, Error> {
    // Generate unique connection ID
    let connection_id = uuid::Uuid::new_v4().simple().to_string();

    // Extract client IP with security checks against spoofing
    let client_ip = kalamdb_auth::extract_client_ip_secure(&req);

    info!("New WebSocket connection request: {} (authentication required within 3 seconds)", connection_id);

    // Create WebSocketSession without authentication (will be authenticated via message)
    let session = WebSocketSession::new(
        connection_id.clone(),
        None, // No user_id yet - will be set after authentication
        client_ip,
        Some(rate_limiter.get_ref().clone()),
        live_query_manager.get_ref().clone(),
        app_context.get_ref().clone(),
        user_repo.get_ref().clone(),
    );

    // Start WebSocket actor
    ws::start(session, &req, stream)
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::rate_limiter::RateLimiter;
//     use actix_web::{test, App};
//     use kalamdb_core::live_query::{LiveQueryManager, NodeId};
//     use kalamdb_store::RocksDbInit;
//     use std::sync::Arc;
//     use tempfile::TempDir;

//     #[actix_rt::test]
//     async fn test_websocket_endpoint() {
//         let rate_limiter = Arc::new(RateLimiter::new());

//         let temp_dir = TempDir::new().expect("temp dir");
//         let db_path = temp_dir.path().to_str().unwrap().to_string();
//         let db_init = RocksDbInit::new(&db_path);
//         let db = db_init.open().expect("open RocksDB");
//         let backend: Arc<dyn kalamdb_store::StorageBackend> =
//             Arc::new(kalamdb_store::RocksDBBackend::new(db.clone()));
//         // Build provider-backed user repository
//         let users_provider = kalamdb_core::tables::system::UsersTableProvider::new(backend);
//         let user_repo: Arc<dyn kalamdb_auth::UserRepository> =
//             Arc::new(kalamdb_auth::ProviderUserRepo::new(Arc::new(users_provider)));
//         let live_query_manager = Arc::new(LiveQueryManager::new(
//             kalam_sql,
//             NodeId::new("test-node".to_string()),
//             None,
//             None,
//             None,
//         ));

//         let app = test::init_service(
//             App::new()
//         .app_data(web::Data::new(user_repo))
//                 .app_data(web::Data::new(rate_limiter))
//                 .app_data(web::Data::new(live_query_manager))
//                 .service(websocket_handler_v1),
//         )
//         .await;

//         // Test without Authorization header - should return 401
//         let req = test::TestRequest::get()
//             .uri("/ws")
//             .insert_header(("upgrade", "websocket"))
//             .insert_header(("connection", "upgrade"))
//             .insert_header(("sec-websocket-version", "13"))
//             .insert_header(("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ=="))
//             .to_request();

//         let resp = test::call_service(&app, req).await;
//         assert_eq!(resp.status(), 401); // Unauthorized without authentication
//     }
// }
