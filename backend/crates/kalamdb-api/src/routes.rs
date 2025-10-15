// Routes module - SQL-only API
use actix_web::web;
// use crate::handlers;

/// Configure API routes for SQL-only API
/// 
/// Routes temporarily disabled until DataFusion integration
/// Only one endpoint will be exposed:
/// - POST /api/sql - Execute SQL statements
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api")
            // TODO: Re-enable after DataFusion integration
            // .route("/sql", web::post().to(handlers::sql_handler))
    );
}

