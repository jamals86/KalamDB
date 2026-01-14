//! System.tables table schema (system_tables in RocksDB)
//!
//! This module defines the schema for the system.tables table.
//! - TableDefinition: Source of truth for columns, types, comments
//! - Arrow schema: Derived from TableDefinition, memoized via OnceLock

use datafusion::arrow::datatypes::SchemaRef;
use kalamdb_commons::datatypes::KalamDataType;
use kalamdb_commons::schemas::{
    ColumnDefault, ColumnDefinition, TableDefinition, TableOptions, TableType,
};
use kalamdb_commons::{NamespaceId, SystemTable, TableName};
use std::sync::OnceLock;

/// System tables table schema definition
///
/// Provides typed access to the table definition and Arrow schema.
/// Contains the full TableDefinition as the single source of truth.
#[derive(Debug, Clone, Copy)]
pub struct TablesTableSchema;

impl TablesTableSchema {
    /// Get the TableDefinition for system.tables
    ///
    /// This is the single source of truth for:
    /// - Column definitions (names, types, nullability)
    /// - Column ordering (ordinal_position)
    /// - Column comments/descriptions
    ///
    /// Schema (flattened view of TableDefinition):
    /// - table_id TEXT PRIMARY KEY (composite: namespace_id:table_name)
    /// - table_name TEXT NOT NULL
    /// - namespace_id TEXT NOT NULL
    /// - table_type TEXT NOT NULL
    /// - created_at TIMESTAMP NOT NULL
    /// - schema_version INT NOT NULL
    /// - columns TEXT NOT NULL (JSON array)
    /// - table_comment TEXT (nullable)
    /// - updated_at TIMESTAMP NOT NULL
    /// - options TEXT (nullable, serialized TableOptions JSON)
    /// - access_level TEXT (nullable, for Shared tables)
    /// - is_latest BOOLEAN NOT NULL
    /// - storage_id TEXT (nullable)
    /// - use_user_storage BOOLEAN (nullable, for User tables)
    pub fn definition() -> TableDefinition {
        let columns = vec![
            ColumnDefinition::new(
                1,
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
                2,
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
                3,
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
                4,
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
                5,
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
                6,
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
                7,
                "columns",
                7,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Column definitions as JSON array".to_string()),
            ),
            ColumnDefinition::new(
                8,
                "table_comment",
                8,
                KalamDataType::Text,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Optional table description or comment".to_string()),
            ),
            ColumnDefinition::new(
                9,
                "updated_at",
                9,
                KalamDataType::Timestamp,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Last modification timestamp".to_string()),
            ),
            ColumnDefinition::new(
                10,
                "options",
                10,
                KalamDataType::Text,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Serialized table options (JSON)".to_string()),
            ),
            ColumnDefinition::new(
                11,
                "access_level",
                11,
                KalamDataType::Text,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Access level for Shared tables: public, private, protected".to_string()),
            ),
            ColumnDefinition::new(
                12,
                "is_latest",
                12,
                KalamDataType::Boolean,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Whether this is the latest version of the table schema".to_string()),
            ),
            ColumnDefinition::new(
                13,
                "storage_id",
                13,
                KalamDataType::Text,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Storage backend identifier for this table".to_string()),
            ),
            ColumnDefinition::new(
                14,
                "use_user_storage",
                14,
                KalamDataType::Boolean,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Whether this table uses user-specific storage assignment".to_string()),
            ),
        ];

        TableDefinition::new(
            NamespaceId::system(),
            TableName::new(SystemTable::Tables.table_name()),
            TableType::System,
            columns,
            TableOptions::system(),
            Some("Registry of all tables in the database".to_string()),
        )
        .expect("Failed to create system.tables table definition")
    }

    /// Get the cached Arrow schema for system.tables table
    pub fn schema() -> SchemaRef {
        static SCHEMA: OnceLock<SchemaRef> = OnceLock::new();
        SCHEMA
            .get_or_init(|| {
                Self::definition()
                    .to_arrow_schema()
                    .expect("Failed to convert tables TableDefinition to Arrow schema")
            })
            .clone()
    }

    /// Get the table name
    pub fn table_name() -> &'static str {
        SystemTable::Tables.table_name()
    }

    /// Get the column family name in RocksDB
    pub fn column_family_name() -> &'static str {
        SystemTable::Tables
            .column_family_name()
            .expect("Tables is a table, not a view")
    }

    /// Get the partition key for storage
    pub fn partition() -> &'static str {
        Self::column_family_name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_tables_table_schema() {
        let schema = TablesTableSchema::schema();
        // Schema built from TableDefinition, verify field count matches definition
        // Expecting 14 fields: table_id, table_name, namespace_id, table_type, created_at, 
        // schema_version, columns, table_comment, updated_at, options, access_level, is_latest,
        // storage_id, use_user_storage
        assert_eq!(schema.fields().len(), 14);

        // Verify fields exist (order guaranteed by TableDefinition's ordinal_position)
        let field_names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
        assert!(field_names.contains(&"table_id"));
        assert!(field_names.contains(&"table_name"));
        assert!(field_names.contains(&"namespace_id"));
        assert!(field_names.contains(&"table_type"));
        assert!(field_names.contains(&"created_at"));
        assert!(field_names.contains(&"columns"));
        assert!(field_names.contains(&"options"));
        assert!(field_names.contains(&"access_level"));
        assert!(field_names.contains(&"is_latest"));
    }

    #[test]
    fn test_tables_table_name() {
        assert_eq!(TablesTableSchema::table_name(), "tables");
        assert_eq!(
            TablesTableSchema::column_family_name(),
            SystemTable::Tables
                .column_family_name()
                .expect("Tables is a table, not a view")
        );
    }

    #[test]
    fn test_schema_caching() {
        let schema1 = TablesTableSchema::schema();
        let schema2 = TablesTableSchema::schema();
        assert!(Arc::ptr_eq(&schema1, &schema2), "Schema should be cached");
    }
}
