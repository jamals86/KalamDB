use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::fmt;

use super::{JobId, NodeId};
use crate::StorageKey;

/// Unique identifier for a job run on a specific node.
///
/// Format: "{node_id}|{job_id}"
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Encode, Decode)]
pub struct JobNodeId(String);

impl JobNodeId {
    pub fn new(job_id: &JobId, node_id: &NodeId) -> Self {
        Self(format!("{}|{}", node_id, job_id.as_str()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }

    pub fn prefix_for_node(node_id: &NodeId) -> String {
        format!("{}|", node_id)
    }
}

impl fmt::Display for JobNodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for JobNodeId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for JobNodeId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl StorageKey for JobNodeId {
    fn storage_key(&self) -> Vec<u8> {
        self.0.as_bytes().to_vec()
    }

    fn from_storage_key(bytes: &[u8]) -> Result<Self, String> {
        String::from_utf8(bytes.to_vec()).map(JobNodeId).map_err(|e| e.to_string())
    }
}
