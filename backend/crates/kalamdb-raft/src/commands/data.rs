//! Data shard commands (user tables, shared tables, live queries)
//!
//! Each data command carries a `required_meta_index` watermark, which is the
//! `Meta` group's last applied index on the leader at proposal time. Followers
//! buffer data commands until local `Meta` has applied at least that index.

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
///
/// Each variant carries `required_meta_index` for watermark-based ordering.
/// Followers must buffer commands until `Meta.last_applied_index() >= required_meta_index`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UserDataCommand {
    // === User Table Data ===
    
    /// Insert rows into a user table
    Insert {
        /// Watermark: Meta group's last_applied_index at proposal time
        required_meta_index: u64,
        table_id: TableId,
        user_id: UserId,
        /// Serialized rows (Arrow IPC or custom format)
        rows_data: Vec<u8>,
    },

    /// Update rows in a user table
    Update {
        /// Watermark: Meta group's last_applied_index at proposal time
        required_meta_index: u64,
        table_id: TableId,
        user_id: UserId,
        /// Serialized updates
        updates_data: Vec<u8>,
        /// Optional filter (serialized)
        filter_data: Option<Vec<u8>>,
    },

    /// Delete rows from a user table
    Delete {
        /// Watermark: Meta group's last_applied_index at proposal time
        required_meta_index: u64,
        table_id: TableId,
        user_id: UserId,
        /// Optional filter (serialized)
        filter_data: Option<Vec<u8>>,
    },

    // === Live Query Subscriptions ===
    
    /// Register a new live query subscription
    RegisterLiveQuery {
        /// Watermark: Meta group's last_applied_index at proposal time
        required_meta_index: u64,
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
        /// Watermark: Meta group's last_applied_index at proposal time
        required_meta_index: u64,
        subscription_id: String,
        user_id: UserId,
    },

    /// Clean up all subscriptions from a failed node
    CleanupNodeSubscriptions {
        /// Watermark: Meta group's last_applied_index at proposal time
        required_meta_index: u64,
        user_id: UserId,
        failed_node_id: NodeId,
    },

    /// Heartbeat to keep subscription alive
    PingLiveQuery {
        /// Watermark: Meta group's last_applied_index at proposal time
        required_meta_index: u64,
        subscription_id: String,
        user_id: UserId,
        pinged_at: DateTime<Utc>,
    },
}

impl UserDataCommand {
    /// Get the required_meta_index watermark for this command
    pub fn required_meta_index(&self) -> u64 {
        match self {
            UserDataCommand::Insert { required_meta_index, .. } => *required_meta_index,
            UserDataCommand::Update { required_meta_index, .. } => *required_meta_index,
            UserDataCommand::Delete { required_meta_index, .. } => *required_meta_index,
            UserDataCommand::RegisterLiveQuery { required_meta_index, .. } => *required_meta_index,
            UserDataCommand::UnregisterLiveQuery { required_meta_index, .. } => *required_meta_index,
            UserDataCommand::CleanupNodeSubscriptions { required_meta_index, .. } => *required_meta_index,
            UserDataCommand::PingLiveQuery { required_meta_index, .. } => *required_meta_index,
        }
    }
    
    /// Set the required_meta_index watermark for this command
    pub fn set_required_meta_index(&mut self, index: u64) {
        match self {
            UserDataCommand::Insert { required_meta_index, .. } => *required_meta_index = index,
            UserDataCommand::Update { required_meta_index, .. } => *required_meta_index = index,
            UserDataCommand::Delete { required_meta_index, .. } => *required_meta_index = index,
            UserDataCommand::RegisterLiveQuery { required_meta_index, .. } => *required_meta_index = index,
            UserDataCommand::UnregisterLiveQuery { required_meta_index, .. } => *required_meta_index = index,
            UserDataCommand::CleanupNodeSubscriptions { required_meta_index, .. } => *required_meta_index = index,
            UserDataCommand::PingLiveQuery { required_meta_index, .. } => *required_meta_index = index,
        }
    }
    
    /// Get the user_id for routing to the correct shard
    pub fn user_id(&self) -> &UserId {
        match self {
            UserDataCommand::Insert { user_id, .. } => user_id,
            UserDataCommand::Update { user_id, .. } => user_id,
            UserDataCommand::Delete { user_id, .. } => user_id,
            UserDataCommand::RegisterLiveQuery { user_id, .. } => user_id,
            UserDataCommand::UnregisterLiveQuery { user_id, .. } => user_id,
            UserDataCommand::CleanupNodeSubscriptions { user_id, .. } => user_id,
            UserDataCommand::PingLiveQuery { user_id, .. } => user_id,
        }
    }
}

/// Commands for shared data shards (1 shard by default)
///
/// Handles: shared table INSERT/UPDATE/DELETE operations
///
/// Routing: Phase 1 uses single shard; future may shard by table_id
///
/// Each variant carries `required_meta_index` for watermark-based ordering.
/// Followers must buffer commands until `Meta.last_applied_index() >= required_meta_index`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SharedDataCommand {
    /// Insert rows into a shared table
    Insert {
        /// Watermark: Meta group's last_applied_index at proposal time
        required_meta_index: u64,
        table_id: TableId,
        /// Serialized rows
        rows_data: Vec<u8>,
    },

    /// Update rows in a shared table
    Update {
        /// Watermark: Meta group's last_applied_index at proposal time
        required_meta_index: u64,
        table_id: TableId,
        /// Serialized updates
        updates_data: Vec<u8>,
        /// Optional filter (serialized)
        filter_data: Option<Vec<u8>>,
    },

    /// Delete rows from a shared table
    Delete {
        /// Watermark: Meta group's last_applied_index at proposal time
        required_meta_index: u64,
        table_id: TableId,
        /// Optional filter (serialized)
        filter_data: Option<Vec<u8>>,
    },
}

impl SharedDataCommand {
    /// Get the required_meta_index watermark for this command
    pub fn required_meta_index(&self) -> u64 {
        match self {
            SharedDataCommand::Insert { required_meta_index, .. } => *required_meta_index,
            SharedDataCommand::Update { required_meta_index, .. } => *required_meta_index,
            SharedDataCommand::Delete { required_meta_index, .. } => *required_meta_index,
        }
    }
    
    /// Set the required_meta_index watermark for this command
    pub fn set_required_meta_index(&mut self, index: u64) {
        match self {
            SharedDataCommand::Insert { required_meta_index, .. } => *required_meta_index = index,
            SharedDataCommand::Update { required_meta_index, .. } => *required_meta_index = index,
            SharedDataCommand::Delete { required_meta_index, .. } => *required_meta_index = index,
        }
    }
    
    /// Get the table_id for this command
    pub fn table_id(&self) -> &TableId {
        match self {
            SharedDataCommand::Insert { table_id, .. } => table_id,
            SharedDataCommand::Update { table_id, .. } => table_id,
            SharedDataCommand::Delete { table_id, .. } => table_id,
        }
    }
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
