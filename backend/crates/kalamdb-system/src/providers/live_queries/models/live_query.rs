//! Live query subscription entity for system.live_queries table.

use kalamdb_commons::{
    datatypes::KalamDataType,
    models::ids::{LiveQueryId, NamespaceId, UserId},
    models::NodeId,
    models::schemas::{ColumnDefinition, ColumnDefault, TableDefinition, TableOptions, TableType},
    system_tables::SystemTable,
    KSerializable, TableName,
};
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

use super::LiveQueryStatus;

/// Live query subscription entity for system.live_queries.
///
/// Represents an active live query subscription (WebSocket connection).
///
/// ## Fields
/// - `live_id`: Unique live query ID (format: {user_id}-{conn_id}-{table_name}-{subscription_id})
/// - `connection_id`: WebSocket connection identifier
/// - `subscription_id`: Client-provided subscription identifier
/// - `namespace_id`: Namespace ID
/// - `table_name`: Table being queried
/// - `user_id`: User who created the subscription
/// - `query`: SQL query text
/// - `options`: Optional JSON configuration
/// - `status`: Current status (Active, Paused, Completed, Error)
/// - `created_at`: Unix timestamp in milliseconds when subscription was created
/// - `last_update`: Unix timestamp in milliseconds of last update notification
/// - `changes`: Number of changes sent
/// - `node_id`: Node/server handling this subscription
///
/// **Note**: `last_seq_id` is tracked in-memory only (in WebSocketSession.subscription_metadata),
/// not persisted to system.live_queries table.
///
/// ## Serialization
/// - **RocksDB**: Bincode (compact binary format)
/// - **API**: JSON via Serde
///
/// ## Example
///
/// ```rust,ignore
/// use kalamdb_commons::models::ids::{ConnectionId, LiveQueryId, NamespaceId, UserId};
/// use kalamdb_system::providers::live_queries::models::{LiveQuery, LiveQueryStatus};
/// use kalamdb_commons::{NodeId, TableName};
///
/// let user_id = UserId::new("u_123");
/// let connection_id = ConnectionId::new("conn_456");
///
/// let live_query = LiveQuery {
///     live_id: LiveQueryId::new(user_id.clone(), connection_id.clone(), "sub_1"),
///     connection_id: connection_id.as_str().to_string(),
///     subscription_id: "sub_1".to_string(),
///     namespace_id: NamespaceId::new("default"),
///     table_name: TableName::new("events"),
///     user_id,
///     query: "SELECT * FROM events WHERE type = 'click'".to_string(),
///     options: Some(r#"{"include_initial": true}"#.to_string()),
///     status: LiveQueryStatus::Active,
///     created_at: 1730000000000,
///     last_update: 1730000300000,
///     last_ping_at: 1730000300000,
///     changes: 42,
///     node_id: NodeId::from(1u64),
/// };
/// ```
/// LiveQuery struct with fields ordered for optimal memory alignment.
/// 8-byte aligned fields first, then smaller types.
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug, PartialEq)]
pub struct LiveQuery {
    // 8-byte aligned fields first (i64, String/pointer types)
    pub created_at: i64,   // Unix timestamp in milliseconds
    pub last_update: i64,  // Unix timestamp in milliseconds
    pub last_ping_at: i64, // Unix timestamp in milliseconds (for heartbeat/failover)
    pub changes: i64,
    pub live_id: LiveQueryId, // Format: {user_id}-{unique_conn_id}-{table_name}-{subscription_id}
    pub connection_id: String,
    pub subscription_id: String,
    //TODO: Use TableId type INSTEAD OF BOTH table_name AND namespace_id
    pub namespace_id: NamespaceId,
    pub table_name: TableName,
    pub user_id: UserId,
    pub query: String,
    pub options: Option<String>, // TODO: Switch to: JSON
    /// Node identifier that holds this subscription's WebSocket connection
    #[bincode(with_serde)]
    pub node_id: NodeId,
    // Enum (typically 1-4 bytes depending on variant count)
    pub status: LiveQueryStatus, // Active, Paused, Completed, Error
}

// KSerializable implementation for EntityStore support
impl KSerializable for LiveQuery {}

impl LiveQuery {
    /// Generate TableDefinition for system.live_queries
    pub fn definition() -> TableDefinition {
        let columns = vec![
            ColumnDefinition::new(
                1,
                "live_id",
                1,
                KalamDataType::Text,
                false,
                true,
                false,
                ColumnDefault::None,
                Some("Unique live query ID (format: {user_id}-{conn_id}-{table_name}-{subscription_id})".to_string()),
            ),
            ColumnDefinition::new(
                2,
                "connection_id",
                2,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("WebSocket connection identifier".to_string()),
            ),
            ColumnDefinition::new(
                3,
                "subscription_id",
                3,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Client-provided subscription identifier".to_string()),
            ),
            ColumnDefinition::new(
                4,
                "namespace_id",
                4,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Namespace ID".to_string()),
            ),
            ColumnDefinition::new(
                5,
                "table_name",
                5,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Table being queried".to_string()),
            ),
            ColumnDefinition::new(
                6,
                "user_id",
                6,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("User who created the subscription".to_string()),
            ),
            ColumnDefinition::new(
                7,
                "query",
                7,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("SQL query text".to_string()),
            ),
            ColumnDefinition::new(
                8,
                "options",
                8,
                KalamDataType::Json,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Optional JSON configuration".to_string()),
            ),
            ColumnDefinition::new(
                9,
                "status",
                9,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Current status (Active, Paused, Completed, Error)".to_string()),
            ),
            ColumnDefinition::new(
                10,
                "created_at",
                10,
                KalamDataType::Timestamp,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Unix timestamp in milliseconds when subscription was created".to_string()),
            ),
            ColumnDefinition::new(
                11,
                "last_update",
                11,
                KalamDataType::Timestamp,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Unix timestamp in milliseconds of last update notification".to_string()),
            ),
            ColumnDefinition::new(
                12,
                "last_ping_at",
                12,
                KalamDataType::Timestamp,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Unix timestamp in milliseconds of last heartbeat/ping".to_string()),
            ),
            ColumnDefinition::new(
                13,
                "changes",
                13,
                KalamDataType::BigInt,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Number of changes sent".to_string()),
            ),
            ColumnDefinition::new(
                14,
                "node_id",
                14,
                KalamDataType::BigInt,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Node/server handling this subscription".to_string()),
            ),
        ];

        TableDefinition::new(
            NamespaceId::system(),
            TableName::new(SystemTable::LiveQueries.table_name()),
            TableType::System,
            columns,
            TableOptions::system(),
            Some("Active live query subscriptions (WebSocket connections)".to_string()),
        )
        .expect("Failed to create system.live_queries table definition")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_live_query_serialization() {
        let live_query = LiveQuery {
            live_id: "u_123-conn_456-events-sub_1".into(),
            connection_id: "conn_456".to_string(),
            subscription_id: "sub_1".to_string(),
            namespace_id: NamespaceId::new("default"),
            table_name: TableName::new("events"),
            user_id: UserId::new("u_123"),
            query: "SELECT * FROM events".to_string(),
            options: Some(r#"{"include_initial": true}"#.to_string()),
            status: LiveQueryStatus::Active,
            created_at: 1730000000000,
            last_update: 1730000300000,
            last_ping_at: 1730000300000,
            changes: 42,
            node_id: NodeId::from(1u64),
        };

        // Test bincode serialization
        let config = bincode::config::standard();
        let bytes = bincode::encode_to_vec(&live_query, config).unwrap();
        let (deserialized, _): (LiveQuery, _) = bincode::decode_from_slice(&bytes, config).unwrap();
        assert_eq!(live_query, deserialized);
    }
}
