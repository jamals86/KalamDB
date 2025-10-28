//! WebSocket test utilities for KalamDB integration tests.
//!
//! This module provides helpers for testing WebSocket-based live queries:
//! - Connection management
//! - Subscription setup
//! - Notification validation
//! - Change tracking assertions
//!
//! # Architecture
//!
//! WebSocket testing uses the following approach:
//! 1. Connect to WebSocket endpoint (/ws)
//! 2. Send subscription message with query_id and SQL
//! 3. Wait for initial data and notifications
//! 4. Validate notification format and content
//! 5. Assert change types (INSERT, UPDATE, DELETE)
//!
//! # Usage
//!
//! ```no_run
//! use integration::common::websocket::{WebSocketClient, SubscriptionMessage, assert_insert_notification};
//!
//! #[actix_web::test]
//! async fn test_live_query() {
//!     let mut ws = WebSocketClient::connect("ws://localhost:8080/ws").await;
//!     
//!     ws.subscribe("messages", "SELECT * FROM app.messages WHERE user_id = 'user123'").await;
//!     
//!     // Trigger an INSERT...
//!     
//!     let notification = ws.wait_for_notification(Duration::from_secs(5)).await;
//!     assert_insert_notification(&notification, "messages");
//! }
//! ```

use anyhow::{bail, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::MaybeTlsStream;
use tokio_tungstenite::{
    connect_async_with_config, tungstenite::protocol::Message,
    tungstenite::protocol::WebSocketConfig, WebSocketStream,
};

/// WebSocket subscription message format.
///
/// Sent from client to server to establish a live query subscription.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionMessage {
    /// Subscriptions to establish
    pub subscriptions: Vec<Subscription>,
}

/// Individual subscription definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    /// Client-defined query ID (for multiplexing)
    pub query_id: String,
    /// SQL query to subscribe to
    pub sql: String,
    /// Optional subscription options
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Value>,
}

/// WebSocket notification message format.
///
/// Sent from server to client when data changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationMessage {
    /// Query ID this notification belongs to
    pub query_id: String,
    /// Change type: INSERT, UPDATE, or DELETE
    #[serde(rename = "type")]
    pub change_type: String,
    /// Changed data (new values for INSERT/UPDATE, old values for DELETE)
    pub data: serde_json::Map<String, Value>,
    /// Old values (for UPDATE operations)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_values: Option<serde_json::Map<String, Value>>,
    /// Timestamp of the change
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

/// WebSocket initial data message format.
///
/// Sent from server to client with initial query results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitialDataMessage {
    /// Query ID this data belongs to
    pub query_id: String,
    /// Initial rows
    pub rows: Vec<Value>,
    /// Row count
    pub count: usize,
}

/// WebSocket client for testing.
///
/// Real implementation using tokio-tungstenite for actual WebSocket connections.
pub struct WebSocketClient {
    /// WebSocket stream
    ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    /// Active subscriptions
    subscriptions: Vec<Subscription>,
    /// Received notifications (for testing)
    notifications: Vec<NotificationMessage>,
}

impl WebSocketClient {
    /// Connect to WebSocket endpoint.
    ///
    /// # Arguments
    ///
    /// * `url` - WebSocket URL (e.g., "ws://localhost:8080/ws")
    ///
    /// # Example
    ///
    /// ```no_run
    /// let ws = WebSocketClient::connect("ws://localhost:8080/ws").await;
    /// ```
    pub async fn connect(url: &str) -> Result<Self> {
        Self::connect_with_auth(url, None).await
    }

    /// Connect to WebSocket endpoint with authentication.
    ///
    /// # Arguments
    ///
    /// * `url` - WebSocket URL
    /// * `token` - Optional JWT token for authentication
    pub async fn connect_with_auth(url: &str, token: Option<&str>) -> Result<Self> {
        use tokio_tungstenite::tungstenite::http::Request;

        // Build HTTP request with optional Authorization header
        let mut request = Request::builder()
            .uri(url)
            .header(
                "Host",
                url.split("://")
                    .nth(1)
                    .and_then(|s| s.split('/').next())
                    .unwrap_or("localhost"),
            )
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tokio_tungstenite::tungstenite::handshake::client::generate_key(),
            );

        if let Some(token) = token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let request = request
            .body(())
            .map_err(|e| anyhow::anyhow!("Failed to build request: {}", e))?;

        // Connect with custom request
        let (ws_stream, _) =
            connect_async_with_config(request, Some(WebSocketConfig::default()), false).await?;

        Ok(Self {
            ws_stream,
            subscriptions: Vec::new(),
            notifications: Vec::new(),
        })
    }

    /// Subscribe to a live query.
    ///
    /// # Arguments
    ///
    /// * `query_id` - Client-defined query ID
    /// * `sql` - SQL query to subscribe to
    ///
    /// # Example
    ///
    /// ```no_run
    /// ws.subscribe("messages", "SELECT * FROM app.messages WHERE user_id = 'user123'").await;
    /// ```
    pub async fn subscribe(&mut self, query_id: &str, sql: &str) -> Result<()> {
        let subscription = Subscription {
            query_id: query_id.to_string(),
            sql: sql.to_string(),
            options: None,
        };

        self.subscriptions.push(subscription.clone());

        // Send subscription message to server
        let message = SubscriptionMessage {
            subscriptions: vec![subscription],
        };

        let json_str = serde_json::to_string(&message)?;
        self.ws_stream
            .send(Message::Text(json_str.into()))
            .await?;

        Ok(())
    }

    /// Subscribe with options.
    ///
    /// # Arguments
    ///
    /// * `query_id` - Client-defined query ID
    /// * `sql` - SQL query
    /// * `options` - Subscription options (e.g., batch size, buffer size)
    pub async fn subscribe_with_options(
        &mut self,
        query_id: &str,
        sql: &str,
        options: Value,
    ) -> Result<()> {
        let subscription = Subscription {
            query_id: query_id.to_string(),
            sql: sql.to_string(),
            options: Some(options),
        };

        self.subscriptions.push(subscription.clone());

        let message = SubscriptionMessage {
            subscriptions: vec![subscription],
        };

        let json_str = serde_json::to_string(&message)?;
        self.ws_stream
            .send(Message::Text(json_str.into()))
            .await?;

        Ok(())
    }

    /// Wait for a notification with timeout.
    ///
    /// # Arguments
    ///
    /// * `timeout_duration` - Maximum time to wait
    ///
    /// # Returns
    ///
    /// The next notification, or an error if timeout is reached
    pub async fn wait_for_notification(
        &mut self,
        timeout_duration: Duration,
    ) -> Result<NotificationMessage> {
        match timeout(timeout_duration, self.ws_stream.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                let notification: NotificationMessage = serde_json::from_str(&text)?;
                self.notifications.push(notification.clone());
                Ok(notification)
            }
            Ok(Some(Ok(_))) => {
                bail!("Received non-text WebSocket message")
            }
            Ok(Some(Err(e))) => {
                bail!("WebSocket error: {}", e)
            }
            Ok(None) => {
                bail!("WebSocket connection closed")
            }
            Err(_) => {
                bail!(
                    "Timeout waiting for notification after {:?}",
                    timeout_duration
                )
            }
        }
    }

    /// Wait for initial data response.
    ///
    /// # Arguments
    ///
    /// * `query_id` - Query ID to wait for
    /// * `timeout_duration` - Maximum time to wait
    pub async fn wait_for_initial_data(
        &mut self,
        query_id: &str,
        timeout_duration: Duration,
    ) -> Result<InitialDataMessage> {
        match timeout(timeout_duration, self.ws_stream.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                let initial_data: InitialDataMessage = serde_json::from_str(&text)?;
                if initial_data.query_id == query_id {
                    Ok(initial_data)
                } else {
                    bail!("Received initial data for different query_id")
                }
            }
            Ok(Some(Ok(_))) => {
                bail!("Received non-text WebSocket message")
            }
            Ok(Some(Err(e))) => {
                bail!("WebSocket error: {}", e)
            }
            Ok(None) => {
                bail!("WebSocket connection closed")
            }
            Err(_) => {
                bail!(
                    "Timeout waiting for initial data after {:?}",
                    timeout_duration
                )
            }
        }
    }

    /// Receive and buffer multiple notifications without blocking.
    ///
    /// Collects all available messages from the WebSocket stream.
    pub async fn receive_notifications(&mut self, timeout_duration: Duration) -> Result<()> {
        let deadline = tokio::time::Instant::now() + timeout_duration;

        while tokio::time::Instant::now() < deadline {
            match timeout(Duration::from_millis(50), self.ws_stream.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => {
                    if let Ok(notification) = serde_json::from_str::<NotificationMessage>(&text) {
                        self.notifications.push(notification);
                    }
                }
                Ok(Some(Ok(_))) => {
                    // Ignore non-text messages
                }
                Ok(Some(Err(_))) => {
                    break;
                }
                Ok(None) => {
                    break;
                }
                Err(_) => {
                    // Timeout on this iteration, continue loop
                    continue;
                }
            }
        }

        Ok(())
    }

    /// Disconnect from WebSocket.
    pub async fn disconnect(&mut self) -> Result<()> {
        self.ws_stream.close(None).await?;
        self.subscriptions.clear();
        self.notifications.clear();
        Ok(())
    }

    /// Get active subscription count.
    pub fn subscription_count(&self) -> usize {
        self.subscriptions.len()
    }

    /// Check if subscribed to a specific query_id.
    pub fn is_subscribed(&self, query_id: &str) -> bool {
        self.subscriptions.iter().any(|s| s.query_id == query_id)
    }

    /// Get all received notifications.
    ///
    /// Returns a reference to all notifications received since connection.
    pub fn get_notifications(&self) -> &[NotificationMessage] {
        &self.notifications
    }

    /// Clear notification buffer.
    pub fn clear_notifications(&mut self) {
        self.notifications.clear();
    }
}

/// Assert that a notification is an INSERT operation.
///
/// # Arguments
///
/// * `notification` - The notification to validate
/// * `expected_query_id` - Expected query ID
///
/// # Example
///
/// ```no_run
/// assert_insert_notification(&notification, "messages");
/// ```
pub fn assert_insert_notification(notification: &NotificationMessage, expected_query_id: &str) {
    assert_eq!(notification.query_id, expected_query_id);
    assert_eq!(notification.change_type, "INSERT");
    assert!(
        notification.old_values.is_none(),
        "INSERT should not have old_values"
    );
}

/// Assert that a notification is an UPDATE operation.
///
/// # Arguments
///
/// * `notification` - The notification to validate
/// * `expected_query_id` - Expected query ID
///
/// # Example
///
/// ```no_run
/// assert_update_notification(&notification, "messages");
/// ```
pub fn assert_update_notification(notification: &NotificationMessage, expected_query_id: &str) {
    assert_eq!(notification.query_id, expected_query_id);
    assert_eq!(notification.change_type, "UPDATE");
    assert!(
        notification.old_values.is_some(),
        "UPDATE should have old_values"
    );
}

/// Assert that a notification is a DELETE operation.
///
/// # Arguments
///
/// * `notification` - The notification to validate
/// * `expected_query_id` - Expected query ID
///
/// # Example
///
/// ```no_run
/// assert_delete_notification(&notification, "messages");
/// ```
pub fn assert_delete_notification(notification: &NotificationMessage, expected_query_id: &str) {
    assert_eq!(notification.query_id, expected_query_id);
    assert_eq!(notification.change_type, "DELETE");
}

/// Assert that notification data contains a specific field with value.
///
/// # Arguments
///
/// * `notification` - The notification to validate
/// * `field` - Field name to check
/// * `expected_value` - Expected value
pub fn assert_notification_field(
    notification: &NotificationMessage,
    field: &str,
    expected_value: &Value,
) {
    let actual_value = notification
        .data
        .get(field)
        .unwrap_or_else(|| panic!("Field '{}' not found in notification data", field));

    assert_eq!(
        actual_value, expected_value,
        "Field '{}' has unexpected value",
        field
    );
}

/// Assert that a subscription was registered in system.live_queries.
///
/// This would be used with a query against system.live_queries.
///
/// # Arguments
///
/// * `query_result` - Result from SELECT * FROM system.live_queries
/// * `expected_query_id` - Expected query ID
pub fn assert_subscription_registered(query_result: &Value, expected_query_id: &str) {
    let rows = query_result
        .as_array()
        .expect("Query result should be an array");

    let found = rows.iter().any(|row| {
        row.get("query_id")
            .and_then(|v| v.as_str())
            .map(|id| id.contains(expected_query_id))
            .unwrap_or(false)
    });

    assert!(
        found,
        "Subscription '{}' not found in system.live_queries",
        expected_query_id
    );
}

/// Create a subscription message JSON.
///
/// # Arguments
///
/// * `query_id` - Query ID
/// * `sql` - SQL query
///
/// # Returns
///
/// JSON value for subscription message
pub fn create_subscription_message(query_id: &str, sql: &str) -> Value {
    json!({
        "subscriptions": [
            {
                "query_id": query_id,
                "sql": sql
            }
        ]
    })
}

/// Create a subscription message with options.
///
/// # Arguments
///
/// * `query_id` - Query ID
/// * `sql` - SQL query
/// * `options` - Subscription options
pub fn create_subscription_message_with_options(
    query_id: &str,
    sql: &str,
    options: Value,
) -> Value {
    json!({
        "subscriptions": [
            {
                "query_id": query_id,
                "sql": sql,
                "options": options
            }
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "Requires running server"]
    async fn test_websocket_client_connect() {
        let ws = WebSocketClient::connect("ws://localhost:8080/ws")
            .await
            .unwrap();
        assert_eq!(ws.subscription_count(), 0);
    }

    #[tokio::test]
    #[ignore = "Requires running server"]
    async fn test_subscribe() {
        let mut ws = WebSocketClient::connect("ws://localhost:8080/ws")
            .await
            .unwrap();

        ws.subscribe("messages", "SELECT * FROM app.messages")
            .await
            .unwrap();

        assert_eq!(ws.subscription_count(), 1);
        assert!(ws.is_subscribed("messages"));
    }

    #[test]
    fn test_create_subscription_message() {
        let message = create_subscription_message("test", "SELECT * FROM app.messages");

        assert!(message.get("subscriptions").is_some());
        let subs = message.get("subscriptions").unwrap().as_array().unwrap();
        assert_eq!(subs.len(), 1);

        let sub = &subs[0];
        assert_eq!(sub.get("query_id").unwrap().as_str().unwrap(), "test");
        assert_eq!(
            sub.get("sql").unwrap().as_str().unwrap(),
            "SELECT * FROM app.messages"
        );
    }

    #[test]
    fn test_assert_insert_notification() {
        let mut data = serde_json::Map::new();
        data.insert("id".to_string(), json!(1));
        data.insert("content".to_string(), json!("test"));

        let notification = NotificationMessage {
            query_id: "messages".to_string(),
            change_type: "INSERT".to_string(),
            data,
            old_values: None,
            timestamp: None,
        };

        assert_insert_notification(&notification, "messages");
    }

    #[test]
    fn test_assert_update_notification() {
        let mut data = serde_json::Map::new();
        data.insert("id".to_string(), json!(1));
        data.insert("content".to_string(), json!("updated"));

        let mut old_values = serde_json::Map::new();
        old_values.insert("id".to_string(), json!(1));
        old_values.insert("content".to_string(), json!("original"));

        let notification = NotificationMessage {
            query_id: "messages".to_string(),
            change_type: "UPDATE".to_string(),
            data,
            old_values: Some(old_values),
            timestamp: None,
        };

        assert_update_notification(&notification, "messages");
    }

    #[test]
    fn test_assert_notification_field() {
        let mut data = serde_json::Map::new();
        data.insert("id".to_string(), json!(1));
        data.insert("content".to_string(), json!("test"));

        let notification = NotificationMessage {
            query_id: "messages".to_string(),
            change_type: "INSERT".to_string(),
            data,
            old_values: None,
            timestamp: None,
        };

        assert_notification_field(&notification, "id", &json!(1));
        assert_notification_field(&notification, "content", &json!("test"));
    }
}
