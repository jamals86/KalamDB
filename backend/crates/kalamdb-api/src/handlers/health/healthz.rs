//! Liveness probe handler

use actix_web::{HttpRequest, HttpResponse, Responder};
use kalamdb_auth::extract_client_ip_secure;

use super::models::HealthResponse;

/// GET /healthz - Kubernetes-style liveness probe
///
/// Returns 200 OK if the server is running.
/// SECURITY: Localhost-only to prevent version disclosure to remote clients.
pub async fn healthz_handler(req: HttpRequest) -> impl Responder {
    let connection_info = extract_client_ip_secure(&req);
    if !connection_info.is_localhost() {
        return HttpResponse::Forbidden().json(serde_json::json!({
            "error": "Access denied. Health endpoint is localhost-only."
        }));
    }
    HttpResponse::Ok().json(HealthResponse::ok_with_version(env!("CARGO_PKG_VERSION")))
}
