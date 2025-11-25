// File: backend/crates/kalamdb-commons/src/models/live_query_id.rs
// Type-safe composite identifier for live query subscriptions

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::models::{ConnectionId, UserId};
use crate::StorageKey;

/// Unique identifier for live query subscriptions.
/// 
/// Composite key containing user, connection, and subscription information.
/// Format when serialized: {user_id}-{connection_id}-{subscription_id}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Encode, Decode)]
pub struct LiveQueryId {
    pub user_id: UserId,
    pub connection_id: ConnectionId,
    pub subscription_id: String,
}

impl LiveQueryId {
    /// Creates a new LiveQueryId from composite components
    pub fn new(
        user_id: UserId,
        connection_id: ConnectionId,
        subscription_id: impl Into<String>,
    ) -> Self {
        Self {
            user_id,
            connection_id,
            subscription_id: subscription_id.into(),
        }
    }

    /// Parse LiveQueryId from string format
    pub fn from_string(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.splitn(3, '-').collect();
        if parts.len() != 3 {
            return Err(format!(
                "Invalid live_query_id format: {}. Expected: {{user_id}}-{{connection_id}}-{{subscription_id}}",
                s
            ));
        }

        Ok(Self {
            user_id: UserId::new(parts[0].to_string()),
            connection_id: ConnectionId::new(parts[1].to_string()),
            subscription_id: parts[2].to_string(),
        })
    }

    /// Returns the live query ID as a formatted string
    pub fn as_str(&self) -> String {
        self.to_string()
    }

    /// Get the live query ID as bytes for storage
    pub fn as_bytes(&self) -> Vec<u8> {
        self.to_string().into_bytes()
    }

    /// Consumes the wrapper and returns the formatted String
    pub fn into_string(self) -> String {
        self.to_string()
    }

    // Accessor methods
    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }

    pub fn connection_id(&self) -> &ConnectionId {
        &self.connection_id
    }
    
    pub fn subscription_id(&self) -> &str {
        &self.subscription_id
    }
}


impl fmt::Display for LiveQueryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}-{}-{}",
            self.user_id.as_str(),
            self.connection_id,
            self.subscription_id
        )
    }
}

impl From<String> for LiveQueryId {
    fn from(s: String) -> Self {
        Self::from_string(&s).expect("Invalid LiveQueryId string format")
    }
}

impl From<&str> for LiveQueryId {
    fn from(s: &str) -> Self {
        Self::from_string(s).expect("Invalid LiveQueryId string format")
    }
}

impl AsRef<str> for LiveQueryId {
    fn as_ref(&self) -> &str {
        // Note: This returns a temporary string, not ideal but maintains compatibility
        // Consider using to_string() directly instead
        Box::leak(Box::new(self.to_string())).as_str()
    }
}

impl AsRef<[u8]> for LiveQueryId {
    fn as_ref(&self) -> &[u8] {
        // Note: This returns a temporary byte slice, not ideal but maintains compatibility
        Box::leak(Box::new(self.to_string())).as_bytes()
    }
}

// Ensure Send and Sync are implemented
unsafe impl Send for LiveQueryId {}
unsafe impl Sync for LiveQueryId {}

impl StorageKey for LiveQueryId {
    fn storage_key(&self) -> Vec<u8> {
        self.to_string().into_bytes()
    }

    fn from_storage_key(bytes: &[u8]) -> Result<Self, String> {
        let s = String::from_utf8(bytes.to_vec()).map_err(|e| e.to_string())?;
        Self::from_string(&s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_live_query_id() -> LiveQueryId {
        let user_id = UserId::new("user1");
        let connection_id = ConnectionId::new("conn123");
        LiveQueryId::new(user_id, connection_id, "sub456")
    }

    #[test]
    fn test_live_query_id_new() {
        let id = create_test_live_query_id();
        assert_eq!(id.subscription_id(), "sub456");
        assert_eq!(id.user_id().as_str(), "user1");
    }

    #[test]
    fn test_live_query_id_from_string() {
        let id_str = "user1-conn123-sub456";
        let id = LiveQueryId::from_string(id_str).unwrap();
        assert_eq!(id.user_id().as_str(), "user1");
        assert_eq!(id.connection_id().as_str(), "conn123");
        assert_eq!(id.subscription_id(), "sub456");
    }

    #[test]
    fn test_live_query_id_to_string() {
        let id = create_test_live_query_id();
        let s = id.to_string();
        assert_eq!(s, "user1-conn123-sub456");
    }

    #[test]
    fn test_live_query_id_as_bytes() {
        let id = create_test_live_query_id();
        let bytes = id.as_bytes();
        assert_eq!(bytes, b"user1-conn123-sub456");
    }

    #[test]
    fn test_live_query_id_display() {
        let id = create_test_live_query_id();
        assert_eq!(format!("{}", id), "user1-conn123-sub456");
    }

    #[test]
    fn test_live_query_id_into_string() {
        let id = create_test_live_query_id();
        let s = id.into_string();
        assert_eq!(s, "user1-conn123-sub456");
    }

    #[test]
    fn test_live_query_id_clone() {
        let id1 = create_test_live_query_id();
        let id2 = id1.clone();
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_live_query_id_serialization() {
        let id = create_test_live_query_id();
        let json = serde_json::to_string(&id).unwrap();
        let deserialized: LiveQueryId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deserialized);
    }

    #[test]
    fn test_storage_key_roundtrip() {
        let id = create_test_live_query_id();
        let key = id.storage_key();
        let restored = LiveQueryId::from_storage_key(&key).unwrap();
        assert_eq!(id, restored);
    }
}
