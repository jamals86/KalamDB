use kalamdb_commons::datatypes::KalamDataType;
use kalamdb_commons::schemas::{
    ColumnDefault, ColumnDefinition, TableDefinition, TableOptions, TableType,
};
use kalamdb_commons::{NamespaceId, TableName};

/// Create TableDefinition for system.live_queries table
///
/// Schema:
/// - live_id TEXT PRIMARY KEY
/// - connection_id TEXT NOT NULL
/// - namespace_id TEXT NOT NULL
/// - table_name TEXT NOT NULL
/// - query_id TEXT NOT NULL
/// - user_id TEXT NOT NULL
/// - query TEXT NOT NULL
/// - options TEXT (nullable, JSON)
/// - created_at TIMESTAMP NOT NULL
/// - last_update TIMESTAMP NOT NULL
/// - changes BIGINT NOT NULL
/// - node TEXT NOT NULL
pub fn live_queries_table_definition() -> TableDefinition {
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
            Some(
                "Live query identifier (format: {user_id}-{conn_id}-{table}-{query_id})"
                    .to_string(),
            ),
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
            "namespace_id",
            3,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Namespace containing the table".to_string()),
        ),
        ColumnDefinition::new(
            4,
            "table_name",
            4,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Table being queried".to_string()),
        ),
        ColumnDefinition::new(
            5,
            "query_id",
            5,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Query identifier (UUID)".to_string()),
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
            Some("User who created the live query".to_string()),
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
            Some("SQL query for real-time subscription".to_string()),
        ),
        ColumnDefinition::new(
            8,
            "options",
            8,
            KalamDataType::Text,
            true, // NULLABLE
            false,
            false,
            ColumnDefault::None,
            Some("Query options (JSON)".to_string()),
        ),
        ColumnDefinition::new(
            9,
            "created_at",
            9,
            KalamDataType::Timestamp,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Live query creation timestamp".to_string()),
        ),
        ColumnDefinition::new(
            10,
            "last_update",
            10,
            KalamDataType::Timestamp,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Last update sent to client".to_string()),
        ),
        ColumnDefinition::new(
            11,
            "changes",
            11,
            KalamDataType::BigInt,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Number of changes pushed to client".to_string()),
        ),
        ColumnDefinition::new(
            12,
            "node",
            12,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Server node handling this live query".to_string()),
        ),
    ];

    TableDefinition::new(
        NamespaceId::system(),
        TableName::new("live_queries"),
        TableType::System,
        columns,
        TableOptions::system(),
        Some("Active WebSocket live query subscriptions".to_string()),
    )
    .expect("Failed to create system.live_queries table definition")
}
