//! Connections Manager
//!
//! Unified manager for both WebSocket and consumer connections:
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
//!
//! Used by:
//! - WebSocket handlers for live queries
//! - Topic consumer handlers for pub/sub

use super::super::models::{
    ConnectionEvent, ConnectionRegistration, ConnectionState, SharedConnectionState,
    SubscriptionHandle, EVENT_CHANNEL_CAPACITY, NOTIFICATION_CHANNEL_CAPACITY,
};
use dashmap::DashMap;
use kalamdb_commons::models::{ConnectionId, ConnectionInfo, LiveQueryId, TableId, UserId};
use kalamdb_commons::{NodeId, Notification};
use log::{debug, info, warn};
use parking_lot::RwLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Connections Manager
///
/// Responsibilities:
/// - Connection lifecycle (register, unregister, heartbeat check)
/// - Authentication state management
/// - Maintaining indices for efficient lookups
/// - Background heartbeat checker
///
/// Used by both:
/// - WebSocket handlers for live queries
/// - HTTP handlers for topic consumer long polling
///
/// NOTE: Subscription management is delegated to LiveQueryManager and TopicConsumerManager.
/// The manager only maintains indices for notification routing.
pub struct ConnectionsManager {
    // === Primary Storage ===
    /// All active connections: ConnectionId → SharedConnectionState
    connections: DashMap<ConnectionId, SharedConnectionState>,

    // === Secondary Indices (for efficient lookups) ===
    /// (UserId, TableId) → DashMap<LiveQueryId, SubscriptionHandle> for O(1) notification delivery
    /// Uses lightweight handles (~48 bytes) instead of full state (~800+ bytes)
    user_table_subscriptions:
        DashMap<(UserId, TableId), Arc<DashMap<LiveQueryId, SubscriptionHandle>>>,
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
            connections: DashMap::new(),
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
            connection_id,
            state: shared_state,
            notification_rx,
            event_rx,
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
        // let span = tracing::debug_span!(
        //     "live_query.push",
        //     live_id = %live_id,
        //     delivered = Empty
        // );
        // let _span_guard = span.entered();
        // let mut delivered = false;

        if let Some(conn_id) = self.live_id_to_connection.get(live_id) {
            if let Some(shared_state) = self.connections.get(conn_id.value()) {
                let state = shared_state.read();
                let notification = Arc::new(notification);
                if let Err(e) = state.notification_tx.try_send(notification) {
                    if matches!(e, mpsc::error::TrySendError::Full(_)) {
                        warn!("Notification channel full for {}, dropping notification", live_id);
                    }
                }
                //else {
                //    delivered = true;
                //}
            }
        }

        // tracing::Span::current().record("delivered", delivered);
    }

    /// Send notification to all subscriptions for a table (for a specific user)
    #[inline]
    pub fn notify_table_for_user(
        &self,
        user_id: &UserId,
        table_id: &TableId,
        notification: Notification,
    ) {
        // let span = tracing::debug_span!(
        //     "live_query.push",
        //     user_id = %user_id,
        //     table_id = %table_id,
        //     delivered_count = Empty,
        //     dropped_count = Empty
        // );
        // let _span_guard = span.entered();

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
