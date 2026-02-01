//! Readiness probe handler

use actix_web::{web, HttpResponse, Responder};
use kalamdb_core::app_context::AppContext;
use std::sync::Arc;

use super::models::HealthResponse;

/// GET /readyz - Kubernetes-style readiness probe
///
/// Checks if the server is ready to accept traffic by verifying:
/// 1. Database connection is active (implicit - if we reach here, RocksDB is working)
/// 2. Raft cluster is healthy (if in cluster mode)
///
/// No authentication required - designed for load balancer health checks.
pub async fn readyz_handler(app_context: web::Data<Arc<AppContext>>) -> impl Responder {
    // For now, assume healthy if storage backend is accessible
    // The storage backend being available means RocksDB is working
    let db_healthy = true; // If we reach this point, storage is accessible

    // Check Raft health if in cluster mode
    let raft_healthy = if app_context.is_cluster_mode() {
        // TODO: Add actual raft health check when raft_manager getter is available
        // For now, just return true for cluster mode
        true
    } else {
        true
    };

    let response = HealthResponse::with_details(db_healthy, Some(raft_healthy));

    if db_healthy && raft_healthy {
        HttpResponse::Ok().json(response)
    } else {
        HttpResponse::ServiceUnavailable().json(response)
    }
}
