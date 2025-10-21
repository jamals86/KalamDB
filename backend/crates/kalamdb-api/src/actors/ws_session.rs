//! WebSocket session actor
//!
//! This module provides an Actix actor for managing WebSocket connections and live query subscriptions.

use actix::{Actor, ActorContext, AsyncContext, Handler, Message, StreamHandler};
use actix_web_actors::ws;
use log::{debug, error, info, warn};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::models::{Notification, SubscriptionRequest};
use crate::rate_limiter::RateLimiter;

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
    pub user_id: Option<kalamdb_core::catalog::UserId>,

    /// Rate limiter for message and subscription limits
    pub rate_limiter: Option<Arc<RateLimiter>>,

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
        user_id: Option<kalamdb_core::catalog::UserId>,
        rate_limiter: Option<Arc<RateLimiter>>,
    ) -> Self {
        Self {
            connection_id,
            user_id,
            rate_limiter,
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

        // TODO: Cleanup subscriptions from live query manager
        // This will be implemented in T085 (subscription cleanup)
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

                        // Register each subscription
                        for subscription in sub_req.subscriptions {
                            // Authorization: Verify user_id is available for subscription
                            if self.user_id.is_none() {
                                error!(
                                    "Subscription rejected: no authenticated user (subscription_id={})",
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
                                continue;
                            }

                            // Rate limiting: Check subscription limit per user
                            if let (Some(ref uid), Some(ref limiter)) = (&self.user_id, &self.rate_limiter) {
                                if !limiter.check_subscription_limit(uid) {
                                    warn!(
                                        "Subscription limit exceeded: user_id={}, subscription_id={}",
                                        uid.as_ref(), subscription.id
                                    );
                                    let error_msg = Notification::error(
                                        subscription.id.clone(),
                                        "SUBSCRIPTION_LIMIT_EXCEEDED".to_string(),
                                        "Maximum number of subscriptions reached for this user.".to_string(),
                                    );
                                    if let Ok(json) = serde_json::to_string(&error_msg) {
                                        ctx.text(json);
                                    }
                                    continue;
                                }
                                // Increment subscription count for rate limiting
                                limiter.increment_subscription(uid);
                            }

                            // Authorization: For user tables, enforce user_id filtering
                            // This is enforced at the live query manager level (T174)
                            // The manager will automatically inject WHERE user_id = {current_user_id}
                            
                            info!(
                                "Subscription registered: id={}, sql={}, user_id={}",
                                subscription.id, 
                                subscription.sql,
                                self.user_id.as_ref().map(|id| id.as_ref()).unwrap_or("none")
                            );
                            self.subscriptions.push(subscription.id.clone());

                            // TODO: Register in live query manager (T054)
                            // The live query manager will:
                            // 1. Parse the SQL to detect if it targets a user table
                            // 2. Automatically inject user_id filter for user tables (T174)
                            // 3. Reject subscriptions that try to access other users' data
                            
                            // TODO: Fetch initial data if last_rows is set (T052)
                        }

                        // Send acknowledgment
                        let ack = serde_json::json!({
                            "type": "ack",
                            "message": format!("{} subscriptions registered", self.subscriptions.len())
                        });
                        ctx.text(ack.to_string());
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
pub struct SendNotification(pub Notification);

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

    #[test]
    fn test_websocket_session_creation() {
        let user_id = Some(kalamdb_core::catalog::UserId::from("user-123"));
        let session = WebSocketSession::new("test-conn-123".to_string(), user_id.clone(), None);
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
