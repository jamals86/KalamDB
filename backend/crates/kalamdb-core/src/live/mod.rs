//! Live query module for subscription management
//!
//! This module handles WebSocket-based live query subscriptions and
//! real-time change notifications.
//!
//! Moved from kalamdb-live crate to kalamdb-core to avoid circular dependencies.

pub mod error;
pub mod filter_eval;
pub mod initial_data;
pub mod manager;
pub mod notification;
pub mod query_parser;
pub mod registry;
pub mod subscription;
pub mod types;

// Re-export types from kalamdb-commons (canonical source)
pub use kalamdb_commons::models::{ConnectionId, LiveQueryId, TableId, UserId};
pub use kalamdb_commons::NodeId;

// Re-export from registry (consolidated connection management)
pub use registry::{
    ConnectionEvent, ConnectionRegistration, ConnectionsManager, ConnectionState,
    NotificationSender, SubscriptionState,
};

pub use filter_eval::{matches as filter_matches, parse_where_clause};
pub use initial_data::{InitialDataFetcher, InitialDataOptions, InitialDataResult};
pub use manager::LiveQueryManager;
pub use notification::NotificationService;
pub use query_parser::QueryParser;
pub use registry::SharedConnectionState;
pub use subscription::{RegisteredSubscription, SubscriptionService};
pub use types::{ChangeNotification, ChangeType, RegistryStats, SubscriptionResult};

