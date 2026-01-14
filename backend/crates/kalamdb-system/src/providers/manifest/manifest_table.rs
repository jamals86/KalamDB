//! System.manifest table schema
//!
//! This module defines the schema for the system.manifest table (manifest cache).
//! - TableDefinition: Source of truth for columns, types, comments
//! - Arrow schema: Derived from TableDefinition, memoized via OnceLock

use datafusion::arrow::datatypes::SchemaRef;
use kalamdb_commons::datatypes::KalamDataType;
use kalamdb_commons::schemas::{
    ColumnDefault, ColumnDefinition, TableDefinition, TableOptions, TableType,
};
use kalamdb_commons::{NamespaceId, SystemTable, TableName};
use std::sync::OnceLock;

/// Schema provider for system.manifest table
///
/// Provides typed access to the table definition and Arrow schema.
/// Contains the full TableDefinition as the single source of truth.
#[derive(Debug, Clone, Copy)]
pub struct ManifestTableSchema;

impl ManifestTableSchema {
    /// Get the TableDefinition for system.manifest
    ///
    /// This is the single source of truth for:
    /// - Column definitions (names, types, nullability)
    /// - Column ordering (ordinal_position)
    /// - Column comments/descriptions
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
    pub fn definition() -> TableDefinition {
        let columns = vec![
            ColumnDefinition::new(
                1,
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
                2,
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
                3,
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
                4,
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
                5,
                "etag",
                5,
                KalamDataType::Text,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Storage ETag or version identifier".to_string()),
            ),
            ColumnDefinition::new(
                6,
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
                7,
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
                8,
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
                9,
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
                10,
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
                11,
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
            TableName::new(SystemTable::Manifest.table_name()),
            TableType::System,
            columns,
            TableOptions::system(),
            Some("Manifest cache entries for query optimization".to_string()),
        )
        .expect("Failed to create system.manifest table definition")
    }

    /// Get the cached Arrow schema for system.manifest table
    pub fn schema() -> SchemaRef {
        static SCHEMA: OnceLock<SchemaRef> = OnceLock::new();
        SCHEMA
            .get_or_init(|| {
                Self::definition()
                    .to_arrow_schema()
                    .expect("Failed to convert manifest TableDefinition to Arrow schema")
            })
            .clone()
    }

    /// Get the table name
    pub fn table_name() -> &'static str {
        SystemTable::Manifest.table_name()
    }

    /// Get the column family name in RocksDB
    pub fn column_family_name() -> &'static str {
        SystemTable::Manifest
            .column_family_name()
            .expect("Manifest is a table, not a view")
    }
}
