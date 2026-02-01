//! System.topic_offsets table schema (system_topic_offsets in RocksDB)
//!
//! This module defines the schema for the system.topic_offsets table.
//! - TableDefinition: Source of truth for columns, types, comments
//! - Arrow schema: Derived from TableDefinition, memoized via OnceLock

use datafusion::arrow::datatypes::SchemaRef;
use kalamdb_commons::schemas::TableDefinition;
use kalamdb_commons::SystemTable;
use std::sync::OnceLock;

/// System topic_offsets table schema definition
///
/// Provides typed access to the table definition and Arrow schema.
/// Contains the full TableDefinition as the single source of truth.
#[derive(Debug, Clone, Copy)]
pub struct TopicOffsetsTableSchema;

impl TopicOffsetsTableSchema {
    /// Get the TableDefinition for system.topic_offsets
    ///
    /// This is the single source of truth for:
    /// - Column definitions (names, types, nullability)
    /// - Column ordering (ordinal_position)
    /// - Column comments/descriptions
    /// - Composite primary key (topic_id, group_id, partition_id)
    ///
    /// Schema:
    /// - topic_id TEXT PRIMARY KEY
    /// - group_id TEXT PRIMARY KEY
    /// - partition_id INT PRIMARY KEY
    /// - last_acked_offset BIGINT NOT NULL DEFAULT 0
    /// - updated_at TIMESTAMP NOT NULL
    pub fn definition() -> TableDefinition {
        use super::models::TopicOffset;
        TopicOffset::definition()
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
                table_def.to_arrow_schema().expect("Failed to convert topic_offsets TableDefinition to Arrow schema")
            })
            .clone()
    }

    /// Get the table name
    pub fn table_name() -> &'static str {
        SystemTable::TopicOffsets.table_name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topic_offsets_schema() {
        let def = TopicOffsetsTableSchema::definition();
        assert_eq!(def.table_name.as_str(), "topic_offsets");
        assert_eq!(def.columns.len(), 5);

        // Composite primary key
        assert_eq!(def.columns[0].column_name, "topic_id");
        assert_eq!(def.columns[0].is_primary_key, true);
        assert_eq!(def.columns[1].column_name, "group_id");
        assert_eq!(def.columns[1].is_primary_key, true);
        assert_eq!(def.columns[2].column_name, "partition_id");
        assert_eq!(def.columns[2].is_primary_key, true);
    }

    #[test]
    fn test_topic_offsets_arrow_schema() {
        let schema = TopicOffsetsTableSchema::schema();
        assert_eq!(schema.fields().len(), 5);
        assert_eq!(schema.field(0).name(), "topic_id");
        assert_eq!(schema.field(1).name(), "group_id");
        assert_eq!(schema.field(2).name(), "partition_id");
    }

    #[test]
    fn test_topic_offsets_schema_cached() {
        let schema1 = TopicOffsetsTableSchema::schema();
        let schema2 = TopicOffsetsTableSchema::schema();
        assert!(std::ptr::eq(schema1.as_ref(), schema2.as_ref()));
    }
}
