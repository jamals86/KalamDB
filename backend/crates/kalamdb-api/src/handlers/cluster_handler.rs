//! Cluster health and status handlers
//!
//! Provides endpoints for monitoring cluster health and OpenRaft metrics.
//! Access is restricted to:
//! - Requests from localhost/same machine
//! - Authenticated requests with valid token

use actix_web::{web, HttpRequest, HttpResponse};
use kalamdb_core::app_context::AppContext;
use kalamdb_core::metrics::{BUILD_DATE, SERVER_VERSION};
use kalamdb_raft::ServerStateExt;
use serde::Serialize;
use std::sync::Arc;

/// Response for cluster health endpoint
#[derive(Serialize)]
pub struct ClusterHealthResponse {
    /// Overall health status
    pub status: String,
    /// Server version
    pub version: String,
    /// Build date
    pub build_date: String,
    /// Whether in cluster mode
    pub is_cluster_mode: bool,
    /// Cluster ID (empty for standalone)
    pub cluster_id: String,
    /// Current node ID (0 for standalone)
    pub node_id: u64,
    /// Whether this node is a leader for the Meta group
    pub is_leader: bool,
    /// Total number of Raft groups
    pub total_groups: u32,
    /// Number of groups this node leads
    pub groups_leading: u32,
    /// Current Raft term
    pub current_term: u64,
    /// Last applied log index
    pub last_applied: Option<u64>,
    /// Milliseconds since quorum acknowledgment (leader health indicator)
    pub millis_since_quorum_ack: Option<u64>,
    /// Nodes in the cluster with their status
    pub nodes: Vec<NodeHealth>,
}

/// Health status of a single node
#[derive(Serialize)]
pub struct NodeHealth {
    /// Node ID
    pub node_id: u64,
    /// Node role (leader, follower, learner, candidate)
    pub role: String,
    /// Node status (active, offline, joining, catching_up, unknown)
    pub status: String,
    /// API address
    pub api_addr: String,
    /// Whether this is the current node
    pub is_self: bool,
    /// Whether this node is the leader
    pub is_leader: bool,
    /// Replication lag in log entries (only for leader viewing followers)
    pub replication_lag: Option<u64>,
    /// Catchup progress percentage (0-100), None if not catching up
    pub catchup_progress_pct: Option<u8>,
}

/// Check if the request is from localhost or same machine
fn is_local_request(req: &HttpRequest) -> bool {
    if let Some(peer_addr) = req.peer_addr() {
        let ip = peer_addr.ip();
        return ip.is_loopback() || ip.to_string() == "127.0.0.1" || ip.to_string() == "::1";
    }
    false
}

/// Check if the request has valid authorization
fn has_valid_auth(req: &HttpRequest) -> bool {
    // Check for Authorization header (Bearer token or Basic auth)
    req.headers().get("Authorization").is_some()
}

/// Cluster health endpoint handler
///
/// Returns detailed cluster health information including:
/// - Node roles and status
/// - Replication metrics
/// - Catchup progress
///
/// Access restricted to:
/// - Localhost requests (no auth required)
/// - Authenticated requests (with valid token)
pub async fn cluster_health_handler(
    req: HttpRequest,
    ctx: web::Data<Arc<AppContext>>,
) -> HttpResponse {
    // Check access
    if !is_local_request(&req) && !has_valid_auth(&req) {
        return HttpResponse::Forbidden().json(serde_json::json!({
            "error": "Access denied. Use localhost or provide valid authorization."
        }));
    }

    let cluster_info = ctx.executor().get_cluster_info();

    // Calculate overall health status
    let status = if cluster_info.is_cluster_mode {
        // In cluster mode, check if we have a leader
        let has_leader = cluster_info.nodes.iter().any(|n| n.is_leader);
        let self_node = cluster_info.nodes.iter().find(|n| n.is_self);
        let is_active = self_node.map(|n| n.status.as_str() == "active").unwrap_or(false);

        if has_leader && is_active {
            "healthy"
        } else if is_active {
            "degraded" // No leader known but node is active
        } else {
            "unhealthy"
        }
    } else {
        "healthy" // Standalone is always healthy if responding
    };

    // Convert nodes
    let nodes: Vec<NodeHealth> = cluster_info
        .nodes
        .iter()
        .map(|n| NodeHealth {
            node_id: n.node_id.as_u64(),
            role: n.role.as_str().to_string(),
            status: n.status.as_str().to_string(),
            api_addr: n.api_addr.clone(),
            is_self: n.is_self,
            is_leader: n.is_leader,
            replication_lag: n.replication_lag,
            catchup_progress_pct: n.catchup_progress_pct,
        })
        .collect();

    // Find self node for groups_leading
    let groups_leading = cluster_info
        .nodes
        .iter()
        .find(|n| n.is_self)
        .map(|n| n.groups_leading)
        .unwrap_or(0);

    let response = ClusterHealthResponse {
        status: status.to_string(),
        version: SERVER_VERSION.to_string(),
        build_date: BUILD_DATE.to_string(),
        is_cluster_mode: cluster_info.is_cluster_mode,
        cluster_id: cluster_info.cluster_id,
        node_id: cluster_info.current_node_id.as_u64(),
        is_leader: cluster_info.nodes.iter().any(|n| n.is_self && n.is_leader),
        total_groups: cluster_info.total_groups,
        groups_leading,
        current_term: cluster_info.current_term,
        last_applied: cluster_info.last_applied,
        millis_since_quorum_ack: cluster_info.millis_since_quorum_ack,
        nodes,
    };

    HttpResponse::Ok().json(response)
}
