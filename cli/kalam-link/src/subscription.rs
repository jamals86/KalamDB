//! WebSocket subscription management for real-time updates.
//!
//! Provides real-time change notifications via WebSocket connections.
//! Implements T079 and T080: WebSocket connection establishment and message parsing.

use crate::{
    auth::AuthProvider,
    error::{KalamLinkError, Result},
    models::{ChangeEvent, ServerMessage, SubscriptionOptions},
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        client::IntoClientRequest,
        error::Error as WsError,
        http::header::{HeaderName, HeaderValue, AUTHORIZATION},
        protocol::Message,
    },
};
use uuid::Uuid;

type WebSocketStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

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
/// Configuration for establishing a WebSocket subscription.
#[derive(Debug, Clone)]
pub struct SubscriptionConfig {
    /// Subscription identifier (if pre-allocated by server). If `None`, the client generates one.
    pub id: Option<String>,
    /// SQL query to register for live updates
    pub sql: String,
    /// Optional subscription options (e.g., last_rows)
    pub options: Option<SubscriptionOptions>,
    /// Override WebSocket URL (falls back to base_url conversion when `None`)
    pub ws_url: Option<String>,
}

fn resolve_ws_url(base_url: &str, override_url: Option<&str>) -> String {
    if let Some(url) = override_url {
        return url.to_string();
    }

    let normalized = base_url.trim_end_matches('/');
    let ws_base = normalized
        .replace("http://", "ws://")
        .replace("https://", "wss://");
    format!("{}/v1/ws", ws_base)
}

fn build_request_url(ws_url: &str, auth: &AuthProvider) -> String {
    match auth {
        AuthProvider::JwtToken(token) => append_query(ws_url, "token", token),
        AuthProvider::ApiKey(key) => append_query(ws_url, "api_key", key),
        AuthProvider::None => ws_url.to_string(),
    }
}

fn append_query(base: &str, key: &str, value: &str) -> String {
    if base.contains('?') {
        format!("{}&{}={}", base, key, value)
    } else {
        format!("{}?{}={}", base, key, value)
    }
}

fn apply_ws_auth_headers(
    request: &mut tokio_tungstenite::tungstenite::http::Request<()>,
    auth: &AuthProvider,
    user_id: Option<&str>,
) -> Result<()> {
    match auth {
        AuthProvider::JwtToken(token) => {
            let value = format!("Bearer {}", token);
            let header_value = HeaderValue::from_str(&value).map_err(|e| {
                KalamLinkError::ConfigurationError(format!(
                    "Invalid JWT token for Authorization header: {}",
                    e
                ))
            })?;
            request.headers_mut().insert(AUTHORIZATION, header_value);
        }
        AuthProvider::ApiKey(key) => {
            let header_value = HeaderValue::from_str(key).map_err(|e| {
                KalamLinkError::ConfigurationError(format!("Invalid API key header value: {}", e))
            })?;
            request
                .headers_mut()
                .insert(HeaderName::from_static("x-api-key"), header_value);
        }
        AuthProvider::None => {}
    }

    if let Some(uid) = user_id {
        if !uid.is_empty() {
            if let Ok(header_value) = HeaderValue::from_str(uid) {
                request
                    .headers_mut()
                    .insert(HeaderName::from_static("x-user-id"), header_value);
            }
        }
    }

    Ok(())
}

async fn send_subscription_request(
    ws_stream: &mut WebSocketStream,
    subscription_id: &str,
    sql: &str,
    options: Option<SubscriptionOptions>,
) -> Result<()> {
    let mut subscription = serde_json::Map::new();
    subscription.insert("id".to_string(), json!(subscription_id));
    subscription.insert("sql".to_string(), json!(sql));

    if let Some(opts) = options {
        let mut options_map = serde_json::Map::new();
        if let Some(last_rows) = opts.last_rows {
            options_map.insert("last_rows".to_string(), json!(last_rows));
        }

        if !options_map.is_empty() {
            subscription.insert("options".to_string(), Value::Object(options_map));
        }
    }

    let payload = Value::Object({
        let mut root = serde_json::Map::new();
        root.insert(
            "subscriptions".to_string(),
            Value::Array(vec![Value::Object(subscription)]),
        );
        root
    });

    ws_stream
        .send(Message::Text(payload.to_string()))
        .await
        .map_err(|e| KalamLinkError::WebSocketError(format!("Failed to subscribe: {}", e)))
}

fn parse_message(text: &str, fallback_subscription_id: &str) -> Result<Option<ChangeEvent>> {
    // First try to parse as ServerMessage (typed)
    match serde_json::from_str::<ServerMessage>(text) {
        Ok(msg) => {
            let event = match msg {
                ServerMessage::SubscriptionAck {
                    subscription_id,
                    last_rows: _,
                } => ChangeEvent::Ack {
                    subscription_id: Some(subscription_id),
                    message: Some("Subscription registered".to_string()),
                },
                ServerMessage::InitialData {
                    subscription_id,
                    rows,
                    count: _,
                } => ChangeEvent::InitialData {
                    subscription_id,
                    rows: rows.into_iter().map(|row| serde_json::to_value(row).unwrap_or(Value::Null)).collect(),
                },
                ServerMessage::Change {
                    subscription_id,
                    change_type,
                    rows,
                    old_values,
                } => {
                    use crate::models::ChangeTypeRaw;
                    match change_type {
                        ChangeTypeRaw::Insert => ChangeEvent::Insert {
                            subscription_id,
                            rows: rows
                                .unwrap_or_default()
                                .into_iter()
                                .map(|row| serde_json::to_value(row).unwrap_or(Value::Null))
                                .collect(),
                        },
                        ChangeTypeRaw::Update => ChangeEvent::Update {
                            subscription_id,
                            rows: rows
                                .unwrap_or_default()
                                .into_iter()
                                .map(|row| serde_json::to_value(row).unwrap_or(Value::Null))
                                .collect(),
                            old_rows: old_values
                                .unwrap_or_default()
                                .into_iter()
                                .map(|row| serde_json::to_value(row).unwrap_or(Value::Null))
                                .collect(),
                        },
                        ChangeTypeRaw::Delete => ChangeEvent::Delete {
                            subscription_id,
                            old_rows: old_values
                                .unwrap_or_default()
                                .into_iter()
                                .map(|row| serde_json::to_value(row).unwrap_or(Value::Null))
                                .collect(),
                        },
                    }
                }
                ServerMessage::Error {
                    subscription_id,
                    code,
                    message,
                } => ChangeEvent::Error {
                    subscription_id,
                    code,
                    message,
                },
            };
            return Ok(Some(event));
        }
        Err(_) => {
            // Fall back to manual parsing for backwards compatibility
        }
    }

    // Fallback: Parse as generic JSON (for legacy messages or unknown formats)
    let value: Value = serde_json::from_str(text).map_err(|e| {
        KalamLinkError::SerializationError(format!("Failed to parse message: {}", e))
    })?;

    let object = match value.as_object() {
        Some(obj) => obj,
        None => return Ok(Some(ChangeEvent::Unknown { raw: value })),
    };

    let msg_type = object
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();

    // Ignore ping/pong
    if matches!(msg_type.as_str(), "ping" | "pong") {
        return Ok(None);
    }

    // Legacy message handling for backwards compatibility
    if msg_type == "ack" || msg_type == "subscribed" {
        let subscription_id = extract_subscription_id(object, fallback_subscription_id);
        let message = object
            .get("message")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        return Ok(Some(ChangeEvent::Ack {
            subscription_id: Some(subscription_id),
            message,
        }));
    }

    if msg_type == "snapshot" {
        let subscription_id = extract_subscription_id(object, fallback_subscription_id);
        let rows = object
            .get("rows")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        return Ok(Some(ChangeEvent::InitialData {
            subscription_id,
            rows,
        }));
    }

    if msg_type == "notification" {
        let subscription_id = extract_subscription_id(object, fallback_subscription_id);
        let change_type = object
            .get("change_type")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        let rows = object
            .get("rows")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let old_rows = object
            .get("old_values")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        return Ok(Some(match change_type.as_str() {
            "insert" => ChangeEvent::Insert {
                subscription_id,
                rows,
            },
            "update" => ChangeEvent::Update {
                subscription_id,
                rows,
                old_rows,
            },
            "delete" => ChangeEvent::Delete {
                subscription_id,
                old_rows,
            },
            _ => ChangeEvent::Unknown {
                raw: Value::Object(object.clone()),
            },
        }));
    }

    // More legacy backwards compatibility
    let legacy_type = object
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_uppercase();

    let subscription_id = extract_subscription_id(object, fallback_subscription_id);

    let legacy_event = match legacy_type.as_str() {
        "INSERT" => {
            let row = object.get("row").cloned().unwrap_or(Value::Null);
            let rows = if row.is_null() { Vec::new() } else { vec![row] };
            ChangeEvent::Insert {
                subscription_id,
                rows,
            }
        }
        "UPDATE" => {
            let new_row = object.get("new_row").cloned().unwrap_or(Value::Null);
            let old_row = object.get("old_row").cloned().unwrap_or(Value::Null);
            ChangeEvent::Update {
                subscription_id,
                rows: if new_row.is_null() {
                    Vec::new()
                } else {
                    vec![new_row]
                },
                old_rows: if old_row.is_null() {
                    Vec::new()
                } else {
                    vec![old_row]
                },
            }
        }
        "DELETE" => {
            let row = object.get("row").cloned().unwrap_or(Value::Null);
            ChangeEvent::Delete {
                subscription_id,
                old_rows: if row.is_null() { Vec::new() } else { vec![row] },
            }
        }
        "SNAPSHOT" => {
            let rows = object
                .get("rows")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            ChangeEvent::InitialData {
                subscription_id,
                rows,
            }
        }
        "ACK" => {
            let message = object
                .get("query")
                .and_then(|v| v.as_str())
                .map(|s| format!("Subscribed to {}", s));
            ChangeEvent::Ack {
                subscription_id: Some(subscription_id),
                message,
            }
        }
        "ERROR" => {
            let message = object
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Subscription error")
                .to_string();
            ChangeEvent::Error {
                subscription_id,
                code: "ERROR".to_string(),
                message,
            }
        }
        _ => ChangeEvent::Unknown {
            raw: Value::Object(object.clone()),
        },
    };

    Ok(Some(legacy_event))
}

fn extract_subscription_id(object: &serde_json::Map<String, Value>, fallback: &str) -> String {
    object
        .get("subscription_id")
        .and_then(|v| v.as_str())
        .or_else(|| object.get("query_id").and_then(|v| v.as_str()))
        .unwrap_or(fallback)
        .to_string()
}

impl SubscriptionConfig {
    /// Create a new configuration with required SQL.
    pub fn new(sql: impl Into<String>) -> Self {
        Self {
            id: None,
            sql: sql.into(),
            options: None,
            ws_url: None,
        }
    }
}

pub struct SubscriptionManager {
    ws_stream: WebSocketStream,
    subscription_id: String,
}

impl SubscriptionManager {
    /// Create a new WebSocket subscription
    ///
    /// **Implements T079**: WebSocket connection establishment
    pub(crate) async fn new(
        base_url: &str,
        config: SubscriptionConfig,
        auth: &AuthProvider,
        user_id: Option<&str>,
    ) -> Result<Self> {
        let SubscriptionConfig {
            id,
            sql,
            options,
            ws_url,
        } = config;

        let ws_endpoint = resolve_ws_url(base_url, ws_url.as_deref());
        let request_url = build_request_url(&ws_endpoint, auth);

        // Connect to WebSocket
        let mut request = request_url.into_client_request().map_err(|e| {
            KalamLinkError::WebSocketError(format!("Failed to build WebSocket request: {}", e))
        })?;

        apply_ws_auth_headers(&mut request, auth, user_id)?;

        let (ws_stream, _) = match connect_async(request).await {
            Ok(result) => result,
            Err(WsError::Http(response)) => {
                let status = response.status();
                let body_opt = response.into_body();
                let body_text = body_opt
                    .as_ref()
                    .and_then(|b| {
                        if b.is_empty() {
                            None
                        } else {
                            Some(String::from_utf8_lossy(b).into_owned())
                        }
                    })
                    .unwrap_or_default();

                let message = match status.as_u16() {
                    401 => "Unauthorized: WebSocket requires valid credentials".to_string(),
                    403 => "Forbidden: Access to WebSocket denied".to_string(),
                    code => {
                        if body_text.is_empty() {
                            format!("WebSocket HTTP error: {}", code)
                        } else {
                            format!("WebSocket HTTP error {}: {}", code, body_text)
                        }
                    }
                };
                return Err(KalamLinkError::WebSocketError(message));
            }
            Err(e) => {
                return Err(KalamLinkError::WebSocketError(format!(
                    "Connection failed: {}",
                    e
                )));
            }
        };

        let mut ws_stream = ws_stream;
        let subscription_id = id.unwrap_or_else(|| format!("sub-{}", Uuid::new_v4()));

        // Send subscription request
        send_subscription_request(&mut ws_stream, &subscription_id, &sql, options).await?;

        Ok(Self {
            ws_stream,
            subscription_id,
        })
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
                    match parse_message(&text, &self.subscription_id) {
                        Ok(Some(event)) => {
                            if let Some(id) = event.subscription_id() {
                                if id != self.subscription_id {
                                    self.subscription_id = id.to_string();
                                }
                            }
                            return Some(Ok(event));
                        }
                        Ok(None) => continue,
                        Err(e) => return Some(Err(e)),
                    }
                }
                Some(Ok(Message::Close(_))) => {
                    // WebSocket closed normally
                    return None;
                }
                Some(Ok(Message::Ping(payload))) => {
                    // Reply to ping to keep connection alive
                    if let Err(e) = self.ws_stream.send(Message::Pong(payload)).await {
                        return Some(Err(KalamLinkError::WebSocketError(e.to_string())));
                    }
                    continue;
                }
                Some(Ok(Message::Pong(_))) => {
                    // Keepalive response
                    continue;
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
    pub fn subscription_id(&self) -> &str {
        &self.subscription_id
    }

    /// Close the subscription gracefully
    pub async fn close(mut self) -> Result<()> {
        // Attempt best-effort unsubscribe before closing
        let unsubscribe_message = json!({
            "type": "unsubscribe",
            "subscription_ids": [self.subscription_id.clone()],
        });
        let _ = self
            .ws_stream
            .send(Message::Text(unsubscribe_message.to_string()))
            .await;

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
        assert_eq!(
            resolve_ws_url("http://localhost:3000", None),
            "ws://localhost:3000/v1/ws"
        );
        assert_eq!(
            resolve_ws_url("https://api.example.com", None),
            "wss://api.example.com/v1/ws"
        );
        assert_eq!(
            resolve_ws_url("http://localhost:3000", Some("ws://override/ws")),
            "ws://override/ws"
        );
    }
}
