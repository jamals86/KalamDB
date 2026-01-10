//! Live query subscription entity for system.live_queries table.

use crate::models::{
    ids::{LiveQueryId, NamespaceId, UserId},
    NodeId,
    TableName,
};
use crate::types::LiveQueryStatus;
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

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
/// - `node`: Node/server handling this subscription
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
/// ```rust
/// use kalamdb_commons::types::LiveQuery;
/// use kalamdb_commons::{UserId, NamespaceId, TableName, LiveQueryId};
///
/// let live_query = LiveQuery {
///     live_id: LiveQueryId::new("u_123-conn_456-events-sub_1"),
///     connection_id: "conn_456".to_string(),
///     subscription_id: "sub_1".to_string(),
///     namespace_id: NamespaceId::new("default"),
///     table_name: TableName::new("events"),
///     user_id: UserId::new("u_123"),
///     query: "SELECT * FROM events WHERE type = 'click'".to_string(),
///     options: Some(r#"{"include_initial": true}"#.to_string()),
///     status: LiveQueryStatus::Active,
///     created_at: 1730000000000,
///     last_update: 1730000300000,
///     changes: 42,
///     node: "server-01".to_string(),
/// };
/// ```
/// LiveQuery struct with fields ordered for optimal memory alignment.
/// 8-byte aligned fields first, then smaller types.
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug, PartialEq)]
pub struct LiveQuery {
    // 8-byte aligned fields first (i64, String/pointer types)
    pub created_at: i64,         // Unix timestamp in milliseconds
    pub last_update: i64,        // Unix timestamp in milliseconds
    pub last_ping_at: i64,       // Unix timestamp in milliseconds (for heartbeat/failover)
    pub changes: i64,
    pub live_id: LiveQueryId, // Format: {user_id}-{unique_conn_id}-{table_name}-{subscription_id}
    pub connection_id: String,
    pub subscription_id: String,
    //TODO: Use TableId type INSTEAD OF BOTH table_name AND namespace_id
    pub namespace_id: NamespaceId,
    pub table_name: TableName,
    pub user_id: UserId,
    pub query: String,
    pub options: Option<String>, // JSON
    /// Node identifier that holds this subscription's WebSocket connection
    #[bincode(with_serde)]
    pub node_id: NodeId,
    // Enum (typically 1-4 bytes depending on variant count)
    pub status: LiveQueryStatus, // Active, Paused, Completed, Error
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
