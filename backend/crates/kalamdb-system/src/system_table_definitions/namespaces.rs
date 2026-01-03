use kalamdb_commons::datatypes::KalamDataType;
use kalamdb_commons::schemas::{
    ColumnDefault, ColumnDefinition, TableDefinition, TableOptions, TableType,
};
use kalamdb_commons::{NamespaceId, TableName};

/// Create TableDefinition for system.namespaces table
///
/// Schema:
/// - namespace_id TEXT PRIMARY KEY
/// - name TEXT NOT NULL
/// - created_at TIMESTAMP NOT NULL
/// - options TEXT (nullable, JSON configuration)
/// - table_count INT NOT NULL
pub fn namespaces_table_definition() -> TableDefinition {
    let columns = vec![
        ColumnDefinition::new(
            1,
            "namespace_id",
            1,
            KalamDataType::Text,
            false,
            true,
            false,
            ColumnDefault::None,
            Some("Namespace identifier".to_string()),
        ),
        ColumnDefinition::new(
            2,
            "name",
            2,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Namespace name".to_string()),
        ),
        ColumnDefinition::new(
            3,
            "created_at",
            3,
            KalamDataType::Timestamp,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Namespace creation timestamp".to_string()),
        ),
        ColumnDefinition::new(
            4,
            "options",
            4,
            KalamDataType::Text,
            true, // NULLABLE
            false,
            false,
            ColumnDefault::None,
            Some("Namespace configuration options (JSON)".to_string()),
        ),
        ColumnDefinition::new(
            5,
            "table_count",
            5,
            KalamDataType::Int,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Number of tables in this namespace".to_string()),
        ),
    ];

    TableDefinition::new(
        NamespaceId::system(),
        TableName::new("namespaces"),
        TableType::System,
        columns,
        TableOptions::system(),
        Some("Database namespaces for multi-tenancy".to_string()),
    )
    .expect("Failed to create system.namespaces table definition")
}
