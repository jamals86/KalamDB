//! System.jobs table schema (system_jobs in RocksDB)
//!
//! This module defines the schema for the system.jobs table.
//! - TableDefinition: Source of truth for columns, types, comments
//! - Arrow schema: Derived from TableDefinition, memoized via OnceLock

use datafusion::arrow::datatypes::SchemaRef;
use kalamdb_commons::datatypes::KalamDataType;
use kalamdb_commons::schemas::{
    ColumnDefault, ColumnDefinition, TableDefinition, TableOptions, TableType,
};
use kalamdb_commons::{NamespaceId, SystemTable, TableName};
use std::sync::OnceLock;

/// System jobs table schema definition
///
/// Provides typed access to the table definition and Arrow schema.
/// Contains the full TableDefinition as the single source of truth.
#[derive(Debug, Clone, Copy)]
pub struct JobsTableSchema;

impl JobsTableSchema {
    /// Get the TableDefinition for system.jobs
    ///
    /// This is the single source of truth for:
    /// - Column definitions (names, types, nullability)
    /// - Column ordering (ordinal_position)
    /// - Column comments/descriptions
    ///
    /// Schema:
    /// - job_id TEXT PRIMARY KEY
    /// - job_type TEXT NOT NULL
    /// - status TEXT NOT NULL
    /// - parameters TEXT (JSON containing namespace_id, table_name, etc.)
    /// - result TEXT
    /// - trace TEXT
    /// - memory_used BIGINT
    /// - cpu_used BIGINT
    /// - created_at TIMESTAMP NOT NULL DEFAULT NOW()
    /// - started_at TIMESTAMP
    /// - completed_at TIMESTAMP
    /// - node_id TEXT NOT NULL
    /// - error_message TEXT
    pub fn definition() -> TableDefinition {
        let columns = vec![
            ColumnDefinition::new(
                1,
                "job_id",
                1,
                KalamDataType::Text,
                false,
                true,
                false,
                ColumnDefault::None,
                Some("Job identifier (UUID)".to_string()),
            ),
            ColumnDefinition::new(
                2,
                "job_type",
                2,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Job type: flush, retention, cleanup, etc.".to_string()),
            ),
            ColumnDefinition::new(
                3,
                "status",
                3,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Job status: pending, running, completed, failed, cancelled".to_string()),
            ),
            ColumnDefinition::new(
                4,
                "parameters",
                4,
                KalamDataType::Json,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Job parameters (JSON) - contains namespace_id, table_name, etc.".to_string()),
            ),
            ColumnDefinition::new(
                5,
                "result",
                5,
                KalamDataType::Text,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Job result".to_string()),
            ),
            ColumnDefinition::new(
                6,
                "trace",
                6,
                KalamDataType::Text,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Job trace information".to_string()),
            ),
            ColumnDefinition::new(
                7,
                "memory_used",
                7,
                KalamDataType::BigInt,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Memory used in bytes".to_string()),
            ),
            ColumnDefinition::new(
                8,
                "cpu_used",
                8,
                KalamDataType::BigInt,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("CPU time used in microseconds".to_string()),
            ),
            ColumnDefinition::new(
                9,
                "created_at",
                9,
                KalamDataType::Timestamp,
                false,
                false,
                false,
                ColumnDefault::FunctionCall {
                    name: "NOW".to_string(),
                    args: vec![],
                },
                Some("Job creation timestamp".to_string()),
            ),
            ColumnDefinition::new(
                10,
                "started_at",
                10,
                KalamDataType::Timestamp,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Job start timestamp".to_string()),
            ),
            ColumnDefinition::new(
                11,
                "completed_at",
                11,
                KalamDataType::Timestamp,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Job completion timestamp".to_string()),
            ),
            ColumnDefinition::new(
                12,
                "node_id",
                12,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Node ID executing the job".to_string()),
            ),
            ColumnDefinition::new(
                13,
                "leader_node_id",
                13,
                KalamDataType::Text,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Node ID that performed leader actions (if any)".to_string()),
            ),
            ColumnDefinition::new(
                14,
                "leader_status",
                14,
                KalamDataType::Text,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Status of leader-only actions (running, completed, failed)".to_string()),
            ),
            ColumnDefinition::new(
                15,
                "error_message",
                15,
                KalamDataType::Text,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Error message if job failed".to_string()),
            ),
        ];

        TableDefinition::new(
            NamespaceId::system(),
            TableName::new(SystemTable::Jobs.table_name()),
            TableType::System,
            columns,
            TableOptions::system(),
            Some("Background jobs for database maintenance".to_string()),
        )
        .expect("Failed to create system.jobs table definition")
    }

    /// Get the cached Arrow schema for the system.jobs table
    ///
    /// Memoizes the schema via `OnceLock` to ensure it is created exactly once and reused
    /// across all callers without synchronization overhead.
    pub fn schema() -> SchemaRef {
        static SCHEMA: OnceLock<SchemaRef> = OnceLock::new();
        SCHEMA
            .get_or_init(|| {
                Self::definition()
                    .to_arrow_schema()
                    .expect("Failed to convert jobs TableDefinition to Arrow schema")
            })
            .clone()
    }

    /// Get the table name
    pub fn table_name() -> &'static str {
        SystemTable::Jobs.table_name()
    }

    /// Get the column family name in RocksDB
    pub fn column_family_name() -> &'static str {
        SystemTable::Jobs.column_family_name().expect("Jobs is a table, not a view")
    }

    /// Get the partition key for storage
    pub fn partition() -> &'static str {
        Self::column_family_name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_jobs_table_schema() {
        let schema = JobsTableSchema::schema();
        // Schema is built from TableDefinition which has 15 columns
        // (namespace_id and table_name were removed - now stored in parameters JSON)
        // Added: leader_node_id (col 13) and leader_status (col 14) for distributed job execution
        assert_eq!(schema.fields().len(), 15);

        // Verify columns are ordered by ordinal_position (from TableDefinition)
        assert_eq!(schema.field(0).name(), "job_id"); // ordinal 1
        assert_eq!(schema.field(1).name(), "job_type"); // ordinal 2
        assert_eq!(schema.field(2).name(), "status"); // ordinal 3
        assert_eq!(schema.field(3).name(), "parameters"); // ordinal 4
    }

    #[test]
    fn test_jobs_table_name() {
        assert_eq!(JobsTableSchema::table_name(), "jobs");
        assert_eq!(
            JobsTableSchema::column_family_name(),
            SystemTable::Jobs.column_family_name().expect("Jobs is a table, not a view")
        );
    }

    #[test]
    fn test_schema_caching() {
        let schema1 = JobsTableSchema::schema();
        let schema2 = JobsTableSchema::schema();
        assert!(Arc::ptr_eq(&schema1, &schema2), "Schema should be cached");
    }
}
