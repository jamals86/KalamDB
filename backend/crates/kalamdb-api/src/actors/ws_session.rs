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

use crate::models::{Notification, SubscriptionRequest};
use crate::rate_limiter::RateLimiter;
use kalamdb_core::live_query::{
    ConnectionId as LiveConnectionId, InitialDataOptions, InitialDataResult, LiveQueryManager,
    LiveQueryOptions,
};
// UserId is from kalamdb_commons, not from live_query
type LiveUserId = UserId;

/// How often heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// How long before lack of client response causes a timeout
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

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
    /// Unique connection identifier
    pub connection_id: String,

    /// Authenticated user ID (from JWT token)
    /// None if authentication is disabled/optional
    pub user_id: Option<UserId>,

    /// Rate limiter for message and subscription limits
    pub rate_limiter: Option<Arc<RateLimiter>>,

    /// Live query manager for subscription lifecycle
    pub live_query_manager: Arc<LiveQueryManager>,

    /// Registered live query connection ID
    pub live_connection_id: Option<LiveConnectionId>,

    /// Live query user identifier (for manager interactions)
    pub live_user_id: Option<LiveUserId>,

    /// Client must send ping at least once per 10 seconds (CLIENT_TIMEOUT),
    /// otherwise we drop connection.
    pub hb: Instant,

    /// List of active subscription IDs for this connection
    pub subscriptions: Vec<String>,

    /// Subscription metadata cache for batch fetching
    /// Maps subscription_id -> (sql, user_id, snapshot_end_seq, batch_size)
    pub subscription_metadata:
        HashMap<String, (String, UserId, Option<kalamdb_commons::ids::SeqId>, usize)>,
}

impl WebSocketSession {
    /// Create a new WebSocket session
    ///
    /// # Arguments
    /// * `connection_id` - Unique connection identifier
    /// * `user_id` - Authenticated user ID (from JWT token)
    /// * `rate_limiter` - Optional rate limiter for message and subscription limits
    pub fn new(
        connection_id: String,
        user_id: Option<UserId>,
        rate_limiter: Option<Arc<RateLimiter>>,
        live_query_manager: Arc<LiveQueryManager>,
    ) -> Self {
        Self {
            connection_id,
            user_id,
            rate_limiter,
            live_query_manager,
            live_connection_id: None,
            live_user_id: None,
            hb: Instant::now(),
            subscriptions: Vec::new(),
            subscription_metadata: HashMap::new(),
        }
    }

    /// Start the heartbeat process
    fn hb(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            // Check client heartbeats
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                // Heartbeat timed out
                warn!("WebSocket Client heartbeat failed, disconnecting!");

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
        info!("WebSocket connection established: {}", self.connection_id);

        // Start heartbeat process
        self.hb(ctx);

        // Create notification channel for live query updates
        let (notification_tx, mut notification_rx) = tokio::sync::mpsc::unbounded_channel();

        // Spawn task to listen for notifications and send to WebSocket
        let addr = ctx.address();
        actix::spawn(async move {
            while let Some((_live_id, notification)) = notification_rx.recv().await {
                // Send typed notification to WebSocket client
                addr.do_send(SendNotification(notification));
            }
        });

        // Register connection with live query manager
        if let Some(ref user_id) = self.user_id {
            let manager = self.live_query_manager.clone();
            let unique_conn_id = self.connection_id.clone();
            let user_string = user_id.as_str().to_string();

            ctx.wait(
                fut::wrap_future(async move {
                    let live_user = LiveUserId::new(user_string);
                    let conn_id = manager
                        .register_connection(
                            live_user.clone(),
                            unique_conn_id,
                            Some(notification_tx),
                        )
                        .await;
                    (conn_id, live_user)
                })
                .map(|(result, live_user), act: &mut Self, ctx| match result {
                    Ok(conn_id) => {
                        act.live_connection_id = Some(conn_id);
                        act.live_user_id = Some(live_user);
                    }
                    Err(err) => {
                        error!(
                            "Failed to register live query connection {}: {}",
                            act.connection_id, err
                        );
                        ctx.close(None);
                        ctx.stop();
                    }
                }),
            );
        } else {
            error!(
                "WebSocket connection {} missing authenticated user; closing",
                self.connection_id
            );
            ctx.close(None);
            ctx.stop();
        }
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

        if let (Some(ref live_user), Some(ref live_conn)) =
            (&self.live_user_id, &self.live_connection_id)
        {
            let manager = self.live_query_manager.clone();
            let live_user = live_user.clone();
            let live_conn = live_conn.clone();
            actix::spawn(async move {
                if let Err(err) = manager.unregister_connection(&live_user, &live_conn).await {
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
                debug!("Received text message: {}", text);

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
                            ClientMessage::Subscribe { subscriptions } => {
                                let sub_req = crate::models::SubscriptionRequest {
                                    subscriptions: subscriptions
                                        .into_iter()
                                        .map(|s| crate::models::Subscription {
                                            id: s.id,
                                            sql: s.sql,
                                            options: crate::models::SubscriptionOptions {
                                                last_rows: s.options.last_rows.map(|n| n as usize),
                                                batch_size: s.options.batch_size,
                                            },
                                        })
                                        .collect(),
                                };
                                self.handle_subscription_request(ctx, sub_req);
                            }
                            ClientMessage::NextBatch {
                                subscription_id,
                                last_seq_id,
                            } => {
                                self.handle_next_batch_request(ctx, subscription_id, last_seq_id);
                            }
                            ClientMessage::Unsubscribe { subscription_id } => {
                                // TODO: Implement unsubscribe logic
                                warn!("Unsubscribe not yet implemented: {}", subscription_id);
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
    /// Handle subscription request message
    fn handle_subscription_request(
        &mut self,
        ctx: &mut ws::WebsocketContext<Self>,
        sub_req: SubscriptionRequest,
    ) {
        info!("Registering {} subscriptions", sub_req.subscriptions.len());

        // Ensure connection registered with live query manager
        let live_connection_id = match self.live_connection_id.clone() {
            Some(conn_id) => conn_id,
            None => {
                error!(
                    "Live query manager connection missing for {}",
                    self.connection_id
                );
                let error_msg = Notification::error(
                    "connection".to_string(),
                    "CONNECTION_NOT_READY".to_string(),
                    "Live query manager is not ready for subscriptions".to_string(),
                );
                if let Ok(json) = serde_json::to_string(&error_msg) {
                    ctx.text(json);
                }
                return;
            }
        };

        // Register each subscription
        for subscription in sub_req.subscriptions {
            self.process_single_subscription(ctx, &live_connection_id, subscription);
        }
    }

    /// Process a single subscription registration with batched initial data
    fn process_single_subscription(
        &mut self,
        ctx: &mut ws::WebsocketContext<Self>,
        live_connection_id: &LiveConnectionId,
        subscription: crate::models::Subscription,
    ) {
        use kalamdb_commons::websocket::{BatchControl, BatchStatus, MAX_ROWS_PER_BATCH};

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
            Some(InitialDataOptions::last(last_n))
        } else {
            Some(InitialDataOptions::batch(None, None, batch_size))
        };

        let manager = self.live_query_manager.clone();
        let rate_limiter = self.rate_limiter.clone();
        let subscription_id = subscription.id.clone();
        let sql = subscription.sql.clone();
        let sql_for_metadata = sql.clone(); // Clone for metadata storage
        let live_conn_clone = live_connection_id.clone();
        let user_clone = user_id.clone();

        ctx.wait(
            fut::wrap_future(async move {
                match manager
                    .register_subscription_with_initial_data(
                        live_conn_clone,
                        subscription_id.clone(),
                        sql.clone(),
                        LiveQueryOptions { last_rows: None },
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

        ctx.wait(
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
