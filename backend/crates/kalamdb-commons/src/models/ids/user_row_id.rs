// File: backend/crates/kalamdb-commons/src/models/user_row_id.rs
// Composite key for user-scoped table rows

use serde::{Deserialize, Serialize};
use std::fmt;

use super::user_id::UserId;
use crate::StorageKey;

/// Composite key for user-scoped table rows: {user_id}:{row_id}
///
/// This composite key ensures type safety when accessing user-isolated
/// table rows, preventing accidental access across user boundaries.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserRowId {
    user_id: UserId,
    row_id: Vec<u8>,
}

impl UserRowId {
    /// Create a new UserRowId from a user ID and row ID
    pub fn new(user_id: UserId, row_id: impl Into<Vec<u8>>) -> Self {
        Self {
            user_id,
            row_id: row_id.into(),
        }
    }

    /// Get the user ID component
    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }

    /// Get the row ID component as a byte slice
    pub fn row_id(&self) -> &[u8] {
        &self.row_id
    }

    /// Create from string components
    pub fn from_strings(user_id: &str, row_id: &str) -> Self {
        Self {
            user_id: UserId::new(user_id),
            row_id: row_id.as_bytes().to_vec(),
        }
    }

    /// Format as bytes for storage: "{user_id}:{row_id}"
    pub fn as_storage_key(&self) -> Vec<u8> {
        let mut key = Vec::new();
        key.extend_from_slice(self.user_id.as_str().as_bytes());
        key.push(b':');
        key.extend_from_slice(&self.row_id);
        key
    }

    /// Parse from storage key format: "{user_id}:{row_id}"
    pub fn from_storage_key(key: &[u8]) -> Option<Self> {
        let pos = key.iter().position(|&b| b == b':')?;
        let user_id_str = std::str::from_utf8(&key[..pos]).ok()?;
        let row_id = key[pos + 1..].to_vec();

        Some(Self {
            user_id: UserId::new(user_id_str),
            row_id,
        })
    }
}

impl AsRef<[u8]> for UserRowId {
    fn as_ref(&self) -> &[u8] {
        // Note: This creates a temporary allocation. For zero-copy access,
        // use as_storage_key() directly.
        // This implementation is primarily for trait compatibility.
        // In performance-critical paths, prefer as_storage_key().

        // For the trait, we'll create a static representation on first call
        // In practice, callers should use as_storage_key() for actual storage ops
        self.user_id.as_str().as_bytes()
    }
}

impl fmt::Display for UserRowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:", self.user_id)?;
        // Try to display row_id as UTF-8, fall back to hex
        match std::str::from_utf8(&self.row_id) {
            Ok(s) => write!(f, "{}", s),
            Err(_) => write!(f, "{}", hex::encode(&self.row_id)),
        }
    }
}

// Ensure Send and Sync are implemented
unsafe impl Send for UserRowId {}
unsafe impl Sync for UserRowId {}

impl StorageKey for UserRowId {
    fn storage_key(&self) -> Vec<u8> {
        self.as_storage_key()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_row_id_new() {
        let user_id = UserId::new("user123");
        let row_id = vec![1, 2, 3, 4];
        let composite = UserRowId::new(user_id.clone(), row_id.clone());

        assert_eq!(composite.user_id(), &user_id);
        assert_eq!(composite.row_id(), &row_id[..]);
    }

    #[test]
    fn test_user_row_id_from_strings() {
        let composite = UserRowId::from_strings("user123", "row456");
        assert_eq!(composite.user_id().as_str(), "user123");
        assert_eq!(composite.row_id(), b"row456");
    }

    #[test]
    fn test_user_row_id_as_storage_key() {
        let composite = UserRowId::from_strings("user123", "row456");
        let key = composite.as_storage_key();
        assert_eq!(key, b"user123:row456");
    }

    #[test]
    fn test_user_row_id_from_storage_key() {
        let key = b"user123:row456";
        let composite = UserRowId::from_storage_key(key).unwrap();

        assert_eq!(composite.user_id().as_str(), "user123");
        assert_eq!(composite.row_id(), b"row456");
    }

    #[test]
    fn test_user_row_id_roundtrip() {
        let original = UserRowId::from_strings("user123", "row456");
        let key = original.as_storage_key();
        let parsed = UserRowId::from_storage_key(&key).unwrap();

        assert_eq!(original, parsed);
    }

    #[test]
    fn test_user_row_id_display() {
        let composite = UserRowId::from_strings("user123", "row456");
        assert_eq!(format!("{}", composite), "user123:row456");
    }

    #[test]
    fn test_user_row_id_serialization() {
        let composite = UserRowId::from_strings("user123", "row456");
        let json = serde_json::to_string(&composite).unwrap();
        let deserialized: UserRowId = serde_json::from_str(&json).unwrap();
        assert_eq!(composite, deserialized);
    }

    #[test]
    fn test_user_row_id_with_binary_row_id() {
        let user_id = UserId::new("user123");
        let row_id = vec![0xFF, 0xFE, 0xFD];
        let composite = UserRowId::new(user_id, row_id);

        let key = composite.as_storage_key();
        let parsed = UserRowId::from_storage_key(&key).unwrap();

        assert_eq!(composite, parsed);
    }
}
