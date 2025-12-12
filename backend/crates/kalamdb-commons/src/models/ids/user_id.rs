//! Type-safe wrapper for user identifiers.

use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{constants::AuthConstants, StorageKey};

/// Type-safe wrapper for user identifiers.
///
/// Ensures user IDs cannot be accidentally used where namespace IDs or table names
/// are expected.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", derive(bincode::Encode, bincode::Decode))]
pub struct UserId(String);

impl UserId {
    /// Creates a new UserId from a string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the user ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the wrapper and returns the inner String.
    pub fn into_string(self) -> String {
        self.0
    }

    /// Creates a default 'root' user ID.
    pub fn root() -> Self {
        Self(AuthConstants::DEFAULT_ROOT_USER_ID.to_string())
    }

    /// Is admin user?
    pub fn is_admin(&self) -> bool {
        self.as_str() == AuthConstants::DEFAULT_ROOT_USER_ID || self.as_str() == "sys_root"
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

impl AsRef<[u8]> for UserId {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl StorageKey for UserId {
    fn storage_key(&self) -> Vec<u8> {
        self.0.as_bytes().to_vec()
    }

    fn from_storage_key(bytes: &[u8]) -> Result<Self, String> {
        String::from_utf8(bytes.to_vec())
            .map(UserId)
            .map_err(|e| e.to_string())
    }
}
