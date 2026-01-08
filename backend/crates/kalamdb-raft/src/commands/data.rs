//! Data shard commands (user tables, shared tables, live queries)

use chrono::{DateTime, Utc};
use kalamdb_commons::models::{NodeId, UserId};
use kalamdb_commons::TableId;
use serde::{Deserialize, Serialize};

/// Commands for user data shards (32 shards by default)
///
/// Handles:
/// - User table INSERT/UPDATE/DELETE operations
/// - Live query subscriptions (per-user)
///
/// Routing: user_id % num_user_shards
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UserDataCommand {
    // === User Table Data ===
    
    /// Insert rows into a user table
    Insert {
        table_id: TableId,
        user_id: UserId,
        /// Serialized rows (Arrow IPC or custom format)
        rows_data: Vec<u8>,
    },

    /// Update rows in a user table
    Update {
        table_id: TableId,
        user_id: UserId,
        /// Serialized updates
        updates_data: Vec<u8>,
        /// Optional filter (serialized)
        filter_data: Option<Vec<u8>>,
    },

    /// Delete rows from a user table
    Delete {
        table_id: TableId,
        user_id: UserId,
        /// Optional filter (serialized)
        filter_data: Option<Vec<u8>>,
    },

    // === Live Query Subscriptions ===
    
    /// Register a new live query subscription
    RegisterLiveQuery {
        subscription_id: String,
        user_id: UserId,
        query_hash: String,
        table_id: TableId,
        filter_json: Option<String>,
        /// Which node holds the WebSocket connection
        node_id: NodeId,
        created_at: DateTime<Utc>,
    },

    /// Unregister a live query subscription
    UnregisterLiveQuery {
        subscription_id: String,
        user_id: UserId,
    },

    /// Clean up all subscriptions from a failed node
    CleanupNodeSubscriptions {
        user_id: UserId,
        failed_node_id: NodeId,
    },

    /// Heartbeat to keep subscription alive
    PingLiveQuery {
        subscription_id: String,
        user_id: UserId,
        pinged_at: DateTime<Utc>,
    },
}

/// Commands for shared data shards (1 shard by default)
///
/// Handles: shared table INSERT/UPDATE/DELETE operations
///
/// Routing: Phase 1 uses single shard; future may shard by table_id
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SharedDataCommand {
    /// Insert rows into a shared table
    Insert {
        table_id: TableId,
        /// Serialized rows
        rows_data: Vec<u8>,
    },

    /// Update rows in a shared table
    Update {
        table_id: TableId,
        /// Serialized updates
        updates_data: Vec<u8>,
        /// Optional filter (serialized)
        filter_data: Option<Vec<u8>>,
    },

    /// Delete rows from a shared table
    Delete {
        table_id: TableId,
        /// Optional filter (serialized)
        filter_data: Option<Vec<u8>>,
    },
}

/// Response for data operations
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum DataResponse {
    #[default]
    Ok,

    /// Number of rows affected by the operation
    RowsAffected(usize),

    /// Subscription registered
    Subscribed {
        subscription_id: String,
    },

    /// Error
    Error {
        message: String,
    },
}

impl DataResponse {
    /// Create an error response with the given message
    pub fn error(msg: impl Into<String>) -> Self {
        Self::Error { message: msg.into() }
    }

    /// Returns true if this is not an error response
    pub fn is_ok(&self) -> bool {
        !matches!(self, Self::Error { .. })
    }

    /// Returns the number of rows affected, or 0 if not applicable
    pub fn rows_affected(&self) -> usize {
        match self {
            DataResponse::RowsAffected(n) => *n,
            _ => 0,
        }
    }
}
