//! HTTP route registration for KalamDB server.
//!
//! This module wires the Actix-Web application to the shared
//! `kalamdb-api` route configuration so the server keeps its
//! entrypoint lightweight.

use actix_web::web;

/// Register all HTTP and WebSocket routes for the server.
pub fn configure(cfg: &mut web::ServiceConfig) {
    kalamdb_api::routes::configure_routes(cfg);
}

