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
use kalamdb_commons::models::{ConnectionId, LiveQueryId, TableId, UserId};
use kalamdb_commons::websocket::SubscriptionOptions;
use kalamdb_commons::{NodeId, Notification};
use log::{debug, info, warn};
use parking_lot::RwLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Type alias for sending live query notifications to WebSocket clients
pub type NotificationSender = mpsc::UnboundedSender<(LiveQueryId, Notification)>;

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

/// Subscription state - unified struct for both storage and notification delivery
///
/// Contains all metadata needed for:
/// - Notification filtering (filter_expr)
/// - Initial data batch fetching (sql, batch_size, snapshot_end_seq)
/// - Column projections (projections)
#[derive(Debug, Clone)]
pub struct SubscriptionState {
    pub live_id: LiveQueryId,
    pub table_id: TableId,
    pub options: SubscriptionOptions,
    /// Original SQL query for batch fetching
    pub sql: String,
    /// Compiled filter expression from WHERE clause (parsed once at subscription time)
    /// None means no filter (SELECT * without WHERE)
    pub filter_expr: Option<Expr>,
    /// Extracted column projections from SELECT clause
    /// None means SELECT * (all columns)
    pub projections: Option<Vec<String>>,
    /// Batch size for initial data loading
    pub batch_size: usize,
    /// Snapshot boundary SeqId for consistent batch loading
    pub snapshot_end_seq: Option<SeqId>,
    /// Shared notification channel (Arc for zero-copy)
    pub notification_tx: Arc<NotificationSender>,
}

/// Connection state - everything about a connection in one struct
///
/// Handlers hold `Arc<RwLock<ConnectionState>>` for direct access.
pub struct ConnectionState {
    // === Identity ===
    /// Connection ID
    pub connection_id: ConnectionId,
    /// User ID (None until authenticated)
    pub user_id: Option<UserId>,
    /// Client IP address (for localhost bypass check)
    pub client_ip: Option<String>,

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
    /// Channel to send notifications to this connection's WebSocket task
    pub notification_tx: Arc<NotificationSender>,
    /// Channel to send control events (ping, timeout, shutdown)
    pub event_tx: mpsc::UnboundedSender<ConnectionEvent>,
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
    pub fn client_ip(&self) -> Option<&str> {
        self.client_ip.as_deref()
    }

    /// Get a subscription by ID
    pub fn get_subscription(&self, subscription_id: &str) -> Option<SubscriptionState> {
        self.subscriptions.get(subscription_id).map(|s| s.clone())
    }

    /// Update snapshot_end_seq for a subscription
    pub fn update_snapshot_end_seq(&self, subscription_id: &str, snapshot_end_seq: Option<SeqId>) {
        if let Some(mut sub) = self.subscriptions.get_mut(subscription_id) {
            sub.snapshot_end_seq = snapshot_end_seq;
        }
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
/// TODO: We dont need this anymore we can use ConnectionState directly
pub struct ConnectionRegistration {
    /// Shared connection state - hold this for direct access
    pub state: SharedConnectionState,
    /// Receiver for control events (ping, timeout, shutdown)
    pub event_rx: mpsc::UnboundedReceiver<ConnectionEvent>,
    /// Receiver for notifications (live query updates)
    pub notification_rx: mpsc::UnboundedReceiver<(LiveQueryId, Notification)>,
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
    /// User → Connections index: UserId → Vec<ConnectionId>
    /// TODO: Is this needed?
    user_connections: DashMap<UserId, Vec<ConnectionId>>,
    /// (UserId, TableId) → Vec<SubscriptionState> for O(1) notification delivery
    user_table_subscriptions: DashMap<(UserId, TableId), Vec<SubscriptionState>>,
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

    // === Shutdown Coordination ===
    shutdown_token: CancellationToken,
    is_shutting_down: AtomicBool,

    // === Metrics ===
    total_connections: AtomicUsize,
    total_subscriptions: AtomicUsize,
}

impl ConnectionsManager {
    /// Create a new connections manager and start the background heartbeat checker
    pub fn new(
        node_id: NodeId,
        client_timeout: Duration,
        auth_timeout: Duration,
        heartbeat_interval: Duration,
    ) -> Arc<Self> {
        let shutdown_token = CancellationToken::new();

        let registry = Arc::new(Self {
            connections: DashMap::with_capacity(1024),
            user_connections: DashMap::new(),
            user_table_subscriptions: DashMap::new(),
            live_id_to_connection: DashMap::new(),
            node_id,
            client_timeout,
            auth_timeout,
            heartbeat_interval,
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

        info!(
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
    /// Returns None if server is shutting down.
    pub fn register_connection(
        &self,
        connection_id: ConnectionId,
        client_ip: Option<String>,
    ) -> Option<ConnectionRegistration> {
        if self.is_shutting_down.load(Ordering::Acquire) {
            warn!("Rejecting new connection during shutdown: {}", connection_id);
            return None;
        }

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (notification_tx, notification_rx) = mpsc::unbounded_channel();
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
        self.connections.insert(connection_id.clone(), shared_state.clone());
        let count = self.total_connections.fetch_add(1, Ordering::AcqRel) + 1;

        debug!("Connection registered: {} (total: {})", connection_id, count);

        Some(ConnectionRegistration {
            state: shared_state,
            event_rx,
            notification_rx,
        })
    }

    /// Called after successful authentication to update user index
    ///
    /// Must be called after `state.write().mark_authenticated(user_id)`.
    /// TODO: Use the same reference we have of ConnectionState instead of looking up again.
    pub fn on_authenticated(&self, connection_id: &ConnectionId, user_id: UserId) {
        self.user_connections
            .entry(user_id)
            .or_default()
            .push(connection_id.clone());
    }

    /// Unregister a connection and all its subscriptions
    ///
    /// Returns the list of removed LiveQueryIds for cleanup.
    pub fn unregister_connection(&self, connection_id: &ConnectionId) -> Vec<LiveQueryId> {
        let removed_live_ids = if let Some((_, shared_state)) = self.connections.remove(connection_id) {
            self.total_connections.fetch_sub(1, Ordering::AcqRel);

            let state = shared_state.read();

            // Remove from user index
            if let Some(user_id) = &state.user_id {
                if let Some(mut conns) = self.user_connections.get_mut(user_id) {
                    conns.retain(|c| c != connection_id);
                    if conns.is_empty() {
                        drop(conns);
                        self.user_connections.remove(user_id);
                    }
                }

                // Remove from user_table_subscriptions index
                for entry in state.subscriptions.iter() {
                    let sub = entry.value();
                    let key = (user_id.clone(), sub.table_id.clone());
                    if let Some(mut handles) = self.user_table_subscriptions.get_mut(&key) {
                        handles.retain(|h| h.live_id != sub.live_id);
                        if handles.is_empty() {
                            drop(handles);
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
    pub fn index_subscription(
        &self,
        user_id: &UserId,
        connection_id: &ConnectionId,
        live_id: LiveQueryId,
        table_id: TableId,
        subscription_state: SubscriptionState,
    ) {
        // Add to (UserId, TableId) → Vec<SubscriptionState> index
        self.user_table_subscriptions
            .entry((user_id.clone(), table_id))
            .or_default()
            .push(subscription_state);

        // Add to reverse index
        self.live_id_to_connection.insert(live_id, connection_id.clone());
        
        self.total_subscriptions.fetch_add(1, Ordering::AcqRel);
    }

    /// Remove subscription from indices (called by LiveQueryManager after removing from ConnectionState)
    pub fn unindex_subscription(&self, user_id: &UserId, live_id: &LiveQueryId, table_id: &TableId) {
        // Remove from reverse index
        self.live_id_to_connection.remove(live_id);

        // Remove from user_table_subscriptions index
        let key = (user_id.clone(), table_id.clone());
        if let Some(mut handles) = self.user_table_subscriptions.get_mut(&key) {
            handles.retain(|h| &h.live_id != live_id);
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
        self.user_table_subscriptions
            .contains_key(&(user_id.clone(), table_id.clone()))
    }

    /// Get subscriptions for a specific (user, table) pair for notification routing
    #[inline]
    pub fn get_subscriptions_for_table(
        &self,
        user_id: &UserId,
        table_id: &TableId,
    ) -> Vec<SubscriptionState> {
        self.user_table_subscriptions
            .get(&(user_id.clone(), table_id.clone()))
            .map(|states| states.clone())
            .unwrap_or_default()
    }

    /// Check if any subscriptions exist for a table (across all users)
    pub fn has_subscriptions_for_table(&self, table_ref: &str) -> bool {
        self.user_table_subscriptions.iter().any(|entry| {
            entry.key().1.to_string().contains(table_ref)
        })
    }

    // ==================== Notification Delivery ====================

    /// Send notification to a specific subscription
    pub fn notify_subscription(&self, live_id: &LiveQueryId, notification: Notification) {
        if let Some(conn_id) = self.live_id_to_connection.get(live_id) {
            if let Some(shared_state) = self.connections.get(conn_id.value()) {
                let state = shared_state.read();
                let _ = state.notification_tx.send((live_id.clone(), notification));
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
        if let Some(handles) = self.user_table_subscriptions.get(&(user_id.clone(), table_id.clone())) {
            for handle in handles.iter() {
                let _ = handle.notification_tx.send((handle.live_id.clone(), notification.clone()));
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
            let _ = state.event_tx.send(ConnectionEvent::Shutdown);
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
            self.user_connections.clear();
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
                let _ = state.event_tx.send(ConnectionEvent::AuthTimeout);
                to_timeout.push(conn_id.clone());
                continue;
            }

            // Check heartbeat timeout
            if now.duration_since(state.last_heartbeat) > self.client_timeout {
                debug!("Heartbeat timeout for connection: {}", conn_id);
                let _ = state.event_tx.send(ConnectionEvent::HeartbeatTimeout);
                to_timeout.push(conn_id.clone());
                continue;
            }

            // Request ping
            let _ = state.event_tx.send(ConnectionEvent::SendPing);
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
            NodeId::new("test-node".to_string()),
            Duration::from_secs(10),
            Duration::from_secs(3),
            Duration::from_secs(5),
        )
    }

    #[tokio::test]
    async fn test_register_unregister_connection() {
        let registry = create_test_registry();
        let conn_id = ConnectionId::new("conn1");

        let reg = registry.register_connection(conn_id.clone(), Some("127.0.0.1".to_string()));
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

        let reg = registry.register_connection(conn_id.clone(), None).unwrap();
        
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

        let reg = registry.register_connection(ConnectionId::new("conn1"), None);
        assert!(reg.is_none());
    }
}
