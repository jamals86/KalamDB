//! Schema definition for system.namespaces table
//!
//! This module defines the schema for the system.namespaces table.
//! - TableDefinition: Source of truth for columns, types, comments
//! - Arrow schema: Derived from TableDefinition, memoized via OnceLock

use crate::providers::namespaces::models::Namespace;
use datafusion::arrow::datatypes::SchemaRef;
use kalamdb_commons::schemas::TableDefinition;
use kalamdb_commons::SystemTable;
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
        Namespace::definition()
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
