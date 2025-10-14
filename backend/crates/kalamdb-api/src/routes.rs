// Routes module - SQL-only API
use actix_web::web;
use crate::handlers;

/// Configure API routes for SQL-only API
/// 
/// Only one endpoint is exposed:
/// - POST /api/v1/query - Execute SQL INSERT or SELECT statements
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v1")
            .route("/query", web::post().to(handlers::query::query_messages))
    );
}

