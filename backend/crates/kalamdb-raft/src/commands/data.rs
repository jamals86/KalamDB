//! Data shard commands (user tables, shared tables, live queries)
//!
//! Each data command carries a `required_meta_index` watermark, which is the
//! `Meta` group's last applied index on the leader at proposal time. Followers
//! buffer data commands until local `Meta` has applied at least that index.

use chrono::{DateTime, Utc};
use kalamdb_commons::models::{NodeId, UserId, Row};
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
        /// Rows to insert
        rows: Vec<Row>,
    },

    /// Update rows in a user table
    Update {
        /// Watermark: Meta group's last_applied_index at proposal time
        required_meta_index: u64,
        table_id: TableId,
        user_id: UserId,
        /// Updates to apply
        updates: Vec<Row>,
        /// Optional filter (primary key value)
        filter: Option<String>,
    },

    /// Delete rows from a user table
    Delete {
        /// Watermark: Meta group's last_applied_index at proposal time
        required_meta_index: u64,
        table_id: TableId,
        user_id: UserId,
        /// Primary keys to delete
        pk_values: Option<Vec<String>>,
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
        /// Rows to insert
        rows: Vec<Row>,
    },

    /// Update rows in a shared table
    Update {
        /// Watermark: Meta group's last_applied_index at proposal time
        required_meta_index: u64,
        table_id: TableId,
        /// Updates to apply
        updates: Vec<Row>,
        /// Optional filter (primary key value)
        filter: Option<String>,
    },

    /// Delete rows from a shared table
    Delete {
        /// Watermark: Meta group's last_applied_index at proposal time
        required_meta_index: u64,
        table_id: TableId,
        /// Primary keys to delete
        pk_values: Option<Vec<String>>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::models::{NamespaceId, TableName};

    #[test]
    fn test_user_data_command_watermark_get_set() {
        let mut cmd = UserDataCommand::Insert {
            required_meta_index: 100,
            table_id: TableId::new(NamespaceId::from("ns"), TableName::from("table")),
            user_id: UserId::from("user_1"),
            rows: vec![],
        };

        assert_eq!(cmd.required_meta_index(), 100);
        cmd.set_required_meta_index(200);
        assert_eq!(cmd.required_meta_index(), 200);
    }

    #[test]
    fn test_user_data_command_user_id() {
        let user_id = UserId::from("user_123");
        let cmd = UserDataCommand::Delete {
            required_meta_index: 50,
            table_id: TableId::new(NamespaceId::from("ns"), TableName::from("table")),
            user_id: user_id.clone(),
            pk_values: None,
        };

        assert_eq!(cmd.user_id(), &user_id);
    }

    #[test]
    fn test_shared_data_command_watermark() {
        let mut cmd = SharedDataCommand::Update {
            required_meta_index: 300,
            table_id: TableId::new(NamespaceId::from("ns"), TableName::from("shared_table")),
            updates: vec![],
            filter: None,
        };

        assert_eq!(cmd.required_meta_index(), 300);
        cmd.set_required_meta_index(400);
        assert_eq!(cmd.required_meta_index(), 400);
    }

    #[test]
    fn test_shared_data_command_table_id() {
        let table_id = TableId::new(NamespaceId::from("shared_ns"), TableName::from("shared_t"));
        let cmd = SharedDataCommand::Insert {
            required_meta_index: 10,
            table_id: table_id.clone(),
            rows: vec![],
        };

        assert_eq!(cmd.table_id(), &table_id);
    }

    #[test]
    fn test_data_response_is_ok() {
        assert!(DataResponse::Ok.is_ok());
        assert!(DataResponse::RowsAffected(5).is_ok());
        assert!(DataResponse::Subscribed { subscription_id: "sub_1".to_string() }.is_ok());
        assert!(!DataResponse::Error { message: "error".to_string() }.is_ok());
    }

    #[test]
    fn test_data_response_rows_affected() {
        assert_eq!(DataResponse::Ok.rows_affected(), 0);
        assert_eq!(DataResponse::RowsAffected(10).rows_affected(), 10);
        assert_eq!(DataResponse::Error { message: "err".to_string() }.rows_affected(), 0);
    }

    #[test]
    fn test_data_response_error_constructor() {
        let response = DataResponse::error("test error");
        assert!(!response.is_ok());
        match response {
            DataResponse::Error { message } => assert_eq!(message, "test error"),
            _ => panic!("Expected error response"),
        }
    }

    #[test]
    fn test_user_command_serialization() {
        let cmd = UserDataCommand::Insert {
            required_meta_index: 123,
            table_id: TableId::new(NamespaceId::from("test_ns"), TableName::from("test_table")),
            user_id: UserId::from("user_456"),
            rows: vec![],
        };

        assert_eq!(cmd.required_meta_index(), 123);
        assert_eq!(cmd.user_id(), &UserId::from("user_456"));
    }

    #[test]
    fn test_shared_command_serialization() {
        let cmd = SharedDataCommand::Delete {
            required_meta_index: 999,
            table_id: TableId::new(NamespaceId::from("shared"), TableName::from("data")),
            pk_values: None,
        };

        assert_eq!(cmd.required_meta_index(), 999);
    }

    #[test]
    fn test_live_query_commands() {
        let now = Utc::now();
        let cmd = UserDataCommand::RegisterLiveQuery {
            required_meta_index: 50,
            subscription_id: "sub_123".to_string(),
            user_id: UserId::from("user_1"),
            query_hash: "hash_abc".to_string(),
            table_id: TableId::new(NamespaceId::from("ns"), TableName::from("table")),
            filter_json: Some(r#"{"id": 1}"#.to_string()),
            node_id: NodeId::from(1),
            created_at: now,
        };

        assert_eq!(cmd.required_meta_index(), 50);
        assert_eq!(cmd.user_id(), &UserId::from("user_1"));
    }

    #[test]
    fn test_all_user_command_variants_get_watermark() {
        let table_id = TableId::new(NamespaceId::from("n"), TableName::from("t"));
        let user_id = UserId::from("u");

        let commands = vec![
            UserDataCommand::Insert { required_meta_index: 1, table_id: table_id.clone(), user_id: user_id.clone(), rows: vec![] },
            UserDataCommand::Update { required_meta_index: 2, table_id: table_id.clone(), user_id: user_id.clone(), updates: vec![], filter: None },
            UserDataCommand::Delete { required_meta_index: 3, table_id: table_id.clone(), user_id: user_id.clone(), pk_values: None },
            UserDataCommand::RegisterLiveQuery { 
                required_meta_index: 4, subscription_id: "s".to_string(), user_id: user_id.clone(), 
                query_hash: "h".to_string(), table_id: table_id.clone(), filter_json: None, 
                node_id: NodeId::from(1), created_at: Utc::now() 
            },
            UserDataCommand::UnregisterLiveQuery { required_meta_index: 5, subscription_id: "s".to_string(), user_id: user_id.clone() },
            UserDataCommand::CleanupNodeSubscriptions { required_meta_index: 6, user_id: user_id.clone(), failed_node_id: NodeId::from(2) },
            UserDataCommand::PingLiveQuery { required_meta_index: 7, subscription_id: "s".to_string(), user_id: user_id.clone(), pinged_at: Utc::now() },
        ];

        for (i, cmd) in commands.iter().enumerate() {
            assert_eq!(cmd.required_meta_index(), (i + 1) as u64);
        }
    }

    #[test]
    fn test_all_shared_command_variants_get_watermark() {
        let table_id = TableId::new(NamespaceId::from("n"), TableName::from("t"));

        let commands = vec![
            SharedDataCommand::Insert { required_meta_index: 10, table_id: table_id.clone(), rows: vec![] },
            SharedDataCommand::Update { required_meta_index: 20, table_id: table_id.clone(), updates: vec![], filter: None },
            SharedDataCommand::Delete { required_meta_index: 30, table_id: table_id.clone(), pk_values: None },
        ];

        assert_eq!(commands[0].required_meta_index(), 10);
        assert_eq!(commands[1].required_meta_index(), 20);
        assert_eq!(commands[2].required_meta_index(), 30);
    }
}
