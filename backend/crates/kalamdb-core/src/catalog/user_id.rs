//! Type-safe wrapper for user identifiers
//!
//! This module provides a newtype pattern around String to ensure type safety
//! when working with user identifiers throughout the codebase.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Type-safe wrapper for user identifiers
///
/// # Examples
///
/// ```
/// use kalamdb_core::catalog::UserId;
///
/// let user_id = UserId::new("user123");
/// assert_eq!(user_id.as_str(), "user123");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserId(String);

impl UserId {
    /// Create a new UserId from a string
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Get the user ID as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Convert into the underlying String
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for UserId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for UserId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for UserId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_id_creation() {
        let id = UserId::new("user456");
        assert_eq!(id.as_str(), "user456");
    }

    #[test]
    fn test_user_id_equality() {
        let id1 = UserId::new("user1");
        let id2 = UserId::new("user1");
        let id3 = UserId::new("user2");

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_user_id_display() {
        let id = UserId::new("display_user");
        assert_eq!(format!("{}", id), "display_user");
    }
}
