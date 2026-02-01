//! Cluster health and status handlers
//!
//! Provides endpoints for monitoring cluster health and OpenRaft metrics.
//!
//! ## Endpoints
//! - GET /v1/api/cluster/health - Cluster health with OpenRaft metrics (local/auth required)
//!
//! Access is restricted to:
//! - Requests from localhost/same machine
//! - Authenticated requests with valid token

pub mod models;

mod health;

pub use health::cluster_health_handler;
