//! Live query module for subscription management
//!
//! This module handles WebSocket-based live query subscriptions and
//! real-time change notifications.
//!
//! Moved from kalamdb-live crate to kalamdb-core to avoid circular dependencies.

pub mod connection_registry;
pub mod error;
pub mod filter;
pub mod initial_data;
pub mod manager;

pub use connection_registry::{
    ConnectionId, LiveId, LiveQueryOptions, LiveQueryRegistry, NodeId,
    SubscriptionHandle, UserId,
};
pub use filter::{FilterCache, FilterPredicate};
pub use initial_data::{InitialDataFetcher, InitialDataOptions, InitialDataResult};
pub use manager::{
    ChangeNotification, ChangeType, LiveQueryManager, RegistryStats, SubscriptionResult,
};
