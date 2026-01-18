//! Serialization helpers for bincode 2.x API compatibility.
//!
//! This module provides simple helper functions for serializing and deserializing
//! data using bincode 2.x's serde integration.

use crate::error::RaftError;
use serde::{de::DeserializeOwned, Serialize};

/// Encode a value to bytes using bincode.
///
/// Uses the standard bincode 2.x configuration with variable int encoding.
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, RaftError> {
    bincode::serde::encode_to_vec(value, bincode::config::standard())
        .map_err(|e| RaftError::Serialization(e.to_string()))
}

/// Decode a value from bytes using bincode.
///
/// Uses the standard bincode 2.x configuration with variable int encoding.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, RaftError> {
    let (value, _) = bincode::serde::decode_from_slice(bytes, bincode::config::standard())
        .map_err(|e| RaftError::Serialization(e.to_string()))?;
    Ok(value)
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

        // Encode with standard config
        let bytes_standard = bincode::serde::encode_to_vec(&payload, bincode::config::standard()).unwrap();
        println!("Standard config - length: {}", bytes_standard.len());
        println!("Standard bytes: {:02x?}", &bytes_standard);
        
        // Encode with legacy config
        let bytes_legacy = bincode::serde::encode_to_vec(&payload, bincode::config::legacy()).unwrap();
        println!("Legacy config - length: {}", bytes_legacy.len());
        println!("Legacy bytes: {:02x?}", &bytes_legacy);

        // Try decode with standard
        let result_std: Result<(EntryPayload<KalamTypeConfig>, _), _> = 
            bincode::serde::decode_from_slice(&bytes_standard, bincode::config::standard());
        println!("Decode standard->standard: {:?}", result_std.is_ok());

        // Try decode legacy with legacy
        let result_leg: Result<(EntryPayload<KalamTypeConfig>, _), _> = 
            bincode::serde::decode_from_slice(&bytes_legacy, bincode::config::legacy());
        println!("Decode legacy->legacy: {:?}", result_leg.is_ok());

        // Test blank
        let blank: EntryPayload<KalamTypeConfig> = EntryPayload::Blank;
        let blank_bytes = bincode::serde::encode_to_vec(&blank, bincode::config::standard()).unwrap();
        println!("Blank bytes (standard): {:02x?}", &blank_bytes);
        let blank_dec: Result<(EntryPayload<KalamTypeConfig>, _), _> = 
            bincode::serde::decode_from_slice(&blank_bytes, bincode::config::standard());
        println!("Decode blank: {:?}", blank_dec.is_ok());
        
        assert!(result_std.is_ok() || result_leg.is_ok(), "At least one decode should work");
    }

    #[test]
    fn test_entry_payload_membership_with_two_nodes() {
        use crate::storage::{KalamNode, KalamTypeConfig};
        use openraft::{EntryPayload, Membership};
        use std::collections::BTreeMap;

        // Create two nodes - simulating add_learner scenario
        let node1 = KalamNode::new("127.0.0.1:9081", "http://127.0.0.1:8081");
        let node2 = KalamNode::new("127.0.0.1:9082", "http://127.0.0.1:8082");
        
        // Create membership with node 1 as voter, node 2 as learner (in nodes but not in config)
        let mut nodes = BTreeMap::new();
        nodes.insert(1u64, node1);
        nodes.insert(2u64, node2);
        
        // This creates a membership with both nodes as voters
        let membership: Membership<u64, KalamNode> = nodes.into();

        // Create an EntryPayload::Membership
        let payload: EntryPayload<KalamTypeConfig> = EntryPayload::Membership(membership);

        // Encode
        let bytes = encode(&payload).unwrap();
        println!("Encoded 2-node Membership payload length: {}", bytes.len());

        // Decode
        let decoded: EntryPayload<KalamTypeConfig> = decode(&bytes).unwrap();
        
        match (&payload, &decoded) {
            (EntryPayload::Membership(m1), EntryPayload::Membership(m2)) => {
                assert_eq!(m1.nodes().count(), m2.nodes().count());
            },
            _ => panic!("Decoded payload type mismatch"),
        }
    }
}
