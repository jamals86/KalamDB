use kalamdb_commons::datatypes::KalamDataType;
use kalamdb_commons::schemas::{
    ColumnDefault, ColumnDefinition, TableDefinition, TableOptions, TableType,
};
use kalamdb_commons::{NamespaceId, TableName};

/// Create TableDefinition for system.users table
///
/// Schema:
/// - user_id TEXT PRIMARY KEY
/// - username TEXT NOT NULL
/// - password_hash TEXT NOT NULL
/// - role TEXT NOT NULL
/// - created_at TIMESTAMP NOT NULL
/// - updated_at TIMESTAMP NOT NULL
/// - last_seen TIMESTAMP
/// - deleted_at TIMESTAMP
pub fn users_table_definition() -> TableDefinition {
    let columns = vec![
        ColumnDefinition::new(
            "user_id",
            1,
            KalamDataType::Text,
            false, // NOT NULL
            true,  // PRIMARY KEY
            false,
            ColumnDefault::None,
            Some("User identifier (UUID)".to_string()),
        ),
        ColumnDefinition::new(
            "username",
            2,
            KalamDataType::Text,
            false, // NOT NULL
            false,
            false,
            ColumnDefault::None,
            Some("Unique username for authentication".to_string()),
        ),
        ColumnDefinition::new(
            "password_hash",
            3,
            KalamDataType::Text,
            false, // NOT NULL
            false,
            false,
            ColumnDefault::None,
            Some("bcrypt password hash".to_string()),
        ),
        ColumnDefinition::new(
            "role",
            4,
            KalamDataType::Text,
            false, // NOT NULL
            false,
            false,
            ColumnDefault::None,
            Some("User role: user, service, dba, system".to_string()),
        ),
        ColumnDefinition::new(
            "email",
            5,
            KalamDataType::Text,
            true, // NULLABLE
            false,
            false,
            ColumnDefault::None,
            Some("User email address".to_string()),
        ),
        ColumnDefinition::new(
            "auth_type",
            6,
            KalamDataType::Text,
            false, // NOT NULL
            false,
            false,
            ColumnDefault::None,
            Some("Authentication type: Password, OAuth, ApiKey".to_string()),
        ),
        ColumnDefinition::new(
            "auth_data",
            7,
            KalamDataType::Text,
            true, // NULLABLE
            false,
            false,
            ColumnDefault::None,
            Some("Authentication data (JSON for OAuth provider/subject)".to_string()),
        ),
        ColumnDefinition::new(
            "storage_mode",
            8,
            KalamDataType::Text,
            false, // NOT NULL
            false,
            false,
            ColumnDefault::None,
            Some("Preferred storage partitioning mode".to_string()),
        ),
        ColumnDefinition::new(
            "storage_id",
            9,
            KalamDataType::Text,
            true, // NULLABLE
            false,
            false,
            ColumnDefault::None,
            Some("Optional preferred storage configuration ID".to_string()),
        ),
        ColumnDefinition::new(
            "created_at",
            10,
            KalamDataType::Timestamp,
            false, // NOT NULL
            false,
            false,
            ColumnDefault::None,
            Some("Account creation timestamp".to_string()),
        ),
        ColumnDefinition::new(
            "updated_at",
            11,
            KalamDataType::Timestamp,
            false, // NOT NULL
            false,
            false,
            ColumnDefault::None,
            Some("Last account update timestamp".to_string()),
        ),
        ColumnDefinition::new(
            "last_seen",
            12,
            KalamDataType::Timestamp,
            true, // NULLABLE
            false,
            false,
            ColumnDefault::None,
            Some("Last authentication timestamp".to_string()),
        ),
        ColumnDefinition::new(
            "deleted_at",
            13,
            KalamDataType::Timestamp,
            true, // NULLABLE
            false,
            false,
            ColumnDefault::None,
            Some("Soft delete timestamp".to_string()),
        ),
    ];

    TableDefinition::new(
        NamespaceId::system(),
        TableName::new("users"),
        TableType::System,
        columns,
        TableOptions::system(),
        Some("System users for authentication and authorization".to_string()),
    )
    .expect("Failed to create system.users table definition")
}
