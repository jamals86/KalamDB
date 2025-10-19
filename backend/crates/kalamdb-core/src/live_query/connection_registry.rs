//! In-memory WebSocket connection registry
//!
//! This module provides the core data structures for managing WebSocket connections
//! and their live query subscriptions in memory.

use crate::error::KalamDbError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// Node identifier for cluster deployments
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub String);

impl NodeId {
    pub fn new(id: String) -> Self {
        Self(id)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for NodeId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// User identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserId(pub String);

impl UserId {
    pub fn new(id: String) -> Self {
        Self(id)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for UserId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Connection identifier: {user_id}-{unique_conn_id}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectionId {
    pub user_id: String,
    pub unique_conn_id: String,
}

impl ConnectionId {
    /// Create a new connection ID
    pub fn new(user_id: String, unique_conn_id: String) -> Self {
        Self {
            user_id,
            unique_conn_id,
        }
    }

    /// Parse from string format: {user_id}-{unique_conn_id}
    pub fn from_string(s: &str) -> Result<Self, KalamDbError> {
        let parts: Vec<&str> = s.splitn(2, '-').collect();
        if parts.len() != 2 {
            return Err(KalamDbError::InvalidOperation(format!(
                "Invalid connection_id format: {}. Expected: {{user_id}}-{{unique_conn_id}}",
                s
            )));
        }
        Ok(Self {
            user_id: parts[0].to_string(),
            unique_conn_id: parts[1].to_string(),
        })
    }

    /// Serialize to string format
    pub fn to_string(&self) -> String {
        format!("{}-{}", self.user_id, self.unique_conn_id)
    }

    /// Get user_id component
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    /// Get unique_conn_id component
    pub fn unique_conn_id(&self) -> &str {
        &self.unique_conn_id
    }
}

impl fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.user_id, self.unique_conn_id)
    }
}

/// Live query identifier: {connection_id}-{table_name}-{query_id}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LiveId {
    pub connection_id: ConnectionId,
    pub table_name: String,
    pub query_id: String,
}

impl LiveId {
    /// Create a new live query ID
    pub fn new(connection_id: ConnectionId, table_name: String, query_id: String) -> Self {
        Self {
            connection_id,
            table_name,
            query_id,
        }
    }

    /// Parse from string format: {user_id}-{unique_conn_id}-{table_name}-{query_id}
    pub fn from_string(s: &str) -> Result<Self, KalamDbError> {
        let parts: Vec<&str> = s.splitn(4, '-').collect();
        if parts.len() != 4 {
            return Err(KalamDbError::InvalidOperation(format!(
                "Invalid live_id format: {}. Expected: {{user_id}}-{{unique_conn_id}}-{{table_name}}-{{query_id}}",
                s
            )));
        }
        Ok(Self {
            connection_id: ConnectionId {
                user_id: parts[0].to_string(),
                unique_conn_id: parts[1].to_string(),
            },
            table_name: parts[2].to_string(),
            query_id: parts[3].to_string(),
        })
    }

    /// Serialize to string format
    pub fn to_string(&self) -> String {
        format!(
            "{}-{}-{}-{}",
            self.connection_id.user_id,
            self.connection_id.unique_conn_id,
            self.table_name,
            self.query_id
        )
    }

    /// Get connection_id component
    pub fn connection_id(&self) -> &ConnectionId {
        &self.connection_id
    }

    /// Get table_name component
    pub fn table_name(&self) -> &str {
        &self.table_name
    }

    /// Get query_id component
    pub fn query_id(&self) -> &str {
        &self.query_id
    }

    /// Get user_id from connection_id
    pub fn user_id(&self) -> &str {
        &self.connection_id.user_id
    }
}

impl fmt::Display for LiveId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}-{}-{}-{}",
            self.connection_id.user_id,
            self.connection_id.unique_conn_id,
            self.table_name,
            self.query_id
        )
    }
}

/// Live query options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveQueryOptions {
    /// Number of last rows to fetch for initial data
    pub last_rows: Option<u32>,
}

impl Default for LiveQueryOptions {
    fn default() -> Self {
        Self { last_rows: None }
    }
}

/// Live query subscription
#[derive(Debug, Clone)]
pub struct LiveQuery {
    pub live_id: LiveId,
    pub query: String,
    pub options: LiveQueryOptions,
    pub changes: u64,
}

/// Represents a connected WebSocket with all its subscriptions
/// Note: Actix actor address will be added when integrating with WebSocket layer
pub struct UserConnectionSocket {
    pub connection_id: ConnectionId,
    // TODO: Add actor: Addr<WebSocketSession> when integrating with actix-web-actors
    pub live_queries: HashMap<LiveId, LiveQuery>,
}

impl UserConnectionSocket {
    /// Create a new user connection socket
    pub fn new(connection_id: ConnectionId) -> Self {
        Self {
            connection_id,
            live_queries: HashMap::new(),
        }
    }

    /// Add a live query subscription
    pub fn add_live_query(&mut self, live_query: LiveQuery) {
        self.live_queries.insert(live_query.live_id.clone(), live_query);
    }

    /// Remove a live query subscription
    pub fn remove_live_query(&mut self, live_id: &LiveId) -> Option<LiveQuery> {
        self.live_queries.remove(live_id)
    }

    /// Get all live query IDs
    pub fn live_query_ids(&self) -> Vec<LiveId> {
        self.live_queries.keys().cloned().collect()
    }

    /// Get live queries for a specific table
    pub fn live_queries_for_table(&self, table_name: &str) -> Vec<&LiveQuery> {
        self.live_queries
            .values()
            .filter(|lq| lq.live_id.table_name == table_name)
            .collect()
    }
}

/// User connections container
pub struct UserConnections {
    pub sockets: HashMap<ConnectionId, UserConnectionSocket>,
}

impl UserConnections {
    /// Create a new user connections container
    pub fn new() -> Self {
        Self {
            sockets: HashMap::new(),
        }
    }

    /// Add a connection socket
    pub fn add_socket(&mut self, socket: UserConnectionSocket) {
        self.sockets.insert(socket.connection_id.clone(), socket);
    }

    /// Remove a connection socket
    pub fn remove_socket(&mut self, connection_id: &ConnectionId) -> Option<UserConnectionSocket> {
        self.sockets.remove(connection_id)
    }

    /// Get a connection socket
    pub fn get_socket(&self, connection_id: &ConnectionId) -> Option<&UserConnectionSocket> {
        self.sockets.get(connection_id)
    }

    /// Get a mutable connection socket
    pub fn get_socket_mut(&mut self, connection_id: &ConnectionId) -> Option<&mut UserConnectionSocket> {
        self.sockets.get_mut(connection_id)
    }

    /// Get all connection IDs
    pub fn connection_ids(&self) -> Vec<ConnectionId> {
        self.sockets.keys().cloned().collect()
    }
}

impl Default for UserConnections {
    fn default() -> Self {
        Self::new()
    }
}

/// In-memory registry for WebSocket connections and subscriptions
pub struct LiveQueryRegistry {
    pub users: HashMap<UserId, UserConnections>,
    pub node_id: NodeId,
}

impl LiveQueryRegistry {
    /// Create a new live query registry
    pub fn new(node_id: NodeId) -> Self {
        Self {
            users: HashMap::new(),
            node_id,
        }
    }

    /// Register a new WebSocket connection
    pub fn register_connection(&mut self, user_id: UserId, connection_id: ConnectionId) {
        let socket = UserConnectionSocket::new(connection_id);
        self.users
            .entry(user_id)
            .or_insert_with(UserConnections::new)
            .add_socket(socket);
    }

    /// Register a live query subscription
    pub fn register_subscription(&mut self, user_id: &UserId, live_query: LiveQuery) -> Result<(), KalamDbError> {
        let user_connections = self.users.get_mut(user_id).ok_or_else(|| {
            KalamDbError::NotFound(format!("User not connected: {}", user_id))
        })?;

        let connection_id = &live_query.live_id.connection_id;
        let socket = user_connections.get_socket_mut(connection_id).ok_or_else(|| {
            KalamDbError::NotFound(format!("Connection not found: {}", connection_id))
        })?;

        socket.add_live_query(live_query);
        Ok(())
    }

    /// Get all subscriptions for a specific table and user
    pub fn get_subscriptions_for_table(&self, user_id: &UserId, table_name: &str) -> Vec<&LiveQuery> {
        if let Some(user_connections) = self.users.get(user_id) {
            user_connections
                .sockets
                .values()
                .flat_map(|socket| socket.live_queries_for_table(table_name))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Unregister a WebSocket connection and return all its live query IDs
    pub fn unregister_connection(&mut self, user_id: &UserId, connection_id: &ConnectionId) -> Vec<LiveId> {
        if let Some(user_connections) = self.users.get_mut(user_id) {
            if let Some(socket) = user_connections.remove_socket(connection_id) {
                return socket.live_query_ids();
            }
        }
        Vec::new()
    }

    /// Unregister a single live query subscription
    pub fn unregister_subscription(&mut self, live_id: &LiveId) -> Option<ConnectionId> {
        let user_id = UserId::new(live_id.user_id().to_string());
        if let Some(user_connections) = self.users.get_mut(&user_id) {
            if let Some(socket) = user_connections.get_socket_mut(&live_id.connection_id) {
                socket.remove_live_query(live_id)?;
                return Some(live_id.connection_id.clone());
            }
        }
        None
    }

    /// Get the node ID
    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    /// Get total number of active connections
    pub fn total_connections(&self) -> usize {
        self.users.values().map(|uc| uc.sockets.len()).sum()
    }

    /// Get total number of active subscriptions
    pub fn total_subscriptions(&self) -> usize {
        self.users
            .values()
            .flat_map(|uc| uc.sockets.values())
            .map(|socket| socket.live_queries.len())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_id_format() {
        let conn_id = ConnectionId::new("user123".to_string(), "conn_abc".to_string());
        assert_eq!(conn_id.to_string(), "user123-conn_abc");
        assert_eq!(conn_id.user_id(), "user123");
        assert_eq!(conn_id.unique_conn_id(), "conn_abc");
    }

    #[test]
    fn test_connection_id_parse() {
        let conn_id = ConnectionId::from_string("user123-conn_abc").unwrap();
        assert_eq!(conn_id.user_id, "user123");
        assert_eq!(conn_id.unique_conn_id, "conn_abc");
    }

    #[test]
    fn test_live_id_format() {
        let conn_id = ConnectionId::new("user123".to_string(), "conn_abc".to_string());
        let live_id = LiveId::new(conn_id, "messages".to_string(), "q1".to_string());
        assert_eq!(live_id.to_string(), "user123-conn_abc-messages-q1");
        assert_eq!(live_id.table_name(), "messages");
        assert_eq!(live_id.query_id(), "q1");
        assert_eq!(live_id.user_id(), "user123");
    }

    #[test]
    fn test_live_id_parse() {
        let live_id = LiveId::from_string("user123-conn_abc-messages-q1").unwrap();
        assert_eq!(live_id.connection_id.user_id, "user123");
        assert_eq!(live_id.connection_id.unique_conn_id, "conn_abc");
        assert_eq!(live_id.table_name, "messages");
        assert_eq!(live_id.query_id, "q1");
    }

    #[test]
    fn test_registry_register_connection() {
        let mut registry = LiveQueryRegistry::new(NodeId::new("node1".to_string()));
        let user_id = UserId::new("user1".to_string());
        let conn_id = ConnectionId::new("user1".to_string(), "conn1".to_string());
        
        registry.register_connection(user_id.clone(), conn_id.clone());
        
        assert_eq!(registry.total_connections(), 1);
        assert_eq!(registry.total_subscriptions(), 0);
    }

    #[test]
    fn test_registry_register_subscription() {
        let mut registry = LiveQueryRegistry::new(NodeId::new("node1".to_string()));
        let user_id = UserId::new("user1".to_string());
        let conn_id = ConnectionId::new("user1".to_string(), "conn1".to_string());
        
        registry.register_connection(user_id.clone(), conn_id.clone());
        
        let live_id = LiveId::new(conn_id.clone(), "messages".to_string(), "q1".to_string());
        let live_query = LiveQuery {
            live_id: live_id.clone(),
            query: "SELECT * FROM messages".to_string(),
            options: LiveQueryOptions::default(),
            changes: 0,
        };
        
        registry.register_subscription(&user_id, live_query).unwrap();
        
        assert_eq!(registry.total_subscriptions(), 1);
    }

    #[test]
    fn test_registry_get_subscriptions_for_table() {
        let mut registry = LiveQueryRegistry::new(NodeId::new("node1".to_string()));
        let user_id = UserId::new("user1".to_string());
        let conn_id = ConnectionId::new("user1".to_string(), "conn1".to_string());
        
        registry.register_connection(user_id.clone(), conn_id.clone());
        
        let live_id1 = LiveId::new(conn_id.clone(), "messages".to_string(), "q1".to_string());
        let live_query1 = LiveQuery {
            live_id: live_id1.clone(),
            query: "SELECT * FROM messages".to_string(),
            options: LiveQueryOptions::default(),
            changes: 0,
        };
        
        let live_id2 = LiveId::new(conn_id.clone(), "notifications".to_string(), "q2".to_string());
        let live_query2 = LiveQuery {
            live_id: live_id2.clone(),
            query: "SELECT * FROM notifications".to_string(),
            options: LiveQueryOptions::default(),
            changes: 0,
        };
        
        registry.register_subscription(&user_id, live_query1).unwrap();
        registry.register_subscription(&user_id, live_query2).unwrap();
        
        let messages_subs = registry.get_subscriptions_for_table(&user_id, "messages");
        assert_eq!(messages_subs.len(), 1);
        assert_eq!(messages_subs[0].live_id.table_name(), "messages");
    }

    #[test]
    fn test_registry_unregister_connection() {
        let mut registry = LiveQueryRegistry::new(NodeId::new("node1".to_string()));
        let user_id = UserId::new("user1".to_string());
        let conn_id = ConnectionId::new("user1".to_string(), "conn1".to_string());
        
        registry.register_connection(user_id.clone(), conn_id.clone());
        
        let live_id1 = LiveId::new(conn_id.clone(), "messages".to_string(), "q1".to_string());
        let live_query1 = LiveQuery {
            live_id: live_id1.clone(),
            query: "SELECT * FROM messages".to_string(),
            options: LiveQueryOptions::default(),
            changes: 0,
        };
        
        let live_id2 = LiveId::new(conn_id.clone(), "notifications".to_string(), "q2".to_string());
        let live_query2 = LiveQuery {
            live_id: live_id2.clone(),
            query: "SELECT * FROM notifications".to_string(),
            options: LiveQueryOptions::default(),
            changes: 0,
        };
        
        registry.register_subscription(&user_id, live_query1).unwrap();
        registry.register_subscription(&user_id, live_query2).unwrap();
        
        let removed_live_ids = registry.unregister_connection(&user_id, &conn_id);
        assert_eq!(removed_live_ids.len(), 2);
        assert_eq!(registry.total_connections(), 0);
        assert_eq!(registry.total_subscriptions(), 0);
    }

    #[test]
    fn test_registry_unregister_subscription() {
        let mut registry = LiveQueryRegistry::new(NodeId::new("node1".to_string()));
        let user_id = UserId::new("user1".to_string());
        let conn_id = ConnectionId::new("user1".to_string(), "conn1".to_string());
        
        registry.register_connection(user_id.clone(), conn_id.clone());
        
        let live_id = LiveId::new(conn_id.clone(), "messages".to_string(), "q1".to_string());
        let live_query = LiveQuery {
            live_id: live_id.clone(),
            query: "SELECT * FROM messages".to_string(),
            options: LiveQueryOptions::default(),
            changes: 0,
        };
        
        registry.register_subscription(&user_id, live_query).unwrap();
        
        let removed_conn_id = registry.unregister_subscription(&live_id);
        assert!(removed_conn_id.is_some());
        assert_eq!(registry.total_subscriptions(), 0);
    }

    #[test]
    fn test_user_connection_socket() {
        let conn_id = ConnectionId::new("user1".to_string(), "conn1".to_string());
        let mut socket = UserConnectionSocket::new(conn_id.clone());
        
        let live_id = LiveId::new(conn_id.clone(), "messages".to_string(), "q1".to_string());
        let live_query = LiveQuery {
            live_id: live_id.clone(),
            query: "SELECT * FROM messages".to_string(),
            options: LiveQueryOptions::default(),
            changes: 0,
        };
        
        socket.add_live_query(live_query);
        
        let table_queries = socket.live_queries_for_table("messages");
        assert_eq!(table_queries.len(), 1);
        
        let removed = socket.remove_live_query(&live_id);
        assert!(removed.is_some());
        assert_eq!(socket.live_queries.len(), 0);
    }
}
