//! Job-node execution state for system.job_nodes table.

use crate::models::{ids::{JobId, JobNodeId, NodeId}, JobStatus};
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug, PartialEq)]
pub struct JobNode {
    pub created_at: i64,
    pub updated_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub job_id: JobId,
    #[bincode(with_serde)]
    pub node_id: NodeId,
    pub status: JobStatus,
    pub error_message: Option<String>,
}

impl JobNode {
    pub fn id(&self) -> JobNodeId {
        JobNodeId::new(&self.job_id, &self.node_id)
    }
}

// KSerializable implementation for EntityStore support
impl crate::serialization::KSerializable for JobNode {}
