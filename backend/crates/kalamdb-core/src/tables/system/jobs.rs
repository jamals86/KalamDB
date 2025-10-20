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
            Field::new(
                "start_time",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
            Field::new(
                "end_time",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                true,
            ),
            Field::new("parameters", DataType::Utf8, true), // JSON
            Field::new("result", DataType::Utf8, true),     // JSON
            Field::new("trace", DataType::Utf8, true),
            Field::new("memory_used_mb", DataType::Float64, true),
            Field::new("cpu_used_percent", DataType::Float64, true),
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
        assert_eq!(schema.fields().len(), 13);
        assert_eq!(schema.field(0).name(), "job_id");
        assert_eq!(schema.field(1).name(), "job_type");
        assert_eq!(schema.field(2).name(), "table_name");
        assert_eq!(schema.field(3).name(), "status");
        assert_eq!(schema.field(4).name(), "start_time");
        assert_eq!(schema.field(5).name(), "end_time");
        assert_eq!(schema.field(6).name(), "parameters");
        assert_eq!(schema.field(7).name(), "result");
        assert_eq!(schema.field(8).name(), "trace");
        assert_eq!(schema.field(9).name(), "memory_used_mb");
        assert_eq!(schema.field(10).name(), "cpu_used_percent");
        assert_eq!(schema.field(11).name(), "node_id");
        assert_eq!(schema.field(12).name(), "error_message");
    }

    #[test]
    fn test_jobs_table_name() {
        assert_eq!(JobsTable::table_name(), "jobs");
        assert_eq!(JobsTable::column_family_name(), "system_jobs");
    }
}
