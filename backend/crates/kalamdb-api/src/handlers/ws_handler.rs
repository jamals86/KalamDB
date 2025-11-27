//! WebSocket handler for live query subscriptions
//!
//! This module provides the HTTP endpoint for establishing WebSocket connections
//! and managing live query subscriptions using actix-ws (non-actor based).
//!
//! Connection lifecycle and heartbeat management is handled by the shared
//! ConnectionsManager from kalamdb-core.
//!
//! Architecture:
//! - Connection created in ConnectionsManager on WebSocket open
//! - Subscriptions stored in ConnectionState.subscriptions
//! - No local tracking needed - everything is in ConnectionState

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
    ConnectionEvent, ConnectionId, InitialDataOptions, LiveQueryManager, ConnectionsManager,
    SharedConnectionState,
};
use log::{debug, error, info, warn};
use std::sync::Arc;

use crate::models::Notification;
use crate::rate_limiter::RateLimiter;

/// GET /v1/ws - Establish WebSocket connection
///
/// Accepts unauthenticated WebSocket connections.
/// Authentication happens via post-connection Authenticate message (3-second timeout enforced).
/// Uses ConnectionsManager for consolidated connection state management.
#[get("/ws")]
pub async fn websocket_handler(
    req: HttpRequest,
    stream: web::Payload,
    app_context: web::Data<Arc<AppContext>>,
    rate_limiter: web::Data<Arc<RateLimiter>>,
    live_query_manager: web::Data<Arc<LiveQueryManager>>,
    user_repo: web::Data<Arc<dyn UserRepository>>,
    connection_registry: web::Data<Arc<ConnectionsManager>>,
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
///
/// All subscription state is stored in ConnectionState.subscriptions.
/// No local tracking needed - cleanup is handled by ConnectionsManager.
async fn handle_websocket(
    connection_id: ConnectionId,
    client_ip: Option<String>,
    mut session: Session,
    mut msg_stream: actix_ws::MessageStream,
    registration: kalamdb_core::live::ConnectionRegistration,
    registry: Arc<ConnectionsManager>,
    app_context: Arc<AppContext>,
    rate_limiter: Arc<RateLimiter>,
    live_query_manager: Arc<LiveQueryManager>,
    user_repo: Arc<dyn UserRepository>,
) {
    let mut event_rx = registration.event_rx;
    let mut notification_rx = registration.notification_rx;
    let connection_state = registration.state;

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
                        connection_state.write().update_heartbeat();
                        if session.pong(&bytes).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        connection_state.write().update_heartbeat();
                    }
                    Some(Ok(Message::Text(text))) => {
                        connection_state.write().update_heartbeat();

                        // Rate limit check
                        if !rate_limiter.check_message_rate(&connection_id) {
                            warn!("Message rate limit exceeded: {}", connection_id);
                            let _ = send_error(&mut session, "rate_limit", "RATE_LIMIT_EXCEEDED", "Too many messages").await;
                            continue;
                        }

                        if let Err(e) = handle_text_message(
                            &connection_id,
                            &connection_state,
                            &client_ip,
                            &text,
                            &mut session,
                            &registry,
                            &app_context,
                            &rate_limiter,
                            &live_query_manager,
                            &user_repo,
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

    // Cleanup - ConnectionsManager handles subscription cleanup automatically
    cleanup_connection(&connection_id, &connection_state, &registry, &rate_limiter, &live_query_manager).await;
}

/// Handle text message from client
async fn handle_text_message(
    connection_id: &ConnectionId, //TODO: No need to pass the connection_id separately
    connection_state: &SharedConnectionState,
    client_ip: &Option<String>,
    text: &str,
    session: &mut Session,
    registry: &Arc<ConnectionsManager>,
    app_context: &Arc<AppContext>,
    rate_limiter: &Arc<RateLimiter>,
    live_query_manager: &Arc<LiveQueryManager>,
    user_repo: &Arc<dyn UserRepository>,
) -> Result<(), String> {
    let msg: ClientMessage =
        serde_json::from_str(text).map_err(|e| format!("Invalid message: {}", e))?;

    match msg {
        ClientMessage::Authenticate { username, password } => {
            connection_state.write().mark_auth_started();
            handle_authenticate(
                connection_id,
                connection_state,
                client_ip,
                &username,
                &password,
                session,
                registry,
                app_context,
                user_repo,
            )
            .await
        }
        ClientMessage::Subscribe { subscription } => {
            if !connection_state.read().is_authenticated() {
                let _ = send_error(session, "subscribe", "AUTH_REQUIRED", "Authentication required before subscribing").await;
                return Ok(());
            }
            handle_subscribe(
                connection_id,
                connection_state,
                subscription,
                session,
                registry,
                app_context,
                rate_limiter,
                live_query_manager,
            )
            .await
        }
        ClientMessage::NextBatch {
            subscription_id,
            last_seq_id,
        } => {
            if !connection_state.read().is_authenticated() {
                return Ok(());
            }
            handle_next_batch(
                connection_state,
                &subscription_id,
                last_seq_id,
                session,
                live_query_manager,
            )
            .await
        }
        ClientMessage::Unsubscribe { subscription_id } => {
            if !connection_state.read().is_authenticated() {
                return Ok(());
            }
            handle_unsubscribe(
                connection_id,
                connection_state,
                &subscription_id,
                registry,
                rate_limiter,
                live_query_manager,
            )
            .await
        }
    }
}

/// Handle authentication message
async fn handle_authenticate(
    connection_id: &ConnectionId, //TODO: No need to pass the connection_id separately
    connection_state: &SharedConnectionState,
    client_ip: &Option<String>,
    username: &str,
    password: &str,
    session: &mut Session,
    registry: &Arc<ConnectionsManager>,
    app_context: &Arc<AppContext>,
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

    // Mark authenticated in connection state
    connection_state.write().mark_authenticated(user.id.clone());
    // Update registry's user index
    registry.on_authenticated(connection_id, user.id.clone());

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
///
/// Validates subscription ID and rate limits, then delegates to LiveQueryManager
/// which handles all SQL parsing, permission checks, and registration.
async fn handle_subscribe(
    _connection_id: &ConnectionId, // TODO: Remove when fully migrated
    connection_state: &SharedConnectionState,
    subscription: SubscriptionRequest,
    session: &mut Session,
    _registry: &Arc<ConnectionsManager>,  // TODO: Remove when fully migrated
    _app_context: &Arc<AppContext>,
    rate_limiter: &Arc<RateLimiter>,
    live_query_manager: &Arc<LiveQueryManager>,
) -> Result<(), String> {
    let user_id = connection_state.read().user_id()
        .cloned()
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

    let subscription_id = subscription.id.clone();

    // Determine batch size for initial data options
    let batch_size = subscription.options.batch_size.unwrap_or(MAX_ROWS_PER_BATCH);

    // Create initial data options
    let initial_opts = subscription
        .options
        .last_rows
        .map(|n| InitialDataOptions::last(n as usize))
        .unwrap_or_else(|| InitialDataOptions::batch(None, None, batch_size));

    // Register subscription with initial data fetch
    // LiveQueryManager handles all SQL parsing, permission checks, and registration internally
    match live_query_manager
        .register_subscription_with_initial_data(
            connection_state,
            &subscription,
            Some(initial_opts),
        )
        .await
    {
        Ok(result) => {
            info!(
                "Subscription registered: id={}, user_id={}",
                subscription_id,
                user_id.as_str()
            );

            // Update rate limiter
            rate_limiter.increment_subscription(&user_id);

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
            // Map error types to appropriate WebSocket error codes
            let (code, message) = match &e {
                kalamdb_core::error::KalamDbError::PermissionDenied(msg) => ("UNAUTHORIZED", msg.as_str()),
                kalamdb_core::error::KalamDbError::NotFound(msg) => ("NOT_FOUND", msg.as_str()),
                kalamdb_core::error::KalamDbError::InvalidSql(msg) => ("INVALID_SQL", msg.as_str()),
                kalamdb_core::error::KalamDbError::InvalidOperation(msg) => ("UNSUPPORTED", msg.as_str()),
                _ => ("SUBSCRIPTION_FAILED", "Subscription registration failed"),
            };
            error!("Failed to register subscription {}: {}", subscription_id, e);
            let _ = send_error(
                session,
                &subscription_id,
                code,
                message,
            )
            .await;
            Ok(())
        }
    }
}

/// Handle next batch request
///
/// Uses subscription metadata from ConnectionState for batch fetching.
async fn handle_next_batch(
    connection_state: &SharedConnectionState,
    subscription_id: &str,
    last_seq_id: Option<SeqId>,
    session: &mut Session,
    live_query_manager: &Arc<LiveQueryManager>,
) -> Result<(), String> {
    match live_query_manager
        .fetch_initial_data_batch(connection_state, subscription_id, last_seq_id)
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
    connection_id: &ConnectionId,//TODO: No need to pass the connection_id separately
    connection_state: &SharedConnectionState,
    subscription_id: &str,
    _registry: &Arc<ConnectionsManager>,  // TODO: Remove when fully migrated
    rate_limiter: &Arc<RateLimiter>,
    live_query_manager: &Arc<LiveQueryManager>,
) -> Result<(), String> {
    let user_id = connection_state.read().user_id().cloned();
    let user_id = match user_id {
        Some(uid) => uid,
        None => return Ok(()),
    };

    let live_id = LiveQueryId::new(user_id.clone(), connection_id.clone(), subscription_id.to_string());

    if let Err(e) = live_query_manager.unregister_subscription(connection_state, subscription_id, &live_id).await {
        error!("Failed to unsubscribe {}: {}", subscription_id, e);
    }

    // Update rate limiter
    rate_limiter.decrement_subscription(&user_id);

    info!("Unsubscribed: {}", subscription_id);
    Ok(())
}

/// Cleanup connection on close
///
/// ConnectionsManager.unregister_connection handles all subscription cleanup.
async fn cleanup_connection(
    connection_id: &ConnectionId,//TODO: No need to pass the connection_id separately
    connection_state: &SharedConnectionState,
    registry: &Arc<ConnectionsManager>,
    rate_limiter: &Arc<RateLimiter>,
    live_query_manager: &Arc<LiveQueryManager>,
) {
    info!("Cleaning up connection: {}", connection_id);

    // Get user ID and subscription count before unregistering
    let (user_id, subscription_count) = {
        let state = connection_state.read();
        (state.user_id.clone(), state.subscriptions.len())
    };

    // Unregister from unified registry (handles subscription cleanup)
    let removed_live_ids = registry.unregister_connection(connection_id);

    // Unregister from live query manager
    if let Some(ref uid) = user_id {
        let _ = live_query_manager
            .unregister_connection(uid, connection_id)
            .await;

        // Update rate limiter
        rate_limiter.cleanup_connection(connection_id);
        for _ in 0..subscription_count {
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
