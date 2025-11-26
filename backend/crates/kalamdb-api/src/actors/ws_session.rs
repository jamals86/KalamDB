//! WebSocket session actor
//!
//! This module provides an Actix actor for managing WebSocket connections and live query subscriptions.

use actix::{
    fut, Actor, ActorContext, ActorFutureExt, AsyncContext, Handler, Message, StreamHandler,
};
use actix_web_actors::ws;
use kalamdb_commons::models::UserId;
use kalamdb_commons::WebSocketMessage;
use kalamdb_core::error::KalamDbError;
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::models::Notification;
use kalamdb_commons::websocket::SubscriptionRequest;
use crate::rate_limiter::RateLimiter;
use kalamdb_core::live_query::{
    ConnectionId, InitialDataOptions, InitialDataResult, LiveQueryManager,
};
/// How often heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// How long before lack of client response causes a timeout
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

/// How long to wait for authentication message before disconnecting
const AUTH_TIMEOUT: Duration = Duration::from_secs(3);

/// WebSocket session actor
///
/// Manages the lifecycle of a WebSocket connection and handles:
/// - Heartbeat/ping-pong for connection health
/// - Subscription registration
/// - Live query notifications
/// - Error handling
/// - User authentication and authorization
/// - Rate limiting
pub struct WebSocketSession {
    /// Unique connection identifier (generated when socket opens, before auth)
    pub connection_id: ConnectionId,

    /// Authenticated user ID (from JWT token)
    /// None until client sends Authenticate message
    pub user_id: Option<UserId>,

    /// Whether the connection has been authenticated
    pub is_authenticated: bool,

    /// Timestamp when connection was established (for auth timeout)
    pub connected_at: Instant,

    /// Client IP address for localhost bypass check
    pub client_ip: Option<String>,

    /// Rate limiter for message and subscription limits
    pub rate_limiter: Option<Arc<RateLimiter>>,

    /// Live query manager for subscription lifecycle
    pub live_query_manager: Arc<LiveQueryManager>,

    /// App context for audit logging
    pub app_context: Arc<kalamdb_core::app_context::AppContext>,

    /// User repository for authentication
    pub user_repo: Arc<dyn kalamdb_auth::UserRepository>,

    /// Client must send ping at least once per 10 seconds (CLIENT_TIMEOUT),
    /// otherwise we drop connection.
    pub hb: Instant,

    /// Last meaningful client activity (text/ping/pong/subscription)
    pub last_activity: Instant,

    /// List of active subscription IDs for this connection
    pub subscriptions: Vec<String>,

    /// Subscription metadata cache for batch fetching
    /// Maps subscription_id -> (sql, user_id, snapshot_end_seq, batch_size)
    pub subscription_metadata:
        HashMap<String, (String, UserId, Option<kalamdb_commons::ids::SeqId>, usize)>,

    /// Notification channel sender for live query updates
    pub notification_tx: Option<tokio::sync::mpsc::UnboundedSender<(kalamdb_commons::models::LiveQueryId, kalamdb_commons::Notification)>>,
}

impl WebSocketSession {
    /// Create a new WebSocket session
    ///
    /// # Arguments
    /// * `connection_id` - Unique connection identifier
    /// * `user_id` - Authenticated user ID (None until authentication)
    /// * `rate_limiter` - Optional rate limiter for message and subscription limits
    /// * `live_query_manager` - Live query manager for subscriptions
    /// * `app_context` - App context for audit logging
    /// * `user_repo` - User repository for authentication
    /// * `client_ip` - Client IP address for localhost bypass check
    pub fn new(
        connection_id: String,
        user_id: Option<UserId>,
        client_ip: Option<String>,
        rate_limiter: Option<Arc<RateLimiter>>,
        live_query_manager: Arc<LiveQueryManager>,
        app_context: Arc<kalamdb_core::app_context::AppContext>,
        user_repo: Arc<dyn kalamdb_auth::UserRepository>,
    ) -> Self {
        Self {
            connection_id: ConnectionId::new(connection_id),
            user_id,
            is_authenticated: false,
            connected_at: Instant::now(),
            client_ip,
            rate_limiter,
            live_query_manager,
            app_context,
            user_repo,
            hb: Instant::now(),
            last_activity: Instant::now(),
            subscriptions: Vec::new(),
            subscription_metadata: HashMap::new(),
            notification_tx: None,
        }
    }

    /// Start the heartbeat process
    fn hb(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            // Check authentication timeout (3 seconds)
            if !act.is_authenticated && Instant::now().duration_since(act.connected_at) > AUTH_TIMEOUT {
                error!(
                    "WebSocket authentication timeout: connection_id={}, waited {} seconds",
                    act.connection_id,
                    AUTH_TIMEOUT.as_secs()
                );
                
                // Send auth error message before closing
                let msg = WebSocketMessage::AuthError {
                    message: "Authentication timeout - no auth message received within 3 seconds".to_string(),
                };
                if let Ok(json) = serde_json::to_string(&msg) {
                    ctx.text(json);
                }
                
                // Close the WebSocket connection with a proper close frame before stopping
                ctx.close(Some(ws::CloseReason {
                    code: ws::CloseCode::Policy,
                    description: Some("Authentication timeout".to_string()),
                }));
                
                // Stop actor
                ctx.stop();
                return;
            }

            // Check client heartbeats
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                // Heartbeat timed out
                warn!("WebSocket Client heartbeat failed, disconnecting!");

                // Close the WebSocket connection with a proper close frame before stopping
                // This ensures the TCP connection is properly terminated
                ctx.close(Some(ws::CloseReason {
                    code: ws::CloseCode::Normal,
                    description: Some("Heartbeat timeout".to_string()),
                }));
                
                // Stop actor
                ctx.stop();

                return;
            }

            ctx.ping(b"");
        });
    }
}

impl Actor for WebSocketSession {
    type Context = ws::WebsocketContext<Self>;

    /// Called when the actor starts
    fn started(&mut self, ctx: &mut Self::Context) {
        info!("WebSocket connection established: {} (waiting for authentication)", self.connection_id);

        // Start heartbeat process (includes auth timeout check)
        self.hb(ctx);

        // Create notification channel for live query updates
        let (notification_tx, mut notification_rx) = tokio::sync::mpsc::unbounded_channel();
        self.notification_tx = Some(notification_tx);

        // Spawn task to listen for notifications and send to WebSocket
        let addr = ctx.address();
        actix::spawn(async move {
            while let Some((_live_id, notification)) = notification_rx.recv().await {
                // Send typed notification to WebSocket client
                addr.do_send(SendNotification(notification));
            }
        });

        // Note: Connection will be registered with live query manager
        // AFTER successful authentication (see AuthResult handler)
    }

    /// Called when the actor stops
    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("WebSocket connection closed: {}", self.connection_id);

        // Cleanup rate limiter state
        if let Some(ref limiter) = self.rate_limiter {
            limiter.cleanup_connection(&self.connection_id);

            // Decrement subscription counts for this user
            if let Some(ref uid) = self.user_id {
                for _ in 0..self.subscriptions.len() {
                    limiter.decrement_subscription(uid);
                }
            }
        }

        if let Some(ref user_id) = self.user_id {
            let manager = self.live_query_manager.clone();
            let user_id = user_id.clone();
            let live_conn = self.connection_id.clone();
            // Use tokio::spawn instead of actix::spawn to ensure cleanup completes
            // even after the Actix actor has stopped
            tokio::spawn(async move {
                if let Err(err) = manager.unregister_connection(&user_id, &live_conn).await {
                    warn!("Failed to unregister live query connection: {}", err);
                }
            });
        }
    }
}

/// Handle WebSocket messages from the client
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WebSocketSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                self.hb = Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                self.hb = Instant::now();
            }
            Ok(ws::Message::Text(text)) => {
                // Note: Not logging raw message to avoid exposing credentials in Authenticate messages

                // Rate limiting: Check message rate per connection
                if let Some(ref limiter) = self.rate_limiter {
                    if !limiter.check_message_rate(&self.connection_id) {
                        warn!(
                            "WebSocket message rate limit exceeded: connection_id={}",
                            self.connection_id
                        );
                        let error_msg = Notification::error(
                            "rate_limit".to_string(),
                            "RATE_LIMIT_EXCEEDED".to_string(),
                            "Too many messages per second. Please slow down.".to_string(),
                        );
                        if let Ok(json) = serde_json::to_string(&error_msg) {
                            ctx.text(json);
                        }
                        return;
                    }
                }

                // Parse incoming message using typed ClientMessage enum
                match serde_json::from_str::<kalamdb_commons::websocket::ClientMessage>(&text) {
                    Ok(client_msg) => {
                        use kalamdb_commons::websocket::ClientMessage;
                        match client_msg {
                            ClientMessage::Authenticate { username, password } => {
                                self.handle_authenticate(ctx, username, password);
                            }
                            ClientMessage::Subscribe { subscription } => {
                                // Check if authenticated
                                if !self.is_authenticated {
                                    error!("Subscription request before authentication: connection_id={}", self.connection_id);
                                    let error_msg = WebSocketMessage::AuthError {
                                        message: "Authentication required before subscribing".to_string(),
                                    };
                                    if let Ok(json) = serde_json::to_string(&error_msg) {
                                        ctx.text(json);
                                    }
                                    return;
                                }

                                self.handle_subscription(ctx, subscription);
                            }
                            ClientMessage::NextBatch {
                                subscription_id,
                                last_seq_id,
                            } => {
                                // Check if authenticated
                                if !self.is_authenticated {
                                    error!("Next batch request before authentication: connection_id={}", self.connection_id);
                                    return;
                                }

                                self.handle_next_batch_request(ctx, subscription_id, last_seq_id);
                            }
                            ClientMessage::Unsubscribe { subscription_id } => {
                                // Check if authenticated
                                if !self.is_authenticated {
                                    error!("Unsubscribe request before authentication: connection_id={}", self.connection_id);
                                    return;
                                }

                                //TODO: Pass connection_id to handle_unsubscribe
                                self.handle_unsubscribe(ctx, subscription_id);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to parse client message: {}", e);
                        let error_msg = Notification::error(
                            "unknown".to_string(),
                            "INVALID_MESSAGE".to_string(),
                            format!("Failed to parse message: {}", e),
                        );
                        if let Ok(json) = serde_json::to_string(&error_msg) {
                            ctx.text(json);
                        }
                    }
                }
            }
            Ok(ws::Message::Binary(_)) => {
                warn!("Binary messages not supported");
            }
            Ok(ws::Message::Close(reason)) => {
                info!("Client requested close: {:?}", reason);
                ctx.close(reason);
                ctx.stop();
            }
            Err(e) => {
                error!("WebSocket protocol error: {}", e);
                ctx.stop();
            }
            _ => {}
        }
    }
}

impl WebSocketSession {
    /// Handle authentication message from client
    fn handle_authenticate(
        &mut self,
        ctx: &mut ws::WebsocketContext<Self>,
        username: String,
        password: String,
    ) {
        if self.is_authenticated {
            warn!("Client {} already authenticated, ignoring duplicate auth request", self.connection_id);
            return;
        }

        info!("Authenticating WebSocket connection: connection_id={}, username={}", self.connection_id, username);

        // Clone Arc references for async block
        let user_repo = self.user_repo.clone();
        let app_context = self.app_context.clone();
        let connection_id = self.connection_id.clone();
        let client_ip = self.client_ip.clone();
        let addr = ctx.address();

        // Spawn async authentication task
        actix::spawn(async move {
            // Look up user
            let user_result = user_repo.get_user_by_username(&username).await;
            
            let auth_result = match user_result {
                Ok(user) => {
                    // Check if user is deleted
                    if user.deleted_at.is_some() {
                        Err("Invalid username or password".to_string())
                    } else {
                        // Check for localhost root bypass (same logic as HTTP auth)
                        let is_localhost = client_ip
                            .as_deref()
                            .map(|ip| kalamdb_auth::is_localhost_address(ip))
                            .unwrap_or(false);

                        let is_system_internal = user.role == kalamdb_commons::models::Role::System 
                            && user.auth_type == kalamdb_commons::models::AuthType::Internal;

                        // Localhost root bypass: if system/internal user from localhost with no password, allow
                        if is_system_internal && is_localhost && user.password_hash.is_empty() {
                            // Log successful authentication
                            let entry = kalamdb_core::sql::executor::helpers::audit::log_auth_event(
                                &user.id,
                                "LOGIN_WS_LOCALHOST",
                                true,
                                Some("Localhost root bypass".to_string()),
                            );
                            if let Err(e) = kalamdb_core::sql::executor::helpers::audit::persist_audit_entry(&app_context, &entry).await {
                                error!("Failed to persist audit log: {}", e);
                            }

                            Ok((user.id, user.role))
                        } else {
                            // Verify password
                            match kalamdb_auth::password::verify_password(&password, &user.password_hash).await {
                                Ok(true) => {
                                    // Log successful authentication
                                    let entry = kalamdb_core::sql::executor::helpers::audit::log_auth_event(
                                        &user.id,
                                        "LOGIN_WS",
                                        true,
                                        None,
                                    );
                                    if let Err(e) = kalamdb_core::sql::executor::helpers::audit::persist_audit_entry(&app_context, &entry).await {
                                        error!("Failed to persist audit log: {}", e);
                                    }

                                    Ok((user.id, user.role))
                                }
                                Ok(false) => Err("Invalid username or password".to_string()),
                                Err(_) => Err("Authentication error".to_string()),
                            }
                        }
                    }
                }
                Err(_) => Err("Invalid username or password".to_string()),
            };

            // Send result back to actor
            addr.do_send(AuthResult {
                connection_id,
                result: auth_result,
            });
        });
    }

    /// Handle subscription message - process single subscription per request
    fn handle_subscription(
        &mut self,
        ctx: &mut ws::WebsocketContext<Self>,
        mut subscription: SubscriptionRequest,
    ) {
        info!("Registering subscription: {}", subscription.id);

        // Validate subscription ID is non-empty (required field)
        if subscription.id.trim().is_empty() {
            error!("Subscription rejected: empty subscription ID");
            let error_msg = Notification::error(
                "invalid_subscription".to_string(),
                "INVALID_SUBSCRIPTION_ID".to_string(),
                "Subscription ID cannot be empty".to_string(),
            );
            if let Ok(json) = serde_json::to_string(&error_msg) {
                ctx.text(json);
            }
            return;
        }

        // Get authenticated user ID
        let user_id = match self.user_id.clone() {
            Some(uid) => uid,
            None => {
                error!(
                    "Subscription rejected: unauthenticated (subscription_id={})",
                    subscription.id
                );
                let error_msg = Notification::error(
                    subscription.id.clone(),
                    "UNAUTHORIZED".to_string(),
                    "User authentication required for subscriptions".to_string(),
                );
                if let Ok(json) = serde_json::to_string(&error_msg) {
                    ctx.text(json);
                }
                return;
            }
        };

        // Parse SQL and extract components (single parse point)
        let schema_registry = self.app_context.schema_registry();
        let sql = subscription.sql.clone();
        let sub_id_for_error = subscription.id.clone();
        
        // Extract table name
        let raw_table = match kalamdb_core::live::query_parser::QueryParser::extract_table_name(&sql) {
            Ok(t) => t,
            Err(e) => {
                error!("Failed to extract table name from query: {}", e);
                let error_msg = Notification::error(
                    sub_id_for_error,
                    "INVALID_QUERY".to_string(),
                    format!("Failed to parse table name: {}", e),
                );
                if let Ok(json) = serde_json::to_string(&error_msg) {
                    ctx.text(json);
                }
                return;
            }
        };

        let (namespace, table) = match raw_table.split_once('.') {
            Some((ns, tbl)) => (ns, tbl),
            None => {
                error!("Invalid table format (expected namespace.table): {}", raw_table);
                let error_msg = Notification::error(
                    sub_id_for_error,
                    "INVALID_QUERY".to_string(),
                    format!("Query must reference table as namespace.table: {}", raw_table),
                );
                if let Ok(json) = serde_json::to_string(&error_msg) {
                    ctx.text(json);
                }
                return;
            }
        };

        let namespace_id = kalamdb_commons::models::NamespaceId::from(namespace);
        let table_name = kalamdb_commons::models::TableName::from(table);
        let table_id = kalamdb_commons::models::TableId::new(namespace_id.clone(), table_name.clone());

        // Get table definition for permission checks
        let table_def = match schema_registry.get_table_definition(&table_id) {
            Ok(Some(def)) => def,
            Ok(None) => {
                error!("Table not found: {}.{}", namespace, table);
                let error_msg = Notification::error(
                    sub_id_for_error,
                    "TABLE_NOT_FOUND".to_string(),
                    format!("Table {}.{} not found", namespace, table),
                );
                if let Ok(json) = serde_json::to_string(&error_msg) {
                    ctx.text(json);
                }
                return;
            }
            Err(e) => {
                error!("Failed to get table definition: {}", e);
                let error_msg = Notification::error(
                    sub_id_for_error,
                    "SCHEMA_ERROR".to_string(),
                    format!("Failed to get table definition: {}", e),
                );
                if let Ok(json) = serde_json::to_string(&error_msg) {
                    ctx.text(json);
                }
                return;
            }
        };

        // Check permissions based on table type
        let is_admin = user_id.is_admin();
        
        use kalamdb_commons::TableType;
        let permission_result = match table_def.table_type {
            TableType::User => {
                if !is_admin && namespace != user_id.as_str() {
                    Err(format!(
                        "Insufficient privileges to subscribe to user table '{}.{}'",
                        namespace, table
                    ))
                } else {
                    Ok(())
                }
            }
            TableType::System => {
                if !is_admin {
                    Err(format!(
                        "Insufficient privileges to subscribe to system table '{}.{}'",
                        namespace, table
                    ))
                } else {
                    Ok(())
                }
            }
            TableType::Shared => {
                // Shared tables not supported for live queries
                Err(format!(
                    "Shared table subscriptions are not supported: '{}.{}'",
                    namespace, table
                ))
            }
            TableType::Stream => Ok(()), // Stream tables allow all authenticated users
        };

        if let Err(err_msg) = permission_result {
            error!("Permission denied for subscription: {}", err_msg);
            let error_msg = Notification::error(
                sub_id_for_error,
                "UNAUTHORIZED".to_string(),
                err_msg,
            );
            if let Ok(json) = serde_json::to_string(&error_msg) {
                ctx.text(json);
            }
            return;
        }

        // Extract WHERE clause (optional)
        let where_clause = kalamdb_core::live::query_parser::QueryParser::extract_where_clause(&sql);

        // TODO: Extract projections from SELECT clause (for now, assume SELECT *)
        let projections: Option<Vec<String>> = None;

        // Populate parsed fields in subscription request
        subscription.table_id = Some(table_id);
        subscription.where_clause = where_clause;
        subscription.projections = projections;

        // Process subscription
        self.process_single_subscription(ctx, &user_id, subscription);
    }

    /// Process a single subscription registration with batched initial data
    fn process_single_subscription(
        &mut self,
        ctx: &mut ws::WebsocketContext<Self>,
        user_id: &UserId,
        subscription: SubscriptionRequest,
    ) {
        use kalamdb_commons::websocket::{BatchControl, BatchStatus, MAX_ROWS_PER_BATCH};

        // Rate limiting: Check subscription limit per user
        if let Some(ref limiter) = self.rate_limiter {
            if !limiter.check_subscription_limit(&user_id) {
                warn!(
                    "Subscription limit exceeded: user_id={}, subscription_id={}",
                    user_id.as_str(),
                    subscription.id
                );
                let error_msg = Notification::error(
                    subscription.id.clone(),
                    "SUBSCRIPTION_LIMIT_EXCEEDED".to_string(),
                    "Maximum number of subscriptions reached for this user.".to_string(),
                );
                if let Ok(json) = serde_json::to_string(&error_msg) {
                    ctx.text(json);
                }
                return;
            }
        }

        // Use batch-based loading: first batch only
        // We pass None for since_seq and until_seq to start from beginning and determine snapshot boundary
        let batch_size = subscription
            .options
            .batch_size
            .unwrap_or(MAX_ROWS_PER_BATCH);

        let initial_data_options = if let Some(last_n) = subscription.options.last_rows {
            Some(InitialDataOptions::last(last_n as usize))
        } else {
            Some(InitialDataOptions::batch(None, None, batch_size))
        };

        let manager = self.live_query_manager.clone();
        let rate_limiter = self.rate_limiter.clone();
        let subscription_id = subscription.id.clone();
        let sql = subscription.sql.clone();
        let sql_for_metadata = sql.clone(); // Clone for metadata storage
        let user_clone: UserId = user_id.clone();
        let live_conn_clone = self.connection_id.clone();

        ctx.spawn(
            fut::wrap_future(async move {
                match manager
                    .register_subscription_with_initial_data(
                        live_conn_clone,
                        subscription,
                        initial_data_options,
                    )
                    .await
                {
                    Ok(result) => Ok((result, subscription_id.clone())),
                    Err(err) => Err((err, subscription_id.clone())),
                }
            })
            .map(move |outcome, act: &mut Self, ctx| {
                match outcome {
                    Ok((sub_result, sub_id)) => {
                        info!(
                            "Subscription registered: id={}, user_id={}",
                            sub_id,
                            user_clone.as_str()
                        );
                        act.subscriptions.push(sub_id.clone());

                        // Store subscription metadata for batch fetching
                        // We store the snapshot_end_seq returned by the first fetch
                        let snapshot_end_seq = sub_result
                            .initial_data
                            .as_ref()
                            .and_then(|d| d.snapshot_end_seq);
                        act.subscription_metadata.insert(
                            sub_id.clone(),
                            (
                                sql_for_metadata,
                                user_clone.clone(),
                                snapshot_end_seq,
                                batch_size,
                            ),
                        );

                        if let Some(ref limiter) = rate_limiter {
                            limiter.increment_subscription(&user_clone);
                        }

                        // Send subscription acknowledgement with batch control
                        if let Some(initial) = sub_result.initial_data {
                            // total_rows is unknown with lazy loading, set to 0
                            let total_rows = 0;
                            let batch_control = BatchControl {
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
                            };

                            let ack = WebSocketMessage::subscription_ack(
                                sub_id.clone(),
                                total_rows,
                                batch_control.clone(),
                            );
                            if let Ok(json) = serde_json::to_string(&ack) {
                                ctx.text(json);
                            }

                            // Send first batch of initial data
                            let batch_msg = WebSocketMessage::initial_data_batch(
                                sub_id.clone(),
                                initial.rows,
                                batch_control,
                            );
                            if let Ok(json) = serde_json::to_string(&batch_msg) {
                                debug!("Sending first batch: {} bytes", json.len());
                                ctx.text(json);
                            }
                        } else {
                            // No initial data (empty table)
                            let batch_control = BatchControl {
                                batch_num: 0,
                                total_batches: Some(0),
                                has_more: false,
                                status: BatchStatus::Ready,
                                last_seq_id: None,
                                snapshot_end_seq: None,
                            };
                            let ack = WebSocketMessage::subscription_ack(
                                sub_id.clone(),
                                0,
                                batch_control,
                            );
                            if let Ok(json) = serde_json::to_string(&ack) {
                                ctx.text(json);
                            }
                        }
                    }
                    Err((err, sub_id)) => {
                        error!("Failed to register subscription {}: {}", sub_id, err);
                        let error_msg = Notification::error(
                            sub_id.clone(),
                            "SUBSCRIPTION_FAILED".to_string(),
                            err.to_string(),
                        );
                        if let Ok(json) = serde_json::to_string(&error_msg) {
                            ctx.text(json);
                        }
                    }
                }
            }),
        );
    }

    /// Handle next batch request from client
    fn handle_next_batch_request(
        &mut self,
        ctx: &mut ws::WebsocketContext<Self>,
        subscription_id: String,
        last_seq_id: Option<kalamdb_commons::ids::SeqId>,
    ) {
        use kalamdb_commons::websocket::{BatchControl, BatchStatus};

        info!(
            "Next batch request: sub_id={}, last_seq_id={:?}",
            subscription_id, last_seq_id
        );

        // Verify subscription exists
        if !self.subscriptions.contains(&subscription_id) {
            warn!(
                "Next batch request for unknown subscription: {}",
                subscription_id
            );
            let error_msg = Notification::error(
                subscription_id.clone(),
                "SUBSCRIPTION_NOT_FOUND".to_string(),
                "Subscription not found for this connection".to_string(),
            );
            if let Ok(json) = serde_json::to_string(&error_msg) {
                ctx.text(json);
            }
            return;
        }

        // Fetch the requested batch via LiveQueryManager
        let manager = self.live_query_manager.clone();
        let sub_id = subscription_id.clone();
        let sub_id_for_result = sub_id.clone();

        // Get subscription metadata
        let metadata = match self.subscription_metadata.get(&sub_id).cloned() {
            Some(m) => m,
            None => {
                error!("Subscription metadata not found for: {}", sub_id);
                let error_msg = Notification::error(
                    sub_id.clone(),
                    "SUBSCRIPTION_NOT_FOUND".to_string(),
                    "Subscription metadata not found".to_string(),
                );
                if let Ok(json) = serde_json::to_string(&error_msg) {
                    ctx.text(json);
                }
                return;
            }
        };

        let (sql, user_id, snapshot_end_seq, batch_size) = metadata;

        ctx.spawn(
            fut::wrap_future(async move {
                // Fetch next batch of data
                let initial_data_options = Some(InitialDataOptions::batch(
                    last_seq_id,
                    snapshot_end_seq,
                    batch_size,
                ));
                let result = manager
                    .fetch_initial_data_batch(&sql, &user_id, initial_data_options)
                    .await?;

                Ok((result, sub_id_for_result.clone()))
            })
            .map(
                move |result: Result<(InitialDataResult, String), KalamDbError>,
                      _act: &mut Self,
                      ctx| {
                    match result {
                        Ok((initial, sub_id)) => {
                            // Send the batch data
                            let batch_control = BatchControl {
                                batch_num: 0, // Dummy value
                                total_batches: None,
                                has_more: initial.has_more,
                                status: if initial.has_more {
                                    BatchStatus::LoadingBatch
                                } else {
                                    BatchStatus::Ready
                                },
                                last_seq_id: initial.last_seq,
                                snapshot_end_seq: initial.snapshot_end_seq,
                            };

                            let batch_msg = WebSocketMessage::initial_data_batch(
                                sub_id.clone(),
                                initial.rows,
                                batch_control,
                            );

                            if let Ok(json) = serde_json::to_string(&batch_msg) {
                                debug!("Sending batch: {} bytes", json.len());
                                ctx.text(json);
                            }
                        }
                        Err(err) => {
                            error!("Failed to fetch next batch: {}", err);
                            let error_msg = Notification::error(
                                sub_id.clone(),
                                "BATCH_FETCH_FAILED".to_string(),
                                err.to_string(),
                            );
                            if let Ok(json) = serde_json::to_string(&error_msg) {
                                ctx.text(json);
                            }
                        }
                    }
                },
            ),
        );
    }

    /// Handle unsubscribe request
    fn handle_unsubscribe(
        &mut self,
        ctx: &mut ws::WebsocketContext<Self>,
        subscription_id: String,
    ) {
        info!("Unsubscribing: {}", subscription_id);

        // Verify subscription exists
        if !self.subscriptions.contains(&subscription_id) {
            warn!("Unsubscribe request for unknown subscription: {}", subscription_id);
            return;
        }

        // Reconstruct LiveId
        let manager = self.live_query_manager.clone();
        let sub_id = subscription_id.clone();
        let sub_id_for_log = sub_id.clone();
        let connection_id = self.connection_id.clone();
        let rate_limiter = self.rate_limiter.clone();
        let user_id = self.user_id.clone().unwrap();
        let user_id_for_limiter = user_id.clone();

        ctx.spawn(
            fut::wrap_future(async move {
                let live_id = kalamdb_commons::models::LiveQueryId::new(
                    user_id.clone(),
                    connection_id.clone(),
                    sub_id.clone()
                );
                
                manager.unregister_subscription(&live_id).await
            })
            .map(move |result, act: &mut Self, _ctx| {
                match result {
                    Ok(_) => {
                        info!("Unsubscribed successfully: {}", sub_id_for_log);
                        // Remove from local state
                        if let Some(pos) = act.subscriptions.iter().position(|x| x == &sub_id_for_log) {
                            act.subscriptions.remove(pos);
                        }
                        act.subscription_metadata.remove(&sub_id_for_log);
                        
                        // Decrement rate limiter
                        if let Some(ref limiter) = rate_limiter {
                            limiter.decrement_subscription(&user_id_for_limiter);
                        }
                    }
                    Err(e) => {
                        error!("Failed to unsubscribe {}: {}", sub_id_for_log, e);
                    }
                }
            })
        );
    }
}

/// Message for authentication result
#[derive(Message)]
#[rtype(result = "()")]
pub struct AuthResult {
    pub connection_id: ConnectionId,
    pub result: Result<(UserId, kalamdb_commons::models::Role), String>,
}

impl Handler<AuthResult> for WebSocketSession {
    type Result = ();

    fn handle(&mut self, msg: AuthResult, ctx: &mut Self::Context) {
        match msg.result {
            Ok((user_id, role)) => {
                info!(
                    "WebSocket authentication successful: connection_id={}, user_id={}, role={:?}",
                    self.connection_id, user_id.as_str(), role
                );

                // Mark as authenticated
                self.is_authenticated = true;
                self.user_id = Some(user_id.clone());

                // Register connection with live query manager now that we have user_id
                let manager = self.live_query_manager.clone();
                let connection_id = self.connection_id.clone();
                let user_for_manager = user_id.clone();
                let notification_tx = self.notification_tx.clone();

                ctx.spawn(
                    fut::wrap_future(async move {
                        let result = manager
                            .register_connection(
                                user_for_manager.clone(),
                                connection_id.clone(),
                                notification_tx,
                            )
                            .await;
                        (result, connection_id)
                    })
                    .map(|(result, connection_id), _act: &mut Self, ctx| {
                        match result {
                            Ok(conn_id) => {
                                // Connection is now registered (connection_id already stored in self)
                                info!(
                                    "WebSocket connection registered with LiveQueryManager: websocket_id={}, live_id={}",
                                    connection_id,
                                    conn_id
                                );
                            }
                            Err(err) => {
                                error!(
                                    "Failed to register live query connection {}: {}",
                                    connection_id, err
                                );
                                ctx.close(None);
                                ctx.stop();
                            }
                        }
                    }),
                );

                // Send success response
                let success_msg = WebSocketMessage::AuthSuccess {
                    user_id: user_id.as_str().to_string(),
                    role: format!("{:?}", role),
                };
                if let Ok(json) = serde_json::to_string(&success_msg) {
                    ctx.text(json);
                }
            }
            Err(error_msg) => {
                error!(
                    "WebSocket authentication failed: connection_id={}, error={}",
                    self.connection_id, error_msg
                );

                // Log failed authentication
                let entry = kalamdb_core::sql::executor::helpers::audit::log_auth_event(
                    &UserId::new("unknown".to_string()),
                    "LOGIN_WS",
                    false,
                    None,
                );
                let app_context = self.app_context.clone();
                actix::spawn(async move {
                    if let Err(e) = kalamdb_core::sql::executor::helpers::audit::persist_audit_entry(&app_context, &entry).await {
                        error!("Failed to persist audit log: {}", e);
                    }
                });

                // Send error response
                let error_response = WebSocketMessage::AuthError {
                    message: error_msg,
                };
                if let Ok(json) = serde_json::to_string(&error_response) {
                    ctx.text(json);
                }

                // Close connection
                ctx.stop();
            }
        }
    }
}

/// Message for sending notifications to the client
#[derive(Message)]
#[rtype(result = "()")]
pub struct SendNotification(pub kalamdb_commons::Notification);

impl Handler<SendNotification> for WebSocketSession {
    type Result = ();

    fn handle(&mut self, msg: SendNotification, ctx: &mut Self::Context) {
        if let Ok(json) = serde_json::to_string(&msg.0) {
            ctx.text(json);
        }
    }
}

impl Drop for WebSocketSession {
    fn drop(&mut self) {
        // Ensure cleanup happens even if stopped() wasn't called
        // This prevents file descriptor leaks when actors crash or panic
        if let Some(ref limiter) = self.rate_limiter {
            limiter.cleanup_connection(&self.connection_id);
            
            if let Some(ref uid) = self.user_id {
                for _ in 0..self.subscriptions.len() {
                    limiter.decrement_subscription(uid);
                }
            }
        }
        
        debug!("WebSocketSession dropped: {}", self.connection_id);
    }
}
