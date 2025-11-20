use kalamdb_commons::datatypes::KalamDataType;
use kalamdb_commons::schemas::{
    ColumnDefault, ColumnDefinition, TableDefinition, TableOptions, TableType,
};
use kalamdb_commons::{NamespaceId, TableName};

/// Create TableDefinition for system.tables table
///
/// Schema (flattened view of TableDefinition):
/// - table_id TEXT PRIMARY KEY (composite: namespace_id:table_name)
/// - table_name TEXT NOT NULL
/// - namespace_id TEXT NOT NULL
/// - table_type TEXT NOT NULL
/// - created_at TIMESTAMP NOT NULL
/// - schema_version INT NOT NULL
/// - table_comment TEXT (nullable)
/// - updated_at TIMESTAMP NOT NULL
/// - options TEXT (nullable, serialized TableOptions JSON)
pub fn tables_table_definition() -> TableDefinition {
    let columns = vec![
        ColumnDefinition::new(
            "table_id",
            1,
            KalamDataType::Text,
            false,
            true,
            false,
            ColumnDefault::None,
            Some("Table identifier: namespace_id:table_name".to_string()),
        ),
        ColumnDefinition::new(
            "table_name",
            2,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Table name within namespace".to_string()),
        ),
        ColumnDefinition::new(
            "namespace_id",
            3,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Namespace containing this table".to_string()),
        ),
        ColumnDefinition::new(
            "table_type",
            4,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Table type: USER, SHARED, STREAM, SYSTEM".to_string()),
        ),
        ColumnDefinition::new(
            "created_at",
            5,
            KalamDataType::Timestamp,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Table creation timestamp".to_string()),
        ),
        ColumnDefinition::new(
            "schema_version",
            6,
            KalamDataType::Int,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Current schema version number".to_string()),
        ),
        ColumnDefinition::new(
            "table_comment",
            7,
            KalamDataType::Text,
            true, // NULLABLE
            false,
            false,
            ColumnDefault::None,
            Some("Optional table description or comment".to_string()),
        ),
        ColumnDefinition::new(
            "updated_at",
            8,
            KalamDataType::Timestamp,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Last modification timestamp".to_string()),
        ),
        // New in Phase 11: expose serialized TableOptions for visibility via SELECT * FROM system.tables
        ColumnDefinition::new(
            "options",
            9,
            KalamDataType::Text, // Stored as JSON string (variant-aware)
            true,                // NULLABLE for forward compatibility (older rows may not have it)
            false,
            false,
            ColumnDefault::None,
            Some("Serialized table options (JSON)".to_string()),
        ),
        // New in Phase 16: expose access_level for Shared tables
        ColumnDefinition::new(
            "access_level",
            10,
            KalamDataType::Text,
            true, // NULLABLE (only for Shared tables)
            false,
            false,
            ColumnDefault::None,
            Some("Access level for Shared tables: public, private, protected".to_string()),
        ),
    ];

    TableDefinition::new(
        NamespaceId::system(),
        TableName::new("tables"),
        TableType::System,
        columns,
        TableOptions::system(),
        Some("Registry of all tables in the database".to_string()),
    )
    .expect("Failed to create system.tables table definition")
}

/// Create TableDefinition for system.table_schemas table (new in Phase 15)
///
/// Schema:
/// - table_id TEXT PRIMARY KEY (composite: namespace_id:table_name)
/// - schema_version INT NOT NULL
/// - table_definition JSON NOT NULL
/// - created_at TIMESTAMP NOT NULL
/// - updated_at TIMESTAMP NOT NULL
pub fn table_schemas_table_definition() -> TableDefinition {
    let columns = vec![
        ColumnDefinition::new(
            "table_id",
            1,
            KalamDataType::Text,
            false,
            true,
            false,
            ColumnDefault::None,
            Some("Table identifier: namespace_id:table_name".to_string()),
        ),
        ColumnDefinition::new(
            "schema_version",
            2,
            KalamDataType::Int,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Current schema version number".to_string()),
        ),
        ColumnDefinition::new(
            "table_definition",
            3,
            KalamDataType::Json,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Complete TableDefinition JSON including columns and options".to_string()),
        ),
        ColumnDefinition::new(
            "created_at",
            4,
            KalamDataType::Timestamp,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Schema creation timestamp".to_string()),
        ),
        ColumnDefinition::new(
            "updated_at",
            5,
            KalamDataType::Timestamp,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Last schema update timestamp".to_string()),
        ),
    ];

    TableDefinition::new(
        NamespaceId::system(),
        TableName::new("table_schemas"),
        TableType::System,
        columns,
        TableOptions::system(),
        Some("Table schema definitions with versioning".to_string()),
    )
    .expect("Failed to create system.table_schemas table definition")
}
