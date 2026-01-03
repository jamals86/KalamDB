use kalamdb_commons::datatypes::KalamDataType;
use kalamdb_commons::schemas::{
    ColumnDefault, ColumnDefinition, TableDefinition, TableOptions, TableType,
};
use kalamdb_commons::{NamespaceId, TableName};

/// Create TableDefinition for system.jobs table
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
pub fn jobs_table_definition() -> TableDefinition {
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
            KalamDataType::Text,
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
            "error_message",
            13,
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
        TableName::new("jobs"),
        TableType::System,
        columns,
        TableOptions::system(),
        Some("Background jobs for database maintenance".to_string()),
    )
    .expect("Failed to create system.jobs table definition")
}
