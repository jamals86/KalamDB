//! Schema definition for system.storages table
//!
//! This module defines the schema for the system.storages table.
//! - TableDefinition: Source of truth for columns, types, comments
//! - Arrow schema: Derived from TableDefinition, memoized via OnceLock

use datafusion::arrow::datatypes::SchemaRef;
use kalamdb_commons::datatypes::KalamDataType;
use kalamdb_commons::schemas::{
    ColumnDefault, ColumnDefinition, TableDefinition, TableOptions, TableType,
};
use kalamdb_commons::{NamespaceId, SystemTable, TableName};
use std::sync::OnceLock;

/// Schema provider for system.storages table
///
/// Provides typed access to the table definition and Arrow schema.
/// Contains the full TableDefinition as the single source of truth.
#[derive(Debug, Clone, Copy)]
pub struct StoragesTableSchema;

impl StoragesTableSchema {
    /// Get the TableDefinition for system.storages
    ///
    /// This is the single source of truth for:
    /// - Column definitions (names, types, nullability)
    /// - Column ordering (ordinal_position)
    /// - Column comments/descriptions
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
    pub fn definition() -> TableDefinition {
        let columns = vec![
            ColumnDefinition::new(
                1,
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
                2,
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
                3,
                "description",
                3,
                KalamDataType::Text,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Storage description".to_string()),
            ),
            ColumnDefinition::new(
                4,
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
                5,
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
                6,
                "credentials",
                6,
                KalamDataType::Text,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Storage credentials JSON (WARNING: stored as plaintext - use environment variables for sensitive credentials)".to_string()),
            ),
            ColumnDefinition::new(
                7,
                "config_json",
                7,
                KalamDataType::Text,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Backend-specific storage configuration JSON".to_string()),
            ),
            ColumnDefinition::new(
                8,
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
                9,
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
                10,
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
                11,
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
            TableName::new(SystemTable::Storages.table_name()),
            TableType::System,
            columns,
            TableOptions::system(),
            Some("Storage configurations for data persistence".to_string()),
        )
        .expect("Failed to create system.storages table definition")
    }

    /// Get the cached Arrow schema for system.storages table
    pub fn schema() -> SchemaRef {
        static SCHEMA: OnceLock<SchemaRef> = OnceLock::new();
        SCHEMA
            .get_or_init(|| {
                Self::definition()
                    .to_arrow_schema()
                    .expect("Failed to convert storages TableDefinition to Arrow schema")
            })
            .clone()
    }

    /// Get the table name
    pub fn table_name() -> &'static str {
        SystemTable::Storages.table_name()
    }

    /// Get the column family name in RocksDB
    pub fn column_family_name() -> &'static str {
        SystemTable::Storages
            .column_family_name()
            .expect("Storages is a table, not a view")
    }

    /// Get the partition key for storage
    pub fn partition() -> &'static str {
        Self::column_family_name()
    }
}
