//! System.job_nodes table schema (system_job_nodes in RocksDB)

use crate::providers::job_nodes::models::JobNode;
use datafusion::arrow::datatypes::SchemaRef;
use kalamdb_commons::schemas::TableDefinition;
use kalamdb_commons::SystemTable;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy)]
pub struct JobNodesTableSchema;

impl JobNodesTableSchema {
    pub fn definition() -> TableDefinition {
        JobNode::definition()
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
