//! Health check handlers
//!
//! ## Endpoints
//! - GET /healthz - Kubernetes-style liveness probe
//! - GET /readyz - Kubernetes-style readiness probe
//!
//! These endpoints are typically unauthenticated for load balancer health checks.

pub mod models;

mod healthz;
mod readyz;

pub use healthz::healthz_handler;
pub use readyz::readyz_handler;
