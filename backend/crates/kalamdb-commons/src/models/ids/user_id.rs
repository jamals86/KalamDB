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

/// Error type for UserId validation failures
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserIdValidationError(pub String);

impl std::fmt::Display for UserIdValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for UserIdValidationError {}

impl UserId {
    /// Creates a new UserId from a string.
    ///
    /// # Panics
    /// Panics if the ID contains path traversal characters. Use `try_new()` for fallible creation.
    #[inline]
    pub fn new(id: impl Into<String>) -> Self {
        Self::try_new(id).expect("UserId contains invalid characters")
    }

    /// Creates a new UserId from a string, returning an error if validation fails.
    ///
    /// # Security
    /// Validates that the ID does not contain path traversal characters:
    /// - `..` (parent directory)
    /// - `/` or `\` (directory separators)
    /// - Null bytes (`\0`)
    ///
    /// This prevents path traversal attacks when user IDs are used in storage paths.
    pub fn try_new(id: impl Into<String>) -> Result<Self, UserIdValidationError> {
        let id = id.into();
        Self::validate_id(&id)?;
        Ok(Self(id))
    }

    /// Validates a user ID string for security.
    fn validate_id(id: &str) -> Result<(), UserIdValidationError> {
        // Check for path traversal patterns
        if id.contains("..") {
            return Err(UserIdValidationError(
                "User ID cannot contain '..' (path traversal)".to_string(),
            ));
        }
        if id.contains('/') {
            return Err(UserIdValidationError(
                "User ID cannot contain '/' (directory separator)".to_string(),
            ));
        }
        if id.contains('\\') {
            return Err(UserIdValidationError(
                "User ID cannot contain '\\' (directory separator)".to_string(),
            ));
        }
        if id.contains('\0') {
            return Err(UserIdValidationError(
                "User ID cannot contain null bytes".to_string(),
            ));
        }
        // Check for empty ID
        if id.is_empty() {
            return Err(UserIdValidationError(
                "User ID cannot be empty".to_string(),
            ));
        }
        Ok(())
    }

    /// Creates a UserId without validation (for internal use only).
    ///
    /// # Safety
    /// This bypasses security validation. Only use for IDs that are known to be safe
    /// (e.g., loaded from database, generated internally).
    #[inline]
    pub(crate) fn new_unchecked(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the user ID as a string slice.
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the wrapper and returns the inner String.
    #[inline]
    pub fn into_string(self) -> String {
        self.0
    }

    /// Creates a default 'root' user ID.
    #[inline]
    pub fn root() -> Self {
        Self(AuthConstants::DEFAULT_ROOT_USER_ID.to_string())
    }

    /// Is admin user?
    #[inline]
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
    /// Converts a String into UserId.
    ///
    /// # Panics
    /// Panics if the string contains path traversal characters.
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for UserId {
    /// Converts a &str into UserId.
    ///
    /// # Panics
    /// Panics if the string contains path traversal characters.
    fn from(s: &str) -> Self {
        Self::new(s.to_string())
    }
}

impl TryFrom<String> for UserId {
    type Error = UserIdValidationError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::try_new(s)
    }
}

impl TryFrom<&str> for UserId {
    type Error = UserIdValidationError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::try_new(s.to_string())
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
