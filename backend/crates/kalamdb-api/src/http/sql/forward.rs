//! SQL forwarding to leader node in cluster mode.

use std::{sync::Arc, time::Instant};

use actix_web::{HttpRequest, HttpResponse};
use kalamdb_commons::{
    models::{NamespaceId, NodeId, UserId},
    schemas::TableType,
};
use kalamdb_core::{app_context::AppContext, error::KalamDbError};
use kalamdb_raft::{ClusterClient, ForwardSqlRequest, GroupId, RaftExecutor, ShardRouter};
use kalamdb_sql::classifier::SqlStatementKind;
use serde_json::Value as JsonValue;
use uuid::Uuid;

use super::{
    helpers::parse_forward_params,
    models::{ErrorCode, QueryRequest, SqlResponse},
    statements::PreparedApiExecutionStatement,
};

fn header_to_string(req: &HttpRequest, name: &str) -> Option<String> {
    req.headers().get(name).and_then(|v| v.to_str().ok()).map(|v| v.to_string())
}

fn normalize_addr(addr: &str) -> String {
    addr.trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/')
        .to_string()
}

fn resolve_node_id_from_cluster_addr(app_context: &AppContext, addr: &str) -> Option<NodeId> {
    let target = normalize_addr(addr);
    let cluster_info = app_context.executor().get_cluster_info();
    cluster_info
        .nodes
        .iter()
        .find(|n| normalize_addr(&n.api_addr) == target || normalize_addr(&n.rpc_addr) == target)
        .map(|n| n.node_id)
}

fn cluster_client_for(app_context: &AppContext) -> Result<ClusterClient, HttpResponse> {
    let executor = app_context.executor();
    let raft_executor = executor.as_any().downcast_ref::<RaftExecutor>().ok_or_else(|| {
        HttpResponse::ServiceUnavailable().json(SqlResponse::error(
            ErrorCode::ClusterUnavailable,
            "Cluster forwarding requires Raft executor",
            0.0,
        ))
    })?;
    Ok(ClusterClient::new(Arc::clone(raft_executor.manager())))
}

/// Forward target: either the Meta leader or a specific node.
enum ForwardTarget {
    Leader,
    GroupLeader(GroupId),
    Node(NodeId),
}

fn data_group_for_table_type(
    app_context: &AppContext,
    table_type: TableType,
    user_id: &UserId,
) -> Option<GroupId> {
    let executor = app_context.executor();
    let raft_executor = executor.as_any().downcast_ref::<RaftExecutor>()?;
    let config = raft_executor.manager().config();
    let router = ShardRouter::new(config.user_shards, config.shared_shards);

    match table_type {
        TableType::User | TableType::Stream => {
            Some(GroupId::DataUserShard(router.user_shard_id(user_id)))
        },
        TableType::Shared => Some(GroupId::DataSharedShard(router.shared_shard_id())),
        TableType::System => Some(GroupId::Meta),
    }
}

fn prepared_statement_table_type(
    statement: &PreparedApiExecutionStatement,
    app_context: &AppContext,
) -> Option<TableType> {
    statement.prepared_statement.table_type.or_else(|| {
        statement
            .prepared_statement
            .table_id
            .as_ref()
            .and_then(|table_id| app_context.schema_registry().get(table_id))
            .map(|cached| cached.table_entry().table_type)
    })
}

fn prepared_statement_target_group(
    statement: &PreparedApiExecutionStatement,
    app_context: &AppContext,
    user_id: &UserId,
) -> Option<GroupId> {
    let Some(classified) = statement.prepared_statement.classified_statement.as_ref() else {
        return None;
    };

    if matches!(classified.kind(), SqlStatementKind::Select) {
        return prepared_statement_table_type(statement, app_context).and_then(|table_type| {
            match table_type {
                TableType::User | TableType::Shared | TableType::Stream => {
                    data_group_for_table_type(app_context, table_type, user_id)
                },
                TableType::System => None,
            }
        });
    }

    if !classified.is_write_operation() {
        return None;
    }

    match classified.kind() {
        SqlStatementKind::Insert(_) | SqlStatementKind::Update(_) | SqlStatementKind::Delete(_) => {
            prepared_statement_table_type(statement, app_context)
                .and_then(|table_type| data_group_for_table_type(app_context, table_type, user_id))
                .or(Some(GroupId::Meta))
        },
        _ => Some(GroupId::Meta),
    }
}

async fn forward_sql_grpc(
    target: ForwardTarget,
    http_req: &HttpRequest,
    req: &QueryRequest,
    app_context: &AppContext,
    request_id: Option<&str>,
    start_time: Instant,
) -> Option<HttpResponse> {
    let client = match cluster_client_for(app_context) {
        Ok(c) => c,
        Err(resp) => return Some(resp),
    };

    let params = match parse_forward_params(&req.params) {
        Ok(v) => v,
        Err(e) => {
            return Some(HttpResponse::BadRequest().json(SqlResponse::error(
                ErrorCode::InvalidParameter,
                &e,
                start_time.elapsed().as_secs_f64() * 1000.0,
            )));
        },
    };

    let grpc_req = ForwardSqlRequest {
        sql: req.sql.clone(),
        namespace_id: req.namespace_id.as_ref().map(|ns| ns.to_string()),
        authorization_header: header_to_string(http_req, "Authorization"),
        request_id: header_to_string(http_req, "X-Request-ID")
            .or_else(|| request_id.map(ToOwned::to_owned)),
        params,
    };

    let response = match target {
        ForwardTarget::Leader => client.forward_sql_to_leader(grpc_req).await,
        ForwardTarget::GroupLeader(group_id) => {
            client.forward_sql_to_group_leader(group_id, grpc_req).await
        },
        ForwardTarget::Node(node_id) => client.forward_sql_to_node(node_id, grpc_req).await,
    };

    let response = match response {
        Ok(resp) => resp,
        Err(err) => {
            log::warn!("Failed to forward SQL over gRPC: {}", err);
            return Some(HttpResponse::ServiceUnavailable().json(SqlResponse::error(
                ErrorCode::ForwardFailed,
                "Failed to forward request to cluster leader",
                start_time.elapsed().as_secs_f64() * 1000.0,
            )));
        },
    };

    if !response.error.is_empty() && response.body.is_empty() {
        return Some(HttpResponse::BadGateway().json(SqlResponse::error(
            ErrorCode::ForwardFailed,
            &response.error,
            start_time.elapsed().as_secs_f64() * 1000.0,
        )));
    }

    let status = actix_web::http::StatusCode::from_u16(response.status_code as u16)
        .unwrap_or(actix_web::http::StatusCode::BAD_GATEWAY);
    Some(HttpResponse::build(status).content_type("application/json").body(response.body))
}

/// Forwards leader-routed operations to the appropriate leader node in cluster mode.
pub async fn forward_sql_if_follower(
    http_req: &HttpRequest,
    sql: &str,
    params_json: &Option<Vec<JsonValue>>,
    namespace_id: &Option<NamespaceId>,
    app_context: &Arc<AppContext>,
    prepared_statements: &[PreparedApiExecutionStatement],
    user_id: &UserId,
    request_id: Option<&str>,
) -> Option<HttpResponse> {
    let start_time = Instant::now();
    let executor = app_context.executor();

    let write_targets: Vec<GroupId> = prepared_statements
        .iter()
        .filter_map(|statement| {
            prepared_statement_target_group(statement, app_context.as_ref(), user_id)
        })
        .collect();

    if let Some(first_target) = write_targets.first().copied() {
        let target_group = if write_targets.iter().all(|target| *target == first_target) {
            first_target
        } else {
            GroupId::Meta
        };

        if executor.is_leader(target_group).await {
            return None;
        }

        let generated_request_id;
        let request_id = match request_id {
            Some(request_id) => Some(request_id),
            None => {
                generated_request_id = Uuid::now_v7().to_string();
                Some(generated_request_id.as_str())
            },
        };

        let req = QueryRequest {
            sql: sql.to_string(),
            params: params_json.clone(),
            namespace_id: namespace_id.clone(),
        };

        return forward_sql_grpc(
            ForwardTarget::GroupLeader(target_group),
            http_req,
            &req,
            app_context.as_ref(),
            request_id,
            start_time,
        )
        .await;
    }

    None
}

/// Handle typed NOT_LEADER errors by forwarding to the known shard leader over gRPC.
pub async fn handle_not_leader_error(
    err: &KalamDbError,
    http_req: &HttpRequest,
    req: &QueryRequest,
    app_context: &AppContext,
    request_id: Option<&str>,
    start_time: Instant,
) -> Option<HttpResponse> {
    if !app_context.is_cluster_mode() {
        return None;
    }

    let leader_addr = match err {
        KalamDbError::NotLeader { leader_addr } => leader_addr.as_ref(),
        _ => return None,
    };

    if let Some(addr) = leader_addr {
        if let Some(target_node_id) = resolve_node_id_from_cluster_addr(app_context, addr) {
            return forward_sql_grpc(
                ForwardTarget::Node(target_node_id),
                http_req,
                req,
                app_context,
                request_id,
                start_time,
            )
            .await;
        }
        log::debug!(
            target: "sql::forward",
            "NOT_LEADER: leader addr '{}' not found in cluster info, falling back to generic leader forward",
            addr
        );
    }

    forward_sql_grpc(ForwardTarget::Leader, http_req, req, app_context, request_id, start_time)
        .await
}
