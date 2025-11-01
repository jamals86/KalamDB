//! System table schema definitions using consolidated TableDefinition models.
//!
//! This module defines the schemas for all KalamDB system tables:
//! - system.users
//! - system.jobs
//! - system.namespaces
//! - system.storages
//! - system.live_queries
//! - system.tables
//! - system.table_schemas (new)
//!
//! All schemas are defined using the unified TableDefinition model from
//! kalamdb-commons, ensuring consistency across the codebase.

use kalamdb_commons::models::TableId;
use kalamdb_commons::schemas::{ColumnDefinition, ColumnDefault, TableDefinition, TableOptions, TableType};
use kalamdb_commons::types::KalamDataType;

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
            "created_at",
            5,
            KalamDataType::Timestamp,
            false, // NOT NULL
            false,
            false,
            ColumnDefault::None,
            Some("Account creation timestamp".to_string()),
        ),
        ColumnDefinition::new(
            "updated_at",
            6,
            KalamDataType::Timestamp,
            false, // NOT NULL
            false,
            false,
            ColumnDefault::None,
            Some("Last account update timestamp".to_string()),
        ),
        ColumnDefinition::new(
            "last_seen",
            7,
            KalamDataType::Timestamp,
            true, // NULLABLE
            false,
            false,
            ColumnDefault::None,
            Some("Last authentication timestamp".to_string()),
        ),
        ColumnDefinition::new(
            "deleted_at",
            8,
            KalamDataType::Timestamp,
            true, // NULLABLE
            false,
            false,
            ColumnDefault::None,
            Some("Soft delete timestamp".to_string()),
        ),
    ];

    TableDefinition::new(
        "system",
        "users",
        TableType::System,
        columns,
        TableOptions::system(),
        Some("System users for authentication and authorization".to_string()),
    )
    .expect("Failed to create system.users table definition")
}

/// Create TableDefinition for system.jobs table
///
/// Schema:
/// - job_id TEXT PRIMARY KEY
/// - job_type TEXT NOT NULL
/// - status TEXT NOT NULL
/// - created_at TIMESTAMP NOT NULL
/// - started_at TIMESTAMP
/// - completed_at TIMESTAMP
/// - error_message TEXT
pub fn jobs_table_definition() -> TableDefinition {
    let columns = vec![
        ColumnDefinition::new(
            "job_id",
            1,
            KalamDataType::Text,
            false,
            true,
            false,
            ColumnDefault::None,
            Some("Job identifier (UUID)".to_string()),
        ),
        ColumnDefinition::new(
            "job_type",
            2,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Job type: flush, retention, cleanup, etc.".to_string()),
        ),
        ColumnDefinition::new(
            "status",
            3,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Job status: pending, running, completed, failed, cancelled".to_string()),
        ),
        ColumnDefinition::new(
            "created_at",
            4,
            KalamDataType::Timestamp,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Job creation timestamp".to_string()),
        ),
        ColumnDefinition::new(
            "started_at",
            5,
            KalamDataType::Timestamp,
            true,
            false,
            false,
            ColumnDefault::None,
            Some("Job start timestamp".to_string()),
        ),
        ColumnDefinition::new(
            "completed_at",
            6,
            KalamDataType::Timestamp,
            true,
            false,
            false,
            ColumnDefault::None,
            Some("Job completion timestamp".to_string()),
        ),
        ColumnDefinition::new(
            "error_message",
            7,
            KalamDataType::Text,
            true,
            false,
            false,
            ColumnDefault::None,
            Some("Error message if job failed".to_string()),
        ),
    ];

    TableDefinition::new(
        "system",
        "jobs",
        TableType::System,
        columns,
        TableOptions::system(),
        Some("Background jobs for database maintenance".to_string()),
    )
    .expect("Failed to create system.jobs table definition")
}

/// Create TableDefinition for system.namespaces table
///
/// Schema:
/// - namespace_id TEXT PRIMARY KEY
/// - owner_user_id TEXT NOT NULL
/// - created_at TIMESTAMP NOT NULL
pub fn namespaces_table_definition() -> TableDefinition {
    let columns = vec![
        ColumnDefinition::new(
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
            "owner_user_id",
            2,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("User who owns this namespace".to_string()),
        ),
        ColumnDefinition::new(
            "created_at",
            3,
            KalamDataType::Timestamp,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Namespace creation timestamp".to_string()),
        ),
    ];

    TableDefinition::new(
        "system",
        "namespaces",
        TableType::System,
        columns,
        TableOptions::system(),
        Some("Database namespaces for multi-tenancy".to_string()),
    )
    .expect("Failed to create system.namespaces table definition")
}

/// Create TableDefinition for system.storages table
///
/// Schema:
/// - storage_id TEXT PRIMARY KEY
/// - storage_mode TEXT NOT NULL
/// - config JSON NOT NULL
/// - created_at TIMESTAMP NOT NULL
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
            "storage_mode",
            2,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Storage mode: Table, Parquet, S3".to_string()),
        ),
        ColumnDefinition::new(
            "config",
            3,
            KalamDataType::Json,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Storage configuration JSON".to_string()),
        ),
        ColumnDefinition::new(
            "created_at",
            4,
            KalamDataType::Timestamp,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Storage creation timestamp".to_string()),
        ),
    ];

    TableDefinition::new(
        "system",
        "storages",
        TableType::System,
        columns,
        TableOptions::system(),
        Some("Storage configurations for data persistence".to_string()),
    )
    .expect("Failed to create system.storages table definition")
}

/// Create TableDefinition for system.live_queries table
///
/// Schema:
/// - query_id TEXT PRIMARY KEY
/// - user_id TEXT NOT NULL
/// - query TEXT NOT NULL
/// - created_at TIMESTAMP NOT NULL
pub fn live_queries_table_definition() -> TableDefinition {
    let columns = vec![
        ColumnDefinition::new(
            "query_id",
            1,
            KalamDataType::Text,
            false,
            true,
            false,
            ColumnDefault::None,
            Some("Live query identifier (UUID)".to_string()),
        ),
        ColumnDefinition::new(
            "user_id",
            2,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("User who created the live query".to_string()),
        ),
        ColumnDefinition::new(
            "query",
            3,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("SQL query for real-time subscription".to_string()),
        ),
        ColumnDefinition::new(
            "created_at",
            4,
            KalamDataType::Timestamp,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Live query creation timestamp".to_string()),
        ),
    ];

    TableDefinition::new(
        "system",
        "live_queries",
        TableType::System,
        columns,
        TableOptions::system(),
        Some("Active WebSocket live query subscriptions".to_string()),
    )
    .expect("Failed to create system.live_queries table definition")
}

/// Create TableDefinition for system.tables table
///
/// Schema:
/// - table_id TEXT PRIMARY KEY (composite: namespace_id:table_name)
/// - namespace_id TEXT NOT NULL
/// - table_name TEXT NOT NULL
/// - table_type TEXT NOT NULL
/// - created_at TIMESTAMP NOT NULL
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
            "namespace_id",
            2,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Namespace containing this table".to_string()),
        ),
        ColumnDefinition::new(
            "table_name",
            3,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::None,
            Some("Table name within namespace".to_string()),
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
    ];

    TableDefinition::new(
        "system",
        "tables",
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
        "system",
        "table_schemas",
        TableType::System,
        columns,
        TableOptions::system(),
        Some("Table schema definitions with versioning".to_string()),
    )
    .expect("Failed to create system.table_schemas table definition")
}

/// Get TableId for a system table
pub fn system_table_id(table_name: &str) -> TableId {
    TableId::from_strings("system", table_name)
}

/// Get all system table definitions
pub fn all_system_table_definitions() -> Vec<(TableId, TableDefinition)> {
    vec![
        (system_table_id("users"), users_table_definition()),
        (system_table_id("jobs"), jobs_table_definition()),
        (system_table_id("namespaces"), namespaces_table_definition()),
        (system_table_id("storages"), storages_table_definition()),
        (system_table_id("live_queries"), live_queries_table_definition()),
        (system_table_id("tables"), tables_table_definition()),
        (system_table_id("table_schemas"), table_schemas_table_definition()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_users_table_definition() {
        let def = users_table_definition();
        assert_eq!(def.namespace_id, "system");
        assert_eq!(def.table_name, "users");
        assert_eq!(def.table_type, TableType::System);
        assert_eq!(def.columns.len(), 8);
        assert_eq!(def.columns[0].column_name, "user_id");
        assert!(def.columns[0].is_primary_key);
    }

    #[test]
    fn test_all_system_tables() {
        let all_tables = all_system_table_definitions();
        assert_eq!(all_tables.len(), 7);
        
        // Verify all tables are in system namespace
        for (table_id, def) in all_tables {
            assert_eq!(table_id.namespace_id().as_str(), "system");
            assert_eq!(def.namespace_id, "system");
            assert_eq!(def.table_type, TableType::System);
        }
    }

    #[test]
    fn test_table_schemas_definition() {
        let def = table_schemas_table_definition();
        assert_eq!(def.namespace_id, "system");
        assert_eq!(def.table_name, "table_schemas");
        assert_eq!(def.columns.len(), 5);
        assert_eq!(def.columns[2].column_name, "table_definition");
        assert_eq!(def.columns[2].data_type, KalamDataType::Json);
    }
}
