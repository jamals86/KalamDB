//! Cluster message serialization using flexbuffers
//!
//! Provides serialization/deserialization helpers for cluster messages
//! using flexbuffers (part of the FlatBuffers ecosystem). This is used
//! for the `payload` bytes in cluster gRPC messages.
//!
//! flexbuffers is schema-less and works with any `serde::Serialize` type,
//! making it ideal for complex types like `ChangeNotification` which
//! contains `ScalarValue` (30+ variants).

use serde::{Deserialize, Serialize};

/// Serialize a value to flexbuffers bytes.
///
/// Works with any `serde::Serialize` type.
pub fn serialize<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    flexbuffers::to_vec(value).map_err(|e| format!("flexbuffers serialize error: {}", e))
}

/// Deserialize a value from flexbuffers bytes.
///
/// Works with any `serde::Deserialize` type.
pub fn deserialize<'de, T: Deserialize<'de>>(bytes: &'de [u8]) -> Result<T, String> {
    flexbuffers::from_slice(bytes).map_err(|e| format!("flexbuffers deserialize error: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestMessage {
        name: String,
        value: i64,
        tags: Vec<String>,
    }

    #[test]
    fn test_round_trip() {
        let msg = TestMessage {
            name: "test".to_string(),
            value: 42,
            tags: vec!["a".to_string(), "b".to_string()],
        };
        let bytes = serialize(&msg).unwrap();
        let decoded: TestMessage = deserialize(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_empty_string() {
        let msg = TestMessage {
            name: String::new(),
            value: 0,
            tags: vec![],
        };
        let bytes = serialize(&msg).unwrap();
        let decoded: TestMessage = deserialize(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_deserialize_invalid_bytes() {
        let result: Result<TestMessage, _> = deserialize(&[0xFF, 0xFE]);
        assert!(result.is_err());
    }
}
