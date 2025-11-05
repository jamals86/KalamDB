// File: backend/crates/kalamdb-commons/src/models/live_query_id.rs
// Type-safe wrapper for live query identifiers

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Type-safe wrapper for live query identifiers in system.live_queries table.
///
/// Ensures live query IDs cannot be accidentally used where other identifier types
/// are expected.
/// TODO: Use the same LiveId type in kalamdb-core live query manager and provider.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Encode, Decode)]
pub struct LiveQueryId(String);

impl LiveQueryId {
    /// Creates a new LiveQueryId from a string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the live query ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the wrapper and returns the inner String.
    pub fn into_string(self) -> String {
        self.0
    }

    /// Get the live query ID as bytes for storage
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl fmt::Display for LiveQueryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for LiveQueryId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for LiveQueryId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for LiveQueryId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<[u8]> for LiveQueryId {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

// Ensure Send and Sync are implemented
unsafe impl Send for LiveQueryId {}
unsafe impl Sync for LiveQueryId {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_live_query_id_new() {
        let id = LiveQueryId::new("lq-123");
        assert_eq!(id.as_str(), "lq-123");
    }

    #[test]
    fn test_live_query_id_from_string() {
        let id = LiveQueryId::from("lq-456".to_string());
        assert_eq!(id.as_str(), "lq-456");
    }

    #[test]
    fn test_live_query_id_from_str() {
        let id = LiveQueryId::from("lq-789");
        assert_eq!(id.as_str(), "lq-789");
    }

    #[test]
    fn test_live_query_id_as_ref_str() {
        let id = LiveQueryId::new("lq-abc");
        let s: &str = id.as_ref();
        assert_eq!(s, "lq-abc");
    }

    #[test]
    fn test_live_query_id_as_ref_bytes() {
        let id = LiveQueryId::new("lq-def");
        let bytes: &[u8] = id.as_ref();
        assert_eq!(bytes, b"lq-def");
    }

    #[test]
    fn test_live_query_id_as_bytes() {
        let id = LiveQueryId::new("lq-ghi");
        assert_eq!(id.as_bytes(), b"lq-ghi");
    }

    #[test]
    fn test_live_query_id_display() {
        let id = LiveQueryId::new("lq-xyz");
        assert_eq!(format!("{}", id), "lq-xyz");
    }

    #[test]
    fn test_live_query_id_into_string() {
        let id = LiveQueryId::new("lq-123");
        let s = id.into_string();
        assert_eq!(s, "lq-123");
    }

    #[test]
    fn test_live_query_id_clone() {
        let id1 = LiveQueryId::new("lq-clone");
        let id2 = id1.clone();
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_live_query_id_serialization() {
        let id = LiveQueryId::new("lq-serialize");
        let json = serde_json::to_string(&id).unwrap();
        let deserialized: LiveQueryId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deserialized);
    }
}
