//! System.tables table schema (system_tables in RocksDB)
//!
//! This module defines the Arrow schema for the system.tables table.
//! Uses OnceLock for zero-overhead static schema caching.
//! 
//! Phase 4 (Column Ordering): Uses tables_table_definition().to_arrow_schema()
//! to ensure consistent column ordering via ordinal_position field.

use datafusion::arrow::datatypes::SchemaRef;
use std::sync::OnceLock;
use crate::tables::system::system_table_definitions::tables_table_definition;

/// Static schema cache for the tables table
static TABLES_SCHEMA: OnceLock<SchemaRef> = OnceLock::new();

/// System tables table schema definition
pub struct TablesTableSchema;

impl TablesTableSchema {
    /// Get the cached schema for the system.tables table
    ///
    /// Uses OnceLock to ensure the schema is created exactly once and reused
    /// across all providers without synchronization overhead.
    /// 
    /// Schema fields (in ordinal_position order):
    /// - table_id: Utf8 (primary key)
    /// - table_name: Utf8
    /// - namespace: Utf8
    /// - table_type: Utf8
    /// - created_at: Timestamp(Millisecond, None)
    /// - storage_location: Utf8
    /// - storage_id: Utf8 (nullable)
    /// - use_user_storage: Boolean
    /// - flush_policy: Utf8
    /// - schema_version: Int32
    /// - deleted_retention_hours: Int32
    /// - access_level: Utf8 (nullable)
    pub fn schema() -> SchemaRef {
        TABLES_SCHEMA
            .get_or_init(|| {
                tables_table_definition()
                    .to_arrow_schema()
                    .expect("Failed to convert tables TableDefinition to Arrow schema")
            })
            .clone()
    }

    /// Get the table name
    pub fn table_name() -> &'static str {
        kalamdb_commons::SystemTable::Tables.table_name()
    }

    /// Get the column family name in RocksDB
    pub fn column_family_name() -> &'static str {
        kalamdb_commons::SystemTable::Tables.column_family_name()
    }

    /// Get the partition key for storage
    pub fn partition() -> &'static str {
        kalamdb_commons::SystemTable::Tables.column_family_name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_tables_table_schema() {
        let schema = TablesTableSchema::schema();
        // Schema built from TableDefinition, verify field count and names are correct
        assert_eq!(schema.fields().len(), 12);
        
        // Verify fields exist (order guaranteed by TableDefinition's ordinal_position)
        let field_names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
        assert!(field_names.contains(&"table_id"));
        assert!(field_names.contains(&"table_name"));
        assert!(field_names.contains(&"namespace"));
        assert!(field_names.contains(&"table_type"));
        assert!(field_names.contains(&"created_at"));
        assert!(field_names.contains(&"storage_location"));
        assert!(field_names.contains(&"storage_id"));
        assert!(field_names.contains(&"use_user_storage"));
        assert!(field_names.contains(&"flush_policy"));
        assert!(field_names.contains(&"schema_version"));
        assert!(field_names.contains(&"deleted_retention_hours"));
        assert!(field_names.contains(&"access_level"));
    }

    #[test]
    fn test_tables_table_name() {
        assert_eq!(TablesTableSchema::table_name(), "tables");
        assert_eq!(TablesTableSchema::column_family_name(), "system_tables");
    }

    #[test]
    fn test_schema_caching() {
        let schema1 = TablesTableSchema::schema();
        let schema2 = TablesTableSchema::schema();
        assert!(Arc::ptr_eq(&schema1, &schema2), "Schema should be cached");
    }
}
