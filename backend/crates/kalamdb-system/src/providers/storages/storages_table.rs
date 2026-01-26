//! Schema definition for system.storages table
//!
//! This module defines the schema for the system.storages table.
//! - TableDefinition: Source of truth for columns, types, comments
//! - Arrow schema: Derived from TableDefinition, memoized via OnceLock

use datafusion::arrow::datatypes::SchemaRef;
use kalamdb_commons::schemas::TableDefinition;
use kalamdb_commons::SystemTable;
use crate::providers::storages::models::Storage;
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
        Storage::definition()
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
