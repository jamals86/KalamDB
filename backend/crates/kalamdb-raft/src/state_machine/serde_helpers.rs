//! Serialization helpers for bincode 2.x API compatibility.
//!
//! This module provides simple helper functions for serializing and deserializing
//! data using bincode 2.x's serde integration.

use crate::error::RaftError;
use serde::{de::DeserializeOwned, Serialize};

/// Encode a value to bytes using bincode.
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, RaftError> {
    bincode::serde::encode_to_vec(value, bincode::config::standard())
        .map_err(|e| RaftError::Serialization(e.to_string()))
}

/// Decode a value from bytes using bincode.
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
}
