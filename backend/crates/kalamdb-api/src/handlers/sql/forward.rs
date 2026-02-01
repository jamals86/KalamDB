//! SQL forwarding to leader node in cluster mode

use actix_web::{HttpRequest, HttpResponse};
use kalamdb_commons::models::NamespaceId;
use kalamdb_commons::Role;
use kalamdb_raft::GroupId;
use reqwest::Client;
use std::sync::Arc;
use std::time::Instant;

use super::models::{ErrorCode, QueryRequest, SqlResponse};

/// Forwards write operations to the leader node in cluster mode.
///
/// This function checks if we're running in cluster mode and if so, determines
/// whether the SQL statement is a write operation that needs to go to the leader.
pub async fn forward_sql_if_follower(
    http_req: &HttpRequest,
    req: &QueryRequest,
    app_context: &Arc<kalamdb_core::app_context::AppContext>,
    default_namespace: &NamespaceId,
) -> Option<HttpResponse> {
    let executor = app_context.executor();

    if executor.is_leader(GroupId::Meta).await {
        return None;
    }

    // We are a follower - check if this is a write operation
    let statements = match kalamdb_sql::split_statements(&req.sql) {
        Ok(stmts) => stmts,
        Err(_) => {
            let cluster_info = executor.get_cluster_info();
            let leader_api_addr = executor
                .get_leader(GroupId::Meta)
                .await
                .and_then(|leader_id| {
                    cluster_info
                        .nodes
                        .iter()
                        .find(|node| node.node_id == leader_id)
                        .map(|node| node.api_addr.clone())
                });

            return match leader_api_addr {
                Some(api_addr) => forward_to_leader(http_req, req, &api_addr).await,
                None => Some(HttpResponse::ServiceUnavailable().json(SqlResponse::error(
                    ErrorCode::LeaderNotAvailable,
                    "No cluster leader available",
                    0.0,
                ))),
            };
        }
    };

    // Classify each statement to check if any are writes
    let has_write = statements.iter().any(|sql| {
        let stmt = kalamdb_sql::statement_classifier::SqlStatement::classify_and_parse(
            sql,
            default_namespace,
            Role::System,
        )
        .unwrap_or_else(|_| {
            kalamdb_sql::statement_classifier::SqlStatement::new(
                sql.to_string(),
                kalamdb_sql::statement_classifier::SqlStatementKind::Unknown,
            )
        });
        stmt.is_write_operation()
    });

    if has_write {
        let cluster_info = executor.get_cluster_info();
        let leader_api_addr = executor
            .get_leader(GroupId::Meta)
            .await
            .and_then(|leader_id| {
                cluster_info
                    .nodes
                    .iter()
                    .find(|node| node.node_id == leader_id)
                    .map(|node| node.api_addr.clone())
            });

        if let Some(api_addr) = leader_api_addr {
            log::debug!(
                "Forwarding write operation to leader {}: {}...",
                api_addr,
                req.sql.chars().take(50).collect::<String>()
            );
            return forward_to_leader(http_req, req, &api_addr).await;
        }

        return Some(HttpResponse::ServiceUnavailable().json(SqlResponse::error(
            ErrorCode::ClusterUnavailable,
            "No cluster leader available",
            0.0,
        )));
    }

    // Read-only operations can be served locally on the follower
    log::debug!(
        "Serving read operation locally on follower: {}...",
        req.sql.chars().take(50).collect::<String>()
    );
    None
}

/// Forward the request to the leader node
pub async fn forward_to_leader(
    http_req: &HttpRequest,
    req: &QueryRequest,
    leader_api_addr: &str,
) -> Option<HttpResponse> {
    let leader_url = format!("{}/v1/api/sql", leader_api_addr.trim_end_matches('/'));
    let client = Client::new();
    let mut request = client.post(&leader_url).json(req);

    if let Some(auth_header) = http_req.headers().get("Authorization") {
        if let Ok(value) = auth_header.to_str() {
            request = request.header("Authorization", value);
        }
    }
    if let Some(request_id) = http_req.headers().get("X-Request-ID") {
        if let Ok(value) = request_id.to_str() {
            request = request.header("X-Request-ID", value);
        }
    }

    let response = match request.send().await {
        Ok(resp) => resp,
        Err(err) => {
            log::warn!("Failed to forward SQL request to leader {}: {}", leader_url, err);
            return Some(HttpResponse::ServiceUnavailable().json(SqlResponse::error(
                ErrorCode::ForwardFailed,
                "Failed to forward request to cluster leader",
                0.0,
            )));
        }
    };

    let status = actix_web::http::StatusCode::from_u16(response.status().as_u16())
        .unwrap_or(actix_web::http::StatusCode::BAD_GATEWAY);
    let body = response.bytes().await.unwrap_or_default();

    Some(HttpResponse::build(status).content_type("application/json").body(body))
}

/// Extract leader address from NOT_LEADER error message
#[inline]
pub fn extract_leader_addr_from_error(error_msg: &str) -> Option<String> {
    // Fast path: Early exit if NOT_LEADER not present
    if !error_msg.contains("NOT_LEADER") {
        return None;
    }

    // Look for "Leader: Some("url")" pattern (from Debug formatting of Option<String>)
    if let Some(start) = error_msg.find("Leader: Some(") {
        let after_prefix = &error_msg[start + 13..]; // Skip "Leader: Some("
        if let Some(paren_end) = after_prefix.find(')') {
            let content = &after_prefix[..paren_end];
            let url = content.trim().trim_matches('"');
            if !url.is_empty() && url.starts_with("http") {
                return Some(url.to_string());
            }
        }
    }
    None
}

/// Handle NOT_LEADER error by automatically forwarding to leader
pub async fn handle_not_leader_error(
    err: &kalamdb_core::error::KalamDbError,
    http_req: &HttpRequest,
    req: &QueryRequest,
    app_context: &kalamdb_core::app_context::AppContext,
    start_time: Instant,
) -> Option<HttpResponse> {
    // Fast path: Only process in cluster mode
    if !app_context.is_cluster_mode() {
        return None;
    }

    let err_msg = err.to_string();

    // Fast path: Check if NOT_LEADER
    if !err_msg.contains("NOT_LEADER") {
        return None;
    }

    // Extract leader address from error message
    let leader_addr = extract_leader_addr_from_error(&err_msg)?;

    log::info!(
        "Auto-forwarding to shard leader {} due to NOT_LEADER error",
        leader_addr
    );

    // Forward entire request to the shard leader
    match forward_to_leader(http_req, req, &leader_addr).await {
        Some(response) => Some(response),
        None => {
            // Forward failed, return error
            let took = start_time.elapsed().as_secs_f64() * 1000.0;
            Some(HttpResponse::ServiceUnavailable().json(SqlResponse::error(
                ErrorCode::ForwardFailed,
                &format!("Failed to forward request to leader: {}", err),
                took,
            )))
        }
    }
}
