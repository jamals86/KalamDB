// File: backend/crates/kalamdb-commons/src/models/user_name.rs
// Type-safe wrapper for usernames (secondary index key)

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::StorageKey;

/// Type-safe wrapper for usernames used as secondary index keys.
///
/// This newtype ensures usernames cannot be confused with user IDs
/// or other string identifiers, providing compile-time safety for
/// username-to-UserId lookups in the secondary index.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Encode, Decode)]
pub struct UserName(String);

impl UserName {
    /// Creates a new UserName from a string.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Returns the username as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the wrapper and returns the inner String.
    pub fn into_string(self) -> String {
        self.0
    }

    /// Get the username as bytes for storage
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    /// Convert to lowercase for case-insensitive comparisons
    pub fn to_lowercase(&self) -> UserName {
        UserName(self.0.to_lowercase())
    }
}

impl fmt::Display for UserName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for UserName {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for UserName {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for UserName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<[u8]> for UserName {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl StorageKey for UserName {
    fn storage_key(&self) -> Vec<u8> {
        self.0.as_bytes().to_vec()
    }

    fn from_storage_key(bytes: &[u8]) -> Result<Self, String> {
        String::from_utf8(bytes.to_vec())
            .map(UserName)
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_name_new() {
        let name = UserName::new("john_doe");
        assert_eq!(name.as_str(), "john_doe");
    }

    #[test]
    fn test_user_name_from_string() {
        let name = UserName::from("jane_smith".to_string());
        assert_eq!(name.as_str(), "jane_smith");
    }

    #[test]
    fn test_user_name_from_str() {
        let name = UserName::from("alice");
        assert_eq!(name.as_str(), "alice");
    }

    #[test]
    fn test_user_name_as_ref_str() {
        let name = UserName::new("bob");
        let s: &str = name.as_ref();
        assert_eq!(s, "bob");
    }

    #[test]
    fn test_user_name_as_ref_bytes() {
        let name = UserName::new("charlie");
        let bytes: &[u8] = name.as_ref();
        assert_eq!(bytes, b"charlie");
    }

    #[test]
    fn test_user_name_as_bytes() {
        let name = UserName::new("david");
        assert_eq!(name.as_bytes(), b"david");
    }

    #[test]
    fn test_user_name_display() {
        let name = UserName::new("eve");
        assert_eq!(format!("{}", name), "eve");
    }

    #[test]
    fn test_user_name_into_string() {
        let name = UserName::new("frank");
        let s = name.into_string();
        assert_eq!(s, "frank");
    }

    #[test]
    fn test_user_name_clone() {
        let name1 = UserName::new("grace");
        let name2 = name1.clone();
        assert_eq!(name1, name2);
    }

    #[test]
    fn test_user_name_serialization() {
        let name = UserName::new("heidi");
        let json = serde_json::to_string(&name).unwrap();
        let deserialized: UserName = serde_json::from_str(&json).unwrap();
        assert_eq!(name, deserialized);
    }

    #[test]
    fn test_user_name_to_lowercase() {
        let name = UserName::new("IvAN");
        let lower = name.to_lowercase();
        assert_eq!(lower.as_str(), "ivan");
    }

    #[test]
    fn test_user_name_case_sensitivity() {
        let name1 = UserName::new("Alice");
        let name2 = UserName::new("alice");
        assert_ne!(name1, name2); // UserName is case-sensitive by default
        assert_eq!(name1.to_lowercase(), name2.to_lowercase()); // But can be compared case-insensitively
    }
}
