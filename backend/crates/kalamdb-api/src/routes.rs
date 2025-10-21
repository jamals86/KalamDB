//! API routes configuration
//!
//! This module configures all HTTP and WebSocket routes for the KalamDB API.

use crate::handlers;
use actix_web::web;

/// Configure API routes for KalamDB
///
/// Exposes the following endpoints:
/// - POST /api/sql - Execute SQL statements
/// - GET /ws - WebSocket connection for live query subscriptions
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg
        // REST API endpoints
        .service(handlers::execute_sql)
        // WebSocket endpoint
        .service(handlers::websocket_handler);
}
