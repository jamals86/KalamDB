// Routes module
use actix_web::web;
use crate::handlers;

/// Configure API routes
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v1")
            .route("/messages", web::post().to(handlers::messages::create_message))
    );
}
