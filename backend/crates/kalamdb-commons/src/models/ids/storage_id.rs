//! Type-safe wrapper for storage identifiers.

use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::StorageKey;

/// Type-safe wrapper for storage identifiers.
///
/// Ensures storage IDs cannot be accidentally used where user IDs, namespace IDs,
/// or table names are expected.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", derive(bincode::Encode, bincode::Decode))]
pub struct StorageId(String);

impl StorageId {
    /// Creates a new StorageId from a string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the storage ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the wrapper and returns the inner String.
    pub fn into_string(self) -> String {
        self.0
    }

    /// Creates a default 'local' storage ID.
    pub fn local() -> Self {
        Self("local".to_string())
    }

    /// Is this storage ID the local storage?
    pub fn is_local(&self) -> bool {
        self.0 == "local"
    }
}

impl fmt::Display for StorageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for StorageId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for StorageId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for StorageId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<[u8]> for StorageId {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl Default for StorageId {
    fn default() -> Self {
        Self::local()
    }
}

impl StorageKey for StorageId {
    fn storage_key(&self) -> Vec<u8> {
        self.0.as_bytes().to_vec()
    }
}
