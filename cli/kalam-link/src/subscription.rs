//! WebSocket subscription management for real-time updates.
//!
//! Provides real-time change notifications via WebSocket connections.
//! Implements T079 and T080: WebSocket connection establishment and message parsing.

use crate::{
    auth::AuthProvider,
    error::{KalamLinkError, Result},
    models::ChangeEvent,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

type WebSocketStream = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;

/// Manages WebSocket subscriptions for real-time change notifications.
///
/// # Examples
///
/// ```rust,no_run
/// use kalam_link::KalamLinkClient;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let client = KalamLinkClient::builder()
///     .base_url("http://localhost:3000")
///     .build()?;
///
/// let mut subscription = client.subscribe("SELECT * FROM messages").await?;
///
/// while let Some(event) = subscription.next().await {
///     match event {
///         Ok(change) => println!("Change detected: {:?}", change),
///         Err(e) => eprintln!("Error: {}", e),
///     }
/// }
/// # Ok(())
/// # }
/// ```
pub struct SubscriptionManager {
    query: String,
    ws_stream: WebSocketStream,
    subscription_id: Option<String>,
}

impl SubscriptionManager {
    /// Create a new WebSocket subscription
    ///
    /// **Implements T079**: WebSocket connection establishment
    pub(crate) async fn new(
        base_url: &str,
        query: &str,
        auth: &AuthProvider,
    ) -> Result<Self> {
        // Convert http:// to ws:// or https:// to wss://
        let ws_url = base_url
            .replace("http://", "ws://")
            .replace("https://", "wss://");
        let ws_url = format!("{}/ws", ws_url);

        // Build WebSocket connection URL
        let request_url = match auth {
            AuthProvider::JwtToken(token) => format!("{}?token={}", ws_url, token),
            AuthProvider::ApiKey(key) => format!("{}?api_key={}", ws_url, key),
            AuthProvider::None => ws_url,
        };

        // Connect to WebSocket
        let (ws_stream, _) = connect_async(&request_url)
            .await
            .map_err(|e| KalamLinkError::WebSocketError(format!("Connection failed: {}", e)))?;

        let mut manager = Self {
            query: query.to_string(),
            ws_stream,
            subscription_id: None,
        };

        // Send subscription request
        manager.send_subscription_request().await?;

        Ok(manager)
    }

    /// Send the subscription query to the server
    async fn send_subscription_request(&mut self) -> Result<()> {
        let subscribe_message = json!({
            "type": "SUBSCRIBE",
            "query": self.query,
        });

        let message = Message::Text(subscribe_message.to_string());
        self.ws_stream
            .send(message)
            .await
            .map_err(|e| KalamLinkError::WebSocketError(format!("Failed to subscribe: {}", e)))?;

        Ok(())
    }

    /// Receive the next change event from the subscription
    ///
    /// **Implements T080**: WebSocket message parsing for ChangeEvent enum
    ///
    /// Returns `None` when the connection is closed.
    pub async fn next(&mut self) -> Option<Result<ChangeEvent>> {
        loop {
            match self.ws_stream.next().await {
                Some(Ok(Message::Text(text))) => {
                    // Parse JSON message into ChangeEvent
                    match serde_json::from_str::<ChangeEvent>(&text) {
                        Ok(event) => {
                            // Store subscription ID from ACK message
                            if let ChangeEvent::Ack {
                                ref subscription_id,
                                ..
                            } = event
                            {
                                self.subscription_id = Some(subscription_id.clone());
                            }
                            return Some(Ok(event));
                        }
                        Err(e) => {
                            return Some(Err(KalamLinkError::SerializationError(format!(
                                "Failed to parse message: {}",
                                e
                            ))));
                        }
                    }
                }
                Some(Ok(Message::Close(_))) => {
                    // WebSocket closed normally
                    return None;
                }
                Some(Ok(_)) => {
                    // Ignore binary, ping, pong messages
                    continue;
                }
                Some(Err(e)) => {
                    return Some(Err(KalamLinkError::WebSocketError(e.to_string())));
                }
                None => {
                    // Stream ended
                    return None;
                }
            }
        }
    }

    /// Get the subscription ID assigned by the server
    pub fn subscription_id(&self) -> Option<&str> {
        self.subscription_id.as_deref()
    }

    /// Close the subscription gracefully
    pub async fn close(mut self) -> Result<()> {
        if let Some(sub_id) = &self.subscription_id {
            // Send unsubscribe message
            let unsubscribe_message = json!({
                "type": "UNSUBSCRIBE",
                "subscription_id": sub_id,
            });

            let _ = self
                .ws_stream
                .send(Message::Text(unsubscribe_message.to_string()))
                .await;
        }

        // Close WebSocket connection
        self.ws_stream
            .close(None)
            .await
            .map_err(|e| KalamLinkError::WebSocketError(format!("Failed to close: {}", e)))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ws_url_conversion() {
        let http_url = "http://localhost:3000";
        let ws_url = http_url.replace("http://", "ws://");
        assert_eq!(ws_url, "ws://localhost:3000");

        let https_url = "https://api.example.com";
        let wss_url = https_url.replace("https://", "wss://");
        assert_eq!(wss_url, "wss://api.example.com");
    }
}
