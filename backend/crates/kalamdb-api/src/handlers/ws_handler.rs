//! WebSocket handler for live query subscriptions
//!
//! This module provides the HTTP endpoint for establishing WebSocket connections
//! and managing live query subscriptions using actix-ws (non-actor based).
//!
//! Connection lifecycle and heartbeat management is handled by the shared
//! ConnectionRegistry from kalamdb-core.

use actix_web::{get, web, Error, HttpRequest, HttpResponse};
use actix_ws::{CloseCode, CloseReason, Message, Session};
use futures_util::StreamExt;
use kalamdb_auth::UserRepository;
use kalamdb_commons::ids::SeqId;
use kalamdb_commons::models::{LiveQueryId, UserId};
use kalamdb_commons::websocket::{
    BatchControl, BatchStatus, ClientMessage, SubscriptionRequest, MAX_ROWS_PER_BATCH,
};
use kalamdb_commons::WebSocketMessage;
use kalamdb_core::app_context::AppContext;
use kalamdb_core::jobs::health_monitor::{
    decrement_websocket_sessions, increment_websocket_sessions,
};
use kalamdb_core::live::{
    ConnectionEvent, ConnectionId, InitialDataOptions, LiveQueryManager, ConnectionRegistry,
};
use log::{debug, error, info, warn};
use std::sync::Arc;

use crate::models::Notification;
use crate::rate_limiter::RateLimiter;

/// GET /v1/ws - Establish WebSocket connection
///
/// Accepts unauthenticated WebSocket connections.
/// Authentication happens via post-connection Authenticate message (3-second timeout enforced).
/// Uses ConnectionRegistry for consolidated connection state management.
#[get("/ws")]
pub async fn websocket_handler(
    req: HttpRequest,
    stream: web::Payload,
    app_context: web::Data<Arc<AppContext>>,
    rate_limiter: web::Data<Arc<RateLimiter>>,
    live_query_manager: web::Data<Arc<LiveQueryManager>>,
    user_repo: web::Data<Arc<dyn UserRepository>>,
    connection_registry: web::Data<Arc<ConnectionRegistry>>,
) -> Result<HttpResponse, Error> {
    // Check if server is shutting down
    if connection_registry.is_shutting_down() {
        return Ok(HttpResponse::ServiceUnavailable().body("Server is shutting down"));
    }

    // Generate unique connection ID
    let connection_id = ConnectionId::new(uuid::Uuid::new_v4().simple().to_string());

    // Extract client IP with security checks against spoofing
    let client_ip = kalamdb_auth::extract_client_ip_secure(&req);

    info!(
        "New WebSocket connection: {} (auth required within 3s)",
        connection_id
    );

    // Register connection with unified registry (handles heartbeat tracking)
    let registration = match connection_registry.register_connection(
        connection_id.clone(),
        client_ip.clone(),
    ) {
        Some(reg) => reg,
        None => {
            warn!("Rejecting WebSocket during shutdown: {}", connection_id);
            return Ok(HttpResponse::ServiceUnavailable().body("Server shutting down"));
        }
    };

    // Upgrade to WebSocket using actix-ws
    let (response, session, msg_stream) = actix_ws::handle(&req, stream)?;

    // Clone references for the async task
    let registry = connection_registry.get_ref().clone();
    let app_ctx = app_context.get_ref().clone();
    let limiter = rate_limiter.get_ref().clone();
    let lq_manager = live_query_manager.get_ref().clone();
    let user_repository = user_repo.get_ref().clone();
    let conn_id = connection_id.clone();

    // Spawn WebSocket handling task
    actix_web::rt::spawn(async move {
        increment_websocket_sessions();

        handle_websocket(
            conn_id,
            client_ip,
            session,
            msg_stream,
            registration,
            registry,
            app_ctx,
            limiter,
            lq_manager,
            user_repository,
        )
        .await;

        decrement_websocket_sessions();
    });

    Ok(response)
}

/// Main WebSocket handling loop
async fn handle_websocket(
    connection_id: ConnectionId,
    client_ip: Option<String>,
    mut session: Session,
    mut msg_stream: actix_ws::MessageStream,
    registration: kalamdb_core::live::ConnectionRegistration,
    registry: Arc<ConnectionRegistry>,
    app_context: Arc<AppContext>,
    rate_limiter: Arc<RateLimiter>,
    live_query_manager: Arc<LiveQueryManager>,
    user_repo: Arc<dyn UserRepository>,
) {
    let mut event_rx = registration.event_rx;
    let mut notification_rx = registration.notification_rx;

    // Track subscriptions locally for cleanup
    let mut local_subscriptions: Vec<String> = Vec::new();
    // Cache subscription metadata for batch fetching: sub_id -> (sql, user_id, snapshot_end_seq, batch_size)
    let mut subscription_metadata: std::collections::HashMap<
        String,
        (String, UserId, Option<SeqId>, usize),
    > = std::collections::HashMap::new();

    loop {
        tokio::select! {
            biased;

            // Handle control events from registry (highest priority)
            event = event_rx.recv() => {
                match event {
                    Some(ConnectionEvent::SendPing) => {
                        if session.ping(b"").await.is_err() {
                            break;
                        }
                    }
                    Some(ConnectionEvent::AuthTimeout) => {
                        error!("WebSocket auth timeout: {}", connection_id);
                        let msg = WebSocketMessage::AuthError {
                            message: "Authentication timeout - no auth message received within 3 seconds".to_string(),
                        };
                        let _ = send_json(&mut session, &msg).await;
                        let _ = session.close(Some(CloseReason {
                            code: CloseCode::Policy,
                            description: Some("Authentication timeout".into()),
                        })).await;
                        break;
                    }
                    Some(ConnectionEvent::HeartbeatTimeout) => {
                        warn!("WebSocket heartbeat timeout: {}", connection_id);
                        let _ = session.close(Some(CloseReason {
                            code: CloseCode::Normal,
                            description: Some("Heartbeat timeout".into()),
                        })).await;
                        break;
                    }
                    Some(ConnectionEvent::Shutdown) => {
                        info!("WebSocket shutdown requested: {}", connection_id);
                        let _ = session.close(Some(CloseReason {
                            code: CloseCode::Away,
                            description: Some("Server shutting down".into()),
                        })).await;
                        break;
                    }
                    None => {
                        // Channel closed
                        break;
                    }
                }
            }

            // Handle incoming WebSocket messages
            msg = msg_stream.next() => {
                match msg {
                    Some(Ok(Message::Ping(bytes))) => {
                        registry.update_heartbeat(&connection_id);
                        if session.pong(&bytes).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        registry.update_heartbeat(&connection_id);
                    }
                    Some(Ok(Message::Text(text))) => {
                        registry.update_heartbeat(&connection_id);

                        // Rate limit check
                        if !rate_limiter.check_message_rate(&connection_id) {
                            warn!("Message rate limit exceeded: {}", connection_id);
                            let _ = send_error(&mut session, "rate_limit", "RATE_LIMIT_EXCEEDED", "Too many messages").await;
                            continue;
                        }

                        if let Err(e) = handle_text_message(
                            &connection_id,
                            &client_ip,
                            &text,
                            &mut session,
                            &registry,
                            &app_context,
                            &rate_limiter,
                            &live_query_manager,
                            &user_repo,
                            &mut local_subscriptions,
                            &mut subscription_metadata,
                        ).await {
                            error!("Error handling message: {}", e);
                            let _ = session.close(Some(CloseReason {
                                code: CloseCode::Error,
                                description: Some(e),
                            })).await;
                            break;
                        }
                    }
                    Some(Ok(Message::Binary(_))) => {
                        warn!("Binary messages not supported: {}", connection_id);
                        let _ = send_error(&mut session, "protocol", "UNSUPPORTED_DATA", "Binary not supported").await;
                    }
                    Some(Ok(Message::Close(reason))) => {
                        info!("Client requested close: {:?}", reason);
                        let _ = session.close(reason).await;
                        break;
                    }
                    Some(Ok(_)) => {
                        // Continuation, Nop - ignore
                    }
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    None => {
                        // Stream ended
                        debug!("WebSocket stream ended: {}", connection_id);
                        break;
                    }
                }
            }

            // Handle notifications from live query manager
            notification = notification_rx.recv() => {
                match notification {
                    Some((_live_id, notif)) => {
                        if let Ok(json) = serde_json::to_string(&notif) {
                            if session.text(json).await.is_err() {
                                break;
                            }
                        }
                    }
                    None => {
                        // Channel closed
                        break;
                    }
                }
            }
        }
    }

    // Cleanup
    cleanup_connection(
        &connection_id,
        &registry,
        &rate_limiter,
        &live_query_manager,
        &local_subscriptions,
    )
    .await;
}

/// Handle text message from client
async fn handle_text_message(
    connection_id: &ConnectionId,
    client_ip: &Option<String>,
    text: &str,
    session: &mut Session,
    registry: &Arc<ConnectionRegistry>,
    app_context: &Arc<AppContext>,
    rate_limiter: &Arc<RateLimiter>,
    live_query_manager: &Arc<LiveQueryManager>,
    user_repo: &Arc<dyn UserRepository>,
    local_subscriptions: &mut Vec<String>,
    subscription_metadata: &mut std::collections::HashMap<String, (String, UserId, Option<SeqId>, usize)>,
) -> Result<(), String> {
    let msg: ClientMessage =
        serde_json::from_str(text).map_err(|e| format!("Invalid message: {}", e))?;

    match msg {
        ClientMessage::Authenticate { username, password } => {
            registry.mark_auth_started(connection_id);
            handle_authenticate(
                connection_id,
                client_ip,
                &username,
                &password,
                session,
                registry,
                app_context,
                live_query_manager,
                user_repo,
            )
            .await
        }
        ClientMessage::Subscribe { subscription } => {
            if !registry.is_authenticated(connection_id) {
                let _ = send_error(session, "subscribe", "AUTH_REQUIRED", "Authentication required before subscribing").await;
                return Ok(());
            }
            handle_subscribe(
                connection_id,
                subscription,
                session,
                registry,
                app_context,
                rate_limiter,
                live_query_manager,
                local_subscriptions,
                subscription_metadata,
            )
            .await
        }
        ClientMessage::NextBatch {
            subscription_id,
            last_seq_id,
        } => {
            if !registry.is_authenticated(connection_id) {
                return Ok(());
            }
            handle_next_batch(
                connection_id,
                &subscription_id,
                last_seq_id,
                session,
                live_query_manager,
                subscription_metadata,
            )
            .await
        }
        ClientMessage::Unsubscribe { subscription_id } => {
            if !registry.is_authenticated(connection_id) {
                return Ok(());
            }
            handle_unsubscribe(
                connection_id,
                &subscription_id,
                registry,
                rate_limiter,
                live_query_manager,
                local_subscriptions,
                subscription_metadata,
            )
            .await
        }
    }
}

/// Handle authentication message
async fn handle_authenticate(
    connection_id: &ConnectionId,
    client_ip: &Option<String>,
    username: &str,
    password: &str,
    session: &mut Session,
    registry: &Arc<ConnectionRegistry>,
    app_context: &Arc<AppContext>,
    live_query_manager: &Arc<LiveQueryManager>,
    user_repo: &Arc<dyn UserRepository>,
) -> Result<(), String> {
    info!(
        "Authenticating WebSocket: connection_id={}, username={}",
        connection_id, username
    );

    // Look up user
    let user = match user_repo.get_user_by_username(username).await {
        Ok(u) if u.deleted_at.is_none() => u,
        _ => {
            log_failed_auth(app_context).await;
            let _ = send_auth_error(session.clone(), "Invalid username or password").await;
            return Err("Authentication failed".to_string());
        }
    };

    // Check localhost bypass
    let is_localhost = client_ip
        .as_deref()
        .map(kalamdb_auth::is_localhost_address)
        .unwrap_or(false);

    let is_system_internal = user.role == kalamdb_commons::models::Role::System
        && user.auth_type == kalamdb_commons::models::AuthType::Internal;

    let authenticated = if is_system_internal && is_localhost && user.password_hash.is_empty() {
        // Localhost root bypass
        let entry = kalamdb_core::sql::executor::helpers::audit::log_auth_event(
            &user.id,
            "LOGIN_WS_LOCALHOST",
            true,
            Some("Localhost root bypass".to_string()),
        );
        let _ =
            kalamdb_core::sql::executor::helpers::audit::persist_audit_entry(app_context, &entry)
                .await;
        true
    } else {
        // Verify password
        match kalamdb_auth::password::verify_password(password, &user.password_hash).await {
            Ok(true) => {
                let entry = kalamdb_core::sql::executor::helpers::audit::log_auth_event(
                    &user.id,
                    "LOGIN_WS",
                    true,
                    None,
                );
                let _ = kalamdb_core::sql::executor::helpers::audit::persist_audit_entry(
                    app_context,
                    &entry,
                )
                .await;
                true
            }
            _ => false,
        }
    };

    if !authenticated {
        log_failed_auth(app_context).await;
        let _ = send_auth_error(session.clone(), "Invalid username or password").await;
        return Err("Authentication failed".to_string());
    }

    // Mark authenticated in registry
    // ConnectionRegistry now handles all connection state including user_id mapping
    registry.mark_authenticated(connection_id, user.id.clone());

    // Send success
    let msg = WebSocketMessage::AuthSuccess {
        user_id: user.id.as_str().to_string(),
        role: format!("{:?}", user.role),
    };
    let _ = send_json(session, &msg).await;

    info!(
        "WebSocket authenticated: {} as {} ({:?})",
        connection_id,
        user.id.as_str(),
        user.role
    );

    Ok(())
}

/// Handle subscription request
async fn handle_subscribe(
    connection_id: &ConnectionId,
    mut subscription: SubscriptionRequest,
    session: &mut Session,
    registry: &Arc<ConnectionRegistry>,
    app_context: &Arc<AppContext>,
    rate_limiter: &Arc<RateLimiter>,
    live_query_manager: &Arc<LiveQueryManager>,
    local_subscriptions: &mut Vec<String>,
    subscription_metadata: &mut std::collections::HashMap<String, (String, UserId, Option<SeqId>, usize)>,
) -> Result<(), String> {
    let user_id = registry
        .get_user_id(connection_id)
        .ok_or("Not authenticated")?;

    // Validate subscription ID
    if subscription.id.trim().is_empty() {
        let _ = send_error(
            session,
            "invalid_subscription",
            "INVALID_SUBSCRIPTION_ID",
            "Subscription ID cannot be empty",
        )
        .await;
        return Ok(());
    }

    // Rate limit check
    if !rate_limiter.check_subscription_limit(&user_id) {
        let _ = send_error(
            session,
            &subscription.id,
            "SUBSCRIPTION_LIMIT_EXCEEDED",
            "Maximum subscriptions reached",
        )
        .await;
        return Ok(());
    }

    // Parse and validate table
    let sql = &subscription.sql;
    let raw_table = kalamdb_core::live::QueryParser::extract_table_name(sql)
        .map_err(|e| format!("Invalid query: {}", e))?;

    let (namespace, table) = raw_table
        .split_once('.')
        .ok_or("Query must use namespace.table format")?;

    let table_id = kalamdb_commons::models::TableId::new(
        kalamdb_commons::models::NamespaceId::from(namespace),
        kalamdb_commons::models::TableName::from(table),
    );

    // Permission check
    let schema_registry = app_context.schema_registry();
    let table_def = schema_registry
        .get_table_definition(&table_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Table not found: {}.{}", namespace, table))?;

    let is_admin = user_id.is_admin();
    match table_def.table_type {
        kalamdb_commons::TableType::User if !is_admin && namespace != user_id.as_str() => {
            let _ = send_error(
                session,
                &subscription.id,
                "UNAUTHORIZED",
                "Insufficient privileges",
            )
            .await;
            return Ok(());
        }
        kalamdb_commons::TableType::System if !is_admin => {
            let _ = send_error(
                session,
                &subscription.id,
                "UNAUTHORIZED",
                "Insufficient privileges for system table",
            )
            .await;
            return Ok(());
        }
        kalamdb_commons::TableType::Shared => {
            let _ = send_error(
                session,
                &subscription.id,
                "UNSUPPORTED",
                "Shared tables don't support subscriptions",
            )
            .await;
            return Ok(());
        }
        _ => {}
    }

    // Extract WHERE clause and populate parsed fields
    let where_clause = kalamdb_core::live::QueryParser::extract_where_clause(sql);
    subscription.table_id = Some(table_id.clone());
    subscription.where_clause = where_clause;
    subscription.projections = None;

    let batch_size = subscription.options.batch_size.unwrap_or(MAX_ROWS_PER_BATCH);
    let initial_opts = subscription
        .options
        .last_rows
        .map(|n| InitialDataOptions::last(n as usize))
        .unwrap_or_else(|| InitialDataOptions::batch(None, None, batch_size));

    let subscription_id = subscription.id.clone();
    let sql_clone = subscription.sql.clone();

    // Register with live query manager
    match live_query_manager
        .register_subscription_with_initial_data(connection_id.clone(), subscription, Some(initial_opts))
        .await
    {
        Ok(result) => {
            info!(
                "Subscription registered: id={}, user_id={}",
                subscription_id,
                user_id.as_str()
            );

            // Track locally
            local_subscriptions.push(subscription_id.clone());

            // Store metadata
            let snapshot_end_seq = result.initial_data.as_ref().and_then(|d| d.snapshot_end_seq);
            subscription_metadata.insert(
                subscription_id.clone(),
                (sql_clone, user_id.clone(), snapshot_end_seq, batch_size),
            );

            // Update rate limiter
            rate_limiter.increment_subscription(&user_id);

            // Add to unified registry
            let live_id =
                LiveQueryId::new(user_id.clone(), connection_id.clone(), subscription_id.clone());
            let _ = registry.add_subscription(
                connection_id,
                live_id,
                table_id,
                kalamdb_core::live::LiveQueryOptions::default(),
            );

            // Send response
            let batch_control = if let Some(ref initial) = result.initial_data {
                BatchControl {
                    batch_num: 0,
                    total_batches: None,
                    has_more: initial.has_more,
                    status: if initial.has_more {
                        BatchStatus::Loading
                    } else {
                        BatchStatus::Ready
                    },
                    last_seq_id: initial.last_seq,
                    snapshot_end_seq: initial.snapshot_end_seq,
                }
            } else {
                BatchControl {
                    batch_num: 0,
                    total_batches: Some(0),
                    has_more: false,
                    status: BatchStatus::Ready,
                    last_seq_id: None,
                    snapshot_end_seq: None,
                }
            };

            let ack =
                WebSocketMessage::subscription_ack(subscription_id.clone(), 0, batch_control.clone());
            let _ = send_json(session, &ack).await;

            if let Some(initial) = result.initial_data {
                let batch_msg =
                    WebSocketMessage::initial_data_batch(subscription_id, initial.rows, batch_control);
                let _ = send_json(session, &batch_msg).await;
            }

            Ok(())
        }
        Err(e) => {
            error!("Failed to register subscription {}: {}", subscription_id, e);
            let _ = send_error(
                session,
                &subscription_id,
                "SUBSCRIPTION_FAILED",
                &e.to_string(),
            )
            .await;
            Ok(())
        }
    }
}

/// Handle next batch request
async fn handle_next_batch(
    _connection_id: &ConnectionId,
    subscription_id: &str,
    last_seq_id: Option<SeqId>,
    session: &mut Session,
    live_query_manager: &Arc<LiveQueryManager>,
    subscription_metadata: &std::collections::HashMap<String, (String, UserId, Option<SeqId>, usize)>,
) -> Result<(), String> {
    let (sql, user_id, snapshot_end_seq, batch_size) = subscription_metadata
        .get(subscription_id)
        .ok_or("Subscription not found")?
        .clone();

    let opts = Some(InitialDataOptions::batch(
        last_seq_id,
        snapshot_end_seq,
        batch_size,
    ));

    match live_query_manager
        .fetch_initial_data_batch(&sql, &user_id, opts)
        .await
    {
        Ok(result) => {
            let batch_control = BatchControl {
                batch_num: 0,
                total_batches: None,
                has_more: result.has_more,
                status: if result.has_more {
                    BatchStatus::LoadingBatch
                } else {
                    BatchStatus::Ready
                },
                last_seq_id: result.last_seq,
                snapshot_end_seq: result.snapshot_end_seq,
            };

            let msg = WebSocketMessage::initial_data_batch(
                subscription_id.to_string(),
                result.rows,
                batch_control,
            );
            let _ = send_json(session, &msg).await;
            Ok(())
        }
        Err(e) => {
            let _ = send_error(session, subscription_id, "BATCH_FETCH_FAILED", &e.to_string()).await;
            Ok(())
        }
    }
}

/// Handle unsubscribe request
async fn handle_unsubscribe(
    connection_id: &ConnectionId,
    subscription_id: &str,
    registry: &Arc<ConnectionRegistry>,
    rate_limiter: &Arc<RateLimiter>,
    live_query_manager: &Arc<LiveQueryManager>,
    local_subscriptions: &mut Vec<String>,
    subscription_metadata: &mut std::collections::HashMap<String, (String, UserId, Option<SeqId>, usize)>,
) -> Result<(), String> {
    let user_id = match registry.get_user_id(connection_id) {
        Some(uid) => uid,
        None => return Ok(()),
    };

    let live_id = LiveQueryId::new(user_id.clone(), connection_id.clone(), subscription_id.to_string());

    if let Err(e) = live_query_manager.unregister_subscription(&live_id).await {
        error!("Failed to unsubscribe {}: {}", subscription_id, e);
    }

    // Remove from unified registry
    registry.remove_subscription(&live_id);

    // Update local tracking
    if let Some(pos) = local_subscriptions.iter().position(|x| x == subscription_id) {
        local_subscriptions.remove(pos);
    }
    subscription_metadata.remove(subscription_id);

    // Update rate limiter
    rate_limiter.decrement_subscription(&user_id);

    info!("Unsubscribed: {}", subscription_id);
    Ok(())
}

/// Cleanup connection on close
async fn cleanup_connection(
    connection_id: &ConnectionId,
    registry: &Arc<ConnectionRegistry>,
    rate_limiter: &Arc<RateLimiter>,
    live_query_manager: &Arc<LiveQueryManager>,
    local_subscriptions: &[String],
) {
    info!("Cleaning up connection: {}", connection_id);

    // Get user ID before unregistering
    let user_id = registry.get_user_id(connection_id);

    // Unregister from unified registry (handles subscription cleanup)
    let removed_live_ids = registry.unregister_connection(connection_id);

    // Unregister from live query manager
    if let Some(ref uid) = user_id {
        let _ = live_query_manager
            .unregister_connection(uid, connection_id)
            .await;

        // Update rate limiter
        rate_limiter.cleanup_connection(connection_id);
        for _ in local_subscriptions {
            rate_limiter.decrement_subscription(uid);
        }
    }

    debug!(
        "Connection cleanup complete: {} (removed {} subscriptions)",
        connection_id,
        removed_live_ids.len()
    );
}

/// Log failed authentication
async fn log_failed_auth(app_context: &Arc<AppContext>) {
    let entry = kalamdb_core::sql::executor::helpers::audit::log_auth_event(
        &UserId::new("unknown".to_string()),
        "LOGIN_WS",
        false,
        None,
    );
    let _ =
        kalamdb_core::sql::executor::helpers::audit::persist_audit_entry(app_context, &entry).await;
}

/// Send auth error and close (takes ownership of session to close it)
async fn send_auth_error(mut session: Session, message: &str) -> Result<(), ()> {
    let msg = WebSocketMessage::AuthError {
        message: message.to_string(),
    };
    if let Ok(json) = serde_json::to_string(&msg) {
        let _ = session.text(json).await;
    }
    session
        .close(Some(CloseReason {
            code: CloseCode::Policy,
            description: Some("Authentication failed".into()),
        }))
        .await
        .map_err(|_| ())
}

/// Send error notification
async fn send_error(session: &mut Session, id: &str, code: &str, message: &str) -> Result<(), ()> {
    let msg = Notification::error(id.to_string(), code.to_string(), message.to_string());
    send_json(session, &msg).await
}

/// Send JSON message
async fn send_json<T: serde::Serialize>(session: &mut Session, msg: &T) -> Result<(), ()> {
    if let Ok(json) = serde_json::to_string(msg) {
        session.text(json).await.map_err(|_| ())
    } else {
        Err(())
    }
}
