//! WebSocket session actor
//!
//! This module provides an Actix actor for managing WebSocket connections and live query subscriptions.

use actix::{Actor, ActorContext, AsyncContext, Handler, Message, StreamHandler};
use actix_web_actors::ws;
use log::{debug, error, info, warn};
use std::time::{Duration, Instant};

use crate::models::{Notification, SubscriptionRequest};

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
pub struct WebSocketSession {
    /// Unique connection identifier
    pub connection_id: String,
    
    /// Client must send ping at least once per 10 seconds (CLIENT_TIMEOUT),
    /// otherwise we drop connection.
    pub hb: Instant,
    
    /// List of active subscription IDs for this connection
    pub subscriptions: Vec<String>,
}

impl WebSocketSession {
    /// Create a new WebSocket session
    pub fn new(connection_id: String) -> Self {
        Self {
            connection_id,
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
                
                // Parse subscription request
                match serde_json::from_str::<SubscriptionRequest>(&text) {
                    Ok(sub_req) => {
                        info!("Registering {} subscriptions", sub_req.subscriptions.len());
                        
                        // Register each subscription
                        for subscription in sub_req.subscriptions {
                            info!("Subscription registered: id={}, sql={}", subscription.id, subscription.sql);
                            self.subscriptions.push(subscription.id.clone());
                            
                            // TODO: Register in live query manager (T054)
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
        let session = WebSocketSession::new("test-conn-123".to_string());
        assert_eq!(session.connection_id, "test-conn-123");
        assert_eq!(session.subscriptions.len(), 0);
    }
    
    #[test]
    fn test_heartbeat_constants() {
        assert_eq!(HEARTBEAT_INTERVAL, Duration::from_secs(5));
        assert_eq!(CLIENT_TIMEOUT, Duration::from_secs(10));
    }
}
