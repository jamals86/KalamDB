//! Live query subscription entity for system.live_queries table.

use crate::models::{ids::{LiveQueryId, NamespaceId, UserId}, TableName};
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

/// Live query subscription entity for system.live_queries.
///
/// Represents an active live query subscription (WebSocket connection).
///
/// ## Fields
/// - `live_id`: Unique live query ID (format: {user_id}-{conn_id}-{table_name}-{query_id})
/// - `connection_id`: WebSocket connection identifier
/// - `namespace_id`: Namespace ID
/// - `table_name`: Table being queried
/// - `query_id`: Query identifier
/// - `user_id`: User who created the subscription
/// - `query`: SQL query text
/// - `options`: Optional JSON configuration
/// - `created_at`: Unix timestamp in milliseconds when subscription was created
/// - `last_update`: Unix timestamp in milliseconds of last update notification
/// - `changes`: Number of changes sent
/// - `node`: Node/server handling this subscription
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
///     live_id: LiveQueryId::new("u_123-conn_456-events-q_789"),
///     connection_id: "conn_456".to_string(),
///     namespace_id: NamespaceId::new("default"),
///     table_name: TableName::new("events"),
///     query_id: "q_789".to_string(),
///     user_id: UserId::new("u_123"),
///     query: "SELECT * FROM events WHERE type = 'click'".to_string(),
///     options: Some(r#"{"include_initial": true}"#.to_string()),
///     created_at: 1730000000000,
///     last_update: 1730000300000,
///     changes: 42,
///     node: "server-01".to_string(),
/// };
/// ```
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug, PartialEq)]
pub struct LiveQuery {
    pub live_id: LiveQueryId, // Format: {user_id}-{unique_conn_id}-{table_name}-{query_id}
    pub connection_id: String,

    //TODO: Use TableId type INSTEAD OF BOTH table_name AND namespace_id
    pub namespace_id: NamespaceId,
    pub table_name: TableName,
    pub query_id: String,
    pub user_id: UserId,
    pub query: String,
    pub options: Option<String>, // JSON
    pub created_at: i64,         // Unix timestamp in milliseconds
    pub last_update: i64,        // Unix timestamp in milliseconds
    pub changes: i64,
    pub node: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_live_query_serialization() {
        let live_query = LiveQuery {
            live_id: "u_123-conn_456-events-q_789".into(),
            connection_id: "conn_456".to_string(),
            namespace_id: NamespaceId::new("default"),
            table_name: TableName::new("events"),
            query_id: "q_789".to_string(),
            user_id: UserId::new("u_123"),
            query: "SELECT * FROM events".to_string(),
            options: Some(r#"{"include_initial": true}"#.to_string()),
            created_at: 1730000000000,
            last_update: 1730000300000,
            changes: 42,
            node: "server-01".to_string(),
        };

        // Test bincode serialization
        let config = bincode::config::standard();
        let bytes = bincode::encode_to_vec(&live_query, config).unwrap();
        let (deserialized, _): (LiveQuery, _) = bincode::decode_from_slice(&bytes, config).unwrap();
        assert_eq!(live_query, deserialized);
    }
}
