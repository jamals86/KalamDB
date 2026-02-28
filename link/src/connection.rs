//! Shared WebSocket connection manager for real-time subscriptions.
//!
//! Provides a single WebSocket connection multiplexed across multiple
//! subscriptions.  Handles:
//!
//! - Single connection for all subscriptions (no per-subscription connections)
//! - Message routing to the correct subscription by `subscription_id`
//! - Automatic reconnection with exponential backoff
//! - Re-subscription of all active queries after reconnect (resuming from last `seq_id`)
//! - Connection lifecycle events (`on_connect`, `on_disconnect`, `on_error`)
//! - Keepalive pings

use crate::{
    auth::{AuthProvider, ResolvedAuth},
    error::{KalamLinkError, Result},
    event_handlers::{ConnectionError, DisconnectReason, EventHandlers},
    models::{
        ChangeEvent, ClientMessage, ConnectionOptions,
        SubscriptionInfo, SubscriptionOptions, SubscriptionRequest,
    },
    seq_id::SeqId,
    subscription::{
        apply_ws_auth_headers, connect_with_optional_local_bind, decode_ws_payload,
        jitter_keepalive_interval, parse_message, resolve_ws_url, send_auth_and_wait,
        send_next_batch_request, WebSocketStream,
    },
    timeouts::KalamLinkTimeouts,
};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use std::sync::RwLock;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::Instant as TokioInstant;
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, protocol::Message};

/// Default capacity for subscription event channels.
const DEFAULT_EVENT_CHANNEL_CAPACITY: usize = 8192;

/// Maximum text message size (64 MiB).
const MAX_WS_TEXT_MESSAGE_BYTES: usize = 64 << 20;

/// Maximum sleep duration that won't overflow `Instant + Duration`.
/// ~100 years is far enough into the future to be effectively "never".
const FAR_FUTURE: Duration = Duration::from_secs(100 * 365 * 24 * 3600);

/// Current time in millis since Unix epoch.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Build a `Vec<SubscriptionInfo>` snapshot from the internal subs map.
fn snapshot_subscriptions(subs: &HashMap<String, SubEntry>) -> Vec<SubscriptionInfo> {
    subs.iter()
        .map(|(id, entry)| SubscriptionInfo {
            id: id.clone(),
            query: entry.sql.clone(),
            last_seq_id: entry.last_seq_id,
            last_event_time_ms: entry.last_event_time_ms,
            created_at_ms: entry.created_at_ms,
            closed: false,
        })
        .collect()
}

// ── Commands ────────────────────────────────────────────────────────────────

/// Commands sent from the public API to the background connection task.
enum ConnCmd {
    /// Register a new subscription over the shared connection.
    Subscribe {
        id: String,
        sql: String,
        options: SubscriptionOptions,
        event_tx: mpsc::Sender<Result<ChangeEvent>>,
        result_tx: oneshot::Sender<Result<u64>>,
    },
    /// Remove a subscription.
    ///
    /// When `generation` is `Some`, the entry is only removed if its
    /// generation matches.  This prevents a stale close from a
    /// superseded `SubscriptionManager` from killing a re-subscribed
    /// entry that reused the same subscription ID.
    Unsubscribe {
        id: String,
        generation: Option<u64>,
    },
    /// List all active subscriptions.
    ListSubscriptions {
        result_tx: oneshot::Sender<Vec<SubscriptionInfo>>,
    },
    /// Gracefully shut down the connection.
    Shutdown,
}

// ── Per-subscription state ──────────────────────────────────────────────────

/// Internal state for each active subscription within the shared connection.
struct SubEntry {
    sql: String,
    options: SubscriptionOptions,
    event_tx: mpsc::Sender<Result<ChangeEvent>>,
    last_seq_id: Option<SeqId>,
    /// Monotonic generation counter assigned when this entry was created.
    /// Used to ignore stale `Unsubscribe` commands from superseded
    /// `SubscriptionManager` instances that held the same subscription ID.
    generation: u64,
    /// Millis since Unix epoch when this subscription was created.
    created_at_ms: u64,
    /// Millis since Unix epoch of the last routed event.
    last_event_time_ms: Option<u64>,
}

// ── SharedConnection (public handle) ────────────────────────────────────────

/// A single shared WebSocket connection that multiplexes subscriptions.
///
/// Created via [`SharedConnection::connect`].  Subscribe/unsubscribe calls
/// send commands to a background task that owns the WebSocket stream.
pub(crate) struct SharedConnection {
    /// Channel to the background connection task.
    cmd_tx: mpsc::Sender<ConnCmd>,
    /// Sender that [`SubscriptionManager`] instances use to unsubscribe
    /// from within `close()` / `Drop` (fire-and-forget, no async needed).
    /// The tuple is `(subscription_id, generation)` so the connection task
    /// can ignore stale unsubscribes from superseded managers.
    unsub_tx: mpsc::Sender<(String, u64)>,
    /// Whether the WebSocket is currently open and authenticated.
    connected: Arc<AtomicBool>,
    /// Reconnection attempt counter (resets on success).
    reconnect_attempts: Arc<AtomicU32>,
    /// Background task handle.
    _task: JoinHandle<()>,
    /// Bridge task: reads from `unsub_rx` and forwards as `ConnCmd::Unsubscribe`.
    _unsub_bridge: JoinHandle<()>,
}

impl SharedConnection {
    /// Establish a shared WebSocket connection.
    ///
    /// Spawns a background task that:
    /// 1. Connects and authenticates to the WebSocket endpoint
    /// 2. Reads messages and routes them to subscription channels
    /// 3. Processes subscribe/unsubscribe commands
    /// 4. Handles auto-reconnection with exponential backoff
    ///
    /// This method waits for the initial WebSocket handshake and
    /// authentication to complete before returning. If the initial
    /// connection fails, the error is propagated to the caller.
    pub async fn connect(
        base_url: String,
        resolved_auth: Arc<RwLock<ResolvedAuth>>,
        timeouts: KalamLinkTimeouts,
        connection_options: ConnectionOptions,
        event_handlers: EventHandlers,
    ) -> Result<Self> {
        let (cmd_tx, cmd_rx) = mpsc::channel::<ConnCmd>(256);
        let connected = Arc::new(AtomicBool::new(false));
        let reconnect_attempts = Arc::new(AtomicU32::new(0));

        let connected_clone = connected.clone();
        let reconnect_clone = reconnect_attempts.clone();

        // The background task signals this once the initial connection
        // attempt has completed (Ok) or failed (Err).
        let (ready_tx, ready_rx) = oneshot::channel::<Result<()>>();

        let task = tokio::spawn(async move {
            connection_task(
                cmd_rx,
                base_url,
                resolved_auth,
                timeouts,
                connection_options,
                event_handlers,
                connected_clone,
                reconnect_clone,
                Some(ready_tx),
            )
            .await;
        });

        // Wait for the initial connection attempt to finish.
        match ready_rx.await {
            Ok(Ok(())) => {}, // connected successfully
            Ok(Err(e)) => {
                // Initial connection failed — propagate the error.
                // The background task is still running and will attempt
                // auto-reconnection, so we keep it alive.
                log::warn!("Initial shared connection failed: {}", e);
            },
            Err(_) => {
                // The task dropped ready_tx without sending — should not
                // happen, but treat as "initial connection failed".
                log::warn!("Connection task exited before signalling readiness");
            },
        }

        // Create a bridge task that forwards subscription-ID-based unsubscribe
        // requests (used by SubscriptionManager close/Drop) into ConnCmd
        // commands for the connection task.  The channel carries
        // `(subscription_id, generation)` so the connection task can
        // distinguish stale unsubscribes from superseded managers.
        let (unsub_tx, mut unsub_rx) = mpsc::channel::<(String, u64)>(256);
        let cmd_tx_bridge = cmd_tx.clone();
        let unsub_bridge = tokio::spawn(async move {
            while let Some((id, generation)) = unsub_rx.recv().await {
                let _ = cmd_tx_bridge
                    .send(ConnCmd::Unsubscribe { id, generation: Some(generation) })
                    .await;
            }
        });

        Ok(Self {
            cmd_tx,
            unsub_tx,
            connected,
            reconnect_attempts,
            _task: task,
            _unsub_bridge: unsub_bridge,
        })
    }

    /// Add a new subscription to the shared connection.
    ///
    /// Returns the event receiver.  The background task sends subscribe
    /// messages over the WebSocket and routes incoming events.
    /// Add a new subscription to the shared connection.
    ///
    /// Returns `(event_receiver, generation)`.  The generation counter
    /// must be stored in the [`SubscriptionManager`] so that its
    /// `close()` / `Drop` can tag unsubscribe requests — preventing a
    /// stale manager from accidentally removing a re-subscribed entry
    /// that reused the same subscription ID.
    pub async fn subscribe(
        &self,
        id: String,
        sql: String,
        options: SubscriptionOptions,
    ) -> Result<(mpsc::Receiver<Result<ChangeEvent>>, u64)> {
        let (event_tx, event_rx) = mpsc::channel(DEFAULT_EVENT_CHANNEL_CAPACITY);
        let (result_tx, result_rx) = oneshot::channel();

        self.cmd_tx
            .send(ConnCmd::Subscribe {
                id: id.clone(),
                sql,
                options,
                event_tx,
                result_tx,
            })
            .await
            .map_err(|_| {
                KalamLinkError::WebSocketError("Connection task is not running".to_string())
            })?;

        let generation = result_rx.await.map_err(|_| {
            KalamLinkError::WebSocketError("Connection task died before confirming subscribe".to_string())
        })??;

        Ok((event_rx, generation))
    }

    /// Remove a subscription from the shared connection.
    pub async fn unsubscribe(&self, id: &str) -> Result<()> {
        self.cmd_tx
            .send(ConnCmd::Unsubscribe {
                id: id.to_string(),
                generation: None,
            })
            .await
            .map_err(|_| {
                KalamLinkError::WebSocketError("Connection task is not running".to_string())
            })?;
        Ok(())
    }

    /// Gracefully disconnect and shut down the background task.
    pub async fn disconnect(&self) {
        let _ = self.cmd_tx.send(ConnCmd::Shutdown).await;
    }

    /// List all active subscriptions with their current metadata.
    pub async fn list_subscriptions(&self) -> Vec<SubscriptionInfo> {
        let (result_tx, result_rx) = oneshot::channel();
        if self.cmd_tx
            .send(ConnCmd::ListSubscriptions { result_tx })
            .await
            .is_err()
        {
            return Vec::new();
        }
        result_rx.await.unwrap_or_default()
    }

    /// Whether the WebSocket is currently open and authenticated.
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    /// Clone the unsubscribe sender for use by [`SubscriptionManager::from_shared`].
    ///
    /// SubscriptionManagers use this channel in `close()` / `Drop` to send
    /// their `(subscription_id, generation)` pair; the bridge task forwards
    /// it as a `ConnCmd::Unsubscribe` with the generation tag.
    pub(crate) fn unsubscribe_tx(&self) -> mpsc::Sender<(String, u64)> {
        self.unsub_tx.clone()
    }
}

impl Drop for SharedConnection {
    fn drop(&mut self) {
        // Best-effort shutdown signal.
        let _ = self.cmd_tx.try_send(ConnCmd::Shutdown);
    }
}

// ── Background connection task ──────────────────────────────────────────────

/// Establish a WebSocket connection and authenticate.
async fn establish_ws(
    base_url: &str,
    resolved_auth: &RwLock<ResolvedAuth>,
    timeouts: &KalamLinkTimeouts,
    connection_options: &ConnectionOptions,
    event_handlers: &EventHandlers,
) -> Result<(WebSocketStream, AuthProvider)> {
    log::debug!("[kalam-link] Establishing WebSocket connection to {}", base_url);
    let resolved = resolved_auth.read().unwrap().clone();
    log::debug!("[kalam-link] Resolving auth credentials...");
    let auth = resolved.resolve().await?;
    log::debug!("[kalam-link] Auth resolved, building WebSocket request");

    let request_url = resolve_ws_url(base_url, None, connection_options.disable_compression)?;

    let mut request = request_url
        .into_client_request()
        .map_err(|e| KalamLinkError::WebSocketError(format!("Failed to build WebSocket request: {}", e)))?;

    apply_ws_auth_headers(&mut request, &auth)?;

    let connect_result =
        if !KalamLinkTimeouts::is_no_timeout(timeouts.connection_timeout) {
            tokio::time::timeout(
                timeouts.connection_timeout,
                connect_with_optional_local_bind(
                    request,
                    &connection_options.ws_local_bind_addresses,
                    "shared-conn",
                ),
            )
            .await
        } else {
            Ok(connect_with_optional_local_bind(
                request,
                &connection_options.ws_local_bind_addresses,
                "shared-conn",
            )
            .await)
        };

    let mut ws_stream = match connect_result {
        Ok(Ok((stream, _))) => stream,
        Ok(Err(tokio_tungstenite::tungstenite::error::Error::Http(response))) => {
            let status = response.status();
            let body_text = response
                .into_body()
                .as_ref()
                .and_then(|b| {
                    if b.is_empty() { None } else { Some(String::from_utf8_lossy(b).into_owned()) }
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

    // Authenticate
    log::debug!("[kalam-link] Sending auth handshake (timeout={:?})", timeouts.auth_timeout);
    send_auth_and_wait(&mut ws_stream, &auth, timeouts.auth_timeout).await?;
    log::info!("[kalam-link] WebSocket authenticated successfully");

    Ok((ws_stream, auth))
}

/// Send a Subscribe message over an existing WebSocket.
async fn send_subscribe(
    ws: &mut WebSocketStream,
    id: &str,
    sql: &str,
    options: SubscriptionOptions,
) -> Result<()> {
    let msg = ClientMessage::Subscribe {
        subscription: SubscriptionRequest {
            id: id.to_string(),
            sql: sql.to_string(),
            options,
        },
    };
    let payload = serde_json::to_string(&msg).map_err(|e| {
        KalamLinkError::WebSocketError(format!("Failed to serialize subscribe: {}", e))
    })?;
    ws.send(Message::Text(payload.into()))
        .await
        .map_err(|e| KalamLinkError::WebSocketError(format!("Failed to send subscribe: {}", e)))
}

/// Send an Unsubscribe message over the WebSocket.
async fn send_unsubscribe(ws: &mut WebSocketStream, id: &str) -> Result<()> {
    let msg = ClientMessage::Unsubscribe {
        subscription_id: id.to_string(),
    };
    let payload = serde_json::to_string(&msg).map_err(|e| {
        KalamLinkError::WebSocketError(format!("Failed to serialize unsubscribe: {}", e))
    })?;
    ws.send(Message::Text(payload.into()))
        .await
        .map_err(|e| KalamLinkError::WebSocketError(format!("Failed to send unsubscribe: {}", e)))
}

/// Route a parsed `ChangeEvent` to the correct subscription channel.
///
/// Also handles:
/// - Updating `last_seq_id` from batch control
/// - Auto-requesting next batch for paginated initial data
async fn route_event(
    event: ChangeEvent,
    ws: &mut WebSocketStream,
    subs: &mut HashMap<String, SubEntry>,
    _event_handlers: &EventHandlers,
) {
    let sub_id = match event.subscription_id() {
        Some(id) => id.to_string(),
        None => return, // Unknown/auth events — skip
    };

    // Update last_seq_id from batch control
    if let ChangeEvent::InitialDataBatch {
        ref batch_control, ..
    }
    | ChangeEvent::Ack {
        ref batch_control, ..
    } = event
    {
        if let Some(seq_id) = batch_control.last_seq_id {
            if let Some(entry) = subs.get_mut(&sub_id) {
                entry.last_seq_id = Some(seq_id);
                entry.last_event_time_ms = Some(now_ms());
            }
        }
        // Auto-request next batch
        if batch_control.has_more {
            let last_seq = subs.get(&sub_id).and_then(|e| e.last_seq_id);
            if let Err(e) = send_next_batch_request(ws, &sub_id, last_seq).await {
                log::warn!("Failed to send NextBatch for {}: {}", sub_id, e);
            }
        }
    }

    // Find subscription — try exact match, then suffix match (server may prefix IDs)
    let matched_key = if subs.contains_key(&sub_id) {
        Some(sub_id.clone())
    } else {
        subs.keys()
            .find(|cid| sub_id.ends_with(cid.as_str()))
            .cloned()
    };

    if let Some(key) = matched_key {
        if let Some(entry) = subs.get_mut(&key) {
            entry.last_event_time_ms = Some(now_ms());
            if entry.event_tx.send(Ok(event)).await.is_err() {
                // Receiver dropped — the subscription handle was closed
                log::debug!("Subscription {} receiver dropped", sub_id);
            }
        }
    } else {
        log::debug!("No subscription found for id: {}", sub_id);
    }
}

/// Re-subscribe all active subscriptions after a successful reconnect.
async fn resubscribe_all(
    ws: &mut WebSocketStream,
    subs: &HashMap<String, SubEntry>,
    event_handlers: &EventHandlers,
) {
    log::info!(
        "[kalam-link] Re-subscribing {} active subscription(s) after reconnect",
        subs.len()
    );
    for (id, entry) in subs.iter() {
        let mut options = entry.options.clone();
        if let Some(seq_id) = entry.last_seq_id {
            options.from_seq_id = Some(seq_id);
        }

        log::info!(
            "[kalam-link] Re-subscribing '{}' with from_seq_id={:?}",
            id,
            entry.last_seq_id.map(|s| s.to_string())
        );

        if let Err(e) = send_subscribe(ws, id, &entry.sql, options).await {
            log::warn!("Failed to re-subscribe {}: {}", id, e);
            event_handlers.emit_error(ConnectionError::new(
                format!("Failed to re-subscribe {}: {}", id, e),
                true,
            ));
        }
    }
}

/// The main background task managing the shared WebSocket connection.
///
/// Lifecycle:
/// 1. Establish WebSocket connection and authenticate
/// 2. Enter event loop: read WS messages + process commands + keepalive pings
/// 3. On disconnect: attempt auto-reconnection with exponential backoff
/// 4. On reconnect: re-subscribe all active subscriptions with last seq_id
async fn connection_task(
    mut cmd_rx: mpsc::Receiver<ConnCmd>,
    base_url: String,
    resolved_auth: Arc<RwLock<ResolvedAuth>>,
    timeouts: KalamLinkTimeouts,
    connection_options: ConnectionOptions,
    event_handlers: EventHandlers,
    connected: Arc<AtomicBool>,
    reconnect_attempts: Arc<AtomicU32>,
    ready_tx: Option<oneshot::Sender<Result<()>>>,
) {
    let mut subs: HashMap<String, SubEntry> = HashMap::new();
    let mut ws_stream: Option<WebSocketStream> = None;
    let mut shutdown_requested = false;
    // Monotonically increasing counter used to tag each `SubEntry`.
    // When a subscription is re-registered with the same ID (e.g. after
    // a force-reconnect), the old `SubscriptionManager` may still fire
    // an unsubscribe on close/Drop.  By comparing generations we ignore
    // stale unsubscribes that target an older incarnation of the entry.
    let mut next_generation: u64 = 1;

    // Keepalive configuration
    let keepalive_dur = if timeouts.keepalive_interval.is_zero() {
        FAR_FUTURE
    } else {
        jitter_keepalive_interval(timeouts.keepalive_interval, "shared-conn")
    };
    let has_keepalive = !timeouts.keepalive_interval.is_zero();
    let mut idle_deadline = TokioInstant::now() + keepalive_dur;

    // Pong timeout: after sending a Ping, we must receive *some* frame
    // (typically a Pong) within this window or we consider the connection dead.
    let pong_timeout_dur = timeouts.pong_timeout;
    let has_pong_timeout = has_keepalive && !pong_timeout_dur.is_zero();
    let mut awaiting_pong = false;
    let mut pong_deadline = TokioInstant::now() + FAR_FUTURE; // inactive until first Ping

    // Try initial connection (non-fatal if it fails — we'll connect on first subscribe)
    match establish_ws(
        &base_url,
        &resolved_auth,
        &timeouts,
        &connection_options,
        &event_handlers,
    )
    .await
    {
        Ok((stream, _auth)) => {
            ws_stream = Some(stream);
            connected.store(true, Ordering::SeqCst);
            event_handlers.emit_connect();
            idle_deadline = TokioInstant::now() + keepalive_dur;
            if let Some(tx) = ready_tx {
                let _ = tx.send(Ok(()));
            }
        },
        Err(e) => {
            log::warn!("Initial connection failed (will connect on first subscribe): {}", e);
            if let Some(tx) = ready_tx {
                let _ = tx.send(Err(e));
            }
        },
    }

    loop {
        if shutdown_requested {
            // Send unsubscribe for all subs and close
            if let Some(ref mut ws) = ws_stream {
                for id in subs.keys() {
                    let _ = send_unsubscribe(ws, id).await;
                }
                let _ = ws.close(None).await;
            }
            let was_connected = connected.swap(false, Ordering::SeqCst);
            if was_connected {
                event_handlers.emit_disconnect(
                    DisconnectReason::new("Client disconnected"),
                );
            }
            return;
        }

        if let Some(ref mut ws) = ws_stream {
            // We have an active connection — multiplex between WS reads, commands, keepalive, and pong timeout
            let idle_sleep = tokio::time::sleep_until(idle_deadline);
            tokio::pin!(idle_sleep);

            let pong_sleep = tokio::time::sleep_until(pong_deadline);
            tokio::pin!(pong_sleep);

            tokio::select! {
                biased;

                // Pong timeout: no frame arrived since we sent our Ping.
                _ = &mut pong_sleep, if has_pong_timeout && awaiting_pong => {
                    log::warn!(
                        "[kalam-link] Pong timeout ({:?}) — no response to keepalive ping, treating connection as dead",
                        pong_timeout_dur,
                    );
                    event_handlers.emit_disconnect(
                        DisconnectReason::new(format!(
                            "Pong timeout ({:?}) — server unresponsive",
                            pong_timeout_dur,
                        )),
                    );
                    connected.store(false, Ordering::SeqCst);
                    awaiting_pong = false;
                    ws_stream = None;
                    // Fall through to reconnection
                    continue;
                }

                // Commands from the public API
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(ConnCmd::Subscribe { id, sql, options, event_tx, result_tx }) => {
                            // If replacing an existing subscription with the same ID,
                            // unsubscribe the old one on the server first to prevent
                            // zombie subscriptions from accumulating.
                            if subs.contains_key(&id) {
                                log::debug!(
                                    "[kalam-link] Replacing existing subscription '{}' — unsubscribing old one first",
                                    id,
                                );
                                let _ = send_unsubscribe(ws, &id).await;
                                subs.remove(&id);
                            }
                            // Send subscribe message over the shared WebSocket
                            let result = send_subscribe(ws, &id, &sql, options.clone()).await;
                            let gen = next_generation;
                            if result.is_ok() {
                                next_generation += 1;
                                subs.insert(id.clone(), SubEntry {
                                    sql,
                                    options,
                                    event_tx,
                                    last_seq_id: None,
                                    generation: gen,
                                    created_at_ms: now_ms(),
                                    last_event_time_ms: None,
                                });
                            }
                            let _ = result_tx.send(result.map(|()| gen));
                        },
                        Some(ConnCmd::Unsubscribe { id, generation }) => {
                            // When a generation tag is present, only remove
                            // the entry if its generation matches.  This
                            // prevents a stale SubscriptionManager (from
                            // before a re-subscribe with the same ID) from
                            // accidentally unsubscribing the new one.
                            let should_remove = match generation {
                                Some(gen) => subs.get(&id).map_or(false, |e| e.generation == gen),
                                None => true, // Explicit unsubscribe (no generation) always removes
                            };
                            if should_remove {
                                subs.remove(&id);
                                let _ = send_unsubscribe(ws, &id).await;
                            } else {
                                log::debug!(
                                    "[kalam-link] Ignoring stale unsubscribe for '{}' (gen={:?}, current={:?})",
                                    id,
                                    generation,
                                    subs.get(&id).map(|e| e.generation),
                                );
                            }
                        },
                        Some(ConnCmd::ListSubscriptions { result_tx }) => {
                            let _ = result_tx.send(snapshot_subscriptions(&subs));
                        },
                        Some(ConnCmd::Shutdown) => {
                            shutdown_requested = true;
                            continue;
                        },
                        None => {
                            // All senders dropped
                            shutdown_requested = true;
                            continue;
                        },
                    }
                }

                // Keepalive ping
                _ = &mut idle_sleep, if has_keepalive && !awaiting_pong => {
                    log::debug!(
                        "[kalam-link] Keepalive: sending Ping (interval={:?})",
                        keepalive_dur,
                    );
                    if let Err(e) = ws.send(Message::Ping(Bytes::new())).await {
                        log::warn!("Failed to send keepalive ping: {}", e);
                        event_handlers.emit_disconnect(
                            DisconnectReason::new(format!("Keepalive ping failed: {}", e)),
                        );
                        connected.store(false, Ordering::SeqCst);
                        awaiting_pong = false;
                        ws_stream = None;
                        // Fall through to reconnection
                        continue;
                    }
                    event_handlers.emit_send("[ping]");
                    // Arm the pong timeout — any frame arriving resets it.
                    if has_pong_timeout {
                        awaiting_pong = true;
                        pong_deadline = TokioInstant::now() + pong_timeout_dur;
                        log::debug!(
                            "[kalam-link] Keepalive: awaiting Pong within {:?}",
                            pong_timeout_dur,
                        );
                    }
                    idle_deadline = TokioInstant::now() + keepalive_dur;
                }

                // WebSocket messages
                frame = ws.next() => {
                    // Any frame received proves the connection is alive.
                    idle_deadline = TokioInstant::now() + keepalive_dur;
                    if awaiting_pong {
                        log::debug!("[kalam-link] Keepalive: frame received, clearing pong timeout");
                        awaiting_pong = false;
                        pong_deadline = TokioInstant::now() + FAR_FUTURE;
                    }

                    match frame {
                        Some(Ok(Message::Text(text))) => {
                            if text.len() > MAX_WS_TEXT_MESSAGE_BYTES {
                                log::warn!("Text message too large ({} bytes)", text.len());
                                continue;
                            }
                            event_handlers.emit_receive(&text);
                            match parse_message(&text) {
                                Ok(Some(event)) => {
                                    route_event(event, ws, &mut subs, &event_handlers).await;
                                },
                                Ok(None) => {}, // Auth ack etc — skip
                                Err(e) => {
                                    log::warn!("Failed to parse WS message: {}", e);
                                },
                            }
                        },
                        Some(Ok(Message::Binary(data))) => {
                            match decode_ws_payload(&data) {
                                Ok(text) => {
                                    event_handlers.emit_receive(&text);
                                    match parse_message(&text) {
                                        Ok(Some(event)) => {
                                            route_event(event, ws, &mut subs, &event_handlers).await;
                                        },
                                        Ok(None) => {},
                                        Err(e) => {
                                            log::warn!("Failed to parse decompressed WS message: {}", e);
                                        },
                                    }
                                },
                                Err(e) => {
                                    event_handlers.emit_error(ConnectionError::new(e.to_string(), false));
                                },
                            }
                        },
                        Some(Ok(Message::Close(frame))) => {
                            let reason = if let Some(f) = frame {
                                DisconnectReason::with_code(f.reason.to_string(), f.code.into())
                            } else {
                                DisconnectReason::new("Server closed connection")
                            };
                            event_handlers.emit_disconnect(reason);
                            connected.store(false, Ordering::SeqCst);
                            ws_stream = None;
                            // Fall through to reconnection
                            continue;
                        },
                        Some(Ok(Message::Ping(payload))) => {
                            let _ = ws.send(Message::Pong(payload)).await;
                        },
                        Some(Ok(Message::Pong(_))) => {
                            log::debug!("[kalam-link] Keepalive: received Pong");
                        },
                        Some(Ok(Message::Frame(_))) => {},
                        Some(Err(e)) => {
                            let msg = e.to_string();
                            event_handlers.emit_error(ConnectionError::new(&msg, true));
                            event_handlers.emit_disconnect(
                                DisconnectReason::new(format!("WebSocket error: {}", msg)),
                            );
                            connected.store(false, Ordering::SeqCst);
                            ws_stream = None;
                            continue;
                        },
                        None => {
                            event_handlers.emit_disconnect(
                                DisconnectReason::new("WebSocket stream ended"),
                            );
                            connected.store(false, Ordering::SeqCst);
                            ws_stream = None;
                            continue;
                        },
                    }
                }
            }
        } else {
            // ── Not connected — attempt reconnection or wait for commands ──

            if !connection_options.auto_reconnect || shutdown_requested {
                // Just process commands without a connection
                match cmd_rx.recv().await {
                    Some(ConnCmd::Subscribe { result_tx, .. }) => {
                        let _ = result_tx.send(Err(KalamLinkError::WebSocketError(
                            "Not connected and auto-reconnect is disabled".to_string(),
                        )));
                    },
                    Some(ConnCmd::Unsubscribe { id, generation }) => {
                        let should_remove = match generation {
                            Some(gen) => subs.get(&id).map_or(false, |e| e.generation == gen),
                            None => true,
                        };
                        if should_remove {
                            subs.remove(&id);
                        }
                    },
                    Some(ConnCmd::ListSubscriptions { result_tx }) => {
                        let _ = result_tx.send(snapshot_subscriptions(&subs));
                    },
                    Some(ConnCmd::Shutdown) | None => {
                        return;
                    },
                }
                continue;
            }

            // Auto-reconnect with exponential backoff
            let attempt = reconnect_attempts.fetch_add(1, Ordering::SeqCst);
            if let Some(max) = connection_options.max_reconnect_attempts {
                if attempt >= max {
                    log::warn!("Max reconnection attempts ({}) reached", max);
                    event_handlers.emit_error(ConnectionError::new(
                        format!("Max reconnection attempts ({}) reached", max),
                        false,
                    ));
                    // Notify all existing subscriptions that we've given up.
                    let err_msg = "Max reconnection attempts reached".to_string();
                    for (_id, entry) in subs.drain() {
                        let _ = entry.event_tx.try_send(Err(
                            KalamLinkError::WebSocketError(err_msg.clone()),
                        ));
                    }
                    // Drain remaining commands
                    loop {
                        match cmd_rx.recv().await {
                            Some(ConnCmd::Subscribe { result_tx, .. }) => {
                                let _ = result_tx.send(Err(KalamLinkError::WebSocketError(
                                    "Max reconnection attempts reached".to_string(),
                                )));
                            },
                            Some(ConnCmd::Unsubscribe { id, .. }) => { subs.remove(&id); },
                            Some(ConnCmd::ListSubscriptions { result_tx }) => {
                                let _ = result_tx.send(snapshot_subscriptions(&subs));
                            },
                            Some(ConnCmd::Shutdown) | None => return,
                        }
                    }
                }
            }

            let delay = std::cmp::min(
                connection_options.reconnect_delay_ms.saturating_mul(2u64.saturating_pow(attempt)),
                connection_options.max_reconnect_delay_ms,
            );

            log::info!(
                "Attempting reconnection in {}ms (attempt {})",
                delay,
                attempt + 1
            );

            // Wait for the backoff delay, but also listen for shutdown commands
            let sleep_fut = tokio::time::sleep(Duration::from_millis(delay));
            tokio::pin!(sleep_fut);

            let mut got_shutdown = false;
            loop {
                tokio::select! {
                    biased;
                    cmd = cmd_rx.recv() => {
                        match cmd {
                            Some(ConnCmd::Subscribe { id, sql, options, event_tx, result_tx }) => {
                                // If replacing an existing subscription with the same
                                // ID, remove the old entry so we don't accumulate stale
                                // server-side subscriptions after reconnect.
                                if subs.contains_key(&id) {
                                    log::debug!(
                                        "[kalam-link] Replacing queued subscription '{}' during reconnect",
                                        id,
                                    );
                                    subs.remove(&id);
                                }
                                // Queue the subscription — we'll send it after reconnecting
                                let gen = next_generation;
                                next_generation += 1;
                                subs.insert(id, SubEntry {
                                    sql,
                                    options,
                                    event_tx,
                                    last_seq_id: None,
                                    generation: gen,
                                    created_at_ms: now_ms(),
                                    last_event_time_ms: None,
                                });
                                // Don't confirm yet — will confirm after reconnect via
                                // re-subscribe. For now, ack success so the caller gets
                                // the event_rx and can start waiting.
                                let _ = result_tx.send(Ok(gen));
                            },
                            Some(ConnCmd::Unsubscribe { id, generation }) => {
                                let should_remove = match generation {
                                    Some(gen) => subs.get(&id).map_or(false, |e| e.generation == gen),
                                    None => true,
                                };
                                if should_remove {
                                    subs.remove(&id);
                                }
                            },
                            Some(ConnCmd::ListSubscriptions { result_tx }) => {
                                let _ = result_tx.send(snapshot_subscriptions(&subs));
                            },
                            Some(ConnCmd::Shutdown) | None => {
                                got_shutdown = true;
                                break;
                            },
                        }
                    }
                    _ = &mut sleep_fut => {
                        break;
                    }
                }
            }

            if got_shutdown {
                shutdown_requested = true;
                continue;
            }

            // Attempt reconnection
            match establish_ws(
                &base_url,
                &resolved_auth,
                &timeouts,
                &connection_options,
                &event_handlers,
            )
            .await
            {
                Ok((mut stream, _auth)) => {
                    log::info!("Reconnection successful");
                    reconnect_attempts.store(0, Ordering::SeqCst);
                    connected.store(true, Ordering::SeqCst);
                    event_handlers.emit_connect();

                    // Re-subscribe all active subscriptions
                    resubscribe_all(&mut stream, &subs, &event_handlers).await;

                    ws_stream = Some(stream);
                    idle_deadline = TokioInstant::now() + keepalive_dur;
                    awaiting_pong = false;
                    pong_deadline = TokioInstant::now() + FAR_FUTURE;
                },
                Err(e) => {
                    log::warn!("Reconnection attempt {} failed: {}", attempt + 1, e);
                    // Loop back to try again — the next iteration will compute
                    // a new delay from the incremented attempt counter.
                },
            }
        }
    }
}
