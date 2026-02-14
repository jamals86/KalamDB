//! Live query subscription entity for system.live_queries table.

use super::LiveQueryStatus;
use bincode::{Decode, Encode};
use kalamdb_commons::datatypes::KalamDataType;
use kalamdb_commons::models::{
    ids::{LiveQueryId, NamespaceId, UserId},
    NodeId, TableName,
};
use kalamdb_macros::table;
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
/// - `node_id`: Node/server handling this subscription
///
/// **Note**: `last_seq_id` is tracked in-memory only (in WebSocketSession.subscription_metadata),
/// not persisted to system.live_queries table.
///
/// ## Serialization
/// - **RocksDB**: FlatBuffers envelope + FlexBuffers payload
/// - **API**: JSON via Serde
///
/// ## Example
///
/// ```rust
/// use kalamdb_commons::models::ids::{ConnectionId, LiveQueryId, NamespaceId, UserId};
/// use kalamdb_system::LiveQuery;
/// use kalamdb_system::LiveQueryStatus;
/// use kalamdb_commons::{NodeId, TableName};
///
/// let user_id = UserId::new("u_123");
/// let connection_id = ConnectionId::new("conn_456");
///
/// let live_query = LiveQuery {
///     live_id: LiveQueryId::new(user_id.clone(), connection_id.clone(), "sub_1"),
///     connection_id: connection_id.as_str().to_string(),
///     subscription_id: "sub_1".to_string(),
///     namespace_id: NamespaceId::default(),
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
#[table(
    name = "live_queries",
    comment = "Active WebSocket live query subscriptions"
)]
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug, PartialEq)]
pub struct LiveQuery {
    // 8-byte aligned fields first (i64, String/pointer types)
    #[column(
        id = 10,
        ordinal = 10,
        data_type(KalamDataType::Timestamp),
        nullable = false,
        primary_key = false,
        default = "None",
        comment = "Live query creation timestamp"
    )]
    pub created_at: i64, // Unix timestamp in milliseconds
    #[column(
        id = 11,
        ordinal = 11,
        data_type(KalamDataType::Timestamp),
        nullable = false,
        primary_key = false,
        default = "None",
        comment = "Last update sent to client"
    )]
    pub last_update: i64, // Unix timestamp in milliseconds
    #[column(
        id = 14,
        ordinal = 14,
        data_type(KalamDataType::Timestamp),
        nullable = false,
        primary_key = false,
        default = "None",
        comment = "Last ping timestamp for stale detection"
    )]
    pub last_ping_at: i64, // Unix timestamp in milliseconds (for heartbeat/failover)
    #[column(
        id = 12,
        ordinal = 12,
        data_type(KalamDataType::BigInt),
        nullable = false,
        primary_key = false,
        default = "None",
        comment = "Number of changes pushed to client"
    )]
    pub changes: i64,
    #[column(
        id = 1,
        ordinal = 1,
        data_type(KalamDataType::Text),
        nullable = false,
        primary_key = true,
        default = "None",
        comment = "Live query identifier (format: {user_id}-{conn_id}-{table}-{subscription_id})"
    )]
    pub live_id: LiveQueryId, // Format: {user_id}-{unique_conn_id}-{table_name}-{subscription_id}
    #[column(
        id = 2,
        ordinal = 2,
        data_type(KalamDataType::Text),
        nullable = false,
        primary_key = false,
        default = "None",
        comment = "WebSocket connection identifier"
    )]
    pub connection_id: String,
    #[column(
        id = 3,
        ordinal = 3,
        data_type(KalamDataType::Text),
        nullable = false,
        primary_key = false,
        default = "None",
        comment = "Client-provided subscription identifier"
    )]
    pub subscription_id: String,
    //TODO: Use TableId type INSTEAD OF BOTH table_name AND namespace_id
    #[column(
        id = 4,
        ordinal = 4,
        data_type(KalamDataType::Text),
        nullable = false,
        primary_key = false,
        default = "None",
        comment = "Namespace containing the table"
    )]
    pub namespace_id: NamespaceId,
    #[column(
        id = 5,
        ordinal = 5,
        data_type(KalamDataType::Text),
        nullable = false,
        primary_key = false,
        default = "None",
        comment = "Table being queried"
    )]
    pub table_name: TableName,
    #[column(
        id = 6,
        ordinal = 6,
        data_type(KalamDataType::Text),
        nullable = false,
        primary_key = false,
        default = "None",
        comment = "User who created the live query"
    )]
    pub user_id: UserId,
    #[column(
        id = 7,
        ordinal = 7,
        data_type(KalamDataType::Text),
        nullable = false,
        primary_key = false,
        default = "None",
        comment = "SQL query for real-time subscription"
    )]
    pub query: String,
    #[column(
        id = 8,
        ordinal = 8,
        data_type(KalamDataType::Json),
        nullable = true,
        primary_key = false,
        default = "None",
        comment = "Query options (JSON)"
    )]
    pub options: Option<String>, // TODO: Switch to: JSON
    /// Node identifier that holds this subscription's WebSocket connection
    #[bincode(with_serde)]
    #[column(
        id = 13,
        ordinal = 13,
        data_type(KalamDataType::BigInt),
        nullable = false,
        primary_key = false,
        default = "None",
        comment = "Server node ID handling this live query"
    )]
    pub node_id: NodeId,
    // Enum (typically 1-4 bytes depending on variant count)
    #[column(
        id = 9,
        ordinal = 9,
        data_type(KalamDataType::Text),
        nullable = false,
        primary_key = false,
        default = "None",
        comment = "Current status (active, paused, etc.)"
    )]
    pub status: LiveQueryStatus, // Active, Paused, Completed, Error
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::serialization::system_codec::{
        decode_flex, decode_live_query_payload, encode_flex, encode_live_query_payload,
    };
    use serde_json::json;

    #[test]
    fn test_live_query_serialization() {
        let live_query = LiveQuery {
            live_id: "u_123-conn_456-events-sub_1".into(),
            connection_id: "conn_456".to_string(),
            subscription_id: "sub_1".to_string(),
            namespace_id: NamespaceId::default(),
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

        let bytes = serde_json::to_vec(&live_query).unwrap();
        let deserialized: LiveQuery = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(live_query, deserialized);
    }

    #[test]
    fn test_live_query_flatbuffers_flex_roundtrip() {
        let live_query = LiveQuery {
            live_id: "u_123-conn_456-events-sub_1".into(),
            connection_id: "conn_456".to_string(),
            subscription_id: "sub_1".to_string(),
            namespace_id: NamespaceId::default(),
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

        let flex_payload = encode_flex(&live_query).expect("encode flex payload");
        let wrapped =
            encode_live_query_payload(&flex_payload).expect("encode live_query wrapper");
        let unwrapped = decode_live_query_payload(&wrapped).expect("decode live_query wrapper");
        let decoded: LiveQuery = decode_flex(&unwrapped).expect("decode flex payload");

        assert_eq!(decoded, live_query);
    }

    #[test]
    fn test_live_query_decode_rejects_string_node_id() {
        let invalid_payload = json!({
            "created_at": 1730000000000_i64,
            "last_update": 1730000300000_i64,
            "last_ping_at": 1730000300000_i64,
            "changes": 42_i64,
            "live_id": "u_123-conn_456-events-sub_1",
            "connection_id": "conn_456",
            "subscription_id": "sub_1",
            "namespace_id": "default",
            "table_name": "events",
            "user_id": "u_123",
            "query": "SELECT * FROM events",
            "options": null,
            "node_id": "1",
            "status": "Active"
        });

        let flex_payload = encode_flex(&invalid_payload).expect("encode invalid flex payload");
        let wrapped = encode_live_query_payload(&flex_payload).expect("encode live_query wrapper");
        let unwrapped = decode_live_query_payload(&wrapped).expect("decode live_query wrapper");
        let err =
            decode_flex::<LiveQuery>(&unwrapped).expect_err("string node_id must fail");

        assert!(
            err.to_string().contains("expected u64"),
            "unexpected error: {err}"
        );
    }
}
