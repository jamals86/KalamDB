//! Serialization helpers for Raft state machine payloads.
//!
//! This module provides simple helper functions for serializing and deserializing
//! data using JSON.

use crate::error::RaftError;
use serde::{de::DeserializeOwned, Serialize};

/// Encode a value to bytes using JSON.
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, RaftError> {
    serde_json::to_vec(value).map_err(|e| RaftError::Serialization(e.to_string()))
}

/// Decode a value from bytes using JSON.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, RaftError> {
    serde_json::from_slice(bytes).map_err(|e| RaftError::Serialization(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestData {
        id: u64,
        name: String,
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let data = TestData {
            id: 42,
            name: "test".to_string(),
        };
        let bytes = encode(&data).unwrap();
        let decoded: TestData = decode(&bytes).unwrap();
        assert_eq!(data, decoded);
    }

    #[test]
    fn test_entry_payload_membership_roundtrip() {
        use crate::storage::{KalamNode, KalamTypeConfig};
        use openraft::{EntryPayload, Membership};
        use std::collections::BTreeMap;

        // Create a KalamNode
        let node = KalamNode::new("127.0.0.1:9081", "http://127.0.0.1:8081");

        // Create a membership with one node
        let mut nodes = BTreeMap::new();
        nodes.insert(1u64, node);
        let membership: Membership<u64, KalamNode> = nodes.into();

        // Create an EntryPayload::Membership
        let payload: EntryPayload<KalamTypeConfig> = EntryPayload::Membership(membership);

        // Encode using the same helper path used by runtime.
        let bytes = encode(&payload).expect("Membership should encode");

        // Decode should succeed - this was failing before the skip_serializing_if fix
        let decoded: EntryPayload<KalamTypeConfig> = decode(&bytes)
            .expect("Membership should decode - KalamNode must NOT use skip_serializing_if");

        // Verify the decoded data matches
        match (&payload, &decoded) {
            (EntryPayload::Membership(m1), EntryPayload::Membership(m2)) => {
                assert_eq!(m1.nodes().count(), m2.nodes().count(), "Node count should match");
            },
            _ => panic!("Decoded payload type mismatch"),
        }

        // Also verify Blank still works
        let blank: EntryPayload<KalamTypeConfig> = EntryPayload::Blank;
        let blank_bytes = encode(&blank).expect("Blank should encode");
        let _: EntryPayload<KalamTypeConfig> = decode(&blank_bytes).expect("Blank should decode");
    }

    #[test]
    fn test_entry_payload_membership_with_two_nodes() {
        use crate::storage::{KalamNode, KalamTypeConfig};
        use openraft::{EntryPayload, Membership};
        use std::collections::BTreeMap;

        // Create two nodes - simulating add_learner scenario
        let node1 = KalamNode::new("127.0.0.1:9081", "http://127.0.0.1:8081");
        let node2 = KalamNode::new("127.0.0.1:9082", "http://127.0.0.1:8082");

        // Create membership with both nodes
        let mut nodes = BTreeMap::new();
        nodes.insert(1u64, node1);
        nodes.insert(2u64, node2);

        let membership: Membership<u64, KalamNode> = nodes.into();
        let payload: EntryPayload<KalamTypeConfig> = EntryPayload::Membership(membership);

        // Encode and decode roundtrip
        let bytes = encode(&payload).expect("2-node Membership should encode");
        let decoded: EntryPayload<KalamTypeConfig> =
            decode(&bytes).expect("2-node Membership should decode");

        match (&payload, &decoded) {
            (EntryPayload::Membership(m1), EntryPayload::Membership(m2)) => {
                assert_eq!(m1.nodes().count(), m2.nodes().count(), "Node count should match");
                assert_eq!(m1.nodes().count(), 2, "Should have 2 nodes");
            },
            _ => panic!("Decoded payload type mismatch"),
        }
    }
}
