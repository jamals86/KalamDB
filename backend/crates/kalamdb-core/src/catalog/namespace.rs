//! Namespace entity for organizing tables
//!
//! Namespaces are logical containers for tables in KalamDB.

use crate::catalog::NamespaceId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Namespace entity
///
/// Represents a namespace that contains tables. Namespaces are the top-level
/// organizational structure in KalamDB.
///
/// # Examples
///
/// ```
/// use kalamdb_core::catalog::{Namespace, NamespaceId};
/// use chrono::Utc;
/// use std::collections::HashMap;
///
/// let namespace = Namespace {
///     name: NamespaceId::new("app"),
///     created_at: Utc::now(),
///     options: HashMap::new(),
///     table_count: 0,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Namespace {
    /// Unique namespace identifier
    pub name: NamespaceId,

    /// When the namespace was created
    pub created_at: DateTime<Utc>,

    /// Custom namespace configuration
    #[serde(default)]
    pub options: HashMap<String, serde_json::Value>,

    /// Number of tables in this namespace
    #[serde(default)]
    pub table_count: u32,
}

impl Namespace {
    /// Create a new namespace
    pub fn new(name: impl Into<NamespaceId>) -> Self {
        Self {
            name: name.into(),
            created_at: Utc::now(),
            options: HashMap::new(),
            table_count: 0,
        }
    }

    /// Validate namespace name format
    ///
    /// Name must match regex: ^[a-z][a-z0-9_]*$ (lowercase, start with letter)
    /// Name cannot be "system" (reserved)
    pub fn validate_name(name: &str) -> Result<(), String> {
        if name == "system" {
            return Err("Namespace name 'system' is reserved".to_string());
        }

        if !name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase())
        {
            return Err("Namespace name must start with a lowercase letter".to_string());
        }

        if !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(
                "Namespace name can only contain lowercase letters, digits, and underscores"
                    .to_string(),
            );
        }

        Ok(())
    }

    /// Check if this namespace can be deleted (has no tables)
    pub fn can_delete(&self) -> bool {
        self.table_count == 0
    }

    /// Increment the table count
    pub fn increment_table_count(&mut self) {
        self.table_count += 1;
    }

    /// Decrement the table count
    pub fn decrement_table_count(&mut self) {
        if self.table_count > 0 {
            self.table_count -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_creation() {
        let namespace = Namespace::new("test_namespace");
        assert_eq!(namespace.name.as_str(), "test_namespace");
        assert_eq!(namespace.table_count, 0);
        assert!(namespace.options.is_empty());
    }

    #[test]
    fn test_validate_name_valid() {
        assert!(Namespace::validate_name("app").is_ok());
        assert!(Namespace::validate_name("analytics_db").is_ok());
        assert!(Namespace::validate_name("test123").is_ok());
    }

    #[test]
    fn test_validate_name_invalid() {
        assert!(Namespace::validate_name("system").is_err());
        assert!(Namespace::validate_name("App").is_err());
        assert!(Namespace::validate_name("123app").is_err());
        assert!(Namespace::validate_name("app-name").is_err());
    }

    #[test]
    fn test_can_delete() {
        let mut namespace = Namespace::new("test");
        assert!(namespace.can_delete());

        namespace.increment_table_count();
        assert!(!namespace.can_delete());

        namespace.decrement_table_count();
        assert!(namespace.can_delete());
    }

    #[test]
    fn test_table_count_operations() {
        let mut namespace = Namespace::new("test");
        assert_eq!(namespace.table_count, 0);

        namespace.increment_table_count();
        assert_eq!(namespace.table_count, 1);

        namespace.increment_table_count();
        assert_eq!(namespace.table_count, 2);

        namespace.decrement_table_count();
        assert_eq!(namespace.table_count, 1);

        namespace.decrement_table_count();
        assert_eq!(namespace.table_count, 0);

        // Should not go below 0
        namespace.decrement_table_count();
        assert_eq!(namespace.table_count, 0);
    }
}
