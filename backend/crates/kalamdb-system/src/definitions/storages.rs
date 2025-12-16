use kalamdb_commons::datatypes::KalamDataType;
use kalamdb_commons::schemas::{
    ColumnDefault, ColumnDefinition, TableDefinition, TableOptions, TableType,
};
use kalamdb_commons::{NamespaceId, TableName};

/// Create TableDefinition for system.storages table
///
/// Schema:
/// - storage_id TEXT PRIMARY KEY
/// - storage_name TEXT NOT NULL
/// - description TEXT (nullable)
/// - storage_type TEXT NOT NULL
/// - base_directory TEXT NOT NULL
/// - credentials TEXT (nullable)
/// - config_json TEXT (nullable)
/// - shared_tables_template TEXT NOT NULL
/// - user_tables_template TEXT NOT NULL
/// - created_at TIMESTAMP NOT NULL
/// - updated_at TIMESTAMP NOT NULL
pub fn storages_table_definition() -> TableDefinition {
    let columns = vec![
        ColumnDefinition::new(
            "storage_id",
            1,
            KalamDataType::Text,
            false,
            true,
            false,
            ColumnDefault::None,
            Some("Storage identifier".to_string()),
        ),
        ColumnDefinition::new(
            "storage_name",
            2,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Human-readable storage name".to_string()),
        ),
        ColumnDefinition::new(
            "description",
            3,
            KalamDataType::Text,
            true, // NULLABLE
            false,
            false,
            ColumnDefault::None,
            Some("Storage description".to_string()),
        ),
        ColumnDefinition::new(
            "storage_type",
            4,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Storage type: Local, S3, Azure, GCS".to_string()),
        ),
        ColumnDefinition::new(
            "base_directory",
            5,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Base directory path for storage".to_string()),
        ),
        ColumnDefinition::new(
            "credentials",
            6,
            KalamDataType::Text,
            true, // NULLABLE
            false,
            false,
            ColumnDefault::None,
            Some("Encrypted credentials JSON".to_string()),
        ),
        ColumnDefinition::new(
            "config_json",
            7,
            KalamDataType::Text,
            true, // NULLABLE
            false,
            false,
            ColumnDefault::None,
            Some("Backend-specific storage configuration JSON".to_string()),
        ),
        ColumnDefinition::new(
            "shared_tables_template",
            8,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Path template for shared tables".to_string()),
        ),
        ColumnDefinition::new(
            "user_tables_template",
            9,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Path template for user tables".to_string()),
        ),
        ColumnDefinition::new(
            "created_at",
            10,
            KalamDataType::Timestamp,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Storage creation timestamp".to_string()),
        ),
        ColumnDefinition::new(
            "updated_at",
            11,
            KalamDataType::Timestamp,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Last update timestamp".to_string()),
        ),
    ];

    TableDefinition::new(
        NamespaceId::system(),
        TableName::new("storages"),
        TableType::System,
        columns,
        TableOptions::system(),
        Some("Storage configurations for data persistence".to_string()),
    )
    .expect("Failed to create system.storages table definition")
}
