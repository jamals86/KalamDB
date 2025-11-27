//! API routes configuration
//!
//! This module configures all HTTP and WebSocket routes for the KalamDB API.

use crate::handlers;
use actix_web::{web, HttpResponse};
use serde_json::json;

/// Configure API routes for KalamDB
///
/// All endpoints use the /v1 version prefix:
/// - POST /v1/api/sql - Execute SQL statements (requires Auth, except localhost)
/// - GET /v1/ws - WebSocket connection for live query subscriptions
/// - GET /v1/api/healthcheck - Health check endpoint
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/v1")
            .service(
                web::scope("/api")
                    .service(handlers::execute_sql_v1)
                    .route("/healthcheck", web::get().to(healthcheck_handler)),
            )
            .service(handlers::websocket_handler),
    );
}

/// Health check endpoint handler
async fn healthcheck_handler() -> HttpResponse {
    HttpResponse::Ok().json(json!({
        "status": "healthy",
        "version": env!("CARGO_PKG_VERSION"),
        "api_version": "v1",
        "build_date": env!("BUILD_DATE")
    }))
}
