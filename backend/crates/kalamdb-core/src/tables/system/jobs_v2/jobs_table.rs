//! System.jobs table schema (system_jobs in RocksDB)
//!
//! This module defines the Arrow schema for the system.jobs table.
//! Uses OnceLock for zero-overhead static schema caching.

use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use std::sync::{Arc, OnceLock};

/// Static schema cache for the jobs table
static JOBS_SCHEMA: OnceLock<Arc<Schema>> = OnceLock::new();

/// System jobs table schema definition
pub struct JobsTableSchema;

impl JobsTableSchema {
    /// Get the cached schema for the system.jobs table
    /// 
    /// Uses OnceLock to ensure the schema is created exactly once and reused
    /// across all providers without synchronization overhead.
    pub fn schema() -> SchemaRef {
        JOBS_SCHEMA
            .get_or_init(|| {
                Arc::new(Schema::new(vec![
                    Field::new("job_id", DataType::Utf8, false),
                    Field::new("job_type", DataType::Utf8, false),
                    Field::new("namespace_id", DataType::Utf8, false),
                    Field::new("table_name", DataType::Utf8, true),
                    Field::new("status", DataType::Utf8, false),
                    Field::new("parameters", DataType::Utf8, true),
                    Field::new("result", DataType::Utf8, true),
                    Field::new("trace", DataType::Utf8, true),
                    Field::new("memory_used", DataType::Int64, true),
                    Field::new("cpu_used", DataType::Int64, true),
                    Field::new(
                        "created_at",
                        DataType::Timestamp(TimeUnit::Millisecond, None),
                        false,
                    ),
                    Field::new(
                        "started_at",
                        DataType::Timestamp(TimeUnit::Millisecond, None),
                        true,
                    ),
                    Field::new(
                        "completed_at",
                        DataType::Timestamp(TimeUnit::Millisecond, None),
                        true,
                    ),
                    Field::new("node_id", DataType::Utf8, false),
                    Field::new("error_message", DataType::Utf8, true),
                ]))
            })
            .clone()
    }

    /// Get the table name
    pub fn table_name() -> &'static str {
        "jobs"
    }

    /// Get the column family name in RocksDB
    pub fn column_family_name() -> &'static str {
        "system_jobs"
    }

    /// Get the partition key for storage
    pub fn partition() -> &'static str {
        "system_jobs"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jobs_table_schema() {
        let schema = JobsTableSchema::schema();
        assert_eq!(schema.fields().len(), 15);
        assert_eq!(schema.field(0).name(), "job_id");
        assert_eq!(schema.field(1).name(), "job_type");
        assert_eq!(schema.field(2).name(), "namespace_id");
        assert_eq!(schema.field(3).name(), "table_name");
        assert_eq!(schema.field(4).name(), "status");
        assert_eq!(schema.field(13).name(), "node_id");
        assert_eq!(schema.field(14).name(), "error_message");
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
