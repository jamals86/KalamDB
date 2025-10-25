//! System.jobs table schema
//!
//! This module defines the schema for the system.jobs table.

use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use std::sync::Arc;

/// System jobs table schema
pub struct JobsTable;

impl JobsTable {
    /// Get the schema for the system.jobs table
    pub fn schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("job_id", DataType::Utf8, false),
            Field::new("job_type", DataType::Utf8, false), // "flush", "compact", etc.
            Field::new("table_name", DataType::Utf8, true),
            Field::new("status", DataType::Utf8, false), // "running", "completed", "failed"
            Field::new("parameters", DataType::Utf8, true), // JSON array as string
            Field::new("result", DataType::Utf8, true),
            Field::new("trace", DataType::Utf8, true),
            Field::new("memory_used", DataType::Int64, true), // bytes
            Field::new("cpu_used", DataType::Int64, true),     // microseconds
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
    }

    /// Get the table name
    pub fn table_name() -> &'static str {
        "jobs"
    }

    /// Get the column family name
    pub fn column_family_name() -> String {
        format!("system_{}", Self::table_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jobs_table_schema() {
        let schema = JobsTable::schema();
        assert_eq!(schema.fields().len(), 14);
        assert_eq!(schema.field(0).name(), "job_id");
        assert_eq!(schema.field(1).name(), "job_type");
        assert_eq!(schema.field(2).name(), "table_name");
        assert_eq!(schema.field(3).name(), "status");
        assert_eq!(schema.field(4).name(), "parameters");
        assert_eq!(schema.field(5).name(), "result");
        assert_eq!(schema.field(6).name(), "trace");
        assert_eq!(schema.field(7).name(), "memory_used");
        assert_eq!(schema.field(8).name(), "cpu_used");
        assert_eq!(schema.field(9).name(), "created_at");
        assert_eq!(schema.field(10).name(), "started_at");
        assert_eq!(schema.field(11).name(), "completed_at");
        assert_eq!(schema.field(12).name(), "node_id");
        assert_eq!(schema.field(13).name(), "error_message");
    }

    #[test]
    fn test_jobs_table_name() {
        assert_eq!(JobsTable::table_name(), "jobs");
        assert_eq!(JobsTable::column_family_name(), "system_jobs");
    }
}
