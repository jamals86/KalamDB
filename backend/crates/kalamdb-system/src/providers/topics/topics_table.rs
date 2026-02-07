//! System.topics table schema (system_topics in RocksDB)
//!
//! This module defines the schema for the system.topics table.
//! - TableDefinition: Source of truth for columns, types, comments
//! - Arrow schema: Derived from TableDefinition, memoized via OnceLock

use datafusion::arrow::datatypes::SchemaRef;
use kalamdb_commons::schemas::TableDefinition;
use kalamdb_commons::SystemTable;
use std::sync::OnceLock;

/// System topics table schema definition
///
/// Provides typed access to the table definition and Arrow schema.
/// Contains the full TableDefinition as the single source of truth.
#[derive(Debug, Clone, Copy)]
pub struct TopicsTableSchema;

impl TopicsTableSchema {
    /// Get the TableDefinition for system.topics
    ///
    /// This is the single source of truth for:
    /// - Column definitions (names, types, nullability)
    /// - Column ordering (ordinal_position)
    /// - Column comments/descriptions
    ///
    /// Schema:
    /// - topic_id TEXT PRIMARY KEY
    /// - name TEXT NOT NULL (unique)
    /// - alias TEXT (unique, nullable)
    /// - partitions INT NOT NULL DEFAULT 1
    /// - retention_seconds BIGINT (nullable)
    /// - retention_max_bytes BIGINT (nullable)
    /// - routes JSON NOT NULL
    /// - created_at TIMESTAMP NOT NULL
    /// - updated_at TIMESTAMP NOT NULL
    pub fn definition() -> TableDefinition {
        use super::models::Topic;
        Topic::definition()
    }

    /// Get Arrow schema (cached)
    ///
    /// Returns a cached Arrow SchemaRef for query processing.
    /// Schema is built once from the TableDefinition and reused.
    pub fn schema() -> SchemaRef {
        static SCHEMA: OnceLock<SchemaRef> = OnceLock::new();
        SCHEMA
            .get_or_init(|| {
                let table_def = Self::definition();
                table_def
                    .to_arrow_schema()
                    .expect("Failed to convert topics TableDefinition to Arrow schema")
            })
            .clone()
    }

    /// Get the table name
    pub fn table_name() -> &'static str {
        SystemTable::Topics.table_name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topics_schema() {
        let def = TopicsTableSchema::definition();
        assert_eq!(def.table_name.as_str(), "topics");
        assert_eq!(def.columns.len(), 9);
        assert_eq!(def.columns[0].column_name, "topic_id");
        assert_eq!(def.columns[0].is_primary_key, true);
    }

    #[test]
    fn test_topics_arrow_schema() {
        let schema = TopicsTableSchema::schema();
        assert_eq!(schema.fields().len(), 9);
        assert_eq!(schema.field(0).name(), "topic_id");
    }

    #[test]
    fn test_topics_schema_cached() {
        let schema1 = TopicsTableSchema::schema();
        let schema2 = TopicsTableSchema::schema();
        assert!(std::ptr::eq(schema1.as_ref(), schema2.as_ref()));
    }
}
