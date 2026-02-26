//! WebSocket subscription management for real-time updates.
//!
//! Provides real-time change notifications via WebSocket connections.
//! Implements T079 and T080: WebSocket connection establishment and message parsing.

use crate::{
    auth::AuthProvider,
    error::{KalamLinkError, Result},
    event_handlers::{ConnectionError, DisconnectReason, EventHandlers},
    models::{
        BatchStatus, ChangeEvent, ClientMessage, ConnectionOptions, ServerMessage,
        SubscriptionConfig, SubscriptionOptions, SubscriptionRequest, WsAuthCredentials,
    },
    timeouts::KalamLinkTimeouts,
};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use reqwest::Url;
use crate::models::ChangeTypeRaw;
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::io::{Error as IoError, ErrorKind};
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use tokio::net::{lookup_host, TcpSocket, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::Instant as TokioInstant;
use tokio_tungstenite::{
    client_async_tls_with_config, connect_async,
    tungstenite::{
        client::IntoClientRequest,
        error::Error as WsError,
        error::UrlError,
        handshake::client::Response as WsResponse,
        http::header::{HeaderValue, AUTHORIZATION},
        protocol::Message,
    },
};

type WebSocketStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>;

const MAX_WS_TEXT_MESSAGE_BYTES: usize = 64 << 20; // 64 MiB
const MAX_WS_BINARY_MESSAGE_BYTES: usize = 16 << 20; // 16 MiB
const MAX_WS_DECOMPRESSED_MESSAGE_BYTES: usize = 64 << 20; // 64 MiB

/// Default capacity for the internal event channel between the background
/// reader task and the consumer. When full, the reader applies back-pressure
/// by pausing WebSocket reads.
const DEFAULT_EVENT_CHANNEL_CAPACITY: usize = 8192;

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
fn resolve_ws_url(base_url: &str, override_url: Option<&str>, disable_compression: bool) -> Result<String> {
    let base = Url::parse(base_url.trim()).map_err(|e| {
        KalamLinkError::ConfigurationError(format!("Invalid base_url '{}': {}", base_url, e))
    })?;

    validate_ws_url(&base, false, "base_url")?;

    if let Some(url) = override_url {
        let override_parsed = Url::parse(url.trim()).map_err(|e| {
            KalamLinkError::ConfigurationError(format!(
                "Invalid WebSocket override URL '{}': {}",
                url, e
            ))
        })?;

        validate_ws_url(&override_parsed, true, "WebSocket override URL")?;

        if base.scheme() == "https" && override_parsed.scheme() == "ws" {
            return Err(KalamLinkError::ConfigurationError(
                "Refusing insecure ws:// override when base_url uses https://".to_string(),
            ));
        }

        let mut result = override_parsed.to_string();
        if disable_compression {
            result.push_str("?compress=false");
        }
        return Ok(result);
    }

    let mut ws_url = base.clone();
    let ws_scheme = match base.scheme() {
        "http" | "ws" => "ws",
        "https" | "wss" => "wss",
        other => {
            return Err(KalamLinkError::ConfigurationError(format!(
                "Unsupported base_url scheme '{}'; expected http(s) or ws(s)",
                other
            )));
        },
    };

    ws_url.set_scheme(ws_scheme).map_err(|_| {
        KalamLinkError::ConfigurationError("Failed to set WebSocket URL scheme".to_string())
    })?;
    ws_url.set_fragment(None);
    ws_url.set_path("/v1/ws");
    if disable_compression {
        ws_url.set_query(Some("compress=false"));
    } else {
        ws_url.set_query(None);
    }

    Ok(ws_url.to_string())
}

fn validate_ws_url(url: &Url, require_ws_scheme: bool, context: &str) -> Result<()> {
    if url.host_str().is_none() {
        return Err(KalamLinkError::ConfigurationError(format!("{} must include a host", context)));
    }

    if !url.username().is_empty() || url.password().is_some() {
        return Err(KalamLinkError::ConfigurationError(format!(
            "{} must not include username/password credentials",
            context
        )));
    }

    if require_ws_scheme {
        match url.scheme() {
            "ws" | "wss" => {},
            other => {
                return Err(KalamLinkError::ConfigurationError(format!(
                    "{} must use ws:// or wss:// (found '{}')",
                    context, other
                )));
            },
        }
    }

    if url.query().is_some() || url.fragment().is_some() {
        return Err(KalamLinkError::ConfigurationError(format!(
            "{} must not include query parameters or fragments",
            context
        )));
    }

    Ok(())
}

fn hash_start_index(key: &str, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    (hasher.finish() as usize) % len
}

fn parse_local_bind_addresses(addresses: &[String]) -> std::result::Result<Vec<IpAddr>, WsError> {
    let mut parsed = Vec::with_capacity(addresses.len());
    for raw in addresses {
        let candidate = raw.trim();
        if candidate.is_empty() {
            continue;
        }
        let ip: IpAddr = candidate.parse().map_err(|e| {
            WsError::Io(IoError::new(
                ErrorKind::InvalidInput,
                format!("Invalid ws_local_bind_addresses entry '{}': {}", candidate, e),
            ))
        })?;
        if !parsed.contains(&ip) {
            parsed.push(ip);
        }
    }
    Ok(parsed)
}

async fn connect_with_optional_local_bind(
    request: tokio_tungstenite::tungstenite::http::Request<()>,
    local_bind_addresses: &[String],
    subscription_id: &str,
) -> std::result::Result<(WebSocketStream, WsResponse), WsError> {
    if local_bind_addresses.is_empty() {
        return connect_async(request).await;
    }

    let host = request.uri().host().ok_or(WsError::Url(UrlError::NoHostName))?;
    let port = request
        .uri()
        .port_u16()
        .or_else(|| match request.uri().scheme_str() {
            Some("wss") => Some(443),
            Some("ws") => Some(80),
            _ => None,
        })
        .ok_or(WsError::Url(UrlError::UnsupportedUrlScheme))?;

    let remote_addrs: Vec<SocketAddr> =
        lookup_host((host, port)).await.map_err(WsError::Io)?.collect();
    if remote_addrs.is_empty() {
        return Err(WsError::Io(IoError::new(
            ErrorKind::AddrNotAvailable,
            format!("No resolved addresses for {}:{}", host, port),
        )));
    }

    let bind_ips = parse_local_bind_addresses(local_bind_addresses)?;
    if bind_ips.is_empty() {
        return Err(WsError::Io(IoError::new(
            ErrorKind::InvalidInput,
            "ws_local_bind_addresses is configured but empty after parsing",
        )));
    }

    let mut last_error: Option<IoError> = None;
    let mut attempted_connections = 0usize;
    let start = hash_start_index(subscription_id, bind_ips.len());

    for local_offset in 0..bind_ips.len() {
        let local_ip = bind_ips[(start + local_offset) % bind_ips.len()];
        let bind_addr = SocketAddr::new(local_ip, 0);

        for remote_addr in remote_addrs.iter().copied() {
            if remote_addr.is_ipv4() != local_ip.is_ipv4() {
                continue;
            }

            attempted_connections += 1;

            let socket = if remote_addr.is_ipv4() {
                TcpSocket::new_v4()
            } else {
                TcpSocket::new_v6()
            }
            .map_err(WsError::Io)?;

            if let Err(bind_err) = socket.bind(bind_addr) {
                last_error = Some(bind_err);
                continue;
            }

            match socket.connect(remote_addr).await {
                Ok(stream) => {
                    return client_async_tls_with_config(request, stream, None, None).await;
                },
                Err(connect_err) => {
                    last_error = Some(connect_err);
                },
            }
        }
    }

    if attempted_connections == 0 {
        return Err(WsError::Io(IoError::new(
            ErrorKind::InvalidInput,
            "No compatible ws_local_bind_addresses for resolved target address family",
        )));
    }

    Err(WsError::Io(last_error.unwrap_or_else(|| {
        IoError::new(
            ErrorKind::AddrNotAvailable,
            format!(
                "Failed to connect using configured ws_local_bind_addresses ({})",
                local_bind_addresses.join(", ")
            ),
        )
    })))
}

fn apply_ws_auth_headers(
    request: &mut tokio_tungstenite::tungstenite::http::Request<()>,
    auth: &AuthProvider,
) -> Result<()> {
    match auth {
        AuthProvider::BasicAuth(_, _) => {
            return Err(KalamLinkError::AuthenticationError(
                "WebSocket authentication requires a JWT token. Use AuthProvider::jwt_token or login first.".to_string(),
            ));
        },
        AuthProvider::JwtToken(token) => {
            let value = format!("Bearer {}", token);
            let header_value = HeaderValue::from_str(&value).map_err(|e| {
                KalamLinkError::ConfigurationError(format!(
                    "Invalid JWT token for Authorization header: {}",
                    e
                ))
            })?;
            request.headers_mut().insert(AUTHORIZATION, header_value);
        },
        AuthProvider::None => {},
    }

    Ok(())
}

/// Send authentication message and wait for AuthSuccess response
///
/// The WebSocket protocol requires an explicit Authenticate message after connection.
/// This function sends the authentication credentials and waits for the server's response.
async fn send_auth_and_wait(
    ws_stream: &mut WebSocketStream,
    auth: &AuthProvider,
    auth_timeout: Duration,
) -> Result<()> {
    // Convert auth provider to WsAuthCredentials
    let credentials = match auth {
        AuthProvider::BasicAuth(_, _) => {
            return Err(KalamLinkError::AuthenticationError(
                "WebSocket authentication requires a JWT token. Use AuthProvider::jwt_token or login first.".to_string(),
            ));
        },
        AuthProvider::JwtToken(token) => WsAuthCredentials::Jwt {
            token: token.clone(),
        },
        AuthProvider::None => {
            return Err(KalamLinkError::AuthenticationError(
                "Authentication required for WebSocket subscriptions".to_string(),
            ));
        },
    };

    // Send Authenticate message with credentials
    let auth_message = ClientMessage::Authenticate { credentials };
    let payload = serde_json::to_string(&auth_message).map_err(|e| {
        KalamLinkError::WebSocketError(format!("Failed to serialize auth message: {}", e))
    })?;

    ws_stream.send(Message::Text(payload.into())).await.map_err(|e| {
        KalamLinkError::WebSocketError(format!("Failed to send auth message: {}", e))
    })?;

    // Loop until we receive AuthSuccess/AuthError, tolerating Ping/Pong and
    // other non-auth frames that the server may send during the handshake.
    let deadline = TokioInstant::now() + auth_timeout;
    loop {
        let remaining = deadline.saturating_duration_since(TokioInstant::now());
        if remaining.is_zero() {
            return Err(KalamLinkError::TimeoutError(format!(
                "Authentication timeout ({:?})",
                auth_timeout
            )));
        }

        match tokio::time::timeout(remaining, ws_stream.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                match serde_json::from_str::<ServerMessage>(&text) {
                    Ok(ServerMessage::AuthSuccess {
                        user_id: _,
                        role: _,
                    }) => return Ok(()),
                    Ok(ServerMessage::AuthError { message }) => {
                        return Err(KalamLinkError::AuthenticationError(format!(
                            "WebSocket authentication failed: {}",
                            message
                        )));
                    },
                    // Tolerate other messages (e.g. server heartbeats) during
                    // the handshake — keep waiting for the auth reply.
                    Ok(_) => continue,
                    Err(e) => {
                        return Err(KalamLinkError::WebSocketError(format!(
                            "Failed to parse auth response: {}",
                            e
                        )));
                    },
                }
            },
            Ok(Some(Ok(Message::Ping(payload)))) => {
                // Reply to pings during the handshake.
                let _ = ws_stream.send(Message::Pong(payload)).await;
            },
            Ok(Some(Ok(
                Message::Pong(_) | Message::Binary(_) | Message::Frame(_),
            ))) => {
                continue;
            },
            Ok(Some(Ok(Message::Close(_)))) => {
                return Err(KalamLinkError::WebSocketError(
                    "Connection closed during authentication".to_string(),
                ));
            },
            Ok(Some(Err(e))) => {
                return Err(KalamLinkError::WebSocketError(format!(
                    "WebSocket error during authentication: {}",
                    e
                )));
            },
            Ok(None) => {
                return Err(KalamLinkError::WebSocketError(
                    "Connection closed before authentication completed".to_string(),
                ));
            },
            Err(_) => {
                return Err(KalamLinkError::TimeoutError(format!(
                    "Authentication timeout ({:?})",
                    auth_timeout
                )));
            },
        }
    }
}

async fn send_subscription_request(
    ws_stream: &mut WebSocketStream,
    subscription_id: &str,
    sql: &str,
    options: Option<SubscriptionOptions>,
) -> Result<()> {
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
                    schema,
                } => ChangeEvent::Ack {
                    subscription_id,
                    total_rows,
                    batch_control,
                    schema,
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
                },
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
            Ok(Some(event))
        },
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
        },
    }
}

/// Spread keepalive pings across connections to avoid synchronized bursts.
///
/// Uses deterministic jitter derived from subscription_id so reconnecting a
/// subscription preserves its phase and avoids thundering-herd effects.
fn jitter_keepalive_interval(base: Duration, subscription_id: &str) -> Duration {
    if base.is_zero() {
        return base;
    }

    let base_ms = base.as_millis() as u64;
    if base_ms <= 1 {
        return base;
    }

    // +/-20% jitter window.
    let jitter_span = (base_ms / 5).max(1);
    let mut hasher = DefaultHasher::new();
    subscription_id.hash(&mut hasher);
    let hashed = hasher.finish();

    let offset = (hashed % (2 * jitter_span + 1)) as i64 - jitter_span as i64;
    let jittered_ms = if offset >= 0 {
        base_ms.saturating_add(offset as u64)
    } else {
        base_ms.saturating_sub((-offset) as u64).max(1)
    };

    Duration::from_millis(jittered_ms)
}

// ── Standalone helpers used by the background reader task ───────────────────

/// Decode a binary (gzip-compressed) WebSocket payload into a UTF-8 string.
fn decode_ws_payload(data: &[u8]) -> Result<String> {
    if data.len() > MAX_WS_BINARY_MESSAGE_BYTES {
        return Err(KalamLinkError::WebSocketError(format!(
            "Binary WebSocket message too large ({} bytes > {} bytes)",
            data.len(),
            MAX_WS_BINARY_MESSAGE_BYTES
        )));
    }

    let decompressed = crate::compression::decompress_gzip_with_limit(
        data,
        MAX_WS_DECOMPRESSED_MESSAGE_BYTES,
    )
    .map_err(|e| {
        KalamLinkError::WebSocketError(format!("Failed to decompress message: {}", e))
    })?;
    String::from_utf8(decompressed).map_err(|e| {
        KalamLinkError::WebSocketError(format!(
            "Invalid UTF-8 in decompressed message: {}",
            e
        ))
    })
}

/// Send a `NextBatch` request through the WebSocket stream.
async fn send_next_batch_request(
    ws_stream: &mut WebSocketStream,
    subscription_id: &str,
    last_seq_id: Option<crate::seq_id::SeqId>,
) -> Result<()> {
    let message = ClientMessage::NextBatch {
        subscription_id: subscription_id.to_string(),
        last_seq_id,
    };
    let payload = serde_json::to_string(&message).map_err(|e| {
        KalamLinkError::WebSocketError(format!("Failed to serialize NextBatch: {}", e))
    })?;
    ws_stream
        .send(Message::Text(payload.into()))
        .await
        .map_err(|e| KalamLinkError::WebSocketError(format!("Failed to send NextBatch: {}", e)))
}

/// Best-effort Unsubscribe + Close over a WebSocket stream.
async fn send_unsubscribe_and_close(
    ws_stream: &mut WebSocketStream,
    subscription_id: &str,
) {
    let message = ClientMessage::Unsubscribe {
        subscription_id: subscription_id.to_string(),
    };
    if let Ok(payload) = serde_json::to_string(&message) {
        let _ = ws_stream.send(Message::Text(payload.into())).await;
    }
    let _ = ws_stream.close(None).await;
}

/// Parse a text payload, auto-request the next batch when needed, and forward
/// the parsed event to the bounded channel.
///
/// Returns `true` when the reader loop should exit (channel closed or fatal).
async fn process_and_forward(
    text: &str,
    ws_stream: &mut WebSocketStream,
    event_tx: &mpsc::Sender<Result<ChangeEvent>>,
    subscription_id: &str,
) -> bool {
    match parse_message(text) {
        Ok(Some(event)) => {
            // Auto-request next batch when initial data has more pages
            if let ChangeEvent::InitialDataBatch {
                ref batch_control, ..
            } = event
            {
                if batch_control.has_more {
                    if let Err(e) = send_next_batch_request(
                        ws_stream,
                        subscription_id,
                        batch_control.last_seq_id,
                    )
                    .await
                    {
                        let _ = event_tx.send(Err(e)).await;
                        return true;
                    }
                }
            }
            event_tx.send(Ok(event)).await.is_err()
        },
        Ok(None) => false, // skip auth acks etc.
        Err(e) => event_tx.send(Err(e)).await.is_err(),
    }
}

/// Background task that owns the WebSocket stream and forwards parsed events
/// through a bounded channel.
///
/// Responsibilities:
/// - Read WS frames, decompress binary payloads, parse JSON into `ChangeEvent`
/// - Auto-request next batch when `InitialDataBatch.has_more` is set
/// - Send periodic keepalive pings when idle
/// - Graceful shutdown on close signal or stream end
/// - Emit lifecycle events via `EventHandlers`
async fn ws_reader_loop(
    mut ws_stream: WebSocketStream,
    event_tx: mpsc::Sender<Result<ChangeEvent>>,
    close_rx: oneshot::Receiver<()>,
    subscription_id: String,
    keepalive_interval: Option<Duration>,
    event_handlers: EventHandlers,
) {
    // NOTE: emit_connect is called by SubscriptionManager::new() before this
    // task is spawned, so we only handle disconnect / error / receive here.
    tokio::pin!(close_rx);

    let keepalive_dur = keepalive_interval.unwrap_or(Duration::MAX);
    let has_keepalive = keepalive_interval.is_some();
    let mut idle_deadline = TokioInstant::now() + keepalive_dur;

    loop {
        let idle_sleep = tokio::time::sleep_until(idle_deadline);
        tokio::pin!(idle_sleep);

        let frame = tokio::select! {
            biased;

            // Highest priority: graceful shutdown requested by close() / Drop.
            _ = &mut close_rx => {
                send_unsubscribe_and_close(&mut ws_stream, &subscription_id).await;
                event_handlers.emit_disconnect(
                    DisconnectReason::with_code("Subscription closed by client".to_string(), 1000),
                );
                return;
            }

            // Second priority: keepalive idle timer.
            _ = &mut idle_sleep, if has_keepalive => {
                if let Err(e) = ws_stream.send(Message::Ping(Bytes::new())).await {
                    let _ = event_tx
                        .send(Err(KalamLinkError::WebSocketError(format!(
                            "Failed to send keepalive ping: {}", e
                        ))))
                        .await;
                    event_handlers.emit_disconnect(
                        DisconnectReason::new(format!("Keepalive ping failed: {}", e)),
                    );
                    return;
                }
                event_handlers.emit_send("[ping]");
                idle_deadline = TokioInstant::now() + keepalive_dur;
                continue;
            }

            // Normal path: read the next WebSocket frame.
            msg = ws_stream.next() => {
                idle_deadline = TokioInstant::now() + keepalive_dur;
                msg
            }
        };

        match frame {
            Some(Ok(Message::Text(text))) => {
                if text.len() > MAX_WS_TEXT_MESSAGE_BYTES {
                    let _ = event_tx
                        .send(Err(KalamLinkError::WebSocketError(format!(
                            "Text message too large ({} bytes > {} bytes)",
                            text.len(),
                            MAX_WS_TEXT_MESSAGE_BYTES
                        ))))
                        .await;
                    return;
                }
                event_handlers.emit_receive(&text);
                if process_and_forward(&text, &mut ws_stream, &event_tx, &subscription_id)
                    .await
                {
                    return;
                }
            },
            Some(Ok(Message::Binary(data))) => match decode_ws_payload(&data) {
                Ok(text) => {
                    event_handlers.emit_receive(&text);
                    if process_and_forward(
                        &text,
                        &mut ws_stream,
                        &event_tx,
                        &subscription_id,
                    )
                    .await
                    {
                        return;
                    }
                },
                Err(e) => {
                    event_handlers.emit_error(ConnectionError::new(e.to_string(), false));
                    if event_tx.send(Err(e)).await.is_err() {
                        return;
                    }
                },
            },
            Some(Ok(Message::Close(frame))) => {
                let reason = if let Some(f) = frame {
                    DisconnectReason::with_code(f.reason.to_string(), f.code.into())
                } else {
                    DisconnectReason::new("Server closed connection")
                };
                event_handlers.emit_disconnect(reason);
                return;
            },
            Some(Ok(Message::Ping(payload))) => {
                // tokio-tungstenite auto-responds, but be explicit for clarity.
                let _ = ws_stream.send(Message::Pong(payload)).await;
            },
            Some(Ok(Message::Pong(_))) | Some(Ok(Message::Frame(_))) => {},
            Some(Err(e)) => {
                let msg = e.to_string();
                event_handlers.emit_error(ConnectionError::new(&msg, false));
                event_handlers
                    .emit_disconnect(DisconnectReason::new(format!("WebSocket error: {}", msg)));
                let _ = event_tx
                    .send(Err(KalamLinkError::WebSocketError(msg)))
                    .await;
                return;
            },
            None => {
                event_handlers
                    .emit_disconnect(DisconnectReason::new("WebSocket stream ended"));
                return;
            },
        }
    }
}

pub struct SubscriptionManager {
    subscription_id: String,
    /// Receives parsed events from the background WS reader task.
    event_rx: mpsc::Receiver<Result<ChangeEvent>>,
    /// Signal the background task to initiate graceful shutdown.
    /// `None` after `close()` has been called (or consumed by `Drop`).
    close_tx: Option<oneshot::Sender<()>>,
    /// Handle to the background reader task — kept to prevent detached-task
    /// warnings and to allow future `.await` on shutdown.
    _reader_handle: JoinHandle<()>,
    /// Local event buffer for yielding batched events from a single WS message.
    event_queue: VecDeque<ChangeEvent>,
    /// Changes received while initial data is still loading.
    buffered_changes: Vec<ChangeEvent>,
    /// Whether initial data is still loading.
    is_loading: bool,
    timeouts: KalamLinkTimeouts,
    closed: bool,
}

impl SubscriptionManager {
    /// Create a new WebSocket subscription
    ///
    /// **Implements T079**: WebSocket connection establishment
    pub(crate) async fn new(
        base_url: &str,
        config: SubscriptionConfig,
        auth: &AuthProvider,
        timeouts: &KalamLinkTimeouts,
        connection_options: &ConnectionOptions,
        event_handlers: &EventHandlers,
    ) -> Result<Self> {
        let SubscriptionConfig {
            id,
            sql,
            options,
            ws_url,
        } = config;

        let request_url = resolve_ws_url(base_url, ws_url.as_deref(), connection_options.disable_compression)?;

        // Connect to WebSocket with connection timeout
        let mut request = request_url.into_client_request().map_err(|e| {
            KalamLinkError::WebSocketError(format!("Failed to build WebSocket request: {}", e))
        })?;

        apply_ws_auth_headers(&mut request, auth)?;

        // Apply connection timeout
        let connect_result = if !KalamLinkTimeouts::is_no_timeout(timeouts.connection_timeout) {
            tokio::time::timeout(
                timeouts.connection_timeout,
                connect_with_optional_local_bind(
                    request,
                    &connection_options.ws_local_bind_addresses,
                    &id,
                ),
            )
            .await
        } else {
            Ok(connect_with_optional_local_bind(
                request,
                &connection_options.ws_local_bind_addresses,
                &id,
            )
            .await)
        };

        let ws_stream = match connect_result {
            Ok(Ok((stream, _))) => stream,
            Ok(Err(WsError::Http(response))) => {
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
                    },
                };
                event_handlers.emit_error(ConnectionError::new(&message, false));
                return Err(KalamLinkError::WebSocketError(message));
            },
            Ok(Err(e)) => {
                let msg = format!("Connection failed: {}", e);
                event_handlers.emit_error(ConnectionError::new(&msg, true));
                return Err(KalamLinkError::WebSocketError(msg));
            },
            Err(_) => {
                let msg = format!("Connection timeout ({:?})", timeouts.connection_timeout);
                event_handlers.emit_error(ConnectionError::new(&msg, true));
                return Err(KalamLinkError::TimeoutError(msg));
            },
        };

        let mut ws_stream = ws_stream;

        // Send authentication message and wait for AuthSuccess
        // (WebSocket protocol requires explicit Authenticate message even with HTTP headers)
        send_auth_and_wait(&mut ws_stream, auth, timeouts.auth_timeout).await?;

        // Emit on_connect event — WebSocket is established and authenticated
        event_handlers.emit_connect();

        // Use the provided subscription ID (now required)
        let subscription_id = id;
        let keepalive_interval = if timeouts.keepalive_interval.is_zero() {
            None
        } else {
            Some(jitter_keepalive_interval(timeouts.keepalive_interval, &subscription_id))
        };

        // Send subscription request
        send_subscription_request(&mut ws_stream, &subscription_id, &sql, options).await?;

        // Spawn background reader task — all WS I/O happens there from now on.
        let (event_tx, event_rx) = mpsc::channel(DEFAULT_EVENT_CHANNEL_CAPACITY);
        let (close_tx, close_rx) = oneshot::channel();
        let reader_handle = tokio::spawn(ws_reader_loop(
            ws_stream,
            event_tx,
            close_rx,
            subscription_id.clone(),
            keepalive_interval,
            event_handlers.clone(),
        ));

        Ok(Self {
            subscription_id,
            event_rx,
            close_tx: Some(close_tx),
            _reader_handle: reader_handle,
            event_queue: VecDeque::new(),
            buffered_changes: Vec::new(),
            is_loading: true,
            timeouts: timeouts.clone(),
            closed: false,
        })
    }

    fn flush_buffered_changes(&mut self) {
        for change in self.buffered_changes.drain(..) {
            self.event_queue.push_back(change);
        }
    }

    /// Buffer incoming events: hold live changes while initial data is loading,
    /// then flush them in order once the snapshot is complete.
    ///
    /// NOTE: `NextBatch` requests are handled by the background reader task,
    /// not here.  This method only handles local event ordering.
    fn apply_buffering(&mut self, event: ChangeEvent) {
        match event {
            ChangeEvent::Ack {
                ref batch_control, ..
            }
            | ChangeEvent::InitialDataBatch {
                ref batch_control, ..
            } => {
                self.is_loading = batch_control.status != BatchStatus::Ready;
                self.event_queue.push_back(event);
                if !self.is_loading {
                    self.flush_buffered_changes();
                }
            },
            ChangeEvent::Insert { .. }
            | ChangeEvent::Update { .. }
            | ChangeEvent::Delete { .. } => {
                if self.is_loading {
                    self.buffered_changes.push(event);
                } else {
                    self.event_queue.push_back(event);
                }
            },
            _ => {
                self.event_queue.push_back(event);
            },
        }
    }

    /// Receive the next change event from the subscription.
    ///
    /// Events are delivered by a background reader task through a bounded
    /// channel.  The consumer applies initial-data buffering so live changes
    /// are held until the snapshot is complete.
    ///
    /// Returns `None` when the connection is closed.
    pub async fn next(&mut self) -> Option<Result<ChangeEvent>> {
        loop {
            // 1. Drain local event queue first
            if let Some(event) = self.event_queue.pop_front() {
                return Some(Ok(event));
            }

            // 2. If already closed, signal end-of-stream
            if self.closed {
                return None;
            }

            // 3. Read next parsed event from the background reader task
            match self.event_rx.recv().await {
                Some(Ok(event)) => {
                    self.apply_buffering(event);
                    // Loop back to drain event_queue
                },
                Some(Err(e)) => return Some(Err(e)),
                None => {
                    // Channel closed — reader task has exited
                    self.closed = true;
                    return None;
                },
            }
        }
    }

    /// Get the subscription ID assigned by the server
    pub fn subscription_id(&self) -> &str {
        &self.subscription_id
    }

    /// Get the configured timeouts
    pub fn timeouts(&self) -> &KalamLinkTimeouts {
        &self.timeouts
    }

    /// Close the subscription gracefully.
    ///
    /// Signals the background reader task to send an Unsubscribe message and
    /// close the WebSocket connection.  Safe to call multiple times —
    /// subsequent calls are no-ops.
    pub async fn close(&mut self) -> Result<()> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;

        // Signal the background reader task to initiate graceful shutdown.
        if let Some(tx) = self.close_tx.take() {
            let _ = tx.send(());
        }

        Ok(())
    }

    /// Returns `true` if `close()` has been called or `Drop` has run.
    pub fn is_closed(&self) -> bool {
        self.closed
    }
}

impl Drop for SubscriptionManager {
    fn drop(&mut self) {
        // Send the close signal so the background reader task can attempt a
        // graceful Unsubscribe + Close.  If close() was already called,
        // `close_tx` is `None` and this is a no-op.
        //
        // Even without the signal, the reader task will eventually notice that
        // the `event_rx` (Receiver) side was dropped and shut itself down.
        if let Some(tx) = self.close_tx.take() {
            let _ = tx.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── helpers ───────────────────────────────────────────────────────────────

    /// Create a minimal `SubscriptionManager` with no real WebSocket for
    /// testing state-flag logic without a network connection.
    ///
    /// A dummy reader task and channel pair are created so the struct is valid;
    /// the sender is immediately dropped so `event_rx.recv()` returns `None`.
    async fn make_test_sub() -> SubscriptionManager {
        let (_tx, rx) = mpsc::channel(1);
        let (close_tx, close_rx) = oneshot::channel();
        let handle = tokio::spawn(async move {
            // Wait for close signal or sender drop, then exit.
            let _ = close_rx.await;
        });
        SubscriptionManager {
            subscription_id: "unit-test-id".to_string(),
            event_rx: rx,
            close_tx: Some(close_tx),
            _reader_handle: handle,
            event_queue: VecDeque::new(),
            buffered_changes: Vec::new(),
            is_loading: false,
            timeouts: KalamLinkTimeouts::default(),
            closed: false,
        }
    }

    // ── url resolution tests ───────────────────────────────────────────────

    #[test]
    fn test_ws_url_conversion() {
        assert_eq!(
            resolve_ws_url("http://localhost:3000", None, false).unwrap(),
            "ws://localhost:3000/v1/ws"
        );
        assert_eq!(
            resolve_ws_url("https://api.example.com", None, false).unwrap(),
            "wss://api.example.com/v1/ws"
        );
        assert_eq!(
            resolve_ws_url("http://localhost:3000", Some("ws://override/ws"), false).unwrap(),
            "ws://override/ws"
        );
    }

    #[test]
    fn test_ws_url_trailing_slash_stripped() {
        assert_eq!(
            resolve_ws_url("http://localhost:3000/", None, false).unwrap(),
            "ws://localhost:3000/v1/ws"
        );
    }

    #[test]
    fn test_ws_url_rejects_query_and_fragment() {
        assert!(resolve_ws_url(
            "http://localhost:3000",
            Some("wss://api.example.com/v1/ws?token=secret"),
            false
        )
        .is_err());
        assert!(
            resolve_ws_url("http://localhost:3000", Some("wss://api.example.com/v1/ws#frag"), false)
                .is_err()
        );
    }

    #[test]
    fn test_ws_url_rejects_userinfo() {
        assert!(resolve_ws_url(
            "http://localhost:3000",
            Some("wss://user:pass@api.example.com/v1/ws"),
            false
        )
        .is_err());
    }

    #[test]
    fn test_ws_url_rejects_https_downgrade() {
        assert!(
            resolve_ws_url("https://api.example.com", Some("ws://api.example.com/v1/ws"), false).is_err()
        );
    }

    #[test]
    fn test_ws_url_rejects_unsupported_scheme() {
        assert!(
            resolve_ws_url("http://localhost:3000", Some("ftp://api.example.com/v1/ws"), false).is_err()
        );
    }

    #[test]
    fn test_keepalive_jitter_is_deterministic() {
        let base = Duration::from_secs(20);
        let a = jitter_keepalive_interval(base, "sub-a");
        let b = jitter_keepalive_interval(base, "sub-a");
        assert_eq!(a, b, "jitter must be stable for the same subscription");
    }

    #[test]
    fn test_keepalive_jitter_stays_within_bounds() {
        let base = Duration::from_secs(20);
        let jittered = jitter_keepalive_interval(base, "sub-b");
        let min = Duration::from_secs(16); // -20%
        let max = Duration::from_secs(24); // +20%
        assert!(
            jittered >= min && jittered <= max,
            "jittered interval {:?} must be within [{:?}, {:?}]",
            jittered,
            min,
            max
        );
    }

    // ── state-flag unit tests (no network) ────────────────────────────────

    #[tokio::test]
    async fn test_is_not_closed_initially() {
        let sub = make_test_sub().await;
        assert!(!sub.is_closed(), "subscription should start as open");
    }

    #[tokio::test]
    async fn test_close_marks_subscription_as_closed() {
        let mut sub = make_test_sub().await;
        assert!(!sub.is_closed());
        sub.close().await.expect("close should succeed on a stream-less sub");
        assert!(sub.is_closed(), "subscription should be closed after close()");
    }

    #[tokio::test]
    async fn test_close_is_idempotent() {
        let mut sub = make_test_sub().await;
        sub.close().await.expect("first close should succeed");
        sub.close().await.expect("second close should also succeed (no-op)");
        assert!(sub.is_closed());
    }

    #[tokio::test]
    async fn test_next_returns_none_when_stream_is_none() {
        let mut sub = make_test_sub().await;
        // Channel sender is dropped, so next() must return None quickly
        let result = tokio::time::timeout(std::time::Duration::from_millis(100), sub.next())
            .await
            .expect("next() should complete quickly when stream is None");
        assert!(result.is_none(), "next() should return None when stream is None");
    }

    #[tokio::test]
    async fn test_next_returns_none_after_close() {
        let mut sub = make_test_sub().await;
        sub.close().await.unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_millis(100), sub.next())
            .await
            .expect("next() should complete quickly after close");
        assert!(result.is_none());
    }

    /// Verify Drop does not panic even outside a tokio runtime.
    ///
    /// We construct the `SubscriptionManager` inside a temporary runtime,
    /// then drop it after the runtime has shut down.
    #[test]
    fn test_drop_without_runtime_does_not_panic() {
        let sub = {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async { make_test_sub().await })
        };
        // Runtime is gone; Drop only sends on a oneshot channel — no panic.
        drop(sub);
    }

    /// Verify Drop inside a tokio runtime sends the close signal without
    /// panicking.
    #[tokio::test]
    async fn test_drop_inside_runtime_does_not_panic() {
        let sub = make_test_sub().await;
        drop(sub);
        // Yield to let the reader task process the close signal.
        tokio::task::yield_now().await;
    }
}
