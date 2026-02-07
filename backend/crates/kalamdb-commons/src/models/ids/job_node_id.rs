use bincode::de::{BorrowDecoder, Decoder};
use bincode::enc::Encoder;
use bincode::error::{DecodeError, EncodeError};
use bincode::{BorrowDecode, Decode, Encode};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

use super::{JobId, NodeId};
use crate::storage_key::{decode_key, encode_key, encode_prefix};
use crate::StorageKey;

/// Unique identifier for a job run on a specific node.
///
/// Storage key format (storekey tuple encoding): (node_id, job_id)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct JobNodeId {
    node_id: NodeId,
    job_id: JobId,
    #[serde(skip)]
    cached_string: String,
}

// Custom bincode Encode implementation
impl Encode for JobNodeId {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        Encode::encode(&self.node_id, encoder)?;
        Encode::encode(&self.job_id, encoder)?;
        Ok(())
    }
}

// Custom bincode Decode implementation that populates cached_string
impl<Context> Decode<Context> for JobNodeId {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        let node_id = NodeId::decode(decoder)?;
        let job_id = JobId::decode(decoder)?;
        Ok(JobNodeId::new(&job_id, &node_id))
    }
}

// Custom bincode BorrowDecode implementation that populates cached_string
impl<'de, Context> BorrowDecode<'de, Context> for JobNodeId {
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, DecodeError> {
        let node_id = NodeId::borrow_decode(decoder)?;
        let job_id = JobId::borrow_decode(decoder)?;
        Ok(JobNodeId::new(&job_id, &node_id))
    }
}

// Custom serde Deserialize implementation that populates cached_string after deserialization
impl<'de> Deserialize<'de> for JobNodeId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct JobNodeIdHelper {
            node_id: NodeId,
            job_id: JobId,
        }

        let helper = JobNodeIdHelper::deserialize(deserializer)?;
        Ok(JobNodeId::new(&helper.job_id, &helper.node_id))
    }
}

impl JobNodeId {
    pub fn new(job_id: &JobId, node_id: &NodeId) -> Self {
        let cached_string = format!("{}|{}", node_id, job_id.as_str());
        Self {
            node_id: *node_id,
            job_id: job_id.clone(),
            cached_string,
        }
    }

    pub fn from_string(value: &str) -> Result<Self, String> {
        let mut parts = value.splitn(2, '|');
        let node_id = parts
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| format!("Invalid JobNodeId format: {}", value))?;
        let job_id = parts.next().ok_or_else(|| format!("Invalid JobNodeId format: {}", value))?;
        Ok(Self::new(&JobId::new(job_id), &NodeId::new(node_id)))
    }

    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    pub fn job_id(&self) -> &JobId {
        &self.job_id
    }

    pub fn as_str(&self) -> &str {
        &self.cached_string
    }

    pub fn into_string(self) -> String {
        self.cached_string
    }

    pub fn prefix_for_node(node_id: &NodeId) -> Vec<u8> {
        encode_prefix(&(node_id.as_u64(),))
    }
}

impl fmt::Display for JobNodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cached_string)
    }
}

impl From<String> for JobNodeId {
    fn from(value: String) -> Self {
        JobNodeId::from_string(&value).expect("Invalid JobNodeId format")
    }
}

impl From<&str> for JobNodeId {
    fn from(value: &str) -> Self {
        JobNodeId::from_string(value).expect("Invalid JobNodeId format")
    }
}

impl StorageKey for JobNodeId {
    fn storage_key(&self) -> Vec<u8> {
        encode_key(&(self.node_id.as_u64(), self.job_id.as_str()))
    }

    fn from_storage_key(bytes: &[u8]) -> Result<Self, String> {
        if let Ok((node_id, job_id)) = decode_key::<(u64, String)>(bytes) {
            let key = JobNodeId::new(&JobId::new(job_id), &NodeId::new(node_id));
            return Ok(key);
        }

        // Legacy delimiter-based format fallback
        let s = String::from_utf8(bytes.to_vec()).map_err(|e| e.to_string())?;
        JobNodeId::from_string(&s)
    }
}
