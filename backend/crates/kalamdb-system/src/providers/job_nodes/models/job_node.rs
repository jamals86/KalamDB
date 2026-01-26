//! Job-node execution state for system.job_nodes table.

use bincode::{Decode, Encode};
use kalamdb_commons::datatypes::KalamDataType;
use kalamdb_commons::models::ids::{JobId, JobNodeId, NodeId};
use crate::JobStatus;
use kalamdb_commons::KSerializable;
use kalamdb_macros::table;
use serde::{Deserialize, Serialize};

#[table(
    name = "job_nodes",
    comment = "Per-node job execution state"
)]
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug, PartialEq)]
pub struct JobNode {
    #[column(
        id = 5,
        ordinal = 5,
        data_type(KalamDataType::Timestamp),
        nullable = false,
        primary_key = false,
        default = "Function(NOW)",
        comment = "Row creation timestamp"
    )]
    pub created_at: i64,
    #[column(
        id = 8,
        ordinal = 8,
        data_type(KalamDataType::Timestamp),
        nullable = false,
        primary_key = false,
        default = "Function(NOW)",
        comment = "Row updated timestamp"
    )]
    pub updated_at: i64,
    #[column(
        id = 6,
        ordinal = 6,
        data_type(KalamDataType::Timestamp),
        nullable = true,
        primary_key = false,
        default = "None",
        comment = "Node start timestamp"
    )]
    pub started_at: Option<i64>,
    #[column(
        id = 7,
        ordinal = 7,
        data_type(KalamDataType::Timestamp),
        nullable = true,
        primary_key = false,
        default = "None",
        comment = "Node finish timestamp"
    )]
    pub finished_at: Option<i64>,
    #[column(
        id = 1,
        ordinal = 1,
        data_type(KalamDataType::Text),
        nullable = false,
        primary_key = true,
        default = "None",
        comment = "Job identifier"
    )]
    pub job_id: JobId,
    #[bincode(with_serde)]
    #[column(
        id = 2,
        ordinal = 2,
        data_type(KalamDataType::Text),
        nullable = false,
        primary_key = true,
        default = "None",
        comment = "Node identifier"
    )]
    pub node_id: NodeId,
    #[column(
        id = 3,
        ordinal = 3,
        data_type(KalamDataType::Text),
        nullable = false,
        primary_key = false,
        default = "None",
        comment = "Per-node job status"
    )]
    pub status: JobStatus,
    #[column(
        id = 4,
        ordinal = 4,
        data_type(KalamDataType::Text),
        nullable = true,
        primary_key = false,
        default = "None",
        comment = "Error message if node failed"
    )]
    pub error_message: Option<String>,
}

impl JobNode {
    pub fn id(&self) -> JobNodeId {
        JobNodeId::new(&self.job_id, &self.node_id)
    }
}

// KSerializable implementation for EntityStore support
impl KSerializable for JobNode {}
