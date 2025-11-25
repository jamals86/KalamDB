//! WebSocket subscription management for real-time updates.
//!
//! Provides real-time change notifications via WebSocket connections.
//! Implements T079 and T080: WebSocket connection establishment and message parsing.

use crate::{
    auth::AuthProvider,
    error::{KalamLinkError, Result},
    models::{BatchStatus, ChangeEvent, ServerMessage, SubscriptionConfig, SubscriptionOptions},
};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::collections::VecDeque;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        client::IntoClientRequest,
        error::Error as WsError,
        http::header::{HeaderValue, AUTHORIZATION},
        protocol::Message,
    },
};
use std::time::Duration;

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
        AuthProvider::BasicAuth(_username, _password) => {
            // For HTTP Basic Auth, we'll send via Authorization header instead of query params
            // This is more secure than exposing credentials in the URL
            ws_url.to_string()
        }
        AuthProvider::JwtToken(token) => append_query(ws_url, "token", token),
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
) -> Result<()> {
    use base64::{engine::general_purpose, Engine as _};

    match auth {
        AuthProvider::BasicAuth(username, password) => {
            // Encode username:password as base64 (RFC 7617)
            let credentials = format!("{}:{}", username, password);
            let encoded = general_purpose::STANDARD.encode(credentials.as_bytes());
            let value = format!("Basic {}", encoded);
            let header_value = HeaderValue::from_str(&value).map_err(|e| {
                KalamLinkError::ConfigurationError(format!(
                    "Invalid Basic Auth header value: {}",
                    e
                ))
            })?;
            request.headers_mut().insert(AUTHORIZATION, header_value);
        }
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
        AuthProvider::None => {}
    }

    Ok(())
}

async fn send_subscription_request(
    ws_stream: &mut WebSocketStream,
    subscription_id: &str,
    sql: &str,
    options: Option<SubscriptionOptions>,
) -> Result<()> {
    use crate::models::{ClientMessage, SubscriptionRequest};

    let subscription_req = SubscriptionRequest {
        id: subscription_id.to_string(),
        sql: sql.to_string(),
        options: options.unwrap_or_default(),
    };

    let message = ClientMessage::Subscribe {
        subscription: subscription_req,
    };

    let payload = serde_json::to_string(&message).map_err(|e| {
        KalamLinkError::WebSocketError(format!("Failed to serialize subscription: {}", e))
    })?;

    ws_stream
        .send(Message::Text(payload.into()))
        .await
        .map_err(|e| KalamLinkError::WebSocketError(format!("Failed to subscribe: {}", e)))
}

fn parse_message(text: &str) -> Result<Option<ChangeEvent>> {
    // Parse as ServerMessage (typed)
    match serde_json::from_str::<ServerMessage>(text) {
        Ok(msg) => {
            let event = match msg {
                // Auth messages are only used in WASM (post-connection auth)
                // CLI uses header-based auth, so these should not appear here
                // Return None to skip processing (will continue to next message)
                ServerMessage::AuthSuccess { .. } | ServerMessage::AuthError { .. } => {
                    return Ok(None);
                },
                ServerMessage::SubscriptionAck {
                    subscription_id,
                    total_rows,
                    batch_control,
                } => ChangeEvent::Ack {
                    subscription_id,
                    total_rows,
                    batch_control,
                },
                ServerMessage::InitialDataBatch {
                    subscription_id,
                    rows,
                    batch_control,
                } => ChangeEvent::InitialDataBatch {
                    subscription_id,
                    rows,
                    batch_control,
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
                }
            };
            Ok(Some(event))
        }
        Err(e) => {
            // If strict parsing fails, check if it's a ping/pong which might not match ServerMessage structure
            // or just return error.
            // However, tokio-tungstenite handles ping/pong frames at protocol level.
            // If the server sends a text message "ping", we might need to handle it.
            // But usually ping/pong are control frames.
            // Let's assume strict typing.
            Err(KalamLinkError::SerializationError(format!(
                "Failed to parse message as ServerMessage: {}",
                e
            )))
        }
    }
}


pub struct SubscriptionManager {
    ws_stream: WebSocketStream,
    subscription_id: String,
    event_queue: VecDeque<ChangeEvent>,
    buffered_changes: Vec<ChangeEvent>,
    is_loading: bool,
}

impl SubscriptionManager {
    /// Create a new WebSocket subscription
    ///
    /// **Implements T079**: WebSocket connection establishment
    pub(crate) async fn new(
        base_url: &str,
        config: SubscriptionConfig,
        auth: &AuthProvider,
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

        apply_ws_auth_headers(&mut request, auth)?;

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
        
        // Use the provided subscription ID (now required)
        let subscription_id = id;

        // Send subscription request
        send_subscription_request(&mut ws_stream, &subscription_id, &sql, options).await?;

        Ok(Self {
            ws_stream,
            subscription_id,
            event_queue: VecDeque::new(),
            buffered_changes: Vec::new(),
            is_loading: true,
        })
    }

    fn flush_buffered_changes(&mut self) {
        for change in self.buffered_changes.drain(..) {
            self.event_queue.push_back(change);
        }
    }

    /// Receive the next change event from the subscription
    ///
    /// **Implements T080**: WebSocket message parsing for ChangeEvent enum
    ///
    /// Returns `None` when the connection is closed.
    /// Automatically requests next batches when initial data has more batches available.
    pub async fn next(&mut self) -> Option<Result<ChangeEvent>> {
        loop {
            // 1. Drain event queue first
            if let Some(event) = self.event_queue.pop_front() {
                return Some(Ok(event));
            }

            match self.ws_stream.next().await {
                Some(Ok(Message::Text(text))) => {
                    match parse_message(&text) {
                        Ok(Some(event)) => {
                            if let Some(id) = event.subscription_id() {
                                if id != self.subscription_id {
                                    self.subscription_id = id.to_string();
                                }
                            }

                            // Check if this is an initial data batch with more batches pending
                            // and automatically request the next batch
                            if let ChangeEvent::InitialDataBatch {
                                ref batch_control, ..
                            } = event
                            {
                                if batch_control.has_more {
                                    if let Err(e) =
                                        self.request_next_batch(batch_control.last_seq_id).await
                                    {
                                        return Some(Err(e));
                                    }
                                }
                            }

                            // Handle buffering logic
                            match event {
                                ChangeEvent::Ack {
                                    ref batch_control, ..
                                } => {
                                    self.is_loading = batch_control.status != BatchStatus::Ready;
                                    self.event_queue.push_back(event);
                                    if !self.is_loading {
                                        self.flush_buffered_changes();
                                    }
                                }
                                ChangeEvent::InitialDataBatch {
                                    ref batch_control, ..
                                } => {
                                    self.is_loading = batch_control.status != BatchStatus::Ready;
                                    self.event_queue.push_back(event);
                                    if !self.is_loading {
                                        self.flush_buffered_changes();
                                    }
                                }
                                ChangeEvent::Insert { .. }
                                | ChangeEvent::Update { .. }
                                | ChangeEvent::Delete { .. } => {
                                    if self.is_loading {
                                        self.buffered_changes.push(event);
                                    } else {
                                        self.event_queue.push_back(event);
                                    }
                                }
                                _ => {
                                    self.event_queue.push_back(event);
                                }
                            }
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

    /// Request the next batch of initial data
    async fn request_next_batch(
        &mut self,
        last_seq_id: Option<crate::seq_id::SeqId>,
    ) -> Result<()> {
        use crate::models::ClientMessage;

        let message = ClientMessage::NextBatch {
            subscription_id: self.subscription_id.clone(),
            last_seq_id,
        };

        let payload = serde_json::to_string(&message).map_err(|e| {
            KalamLinkError::WebSocketError(format!("Failed to serialize NextBatch: {}", e))
        })?;

        self.ws_stream
            .send(Message::Text(payload.into()))
            .await
            .map_err(|e| KalamLinkError::WebSocketError(format!("Failed to send NextBatch: {}", e)))
    }

    /// Get the subscription ID assigned by the server
    pub fn subscription_id(&self) -> &str {
        &self.subscription_id
    }

    /// Close the subscription gracefully
    pub async fn close(mut self) -> Result<()> {
        use crate::models::ClientMessage;

        // Attempt best-effort unsubscribe before closing
        let message = ClientMessage::Unsubscribe {
            subscription_id: self.subscription_id.clone(),
        };

        let payload = serde_json::to_string(&message).unwrap_or_default();

        if !payload.is_empty() {
            let _ = self.ws_stream.send(Message::Text(payload.into())).await;
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
