use kalamdb_commons::datatypes::KalamDataType;
use kalamdb_commons::schemas::{
    ColumnDefault, ColumnDefinition, TableDefinition, TableOptions, TableType,
};
use kalamdb_commons::{NamespaceId, TableName};

/// Create TableDefinition for system.manifest table
///
/// Schema:
/// - cache_key TEXT PRIMARY KEY (format: namespace:table:scope)
/// - namespace_id TEXT NOT NULL
/// - table_name TEXT NOT NULL
/// - scope TEXT NOT NULL (user_id or "shared")
/// - etag TEXT (nullable, storage version identifier)
/// - last_refreshed TIMESTAMP NOT NULL
/// - last_accessed TIMESTAMP NOT NULL
/// - in_memory BOOLEAN NOT NULL (true if manifest is in hot cache)
/// - source_path TEXT NOT NULL
/// - sync_state TEXT NOT NULL (in_sync, stale, error)
/// - manifest_json TEXT NOT NULL (serialized Manifest object)
pub fn manifest_table_definition() -> TableDefinition {
    let columns = vec![
        ColumnDefinition::new(
            "cache_key",
            1,
            KalamDataType::Text,
            false,
            true,
            false,
            ColumnDefault::None,
            Some("Cache key identifier (format: namespace:table:scope)".to_string()),
        ),
        ColumnDefinition::new(
            "namespace_id",
            2,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Namespace containing the table".to_string()),
        ),
        ColumnDefinition::new(
            "table_name",
            3,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Table name".to_string()),
        ),
        ColumnDefinition::new(
            "scope",
            4,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Scope: user_id for USER tables, 'shared' for SHARED tables".to_string()),
        ),
        ColumnDefinition::new(
            "etag",
            5,
            KalamDataType::Text,
            true, // NULLABLE
            false,
            false,
            ColumnDefault::None,
            Some("Storage ETag or version identifier".to_string()),
        ),
        ColumnDefinition::new(
            "last_refreshed",
            6,
            KalamDataType::Timestamp,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Last successful cache refresh timestamp".to_string()),
        ),
        ColumnDefinition::new(
            "last_accessed",
            7,
            KalamDataType::Timestamp,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Last access timestamp (in-memory tracking)".to_string()),
        ),
        ColumnDefinition::new(
            "in_memory",
            8,
            KalamDataType::Boolean,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("True if manifest is currently in hot cache (RAM)".to_string()),
        ),
        ColumnDefinition::new(
            "source_path",
            9,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Full path to manifest.json in storage".to_string()),
        ),
        ColumnDefinition::new(
            "sync_state",
            10,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Synchronization state: in_sync, stale, error".to_string()),
        ),
        ColumnDefinition::new(
            "manifest_json",
            11,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Serialized Manifest object as JSON".to_string()),
        ),
    ];

    TableDefinition::new(
        NamespaceId::system(),
        TableName::new("manifest"),
        TableType::System,
        columns,
        TableOptions::system(),
        Some("Manifest cache entries for query optimization".to_string()),
    )
    .expect("Failed to create system.manifest table definition")
}
