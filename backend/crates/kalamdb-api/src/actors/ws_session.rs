//! WebSocket session actor
//!
//! This module provides an Actix actor for managing WebSocket connections and live query subscriptions.

use actix::{
    fut, Actor, ActorContext, ActorFutureExt, AsyncContext, Handler, Message, StreamHandler,
};
use actix_web_actors::ws;
use kalamdb_commons::models::UserId;
use kalamdb_commons::WebSocketMessage;
use log::{debug, error, info, warn};
use std::convert::TryFrom;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::models::{Notification, SubscriptionRequest};
use crate::rate_limiter::RateLimiter;
use kalamdb_core::live_query::{
    ConnectionId as LiveConnectionId, InitialDataOptions, LiveQueryManager, LiveQueryOptions,
    UserId as LiveUserId,
};

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

                // Parse subscription request
                match serde_json::from_str::<SubscriptionRequest>(&text) {
                    Ok(sub_req) => {
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
                                        "User authentication required for subscriptions"
                                            .to_string(),
                                    );
                                    if let Ok(json) = serde_json::to_string(&error_msg) {
                                        ctx.text(json);
                                    }
                                    continue;
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
                                        "Maximum number of subscriptions reached for this user."
                                            .to_string(),
                                    );
                                    if let Ok(json) = serde_json::to_string(&error_msg) {
                                        ctx.text(json);
                                    }
                                    continue;
                                }
                            }

                            // Convert last_rows option and validate bounds
                            let last_rows_u32 = match subscription.options.last_rows {
                                Some(value) => match u32::try_from(value) {
                                    Ok(v) => Some(v),
                                    Err(_) => {
                                        let error_msg = Notification::error(
                                            subscription.id.clone(),
                                            "INVALID_OPTION".to_string(),
                                            "last_rows must be between 0 and 4,294,967,295"
                                                .to_string(),
                                        );
                                        if let Ok(json) = serde_json::to_string(&error_msg) {
                                            ctx.text(json);
                                        }
                                        continue;
                                    }
                                },
                                None => None,
                            };

                            debug!(
                                "Subscription options: last_rows_u32={:?}, raw last_rows={:?}",
                                last_rows_u32, subscription.options.last_rows
                            );

                            let initial_data_options =
                                Some(if let Some(last_rows) = last_rows_u32 {
                                    InitialDataOptions::last(last_rows as usize)
                                } else {
                                    // Default: fetch up to 100 recent rows for initial data
                                    InitialDataOptions::last(100)
                                });

                            debug!(
                                "Initial data options: {:?} (will fetch: {})",
                                initial_data_options,
                                initial_data_options.is_some()
                            );

                            let manager = self.live_query_manager.clone();
                            let rate_limiter = self.rate_limiter.clone();
                            let subscription_id = subscription.id.clone();
                            let sql = subscription.sql.clone();
                            let live_conn_clone = live_connection_id.clone();
                            let user_clone = user_id.clone();

                            ctx.wait(
                                fut::wrap_future(async move {
                                    match manager
                                        .register_subscription_with_initial_data(
                                            live_conn_clone,
                                            subscription_id.clone(),
                                            sql.clone(),
                                            LiveQueryOptions {
                                                last_rows: last_rows_u32,
                                            },
                                            initial_data_options,
                                        )
                                        .await
                                    {
                                        Ok(result) => Ok((result, subscription_id.clone(), last_rows_u32)),
                                        Err(err) => Err((err, subscription_id.clone())),
                                    }
                                })
                                .map(move |outcome, act: &mut Self, ctx| {
                                    match outcome {
                                        Ok((sub_result, sub_id, last_rows_opt)) => {
                                            info!(
                                                "Subscription registered: id={}, user_id={}",
                                                sub_id,
                                                user_clone.as_str()
                                            );
                                            act.subscriptions.push(sub_id.clone());

                                            if let Some(ref limiter) = rate_limiter {
                                                limiter.increment_subscription(&user_clone);
                                            }

                                            // Send subscription acknowledgement using typed model
                                            let ack = WebSocketMessage::subscription_ack(
                                                sub_id.clone(),
                                                last_rows_opt.unwrap_or(0),
                                            );
                                            if let Ok(json) = serde_json::to_string(&ack) {
                                                ctx.text(json);
                                            }

                                            // Send initial data if available using typed model
                                            if let Some(initial) = sub_result.initial_data {
                                                info!(
                                                    "Sending initial data for subscription {}: {} rows",
                                                    sub_id,
                                                    initial.rows.len()
                                                );

                                                // Convert Vec<JsonValue> to Vec<HashMap<String, JsonValue>>
                                                let rows: Vec<std::collections::HashMap<String, serde_json::Value>> = initial.rows
                                                    .into_iter()
                                                    .filter_map(|v| {
                                                        if let serde_json::Value::Object(map) = v {
                                                            Some(map.into_iter().collect())
                                                        } else {
                                                            None
                                                        }
                                                    })
                                                    .collect();

                                                debug!("Converted {} JSON values to HashMap rows", rows.len());

                                                let initial_data = WebSocketMessage::initial_data(
                                                    sub_id.clone(),
                                                    rows,
                                                );
                                                if let Ok(json) = serde_json::to_string(&initial_data) {
                                                    debug!("Sending initial_data message: {} bytes", json.len());
                                                    ctx.text(json);
                                                } else {
                                                    error!("Failed to serialize initial data for subscription {}", sub_id);
                                                }
                                            } else {
                                                debug!("No initial data for subscription {}", sub_id);
                                            }
                                        }
                                        Err((err, sub_id)) => {
                                            error!(
                                                "Failed to register subscription {}: {}",
                                                sub_id, err
                                            );
                                            let error_msg = Notification::error(
                                                sub_id.clone(),
                                                "SUBSCRIPTION_FAILED".to_string(),
                                                err.to_string(),
                                            );
                                            if let Ok(json) = serde_json::to_string(&error_msg) {
                                                ctx.text(json);
                                            }

                                            // Close WebSocket connection after sending error
                                            warn!(
                                                "Closing WebSocket connection {} due to subscription failure",
                                                act.connection_id
                                            );
                                            ctx.close(Some(ws::CloseReason {
                                                code: ws::CloseCode::Normal,
                                                description: Some(format!("Subscription failed: {}", err)),
                                            }));
                                            ctx.stop();
                                        }
                                    }
                                }),
                            );
                        }
                    }
                    Err(e) => {
                        error!("Failed to parse subscription request: {}", e);

                        let error_msg = Notification::error(
                            "unknown".to_string(),
                            "INVALID_SUBSCRIPTION".to_string(),
                            format!("Failed to parse subscription request: {}", e),
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

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_core::live_query::{LiveQueryManager, NodeId};
    use tempfile::TempDir;

    #[test]
    fn test_websocket_session_creation() {
        let user_id = Some(UserId::from("user-123"));
        let temp_dir = TempDir::new().unwrap();
        let db_init = kalamdb_store::RocksDbInit::new(temp_dir.path().to_str().unwrap());
        let db = db_init.open().unwrap();
        let backend: Arc<dyn kalamdb_store::StorageBackend> =
            Arc::new(kalamdb_store::RocksDBBackend::new(db.clone()));
        let kalam_sql = Arc::new(kalamdb_sql::KalamSql::new(backend).unwrap());
        let manager = Arc::new(LiveQueryManager::new(
            kalam_sql,
            NodeId::new("test-node".to_string()),
            None,
            None,
            None,
        ));

        let session =
            WebSocketSession::new("test-conn-123".to_string(), user_id.clone(), None, manager);
        assert_eq!(session.connection_id, "test-conn-123");
        assert_eq!(session.user_id, user_id);
        assert_eq!(session.subscriptions.len(), 0);
    }

    #[test]
    fn test_heartbeat_constants() {
        assert_eq!(HEARTBEAT_INTERVAL, Duration::from_secs(5));
        assert_eq!(CLIENT_TIMEOUT, Duration::from_secs(10));
    }
}
