//! WebSocket Connection Registry
//!
//! Single source of truth for all WebSocket connection state:
//! - Connection lifecycle (connected, authenticated, subscriptions)
//! - Heartbeat tracking (last activity, auth timeout)  
//! - Subscription management
//! - Notification delivery
//! - Graceful shutdown coordination
//!
//! Each connection has exactly ONE entry in memory instead of being scattered across
//! multiple data structures.

use crate::error::KalamDbError;
use dashmap::DashMap;
use kalamdb_commons::models::{ConnectionId, LiveQueryId, TableId, UserId};
use kalamdb_commons::websocket::SubscriptionOptions;
use kalamdb_commons::{NodeId, Notification};
use log::{debug, info, warn};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Type alias for sending live query notifications to WebSocket clients
pub type NotificationSender = mpsc::UnboundedSender<(LiveQueryId, Notification)>;

/// Type alias for backward compatibility (LiveQueryOptions → SubscriptionOptions)
pub type LiveQueryOptions = SubscriptionOptions;

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
/// Used in:
/// - ConnectionState.subscriptions (primary storage)
/// - user_table_subscriptions index (for O(1) notification delivery)
#[derive(Debug, Clone)]
pub struct SubscriptionState {
    pub live_id: LiveQueryId,
    pub table_id: TableId,
    pub options: SubscriptionOptions,
    /// Shared notification channel (Arc for zero-copy)
    pub notification_tx: Arc<NotificationSender>,
}

/// Type alias for backward compatibility with code expecting SubscriptionHandle
pub type SubscriptionHandle = SubscriptionState;

/// Connection state - everything about a connection in one struct
///
/// Memory: ~200 bytes base + subscriptions (vs ~300+ bytes when split across 3 structures)
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
    pub is_authenticated: bool, // TODO: We can use UserId presence to check auth too
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

/// Registration result returned when a connection registers
pub struct ConnectionRegistration {
    /// Receiver for control events (ping, timeout, shutdown)
    pub event_rx: mpsc::UnboundedReceiver<ConnectionEvent>,
    /// Receiver for notifications (live query updates)
    pub notification_rx: mpsc::UnboundedReceiver<(LiveQueryId, Notification)>,
}

/// WebSocket Connection Registry
///
/// Single source of truth for all connection state, replacing:
/// - LiveQueryRegistry (subscriptions, notification routing)
/// - HeartbeatManager (timeouts, ping/pong tracking)
///
/// # Performance
/// - DashMap for lock-free concurrent access
/// - Single entry per connection (better cache locality)
/// - O(1) lookups for all operations
/// - Atomic counters for metrics
pub struct ConnectionRegistry {
    // === Primary Storage ===
    /// All active connections: ConnectionId → ConnectionState
    connections: DashMap<ConnectionId, ConnectionState>,

    // === Secondary Indices (for efficient lookups) ===
    /// User → Connections index: UserId → Vec<ConnectionId>
    user_connections: DashMap<UserId, Vec<ConnectionId>>,
    /// (UserId, TableId) → Vec<SubscriptionState> for O(1) notification delivery
    /// This is THE hot path - pre-built states avoid rebuilding on each notify
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
    /// Cancellation token for background tasks
    shutdown_token: CancellationToken,
    /// Flag indicating shutdown in progress
    is_shutting_down: AtomicBool,

    // === Metrics ===
    /// Total connection count (atomic for fast reads)
    total_connections: AtomicUsize,
    /// Total subscription count (atomic for fast reads)
    total_subscriptions: AtomicUsize,
}

impl ConnectionRegistry {
    /// Create a new connection registry and start the background heartbeat checker
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
            "ConnectionRegistry initialized (node={}, client_timeout={}s, auth_timeout={}s)",
            registry.node_id,
            client_timeout.as_secs(),
            auth_timeout.as_secs()
        );

        registry
    }

    // ==================== Connection Lifecycle ====================

    /// Register a new connection (called immediately when WebSocket opens)
    ///
    /// Returns None if server is shutting down.
    pub fn register_connection(
        &self,
        connection_id: ConnectionId,
        client_ip: Option<String>,
    ) -> Option<ConnectionRegistration> {
        if self.is_shutting_down.load(Ordering::Acquire) {
            warn!(
                "Rejecting new connection during shutdown: {}",
                connection_id
            );
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

        self.connections.insert(connection_id.clone(), state);
        let count = self.total_connections.fetch_add(1, Ordering::AcqRel) + 1;

        debug!(
            "Connection registered: {} (total: {})",
            connection_id, count
        );

        Some(ConnectionRegistration {
            event_rx,
            notification_rx,
        })
    }

    /// Unregister a connection and all its subscriptions
    pub fn unregister_connection(&self, connection_id: &ConnectionId) -> Vec<LiveQueryId> {
        let removed_live_ids = if let Some((_, state)) = self.connections.remove(connection_id) {
            self.total_connections.fetch_sub(1, Ordering::AcqRel);

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
                        handles.retain(|h| &h.live_id != &sub.live_id);
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
                // Remove from reverse index
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

    // ==================== Heartbeat/Timeout Management ====================

    /// Update heartbeat timestamp for a connection
    #[inline]
    pub fn update_heartbeat(&self, connection_id: &ConnectionId) {
        if let Some(mut conn) = self.connections.get_mut(connection_id) {
            conn.last_heartbeat = Instant::now();
        }
    }

    /// Mark that authentication has started
    pub fn mark_auth_started(&self, connection_id: &ConnectionId) {
        if let Some(mut conn) = self.connections.get_mut(connection_id) {
            conn.auth_started = true;
        }
    }

    /// Mark connection as authenticated with user ID
    pub fn mark_authenticated(&self, connection_id: &ConnectionId, user_id: UserId) {
        if let Some(mut conn) = self.connections.get_mut(connection_id) {
            conn.is_authenticated = true;
            conn.user_id = Some(user_id.clone());
        }

        // Add to user index
        self.user_connections
            .entry(user_id)
            .or_default()
            .push(connection_id.clone());
    }

    // ==================== Query Methods ====================

    /// Check if connection is authenticated
    pub fn is_authenticated(&self, connection_id: &ConnectionId) -> bool {
        self.connections
            .get(connection_id)
            .map(|c| c.is_authenticated)
            .unwrap_or(false)
    }

    /// Get user ID for a connection
    pub fn get_user_id(&self, connection_id: &ConnectionId) -> Option<UserId> {
        self.connections
            .get(connection_id)
            .and_then(|c| c.user_id.clone())
    }

    /// Get client IP for a connection
    pub fn get_client_ip(&self, connection_id: &ConnectionId) -> Option<String> {
        self.connections
            .get(connection_id)
            .and_then(|c| c.client_ip.clone())
    }

    /// Get notification sender for a connection
    pub fn get_notification_sender(
        &self,
        connection_id: &ConnectionId,
    ) -> Option<Arc<NotificationSender>> {
        self.connections
            .get(connection_id)
            .map(|c| c.notification_tx.clone())
    }

    // ==================== Subscription Management ====================

    /// Add a subscription to a connection
    pub fn add_subscription(
        &self,
        connection_id: &ConnectionId,
        live_id: LiveQueryId,
        table_id: TableId,
        options: SubscriptionOptions
    ) -> Result<(), KalamDbError> {
        let conn = self.connections.get(connection_id).ok_or_else(|| {
            KalamDbError::NotFound(format!("Connection not found: {}", connection_id))
        })?;
        if !conn.is_authenticated {
            return Err(KalamDbError::Unauthorized(
                "Connection not authenticated".to_string(),
            ));
        }

        let user_id = conn.user_id.clone().ok_or_else(|| {
            KalamDbError::InvalidOperation("Connection has no user_id".to_string())
        })?;

        let subscription_id = live_id.subscription_id().to_string();
        let state = SubscriptionState {
            live_id: live_id.clone(),
            table_id: table_id.clone(),
            options,
            notification_tx: conn.notification_tx.clone(),
        };

        conn.subscriptions.insert(subscription_id, state.clone());
        self.total_subscriptions.fetch_add(1, Ordering::AcqRel);

        // Add to (UserId, TableId) → Vec<SubscriptionState> index - THE HOT PATH
        self.user_table_subscriptions
            .entry((user_id, table_id))
            .or_default()
            .push(state);

        // Add to reverse index
        self.live_id_to_connection
            .insert(live_id, connection_id.clone());

        Ok(())
    }

    /// Register a subscription (compatibility API matching old LiveQueryRegistry)
    ///
    /// This wraps add_subscription with the old method signature.
    pub fn register_subscription(
        &self,
        _user_id: UserId,
        table_id: TableId,
        live_id: LiveQueryId,
        connection_id: ConnectionId,
        options: SubscriptionOptions,
    ) -> Result<(), KalamDbError> {
        self.add_subscription(&connection_id, live_id, table_id, options)
    }

    /// Unregister a subscription (compatibility API matching old LiveQueryRegistry)
    ///
    /// Returns the connection ID if found.
    pub fn unregister_subscription(&self, live_id: &LiveQueryId) -> Option<ConnectionId> {
        self.remove_subscription(live_id)
    }

    /// Remove a subscription from a connection
    pub fn remove_subscription(&self, live_id: &LiveQueryId) -> Option<ConnectionId> {
        // Find connection via reverse index
        let connection_id = self.live_id_to_connection.remove(live_id)?.1;

        if let Some(conn) = self.connections.get(&connection_id) {
            let subscription_id = live_id.subscription_id();
            if let Some((_, sub)) = conn.subscriptions.remove(subscription_id) {
                self.total_subscriptions.fetch_sub(1, Ordering::AcqRel);

                // Remove from user_table_subscriptions index
                if let Some(user_id) = &conn.user_id {
                    let key = (user_id.clone(), sub.table_id.clone());
                    if let Some(mut handles) = self.user_table_subscriptions.get_mut(&key) {
                        handles.retain(|h| &h.live_id != live_id);
                        if handles.is_empty() {
                            drop(handles);
                            self.user_table_subscriptions.remove(&key);
                        }
                    }
                }
            }
        }

        Some(connection_id)
    }

    /// Check if a subscription exists
    pub fn has_subscription(&self, connection_id: &ConnectionId, subscription_id: &str) -> bool {
        self.connections
            .get(connection_id)
            .map(|c| c.subscriptions.contains_key(subscription_id))
            .unwrap_or(false)
    }

    /// Get all subscription IDs for a connection
    pub fn get_subscription_ids(&self, connection_id: &ConnectionId) -> Vec<String> {
        self.connections
            .get(connection_id)
            .map(|c| c.subscriptions.iter().map(|e| e.key().clone()).collect())
            .unwrap_or_default()
    }

    /// Get subscriptions for a specific (user, table) pair
    /// 
    /// O(1) lookup via DashMap - returns pre-built states, no iteration needed
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

    /// Check if any subscriptions exist for a table (by table name substring match)
    pub fn has_subscriptions_for_table(&self, table_ref: &str) -> bool {
        self.user_table_subscriptions.iter().any(|entry| {
            entry.key().1.to_string().contains(table_ref)
        })
    }

    // ==================== Notification Delivery ====================

    /// Send notification to a specific subscription
    pub fn notify_subscription(&self, live_id: &LiveQueryId, notification: Notification) {
        if let Some(conn_id) = self.live_id_to_connection.get(live_id) {
            if let Some(conn) = self.connections.get(conn_id.value()) {
                let _ = conn.notification_tx.send((live_id.clone(), notification));
            }
        }
    }

    /// Send notification to all subscriptions for a table (for a specific user)
    /// 
    /// O(1) lookup via pre-built index
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
        info!(
            "Initiating WebSocket shutdown with {} active connections",
            count
        );

        self.is_shutting_down.store(true, Ordering::Release);

        // Send shutdown event to all connections
        for entry in self.connections.iter() {
            let _ = entry.value().event_tx.send(ConnectionEvent::Shutdown);
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

    /// Get connection count
    pub fn connection_count(&self) -> usize {
        self.total_connections.load(Ordering::Acquire)
    }

    /// Get subscription count
    pub fn subscription_count(&self) -> usize {
        self.total_subscriptions.load(Ordering::Acquire)
    }

    /// Get node ID
    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    // Alias for backward compatibility
    pub fn total_connections(&self) -> usize {
        self.connection_count()
    }

    pub fn total_subscriptions(&self) -> usize {
        self.subscription_count()
    }

    // ==================== Background Tasks ====================

    /// Background heartbeat checker task
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

    /// Check all connections for timeouts
    fn check_all_connections(&self) {
        let now = Instant::now();
        let mut to_timeout = Vec::new();

        for entry in self.connections.iter() {
            let conn_id = entry.key();
            let state = entry.value();

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

impl Drop for ConnectionRegistry {
    fn drop(&mut self) {
        self.shutdown_token.cancel();
        debug!("ConnectionRegistry dropped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_registry() -> Arc<ConnectionRegistry> {
        ConnectionRegistry::new(
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

        let _reg = registry.register_connection(conn_id.clone(), None);
        assert!(!registry.is_authenticated(&conn_id));

        registry.mark_auth_started(&conn_id);
        registry.mark_authenticated(&conn_id, user_id.clone());

        assert!(registry.is_authenticated(&conn_id));
        assert_eq!(registry.get_user_id(&conn_id), Some(user_id));
    }

    #[tokio::test]
    async fn test_subscription_management() {
        let registry = create_test_registry();
        let conn_id = ConnectionId::new("conn1");
        let user_id = UserId::new("user1");
        let table_id = TableId::from_strings("ns1", "table1");

        let _reg = registry.register_connection(conn_id.clone(), None);
        registry.mark_authenticated(&conn_id, user_id.clone());

        let live_id = LiveQueryId::new(user_id.clone(), conn_id.clone(), "sub1");

        // Add subscription
        registry
            .add_subscription(&conn_id, live_id.clone(), table_id.clone(), Default::default())
            .unwrap();
        assert_eq!(registry.subscription_count(), 1);
        assert!(registry.has_subscription(&conn_id, "sub1"));

        // Remove subscription
        registry.remove_subscription(&live_id);
        assert_eq!(registry.subscription_count(), 0);
        assert!(!registry.has_subscription(&conn_id, "sub1"));
    }

    #[tokio::test]
    async fn test_reject_during_shutdown() {
        let registry = create_test_registry();

        registry.is_shutting_down.store(true, Ordering::Release);

        let reg = registry.register_connection(ConnectionId::new("conn1"), None);
        assert!(reg.is_none());
    }
}
