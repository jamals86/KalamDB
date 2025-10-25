//! Server-wide middleware configuration helpers.
//!
//! Keeps the Actix application setup focused by providing
//! reusable constructors for CORS and logging layers.

use actix_cors::Cors;
use actix_web::middleware;

/// Build the CORS policy used by the server.
pub fn build_cors() -> Cors {
    Cors::default()
        .allow_any_origin()
        .allow_any_method()
        .allow_any_header()
        .supports_credentials()
        .max_age(3600)
}

/// Build the request logger middleware.
pub fn request_logger() -> middleware::Logger {
    middleware::Logger::default()
}
