//! Live query module for subscription management
//!
//! This module handles WebSocket-based live query subscriptions and
//! real-time change notifications.
//!
//! Live query notifications are now handled through Raft-replicated data appliers.
//! When data is applied on any node (leader or follower), the provider's insert/update/delete
//! methods fire local notifications to connected WebSocket clients.

pub mod connections_manager;
pub mod error;
pub mod failover;
pub mod filter_eval;
pub mod initial_data;
pub mod manager;
pub mod notification;
pub mod query_parser;
pub mod subscription;
pub mod types;

// Re-export types from kalamdb-commons (canonical source)
pub use kalamdb_commons::models::{ConnectionId, LiveQueryId, TableId, UserId};
pub use kalamdb_commons::NodeId;

// Re-export from connections_manager (consolidated connection management)
pub use connections_manager::{
    ConnectionEvent, ConnectionRegistration, ConnectionState, ConnectionsManager,
    NotificationSender, SharedConnectionState, SubscriptionState,
};

pub use failover::{CleanupReport as LiveQueryCleanupReport, LiveQueryFailoverHandler};
pub use filter_eval::{matches as filter_matches, parse_where_clause};
pub use initial_data::{InitialDataFetcher, InitialDataOptions, InitialDataResult};
pub use manager::LiveQueryManager;
pub use notification::NotificationService;
pub use query_parser::QueryParser;
pub use subscription::{RegisteredSubscription, SubscriptionService};
pub use types::{ChangeNotification, ChangeType, RegistryStats, SubscriptionResult};
