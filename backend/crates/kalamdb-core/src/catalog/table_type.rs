//! Table type enumeration
//!
//! This module defines the different types of tables supported in KalamDB.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Table type enumeration
///
/// Defines the different types of tables that can exist in KalamDB:
/// - User: One table instance per user ID
/// - Shared: Single table accessible by all users
/// - System: Internal system tables
/// - Stream: Time-series stream tables
///
/// # Examples
///
/// ```
/// use kalamdb_core::catalog::TableType;
///
/// let user_table = TableType::User;
/// let shared_table = TableType::Shared;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TableType {
    /// One table instance per user ID, isolated data
    User,
    /// Single table accessible by all users (subject to permissions)
    Shared,
    /// Internal system tables (system.users, system.live_queries, etc.)
    System,
    /// Time-series stream tables with TTL-based eviction
    Stream,
}

impl TableType {
    /// Get the string representation of the table type
    pub fn as_str(&self) -> &'static str {
        match self {
            TableType::User => "user",
            TableType::Shared => "shared",
            TableType::System => "system",
            TableType::Stream => "stream",
        }
    }

    /// Parse a table type from a string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "user" => Some(TableType::User),
            "shared" => Some(TableType::Shared),
            "system" => Some(TableType::System),
            "stream" => Some(TableType::Stream),
            _ => None,
        }
    }
}

impl fmt::Display for TableType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_type_as_str() {
        assert_eq!(TableType::User.as_str(), "user");
        assert_eq!(TableType::Shared.as_str(), "shared");
        assert_eq!(TableType::System.as_str(), "system");
        assert_eq!(TableType::Stream.as_str(), "stream");
    }

    #[test]
    fn test_table_type_from_str() {
        assert_eq!(TableType::from_str("user"), Some(TableType::User));
        assert_eq!(TableType::from_str("User"), Some(TableType::User));
        assert_eq!(TableType::from_str("shared"), Some(TableType::Shared));
        assert_eq!(TableType::from_str("system"), Some(TableType::System));
        assert_eq!(TableType::from_str("stream"), Some(TableType::Stream));
        assert_eq!(TableType::from_str("invalid"), None);
    }

    #[test]
    fn test_table_type_display() {
        assert_eq!(format!("{}", TableType::User), "user");
        assert_eq!(format!("{}", TableType::Shared), "shared");
    }
}
