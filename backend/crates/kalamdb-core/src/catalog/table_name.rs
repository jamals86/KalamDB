//! Type-safe wrapper for table identifiers
//!
//! This module provides a newtype pattern around String to ensure type safety
//! when working with table identifiers throughout the codebase.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Type-safe wrapper for table identifiers
///
/// # Examples
///
/// ```
/// use kalamdb_core::catalog::TableName;
///
/// let table_name = TableName::new("messages");
/// assert_eq!(table_name.as_str(), "messages");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TableName(String);

impl TableName {
    /// Create a new TableName from a string
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Get the table name as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Convert into the underlying String
    pub fn into_inner(self) -> String {
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

impl From<kalamdb_commons::models::TableName> for TableName {
    fn from(value: kalamdb_commons::models::TableName) -> Self {
        TableName::new(value.into_string())
    }
}

impl From<&kalamdb_commons::models::TableName> for TableName {
    fn from(value: &kalamdb_commons::models::TableName) -> Self {
        TableName::new(value.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_name_creation() {
        let name = TableName::new("events");
        assert_eq!(name.as_str(), "events");
    }

    #[test]
    fn test_table_name_equality() {
        let name1 = TableName::new("table1");
        let name2 = TableName::new("table1");
        let name3 = TableName::new("table2");

        assert_eq!(name1, name2);
        assert_ne!(name1, name3);
    }

    #[test]
    fn test_table_name_display() {
        let name = TableName::new("my_table");
        assert_eq!(format!("{}", name), "my_table");
    }
}
