//! Type-safe wrapper for table names.

use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Type-safe wrapper for table names.
///
/// Ensures table names cannot be accidentally used where user IDs or namespace IDs
/// are expected.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", derive(bincode::Encode, bincode::Decode))]
pub struct TableName(String);

impl TableName {
    /// Creates a new TableName from a string.
    ///
    /// Table names are case-insensitive - they are normalized to lowercase internally.
    /// For example, `TableName::new("Users")` and `TableName::new("users")` are equal.
    #[inline]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into().to_lowercase())
    }

    /// Returns the table name as a string slice.
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the wrapper and returns the inner String.
    #[inline]
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
        Self(s.to_lowercase())
    }
}

impl From<&str> for TableName {
    fn from(s: &str) -> Self {
        Self(s.to_lowercase())
    }
}

impl AsRef<str> for TableName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_name_case_insensitive() {
        // All these should be equal because names are normalized to lowercase
        let name1 = TableName::new("Users");
        let name2 = TableName::new("users");
        let name3 = TableName::new("USERS");
        let name4 = TableName::from("UsErS".to_string());
        let name5: TableName = "uSeRs".into();

        assert_eq!(name1, name2);
        assert_eq!(name2, name3);
        assert_eq!(name3, name4);
        assert_eq!(name4, name5);

        // Stored value should always be lowercase
        assert_eq!(name1.as_str(), "users");
        assert_eq!(name3.as_str(), "users");
    }

    #[test]
    fn test_table_name_display() {
        let name = TableName::new("MyTable");
        assert_eq!(format!("{}", name), "mytable");
    }
}
