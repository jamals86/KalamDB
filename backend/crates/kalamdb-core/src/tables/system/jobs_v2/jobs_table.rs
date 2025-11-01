//! System.jobs table schema (system_jobs in RocksDB)
//!
//! This module defines the Arrow schema for the system.jobs table.
//! Uses OnceLock for zero-overhead static schema caching.
//! Schema is built from TableDefinition to ensure column ordering by ordinal_position.

use datafusion::arrow::datatypes::SchemaRef;
use std::sync::OnceLock;

use crate::tables::system::system_table_definitions::jobs_table_definition;

/// Static schema cache for the jobs table
static JOBS_SCHEMA: OnceLock<SchemaRef> = OnceLock::new();

/// System jobs table schema definition
pub struct JobsTableSchema;

impl JobsTableSchema {
    /// Get the cached schema for the system.jobs table
    ///
    /// Uses OnceLock to ensure the schema is created exactly once and reused
    /// across all providers without synchronization overhead.
    ///
    /// Schema is built from TableDefinition which ensures columns are ordered
    /// by ordinal_position (Phase 4 requirement for SELECT * column ordering).
    pub fn schema() -> SchemaRef {
        JOBS_SCHEMA
            .get_or_init(|| {
                jobs_table_definition()
                    .to_arrow_schema()
                    .expect("Failed to convert jobs TableDefinition to Arrow schema")
            })
            .clone()
    }

    /// Get the table name
    pub fn table_name() -> &'static str {
        kalamdb_commons::SystemTable::Jobs.table_name()
    }

    /// Get the column family name in RocksDB
    pub fn column_family_name() -> &'static str {
        kalamdb_commons::SystemTable::Jobs.column_family_name()
    }

    /// Get the partition key for storage
    pub fn partition() -> &'static str {
        kalamdb_commons::SystemTable::Jobs.column_family_name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jobs_table_schema() {
        let schema = JobsTableSchema::schema();
        // Schema is built from TableDefinition which has 15 columns
        assert_eq!(schema.fields().len(), 15);

        // Verify columns are ordered by ordinal_position (from TableDefinition)
        assert_eq!(schema.field(0).name(), "job_id"); // ordinal 1
        assert_eq!(schema.field(1).name(), "job_type"); // ordinal 2
        assert_eq!(schema.field(2).name(), "namespace_id"); // ordinal 3
                                                            // ... other fields follow ordinal_position order from jobs_table_definition()
    }

    #[test]
    fn test_jobs_table_name() {
        assert_eq!(JobsTableSchema::table_name(), "jobs");
        assert_eq!(JobsTableSchema::column_family_name(), "system_jobs");
    }

    #[test]
    fn test_schema_caching() {
        let schema1 = JobsTableSchema::schema();
        let schema2 = JobsTableSchema::schema();
        assert!(Arc::ptr_eq(&schema1, &schema2), "Schema should be cached");
    }
}
