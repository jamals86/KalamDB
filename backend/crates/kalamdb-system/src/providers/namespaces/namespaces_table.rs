//! Schema definition for system.namespaces table
//!
//! This module defines the schema for the system.namespaces table.
//! - TableDefinition: Source of truth for columns, types, comments
//! - Arrow schema: Derived from TableDefinition, memoized via OnceLock

use datafusion::arrow::datatypes::SchemaRef;
use kalamdb_commons::datatypes::KalamDataType;
use kalamdb_commons::schemas::{
    ColumnDefault, ColumnDefinition, TableDefinition, TableOptions, TableType,
};
use kalamdb_commons::{NamespaceId, SystemTable, TableName};
use std::sync::OnceLock;

/// Schema provider for system.namespaces table
///
/// Provides typed access to the table definition and Arrow schema.
/// Contains the full TableDefinition as the single source of truth.
#[derive(Debug, Clone, Copy)]
pub struct NamespacesTableSchema;

impl NamespacesTableSchema {
    /// Get the TableDefinition for system.namespaces
    ///
    /// This is the single source of truth for:
    /// - Column definitions (names, types, nullability)
    /// - Column ordering (ordinal_position)
    /// - Column comments/descriptions
    ///
    /// Schema:
    /// - namespace_id TEXT PRIMARY KEY
    /// - name TEXT NOT NULL
    /// - created_at TIMESTAMP NOT NULL
    /// - options TEXT (nullable, JSON configuration)
    /// - table_count INT NOT NULL
    pub fn definition() -> TableDefinition {
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
                KalamDataType::Json,
                true,
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
            TableName::new(SystemTable::Namespaces.table_name()),
            TableType::System,
            columns,
            TableOptions::system(),
            Some("Database namespaces for multi-tenancy".to_string()),
        )
        .expect("Failed to create system.namespaces table definition")
    }

    /// Get the cached Arrow schema for system.namespaces table
    pub fn schema() -> SchemaRef {
        static SCHEMA: OnceLock<SchemaRef> = OnceLock::new();
        SCHEMA
            .get_or_init(|| {
                Self::definition()
                    .to_arrow_schema()
                    .expect("Failed to convert namespaces TableDefinition to Arrow schema")
            })
            .clone()
    }

    /// Get the table name
    pub fn table_name() -> &'static str {
        SystemTable::Namespaces.table_name()
    }

    /// Get the column family name in RocksDB
    pub fn column_family_name() -> &'static str {
        SystemTable::Namespaces
            .column_family_name()
            .expect("Namespaces is a table, not a view")
    }

    /// Get the partition key for storage
    pub fn partition() -> &'static str {
        Self::column_family_name()
    }
}
