//! Type-safe wrapper for namespace identifiers
//!
//! This module provides a newtype pattern around String to ensure type safety
//! when working with namespace identifiers throughout the codebase.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Type-safe wrapper for namespace identifiers
///
/// # Examples
///
/// ```
/// use kalamdb_core::catalog::NamespaceId;
///
/// let namespace_id = NamespaceId::new("my_namespace");
/// assert_eq!(namespace_id.as_str(), "my_namespace");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NamespaceId(String);

impl NamespaceId {
    /// Create a new NamespaceId from a string
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Get the namespace ID as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Convert into the underlying String
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for NamespaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for NamespaceId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for NamespaceId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for NamespaceId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_id_creation() {
        let id = NamespaceId::new("test_namespace");
        assert_eq!(id.as_str(), "test_namespace");
    }

    #[test]
    fn test_namespace_id_equality() {
        let id1 = NamespaceId::new("namespace1");
        let id2 = NamespaceId::new("namespace1");
        let id3 = NamespaceId::new("namespace2");

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_namespace_id_display() {
        let id = NamespaceId::new("display_test");
        assert_eq!(format!("{}", id), "display_test");
    }
}
