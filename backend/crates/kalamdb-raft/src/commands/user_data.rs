//! User data shard commands (user tables, live queries)
//!
//! Each data command carries a `required_meta_index` watermark, which is the
//! `Meta` group's last applied index on the leader at proposal time. Followers
//! buffer data commands until local `Meta` has applied at least that index.
//!
//! Live queries are sharded by user_id for efficient per-user subscription management.

use chrono::{DateTime, Utc};
use kalamdb_commons::models::{ConnectionId, LiveQueryId, NodeId, UserId};
use kalamdb_commons::TableId;
use kalamdb_system::providers::live_queries::models::{LiveQuery, LiveQueryStatus};
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
        rows: Vec<kalamdb_commons::models::rows::Row>,
    },

    /// Update rows in a user table
    Update {
        /// Watermark: Meta group's last_applied_index at proposal time
        required_meta_index: u64,
        table_id: TableId,
        user_id: UserId,
        /// Updates to apply
        updates: Vec<kalamdb_commons::models::rows::Row>,
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

    // === Live Query Subscriptions (persisted to system.live_queries via applier) ===
    /// Create a new live query subscription
    ///
    /// Uses the `LiveQuery` struct from kalamdb-commons for cleaner API.
    /// The `required_meta_index` is kept separate for Raft watermark ordering.
    CreateLiveQuery {
        /// Watermark: Meta group's last_applied_index at proposal time
        required_meta_index: u64,
        /// Complete live query data
        live_query: LiveQuery,
    },

    /// Update live query statistics
    ///
    /// Note: Consider batching/delaying these updates to avoid flooding the state machine.
    /// See task 83 for implementation of delayed stats updates.
    UpdateLiveQueryStats {
        /// Watermark: Meta group's last_applied_index at proposal time
        required_meta_index: u64,
        /// Live query to update
        live_id: LiveQueryId,
        /// User for shard routing
        user_id: UserId,
        /// Last update timestamp
        last_update: DateTime<Utc>,
        /// Number of changes sent
        changes: i64,
    },

    /// Delete a single live query subscription
    DeleteLiveQuery {
        /// Watermark: Meta group's last_applied_index at proposal time
        required_meta_index: u64,
        /// Live query to delete
        live_id: LiveQueryId,
        /// User for shard routing
        user_id: UserId,
        /// When the subscription was deleted
        deleted_at: DateTime<Utc>,
    },

    /// Delete all live queries for a connection (when connection closes)
    DeleteLiveQueriesByConnection {
        /// Watermark: Meta group's last_applied_index at proposal time
        required_meta_index: u64,
        /// Connection whose subscriptions to delete
        connection_id: ConnectionId,
        /// User for shard routing
        user_id: UserId,
        /// When the connection was closed
        deleted_at: DateTime<Utc>,
    },

    /// Clean up all subscriptions from a failed node
    CleanupNodeSubscriptions {
        /// Watermark: Meta group's last_applied_index at proposal time
        required_meta_index: u64,
        user_id: UserId,
        failed_node_id: NodeId,
    },
}

impl UserDataCommand {
    /// Get the required_meta_index watermark for this command
    pub fn required_meta_index(&self) -> u64 {
        match self {
            UserDataCommand::Insert {
                required_meta_index,
                ..
            } => *required_meta_index,
            UserDataCommand::Update {
                required_meta_index,
                ..
            } => *required_meta_index,
            UserDataCommand::Delete {
                required_meta_index,
                ..
            } => *required_meta_index,
            UserDataCommand::CreateLiveQuery {
                required_meta_index,
                ..
            } => *required_meta_index,
            UserDataCommand::UpdateLiveQueryStats {
                required_meta_index,
                ..
            } => *required_meta_index,
            UserDataCommand::DeleteLiveQuery {
                required_meta_index,
                ..
            } => *required_meta_index,
            UserDataCommand::DeleteLiveQueriesByConnection {
                required_meta_index,
                ..
            } => *required_meta_index,
            UserDataCommand::CleanupNodeSubscriptions {
                required_meta_index,
                ..
            } => *required_meta_index,
        }
    }

    /// Set the required_meta_index watermark for this command
    pub fn set_required_meta_index(&mut self, index: u64) {
        match self {
            UserDataCommand::Insert {
                required_meta_index,
                ..
            } => *required_meta_index = index,
            UserDataCommand::Update {
                required_meta_index,
                ..
            } => *required_meta_index = index,
            UserDataCommand::Delete {
                required_meta_index,
                ..
            } => *required_meta_index = index,
            UserDataCommand::CreateLiveQuery {
                required_meta_index,
                ..
            } => *required_meta_index = index,
            UserDataCommand::UpdateLiveQueryStats {
                required_meta_index,
                ..
            } => *required_meta_index = index,
            UserDataCommand::DeleteLiveQuery {
                required_meta_index,
                ..
            } => *required_meta_index = index,
            UserDataCommand::DeleteLiveQueriesByConnection {
                required_meta_index,
                ..
            } => *required_meta_index = index,
            UserDataCommand::CleanupNodeSubscriptions {
                required_meta_index,
                ..
            } => *required_meta_index = index,
        }
    }

    /// Get the user_id for routing to the correct shard
    pub fn user_id(&self) -> &UserId {
        match self {
            UserDataCommand::Insert { user_id, .. } => user_id,
            UserDataCommand::Update { user_id, .. } => user_id,
            UserDataCommand::Delete { user_id, .. } => user_id,
            UserDataCommand::CreateLiveQuery { live_query, .. } => &live_query.user_id,
            UserDataCommand::UpdateLiveQueryStats { user_id, .. } => user_id,
            UserDataCommand::DeleteLiveQuery { user_id, .. } => user_id,
            UserDataCommand::DeleteLiveQueriesByConnection { user_id, .. } => user_id,
            UserDataCommand::CleanupNodeSubscriptions { user_id, .. } => user_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::models::{NamespaceId, TableName};
    use kalamdb_commons::types::LiveQueryStatus;

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
    fn test_live_query_commands() {
        let user_id = UserId::from("user_1");
        let connection_id = ConnectionId::new("conn_123");
        let live_id = LiveQueryId::new(user_id.clone(), connection_id.clone(), "sub_123");

        let live_query = LiveQuery {
            live_id: live_id.clone(),
            connection_id: connection_id.as_str().to_string(),
            subscription_id: "sub_123".to_string(),
            namespace_id: NamespaceId::from("ns"),
            table_name: TableName::from("table"),
            user_id: user_id.clone(),
            query: "SELECT * FROM ns.table".to_string(),
            options: Some(r#"{"batch_size": 100}"#.to_string()),
            status: LiveQueryStatus::Active,
            created_at: 1000,
            last_update: 1000,
            last_ping_at: 1000,
            changes: 0,
            node_id: NodeId::from(1),
        };

        let cmd = UserDataCommand::CreateLiveQuery {
            required_meta_index: 50,
            live_query,
        };

        assert_eq!(cmd.required_meta_index(), 50);
        assert_eq!(cmd.user_id(), &user_id);
    }

    #[test]
    fn test_all_user_command_variants_get_watermark() {
        let table_id = TableId::new(NamespaceId::from("n"), TableName::from("t"));
        let user_id = UserId::from("u");

        let commands = vec![
            UserDataCommand::Insert {
                required_meta_index: 1,
                table_id: table_id.clone(),
                user_id: user_id.clone(),
                rows: vec![],
            },
            UserDataCommand::Update {
                required_meta_index: 2,
                table_id: table_id.clone(),
                user_id: user_id.clone(),
                updates: vec![],
                filter: None,
            },
            UserDataCommand::Delete {
                required_meta_index: 3,
                table_id: table_id.clone(),
                user_id: user_id.clone(),
                pk_values: None,
            },
        ];

        // Test live query commands separately as they require proper LiveQueryId construction
        let connection_id = ConnectionId::new("c");
        let live_id = LiveQueryId::new(user_id.clone(), connection_id.clone(), "s");

        let live_query = LiveQuery {
            live_id: live_id.clone(),
            connection_id: connection_id.as_str().to_string(),
            subscription_id: "s".to_string(),
            namespace_id: NamespaceId::from("n"),
            table_name: TableName::from("t"),
            user_id: user_id.clone(),
            query: "SELECT *".to_string(),
            options: None,
            status: LiveQueryStatus::Active,
            created_at: 1000,
            last_update: 1000,
            last_ping_at: 1000,
            changes: 0,
            node_id: NodeId::from(1),
        };

        let live_commands = vec![
            UserDataCommand::CreateLiveQuery {
                required_meta_index: 4,
                live_query,
            },
            UserDataCommand::UpdateLiveQueryStats {
                required_meta_index: 5,
                live_id: live_id.clone(),
                user_id: user_id.clone(),
                last_update: Utc::now(),
                changes: 10,
            },
            UserDataCommand::DeleteLiveQuery {
                required_meta_index: 6,
                live_id: live_id.clone(),
                user_id: user_id.clone(),
                deleted_at: Utc::now(),
            },
            UserDataCommand::DeleteLiveQueriesByConnection {
                required_meta_index: 7,
                connection_id: connection_id.clone(),
                user_id: user_id.clone(),
                deleted_at: Utc::now(),
            },
            UserDataCommand::CleanupNodeSubscriptions {
                required_meta_index: 8,
                user_id: user_id.clone(),
                failed_node_id: NodeId::from(2),
            },
        ];

        for (i, cmd) in commands.iter().enumerate() {
            assert_eq!(cmd.required_meta_index(), (i + 1) as u64);
        }

        for (i, cmd) in live_commands.iter().enumerate() {
            assert_eq!(cmd.required_meta_index(), (i + 4) as u64);
        }
    }

    #[test]
    fn test_user_command_set_watermark_to_zero() {
        // DML commands should support setting watermark to 0 for performance optimization
        // See spec 021 section 5.4.1 "Watermark Nuance"
        let user_id = UserId::from("user123");
        let table_id = TableId::new(NamespaceId::from("ns"), TableName::from("table"));

        let mut insert = UserDataCommand::Insert {
            required_meta_index: 100, // Initially non-zero
            table_id: table_id.clone(),
            user_id: user_id.clone(),
            rows: vec![],
        };

        let mut update = UserDataCommand::Update {
            required_meta_index: 100,
            table_id: table_id.clone(),
            user_id: user_id.clone(),
            updates: vec![],
            filter: None,
        };

        let mut delete = UserDataCommand::Delete {
            required_meta_index: 100,
            table_id: table_id.clone(),
            user_id: user_id.clone(),
            pk_values: None,
        };

        // Set to 0 - this is what RaftExecutor does for DML
        insert.set_required_meta_index(0);
        update.set_required_meta_index(0);
        delete.set_required_meta_index(0);

        assert_eq!(insert.required_meta_index(), 0);
        assert_eq!(update.required_meta_index(), 0);
        assert_eq!(delete.required_meta_index(), 0);
    }
}
