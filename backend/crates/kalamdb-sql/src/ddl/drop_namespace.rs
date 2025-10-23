//! DROP NAMESPACE statement parser
//!
//! Parses SQL statements like:
//! - DROP NAMESPACE app
//! - DROP NAMESPACE IF EXISTS app

use crate::ddl::{parsing, DdlResult};
use kalamdb_commons::models::NamespaceId;

/// DROP NAMESPACE statement
#[derive(Debug, Clone, PartialEq)]
pub struct DropNamespaceStatement {
    /// Namespace name to drop
    pub name: NamespaceId,

    /// If true, don't error if namespace doesn't exist
    pub if_exists: bool,
}

impl DropNamespaceStatement {
    /// Parse a DROP NAMESPACE statement from SQL
    ///
    /// Supports syntax:
    /// - DROP NAMESPACE name
    /// - DROP NAMESPACE IF EXISTS name
    pub fn parse(sql: &str) -> DdlResult<Self> {
        let (name, if_exists) =
            parsing::parse_create_drop_statement(sql, "DROP NAMESPACE", "IF EXISTS")?;

        Ok(Self {
            name: NamespaceId::new(name),
            if_exists,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_drop_namespace() {
        let stmt = DropNamespaceStatement::parse("DROP NAMESPACE app").unwrap();
        assert_eq!(stmt.name.as_str(), "app");
        assert!(!stmt.if_exists);
    }

    #[test]
    fn test_parse_drop_namespace_if_exists() {
        let stmt = DropNamespaceStatement::parse("DROP NAMESPACE IF EXISTS app").unwrap();
        assert_eq!(stmt.name.as_str(), "app");
        assert!(stmt.if_exists);
    }

    #[test]
    fn test_parse_drop_namespace_lowercase() {
        let stmt = DropNamespaceStatement::parse("drop namespace myapp").unwrap();
        assert_eq!(stmt.name.as_str(), "myapp");
    }

    #[test]
    fn test_parse_drop_namespace_missing_name() {
        let result = DropNamespaceStatement::parse("DROP NAMESPACE");
        assert!(result.is_err());
    }
}
