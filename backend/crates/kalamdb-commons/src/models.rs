//! Type-safe wrapper types for KalamDB identifiers and enums.
//!
//! This module provides newtype wrappers around String to enforce type safety
//! at compile time, preventing accidental mixing of user IDs, namespace IDs,
//! and table names.
//!
//! ## Examples
//!
//! ```rust
//! use kalamdb_commons::models::{UserId, NamespaceId, TableName};
//!
//! let user_id = UserId::new("user_123");
//! let namespace_id = NamespaceId::new("default");
//! let table_name = TableName::new("conversations");
//!
//! // Type safety prevents mixing
//! // let wrong: UserId = namespace_id; // Compile error!
//!
//! // Conversion to string
//! let id_str: &str = user_id.as_str();
//! let owned: String = user_id.into_string();
//! ```

use std::fmt;

/// Type-safe wrapper for user identifiers.
///
/// Ensures user IDs cannot be accidentally used where namespace IDs or table names
/// are expected.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

/// Type-safe wrapper for namespace identifiers.
///
/// Ensures namespace IDs cannot be accidentally used where user IDs or table names
/// are expected.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NamespaceId(String);

impl NamespaceId {
    /// Creates a new NamespaceId from a string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the namespace ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the wrapper and returns the inner String.
    pub fn into_string(self) -> String {
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

/// Type-safe wrapper for table names.
///
/// Ensures table names cannot be accidentally used where user IDs or namespace IDs
/// are expected.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TableName(String);

impl TableName {
    /// Creates a new TableName from a string.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Returns the table name as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the wrapper and returns the inner String.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for TableName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for TableName {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for TableName {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for TableName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Enum representing the type of storage backend in KalamDB.
///
/// - **Filesystem**: Local or network filesystem storage
/// - **S3**: Amazon S3 or S3-compatible object storage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StorageType {
    /// Local or network filesystem storage
    Filesystem,
    /// Amazon S3 or S3-compatible object storage
    S3,
}

impl StorageType {
    /// Returns the storage type as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            StorageType::Filesystem => "filesystem",
            StorageType::S3 => "s3",
        }
    }
}

impl fmt::Display for StorageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl TryFrom<&str> for StorageType {
    type Error = String;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s.to_lowercase().as_str() {
            "filesystem" => Ok(StorageType::Filesystem),
            "s3" => Ok(StorageType::S3),
            _ => Err(format!("Invalid storage type: {}", s)),
        }
    }
}

/// Enum representing the type of table in KalamDB.
///
/// - **USER**: Per-user table with user_id-based partitioning (key format: `{user_id}:{row_id}`)
/// - **SHARED**: Shared table without user partitioning (key format: `{row_id}`)
/// - **STREAM**: Ephemeral stream table with time-based keys (key format: `{timestamp}:{row_id}`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TableType {
    /// Per-user table with user_id partitioning
    User,
    /// Shared table without user partitioning
    Shared,
    /// Ephemeral stream table with time-based eviction
    Stream,
}

impl TableType {
    /// Returns the table type as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            TableType::User => "USER",
            TableType::Shared => "SHARED",
            TableType::Stream => "STREAM",
        }
    }
}

impl fmt::Display for TableType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl TryFrom<&str> for TableType {
    type Error = String;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s.to_uppercase().as_str() {
            "USER" => Ok(TableType::User),
            "SHARED" => Ok(TableType::Shared),
            "STREAM" => Ok(TableType::Stream),
            _ => Err(format!("Invalid table type: {}", s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_id() {
        let user_id = UserId::new("user_123");
        assert_eq!(user_id.as_str(), "user_123");
        assert_eq!(user_id.to_string(), "user_123");
    }

    #[test]
    fn test_namespace_id() {
        let namespace_id = NamespaceId::new("default");
        assert_eq!(namespace_id.as_str(), "default");
        assert_eq!(namespace_id.to_string(), "default");
    }

    #[test]
    fn test_table_name() {
        let table_name = TableName::new("conversations");
        assert_eq!(table_name.as_str(), "conversations");
        assert_eq!(table_name.to_string(), "conversations");
    }

    #[test]
    fn test_table_type() {
        assert_eq!(TableType::User.as_str(), "USER");
        assert_eq!(TableType::Shared.as_str(), "SHARED");
        assert_eq!(TableType::Stream.as_str(), "STREAM");

        assert_eq!(TableType::try_from("USER").unwrap(), TableType::User);
        assert_eq!(TableType::try_from("user").unwrap(), TableType::User);
        assert_eq!(TableType::try_from("SHARED").unwrap(), TableType::Shared);
        assert_eq!(TableType::try_from("STREAM").unwrap(), TableType::Stream);
        assert!(TableType::try_from("INVALID").is_err());
    }
}
