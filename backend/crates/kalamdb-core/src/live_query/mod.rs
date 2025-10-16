//! Live query module for subscription management
//!
//! This module handles WebSocket-based live query subscriptions and
//! real-time change notifications.

pub mod connection_registry;
pub mod manager;

pub use connection_registry::{
    ConnectionId, LiveId, LiveQuery, LiveQueryOptions, LiveQueryRegistry,
    NodeId, UserConnectionSocket, UserConnections, UserId,
};
pub use manager::{LiveQueryManager, RegistryStats};
