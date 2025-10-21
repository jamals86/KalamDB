//! Live query module for subscription management
//!
//! This module handles WebSocket-based live query subscriptions and
//! real-time change notifications.

pub mod change_detector;
pub mod connection_registry;
pub mod filter;
pub mod initial_data;
pub mod manager;

pub use change_detector::{SharedTableChangeDetector, UserTableChangeDetector};
pub use connection_registry::{
    ConnectionId, LiveId, LiveQuery, LiveQueryOptions, LiveQueryRegistry, NodeId,
    UserConnectionSocket, UserConnections, UserId,
};
pub use filter::{FilterCache, FilterPredicate};
pub use initial_data::{InitialDataFetcher, InitialDataOptions, InitialDataResult};
pub use manager::{
    ChangeNotification, ChangeType, LiveQueryManager, RegistryStats, SubscriptionResult,
};
