//! System.job_nodes table schema (system_job_nodes in RocksDB)

use datafusion::arrow::datatypes::SchemaRef;
use kalamdb_commons::datatypes::KalamDataType;
use kalamdb_commons::schemas::{
    ColumnDefault, ColumnDefinition, TableDefinition, TableOptions, TableType,
};
use kalamdb_commons::{NamespaceId, SystemTable, TableName};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy)]
pub struct JobNodesTableSchema;

impl JobNodesTableSchema {
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
                Some("Job identifier".to_string()),
            ),
            ColumnDefinition::new(
                2,
                "node_id",
                2,
                KalamDataType::Text,
                false,
                true,
                false,
                ColumnDefault::None,
                Some("Node identifier".to_string()),
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
                Some("Per-node job status".to_string()),
            ),
            ColumnDefinition::new(
                4,
                "error_message",
                4,
                KalamDataType::Text,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Error message if node failed".to_string()),
            ),
            ColumnDefinition::new(
                5,
                "created_at",
                5,
                KalamDataType::Timestamp,
                false,
                false,
                false,
                ColumnDefault::FunctionCall {
                    name: "NOW".to_string(),
                    args: vec![],
                },
                Some("Row creation timestamp".to_string()),
            ),
            ColumnDefinition::new(
                6,
                "started_at",
                6,
                KalamDataType::Timestamp,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Node start timestamp".to_string()),
            ),
            ColumnDefinition::new(
                7,
                "finished_at",
                7,
                KalamDataType::Timestamp,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Node finish timestamp".to_string()),
            ),
            ColumnDefinition::new(
                8,
                "updated_at",
                8,
                KalamDataType::Timestamp,
                false,
                false,
                false,
                ColumnDefault::FunctionCall {
                    name: "NOW".to_string(),
                    args: vec![],
                },
                Some("Row updated timestamp".to_string()),
            ),
        ];

        TableDefinition::new(
            NamespaceId::system(),
            TableName::new(SystemTable::JobNodes.table_name()),
            TableType::System,
            columns,
            TableOptions::system(),
            Some("Per-node job execution state".to_string()),
        )
        .expect("Failed to create system.job_nodes table definition")
    }

    pub fn schema() -> SchemaRef {
        static SCHEMA: OnceLock<SchemaRef> = OnceLock::new();
        SCHEMA
            .get_or_init(|| {
                Self::definition()
                    .to_arrow_schema()
                    .expect("Failed to convert job_nodes TableDefinition to Arrow schema")
            })
            .clone()
    }

    pub fn table_name() -> &'static str {
        SystemTable::JobNodes.table_name()
    }

    pub fn column_family_name() -> &'static str {
        SystemTable::JobNodes
            .column_family_name()
            .expect("JobNodes is a table, not a view")
    }

    pub fn partition() -> &'static str {
        Self::column_family_name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_nodes_table_schema() {
        let schema = JobNodesTableSchema::schema();
        assert_eq!(schema.fields().len(), 8);
        assert_eq!(schema.field(0).name(), "job_id");
        assert_eq!(schema.field(1).name(), "node_id");
        assert_eq!(schema.field(2).name(), "status");
    }
}
