//! WebSocket Connections Manager
//!
//! Single source of truth for all WebSocket connection state:
//! - Connection lifecycle (connected, authenticated, subscriptions)
//! - Heartbeat tracking (last activity, auth timeout)  
//! - Subscription management
//! - Notification delivery
//! - Graceful shutdown coordination
//!
//! Architecture:
//! - `ConnectionsManager` manages connection lifecycle
//! - `SharedConnectionState` (Arc<RwLock<ConnectionState>>) is returned on registration
//! - Handlers hold the Arc and can access/mutate state directly without ID lookups

use dashmap::DashMap;
use datafusion::sql::sqlparser::ast::Expr;
use kalamdb_commons::ids::SeqId;
use kalamdb_commons::models::{ConnectionId, ConnectionInfo, LiveQueryId, TableId, UserId};
use kalamdb_commons::{NodeId, Notification};
use log::{debug, info, warn};
use parking_lot::{Mutex, RwLock};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Maximum pending notifications per connection before dropping new ones
pub const NOTIFICATION_CHANNEL_CAPACITY: usize = 1000;

/// Maximum pending control events per connection
pub const EVENT_CHANNEL_CAPACITY: usize = 16;

/// Type alias for sending live query notifications to WebSocket clients
pub type NotificationSender = mpsc::Sender<Arc<Notification>>;

/// Type alias for receiving live query notifications
pub type NotificationReceiver = mpsc::Receiver<Arc<Notification>>;

/// Type alias for sending control events to connections
pub type EventSender = mpsc::Sender<ConnectionEvent>;

/// Type alias for receiving control events
pub type EventReceiver = mpsc::Receiver<ConnectionEvent>;

/// Shared connection state - can be held by handlers for direct access
pub type SharedConnectionState = Arc<RwLock<ConnectionState>>;

/// Events sent to connection tasks from the heartbeat checker
#[derive(Debug, Clone)]
pub enum ConnectionEvent {
    /// Send a ping to the client
    SendPing,
    /// Authentication timeout - close connection
    AuthTimeout,
    /// Heartbeat timeout - close connection
    HeartbeatTimeout,
    /// Server is shutting down - close connection gracefully
    Shutdown,
}

/// Lightweight handle for subscription indices
///
/// Contains only the data needed for notification routing.
/// ~48 bytes per handle (vs ~800+ bytes for full SubscriptionState)
#[derive(Debug, Clone)]
pub struct SubscriptionHandle {
    /// Shared filter expression (Arc for zero-copy across indices)
    pub filter_expr: Option<Arc<Expr>>,
    /// Column projections for filtering notification payload (None = all columns)
    pub projections: Option<Arc<Vec<String>>>,
    /// Shared notification channel
    pub notification_tx: Arc<NotificationSender>,
    /// Flow control for initial load buffering and snapshot gating
    pub flow_control: Arc<SubscriptionFlowControl>,
}

/// Buffered notification with optional SeqId ordering key
#[derive(Debug, Clone)]
pub struct BufferedNotification {
    pub seq: Option<SeqId>,
    pub notification: Arc<Notification>,
}

/// Flow control for subscription initial load gating
#[derive(Debug)]
pub struct SubscriptionFlowControl {
    snapshot_end_seq: AtomicI64,
    has_snapshot: AtomicBool,
    initial_complete: AtomicBool,
    buffer: Mutex<Vec<BufferedNotification>>,
}

impl SubscriptionFlowControl {
    pub fn new() -> Self {
        Self {
            snapshot_end_seq: AtomicI64::new(0),
            has_snapshot: AtomicBool::new(false),
            initial_complete: AtomicBool::new(false),
            buffer: Mutex::new(Vec::new()),
        }
    }

    pub fn set_snapshot_end_seq(&self, snapshot_end_seq: Option<SeqId>) {
        if let Some(seq) = snapshot_end_seq {
            self.snapshot_end_seq.store(seq.as_i64(), Ordering::Release);
            self.has_snapshot.store(true, Ordering::Release);

            let max_seq = seq.as_i64();
            let mut buffer = self.buffer.lock();
            buffer.retain(|item| match item.seq {
                Some(item_seq) => item_seq.as_i64() > max_seq,
                None => true,
            });
        } else {
            self.has_snapshot.store(false, Ordering::Release);
        }
    }

    pub fn snapshot_end_seq(&self) -> Option<i64> {
        if self.has_snapshot.load(Ordering::Acquire) {
            Some(self.snapshot_end_seq.load(Ordering::Acquire))
        } else {
            None
        }
    }

    pub fn is_initial_complete(&self) -> bool {
        self.initial_complete.load(Ordering::Acquire)
    }

    pub fn mark_initial_complete(&self) {
        self.initial_complete.store(true, Ordering::Release);
    }

    pub fn buffer_notification(&self, notification: Arc<Notification>, seq: Option<SeqId>) {
        let mut buffer = self.buffer.lock();
        buffer.push(BufferedNotification { seq, notification });
    }

    pub fn drain_buffered_notifications(&self) -> Vec<BufferedNotification> {
        let mut buffer = self.buffer.lock();
        let mut drained = std::mem::take(&mut *buffer);
        drained.sort_by(|a, b| match (a.seq, b.seq) {
            (Some(a_seq), Some(b_seq)) => a_seq.as_i64().cmp(&b_seq.as_i64()),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
        drained
    }
}

/// Subscription state - stored only in ConnectionState.subscriptions
///
/// Contains all metadata needed for:
/// - Notification filtering (filter_expr)
/// - Column projections (projections)
/// - Initial data batch fetching (sql, batch_size, snapshot_end_seq)
/// - Batch pagination tracking (current_batch_num)
///
/// Memory optimization: Uses Arc<str> for SQL and Arc<Expr> for filter
/// to share data with SubscriptionHandle indices.
#[derive(Debug, Clone)]
pub struct SubscriptionState {
    pub live_id: LiveQueryId,
    pub table_id: TableId,
    /// Original SQL query for batch fetching (Arc for zero-copy)
    pub sql: Arc<str>,
    /// Compiled filter expression from WHERE clause (parsed once at subscription time)
    /// None means no filter (SELECT * without WHERE)
    /// Arc-wrapped for sharing with SubscriptionHandle
    pub filter_expr: Option<Arc<Expr>>,
    /// Column projections from SELECT clause (None = SELECT *, i.e., all columns)
    /// Arc-wrapped for sharing with SubscriptionHandle
    pub projections: Option<Arc<Vec<String>>>,
    /// Batch size for initial data loading
    pub batch_size: usize,
    /// Snapshot boundary SeqId for consistent batch loading
    pub snapshot_end_seq: Option<SeqId>,
    /// Current batch number for pagination tracking (0-indexed)
    /// Incremented after each batch is sent
    pub current_batch_num: u32,
    /// Flow control for initial load buffering and snapshot gating
    pub flow_control: Arc<SubscriptionFlowControl>,
}

/// Connection state - everything about a connection in one struct
///
/// Handlers hold `Arc<RwLock<ConnectionState>>` for direct access.
pub struct ConnectionState {
    // === Identity ===
    /// Connection ID
    pub connection_id: ConnectionId,
    /// User ID (None until authenticated)
    pub user_id: Option<UserId>, //TODO: Should we use AuthenticatedUser? which contains also Role/ip
    /// Client IP address (for localhost bypass check)
    pub client_ip: ConnectionInfo,

    // === Authentication State ===
    /// Whether connection has been authenticated
    pub is_authenticated: bool,
    /// Whether auth attempt has started (for timeout logic)
    pub auth_started: bool,

    // === Heartbeat/Timing ===
    /// When connection was established
    pub connected_at: Instant,
    /// Last heartbeat (ping/pong/message activity)
    pub last_heartbeat: Instant,

    // === Subscriptions ===
    /// Active subscriptions for this connection (subscription_id -> state)
    pub subscriptions: DashMap<String, SubscriptionState>,

    // === Channels ===
    /// Channel to send notifications to this connection's WebSocket task (bounded for backpressure)
    pub notification_tx: Arc<NotificationSender>,
    /// Channel to send control events (ping, timeout, shutdown) (bounded)
    pub event_tx: EventSender,
}

impl ConnectionState {
    /// Update heartbeat timestamp - call on any client activity
    #[inline]
    pub fn update_heartbeat(&mut self) {
        self.last_heartbeat = Instant::now();
    }

    /// Mark that authentication has started (for timeout logic)
    #[inline]
    pub fn mark_auth_started(&mut self) {
        self.auth_started = true;
    }

    /// Mark connection as authenticated with user ID
    #[inline]
    pub fn mark_authenticated(&mut self, user_id: UserId) {
        self.is_authenticated = true;
        self.user_id = Some(user_id);
    }

    /// Check if connection is authenticated
    #[inline]
    pub fn is_authenticated(&self) -> bool {
        self.is_authenticated
    }

    /// Get user ID (None if not authenticated)
    #[inline]
    pub fn user_id(&self) -> Option<&UserId> {
        self.user_id.as_ref()
    }

    /// Get connection ID
    #[inline]
    pub fn connection_id(&self) -> &ConnectionId {
        &self.connection_id
    }

    /// Get client IP
    #[inline]
    pub fn client_ip(&self) -> Option<&ConnectionInfo> {
        Some(&self.client_ip)
    }

    /// Get a subscription by ID
    pub fn get_subscription(&self, subscription_id: &str) -> Option<SubscriptionState> {
        self.subscriptions.get(subscription_id).map(|s| s.clone())
    }

    /// Update snapshot_end_seq for a subscription
    pub fn update_snapshot_end_seq(&self, subscription_id: &str, snapshot_end_seq: Option<SeqId>) {
        if let Some(mut sub) = self.subscriptions.get_mut(subscription_id) {
            sub.snapshot_end_seq = snapshot_end_seq;
            sub.flow_control.set_snapshot_end_seq(snapshot_end_seq);
        }
    }

    /// Mark initial load complete and flush buffered notifications
    pub fn complete_initial_load(&self, subscription_id: &str) -> usize {
        if let Some(sub) = self.subscriptions.get(subscription_id) {
            let flow_control = Arc::clone(&sub.flow_control);
            flow_control.mark_initial_complete();
            let buffered = flow_control.drain_buffered_notifications();

            let mut sent = 0usize;
            for item in buffered {
                // TODO: Backpressure limitation: buffered notifications are flushed with try_send.
                // If the channel is full, remaining buffered notifications are dropped.
                // Consider a blocking flush, retry queue, or persistent spool for lossless delivery.
                if let Err(e) = self.notification_tx.try_send(item.notification) {
                    if matches!(e, mpsc::error::TrySendError::Full(_)) {
                        warn!(
                            "Notification channel full while flushing buffered notifications for {}",
                            subscription_id
                        );
                        break;
                    }
                } else {
                    sent += 1;
                }
            }

            return sent;
        }
        0
    }

    /// Increment current_batch_num for a subscription and return the new value
    /// Returns None if subscription not found
    pub fn increment_batch_num(&self, subscription_id: &str) -> Option<u32> {
        if let Some(mut sub) = self.subscriptions.get_mut(subscription_id) {
            sub.current_batch_num += 1;
            Some(sub.current_batch_num)
        } else {
            None
        }
    }

    /// Get current_batch_num for a subscription
    /// Returns None if subscription not found
    pub fn get_batch_num(&self, subscription_id: &str) -> Option<u32> {
        self.subscriptions.get(subscription_id).map(|s| s.current_batch_num)
    }

    /// Get all subscription IDs
    pub fn subscription_ids(&self) -> Vec<String> {
        self.subscriptions.iter().map(|e| e.key().clone()).collect()
    }

    /// Check if subscription exists
    pub fn has_subscription(&self, subscription_id: &str) -> bool {
        self.subscriptions.contains_key(subscription_id)
    }

    /// Get subscription count
    pub fn subscription_count(&self) -> usize {
        self.subscriptions.len()
    }
}

/// Registration result returned when a connection registers
pub struct ConnectionRegistration {
    /// Shared connection state - hold this for direct access
    pub state: SharedConnectionState,
    /// Receiver for control events (ping, timeout, shutdown) (bounded)
    pub event_rx: EventReceiver,
    /// Receiver for notifications (live query updates) (bounded for backpressure)
    pub notification_rx: NotificationReceiver,
}

/// WebSocket Connections Manager
///
/// Responsibilities:
/// - Connection lifecycle (register, unregister, heartbeat check)
/// - Authentication state management
/// - Maintaining indices for efficient lookups
/// - Background heartbeat checker
///
/// NOTE: Subscription management is delegated to LiveQueryManager.
/// The manager only maintains indices for notification routing.
pub struct ConnectionsManager {
    // === Primary Storage ===
    /// All active connections: ConnectionId → SharedConnectionState
    connections: DashMap<ConnectionId, SharedConnectionState>,

    // === Secondary Indices (for efficient lookups) ===
    /// (UserId, TableId) → DashMap<LiveQueryId, SubscriptionHandle> for O(1) notification delivery
    /// Uses lightweight handles (~48 bytes) instead of full state (~800+ bytes)
    user_table_subscriptions: DashMap<(UserId, TableId), Arc<DashMap<LiveQueryId, SubscriptionHandle>>>,
    /// Shared empty map to avoid allocations on lookup misses
    empty_subscriptions: Arc<DashMap<LiveQueryId, SubscriptionHandle>>,
    /// LiveQueryId → ConnectionId reverse index
    live_id_to_connection: DashMap<LiveQueryId, ConnectionId>,

    // === Configuration ===
    /// Node identifier for this server
    pub node_id: NodeId,
    /// Client heartbeat timeout
    client_timeout: Duration,
    /// Authentication timeout
    auth_timeout: Duration,
    /// Heartbeat check interval
    heartbeat_interval: Duration,
    /// Maximum concurrent connections allowed (DoS protection)
    max_connections: usize,

    // === Shutdown Coordination ===
    shutdown_token: CancellationToken,
    is_shutting_down: AtomicBool,

    // === Metrics ===
    total_connections: AtomicUsize,
    total_subscriptions: AtomicUsize,
}

impl ConnectionsManager {
    /// Default maximum connections (10,000 concurrent connections)
    pub const DEFAULT_MAX_CONNECTIONS: usize = 10_000;

    /// Create a new connections manager and start the background heartbeat checker
    pub fn new(
        node_id: NodeId,
        client_timeout: Duration,
        auth_timeout: Duration,
        heartbeat_interval: Duration,
    ) -> Arc<Self> {
        Self::with_max_connections(
            node_id,
            client_timeout,
            auth_timeout,
            heartbeat_interval,
            Self::DEFAULT_MAX_CONNECTIONS,
        )
    }

    /// Create a new connections manager with a custom max connections limit
    pub fn with_max_connections(
        node_id: NodeId,
        client_timeout: Duration,
        auth_timeout: Duration,
        heartbeat_interval: Duration,
        max_connections: usize,
    ) -> Arc<Self> {
        let shutdown_token = CancellationToken::new();

        let registry = Arc::new(Self {
            connections: DashMap::with_capacity(1024),
            user_table_subscriptions: DashMap::new(),
            empty_subscriptions: Arc::new(DashMap::new()),
            live_id_to_connection: DashMap::new(),
            node_id,
            client_timeout,
            auth_timeout,
            heartbeat_interval,
            max_connections,
            shutdown_token,
            is_shutting_down: AtomicBool::new(false),
            total_connections: AtomicUsize::new(0),
            total_subscriptions: AtomicUsize::new(0),
        });

        // Start background heartbeat checker
        let registry_clone = registry.clone();
        tokio::spawn(async move {
            registry_clone.run_heartbeat_checker().await;
        });

        debug!(
            "ConnectionsManager initialized (node={}, client_timeout={}s, auth_timeout={}s)",
            registry.node_id,
            client_timeout.as_secs(),
            auth_timeout.as_secs()
        );

        registry
    }

    // ==================== Connection Lifecycle ====================

    /// Register a new connection (called immediately when WebSocket opens)
    ///
    /// Returns `ConnectionRegistration` containing:
    /// - `SharedConnectionState` - hold this for direct access to connection state
    /// - `event_rx` - control events (ping, timeout, shutdown)
    /// - `notification_rx` - live query notifications
    ///
    /// Returns None if server is shutting down or max connections reached.
    pub fn register_connection(
        &self,
        connection_id: ConnectionId,
        client_ip: ConnectionInfo,
    ) -> Option<ConnectionRegistration> {
        if self.is_shutting_down.load(Ordering::Acquire) {
            warn!("Rejecting new connection during shutdown: {}", connection_id);
            return None;
        }

        // DoS protection: reject if at max connections
        let current = self.total_connections.load(Ordering::Acquire);
        if current >= self.max_connections {
            warn!(
                "Rejecting connection {}: max connections ({}) reached",
                connection_id, self.max_connections
            );
            return None;
        }

        // Use bounded channels for backpressure and memory control
        let (event_tx, event_rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        let (notification_tx, notification_rx) = mpsc::channel(NOTIFICATION_CHANNEL_CAPACITY);
        let now = Instant::now();

        let state = ConnectionState {
            connection_id: connection_id.clone(),
            user_id: None,
            client_ip,
            is_authenticated: false,
            auth_started: false,
            connected_at: now,
            last_heartbeat: now,
            subscriptions: DashMap::new(),
            notification_tx: Arc::new(notification_tx),
            event_tx,
        };

        let shared_state = Arc::new(RwLock::new(state));
        self.connections.insert(connection_id.clone(), Arc::clone(&shared_state));
        let _count = self.total_connections.fetch_add(1, Ordering::AcqRel) + 1;

        Some(ConnectionRegistration {
            state: shared_state,
            event_rx,
            notification_rx,
        })
    }

    /// Called after successful authentication to update user index
    ///
    /// Must be called after `state.write().mark_authenticated(user_id)`.
    pub fn on_authenticated(&self, _connection_id: &ConnectionId, _user_id: UserId) {
        // User index no longer needed - subscriptions are tracked via user_table_subscriptions
    }

    /// Unregister a connection and all its subscriptions
    ///
    /// Returns the list of removed LiveQueryIds for cleanup.
    pub fn unregister_connection(&self, connection_id: &ConnectionId) -> Vec<LiveQueryId> {
        let removed_live_ids =
            if let Some((_, shared_state)) = self.connections.remove(connection_id) {
                self.total_connections.fetch_sub(1, Ordering::AcqRel);

                let state = shared_state.read();

                // Remove from user_table_subscriptions index
                if let Some(user_id) = &state.user_id {
                    // Remove from user_table_subscriptions index
                    for entry in state.subscriptions.iter() {
                        let sub = entry.value();
                        let key = (user_id.clone(), sub.table_id.clone());
                        if let Some(entries) = self.user_table_subscriptions.get(&key) {
                            entries.remove(&sub.live_id);
                            if entries.is_empty() {
                                drop(entries);
                                self.user_table_subscriptions.remove(&key);
                            }
                        }
                    }
                }

                // Collect and remove all subscriptions
                let mut removed = Vec::with_capacity(state.subscriptions.len());
                for entry in state.subscriptions.iter() {
                    let sub = entry.value();
                    removed.push(sub.live_id.clone());
                    self.live_id_to_connection.remove(&sub.live_id);
                }

                let sub_count = removed.len();
                if sub_count > 0 {
                    self.total_subscriptions.fetch_sub(sub_count, Ordering::AcqRel);
                }

                removed
            } else {
                Vec::new()
            };

        if !removed_live_ids.is_empty() {
            debug!(
                "Connection unregistered: {} (removed {} subscriptions)",
                connection_id,
                removed_live_ids.len()
            );
        }

        removed_live_ids
    }

    // ==================== Subscription Index Management ====================
    // NOTE: Actual subscription storage is in ConnectionState.subscriptions
    // These methods maintain secondary indices for efficient notification routing

    /// Add subscription to indices (called by LiveQueryManager after adding to ConnectionState)
    ///
    /// Uses lightweight SubscriptionHandle for the index instead of cloning full state.
    pub fn index_subscription(
        &self,
        user_id: &UserId,
        connection_id: &ConnectionId,
        live_id: LiveQueryId,
        table_id: TableId,
        handle: SubscriptionHandle,
    ) {
        log::debug!(
            "ConnectionsManager::index_subscription: user={}, table={}, live_id={}",
            user_id,
            table_id,
            live_id
        );
        // Add to (UserId, TableId) → Arc<DashMap<LiveQueryId, SubscriptionHandle>> index
        self.user_table_subscriptions
            .entry((user_id.clone(), table_id))
            .and_modify(|handles| {
                handles.insert(live_id.clone(), handle.clone());
            })
            .or_insert_with(|| {
                let handles = DashMap::new();
                handles.insert(live_id.clone(), handle.clone());
                Arc::new(handles)
            });

        // Add to reverse index
        self.live_id_to_connection.insert(live_id, connection_id.clone());

        self.total_subscriptions.fetch_add(1, Ordering::AcqRel);
    }

    /// Remove subscription from indices (called by LiveQueryManager after removing from ConnectionState)
    pub fn unindex_subscription(
        &self,
        user_id: &UserId,
        live_id: &LiveQueryId,
        table_id: &TableId,
    ) {
        // Remove from reverse index
        self.live_id_to_connection.remove(live_id);

        // Remove from user_table_subscriptions index
        let key = (user_id.clone(), table_id.clone());
        if let Some(handles) = self.user_table_subscriptions.get(&key) {
            handles.remove(live_id);
            if handles.is_empty() {
                drop(handles);
                self.user_table_subscriptions.remove(&key);
            }
        }

        self.total_subscriptions.fetch_sub(1, Ordering::AcqRel);
    }

    /// Get connection ID for a live query (for cleanup)
    pub fn get_connection_for_live_id(&self, live_id: &LiveQueryId) -> Option<ConnectionId> {
        self.live_id_to_connection.get(live_id).map(|r| r.clone())
    }

    // ==================== Query Methods ====================

    /// Get shared connection state by ID (rarely needed - handlers should hold their own reference)
    pub fn get_connection(&self, connection_id: &ConnectionId) -> Option<SharedConnectionState> {
        self.connections.get(connection_id).map(|c| c.clone())
    }

    /// Check if any subscriptions exist for a (user, table) pair
    pub fn has_subscriptions(&self, user_id: &UserId, table_id: &TableId) -> bool {
        self.user_table_subscriptions.contains_key(&(user_id.clone(), table_id.clone()))
    }

    /// Get subscription handles for a specific (user, table) pair for notification routing
    ///
    /// Returns lightweight handles containing only the data needed for filtering and routing.
    #[inline]
    pub fn get_subscriptions_for_table(
        &self,
        user_id: &UserId,
        table_id: &TableId,
    ) -> Arc<DashMap<LiveQueryId, SubscriptionHandle>> {
        self.user_table_subscriptions
            .get(&(user_id.clone(), table_id.clone()))
            .map(|handles| Arc::clone(handles.value()))
            .unwrap_or_else(|| Arc::clone(&self.empty_subscriptions))
    }

    /// Check if any subscriptions exist for a table (across all users)
    pub fn has_subscriptions_for_table(&self, table_ref: &str) -> bool {
        let mut parts = table_ref.splitn(2, '.');
        let first = parts.next().unwrap_or("");
        let second = parts.next();
        match second {
            Some(table_name) => self.user_table_subscriptions.iter().any(|entry| {
                let key_table = &entry.key().1;
                key_table.namespace_id().as_str() == first
                    && key_table.table_name().as_str() == table_name
            }),
            None => self
                .user_table_subscriptions
                .iter()
                .any(|entry| entry.key().1.table_name().as_str() == first),
        }
    }

    // ==================== Notification Delivery ====================

    /// Send notification to a specific subscription
    pub fn notify_subscription(&self, live_id: &LiveQueryId, notification: Notification) {
        if let Some(conn_id) = self.live_id_to_connection.get(live_id) {
            if let Some(shared_state) = self.connections.get(conn_id.value()) {
                let state = shared_state.read();
                let notification = Arc::new(notification);
                if let Err(e) = state.notification_tx.try_send(notification) {
                    if matches!(e, mpsc::error::TrySendError::Full(_)) {
                        warn!("Notification channel full for {}, dropping notification", live_id);
                    }
                }
            }
        }
    }

    /// Send notification to all subscriptions for a table (for a specific user)
    #[inline]
    pub fn notify_table_for_user(
        &self,
        user_id: &UserId,
        table_id: &TableId,
        notification: Notification,
    ) {
        if let Some(handles) =
            self.user_table_subscriptions.get(&(user_id.clone(), table_id.clone()))
        {
            let notification = Arc::new(notification);
            for handle in handles.iter() {
                let live_id = handle.key().clone();
                if let Err(e) = handle.notification_tx.try_send(Arc::clone(&notification)) {
                    if matches!(e, mpsc::error::TrySendError::Full(_)) {
                        warn!("Notification channel full for {}, dropping", live_id);
                    }
                }
            }
        }
    }

    // ==================== Shutdown ====================

    /// Initiate graceful shutdown of all connections
    pub async fn shutdown(&self, timeout: Duration) {
        let count = self.total_connections.load(Ordering::Acquire);
        info!("Initiating WebSocket shutdown with {} active connections", count);

        self.is_shutting_down.store(true, Ordering::Release);

        // Send shutdown event to all connections
        for entry in self.connections.iter() {
            let state = entry.value().read();
            let _ = state.event_tx.try_send(ConnectionEvent::Shutdown);
        }

        // Wait for connections to close with timeout
        let deadline = Instant::now() + timeout;
        while self.total_connections.load(Ordering::Acquire) > 0 && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        let remaining = self.total_connections.load(Ordering::Acquire);
        if remaining > 0 {
            warn!("Force closing {} connections after timeout", remaining);
            self.connections.clear();
            self.user_table_subscriptions.clear();
            self.live_id_to_connection.clear();
            self.total_connections.store(0, Ordering::Release);
            self.total_subscriptions.store(0, Ordering::Release);
        }

        self.shutdown_token.cancel();
        info!("WebSocket shutdown complete");
    }

    /// Check if shutdown is in progress
    pub fn is_shutting_down(&self) -> bool {
        self.is_shutting_down.load(Ordering::Acquire)
    }

    // ==================== Metrics ====================

    pub fn connection_count(&self) -> usize {
        self.total_connections.load(Ordering::Acquire)
    }

    pub fn subscription_count(&self) -> usize {
        self.total_subscriptions.load(Ordering::Acquire)
    }

    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    pub fn total_connections(&self) -> usize {
        self.connection_count()
    }

    pub fn total_subscriptions(&self) -> usize {
        self.subscription_count()
    }

    // ==================== Background Tasks ====================

    async fn run_heartbeat_checker(self: Arc<Self>) {
        let mut interval = tokio::time::interval(self.heartbeat_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                biased;
                _ = self.shutdown_token.cancelled() => {
                    debug!("Heartbeat checker shutting down");
                    break;
                }
                _ = interval.tick() => {
                    self.check_all_connections();
                }
            }
        }
    }

    fn check_all_connections(&self) {
        let now = Instant::now();
        let mut to_timeout = Vec::new();

        for entry in self.connections.iter() {
            let conn_id = entry.key();
            let shared_state = entry.value();
            let state = shared_state.read();

            // Check auth timeout (only if auth hasn't started)
            if !state.is_authenticated
                && !state.auth_started
                && now.duration_since(state.connected_at) > self.auth_timeout
            {
                debug!("Auth timeout for connection: {}", conn_id);
                let _ = state.event_tx.try_send(ConnectionEvent::AuthTimeout);
                to_timeout.push(conn_id.clone());
                continue;
            }

            // Check heartbeat timeout
            if now.duration_since(state.last_heartbeat) > self.client_timeout {
                debug!("Heartbeat timeout for connection: {}", conn_id);
                let _ = state.event_tx.try_send(ConnectionEvent::HeartbeatTimeout);
                to_timeout.push(conn_id.clone());
                continue;
            }

            // Request ping
            let _ = state.event_tx.try_send(ConnectionEvent::SendPing);
        }

        // Unregister timed-out connections
        for conn_id in to_timeout {
            self.unregister_connection(&conn_id);
        }
    }
}

impl Drop for ConnectionsManager {
    fn drop(&mut self) {
        self.shutdown_token.cancel();
        debug!("ConnectionsManager dropped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_registry() -> Arc<ConnectionsManager> {
        ConnectionsManager::new(
            NodeId::new(1),
            Duration::from_secs(10),
            Duration::from_secs(3),
            Duration::from_secs(5),
        )
    }

    #[tokio::test]
    async fn test_register_unregister_connection() {
        let registry = create_test_registry();
        let conn_id = ConnectionId::new("conn1");

        let reg = registry.register_connection(
            conn_id.clone(),
            ConnectionInfo::new(Some("127.0.0.1".to_string())),
        );
        assert!(reg.is_some());
        assert_eq!(registry.connection_count(), 1);

        registry.unregister_connection(&conn_id);
        assert_eq!(registry.connection_count(), 0);
    }

    #[tokio::test]
    async fn test_authentication_flow() {
        let registry = create_test_registry();
        let conn_id = ConnectionId::new("conn1");
        let user_id = UserId::new("user1");

        let reg = registry.register_connection(conn_id.clone(), ConnectionInfo::new(None));
        assert!(reg.is_some());
        let reg = reg.unwrap();

        {
            let mut state = reg.state.write();
            assert!(!state.is_authenticated());
            state.mark_auth_started();
            state.mark_authenticated(user_id.clone());
        }

        // Update registry index
        registry.on_authenticated(&conn_id, user_id.clone());

        let state = reg.state.read();
        assert!(state.is_authenticated());
        assert_eq!(state.user_id(), Some(&user_id));
    }

    #[tokio::test]
    async fn test_reject_during_shutdown() {
        let registry = create_test_registry();

        registry.is_shutting_down.store(true, Ordering::Release);

        let reg =
            registry.register_connection(ConnectionId::new("conn1"), ConnectionInfo::new(None));
        assert!(reg.is_none());
    }
}
