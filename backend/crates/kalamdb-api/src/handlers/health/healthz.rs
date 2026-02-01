//! Liveness probe handler

use actix_web::{HttpResponse, Responder};

use super::models::HealthResponse;

/// GET /healthz - Kubernetes-style liveness probe
///
/// This is a simple liveness check that returns 200 OK if the server is running.
/// No authentication required - designed for load balancer health checks.
pub async fn healthz_handler() -> impl Responder {
    HttpResponse::Ok().json(HealthResponse::ok_with_version(env!("CARGO_PKG_VERSION")))
}
