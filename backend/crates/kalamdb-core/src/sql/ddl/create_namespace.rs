//! CREATE NAMESPACE statement parser
//!
//! Parses SQL statements like:
//! - CREATE NAMESPACE app
//! - CREATE NAMESPACE IF NOT EXISTS app

use crate::catalog::NamespaceId;
use crate::error::KalamDbError;

/// CREATE NAMESPACE statement
#[derive(Debug, Clone, PartialEq)]
pub struct CreateNamespaceStatement {
    /// Namespace name to create
    pub name: NamespaceId,

    /// If true, don't error if namespace already exists
    pub if_not_exists: bool,
}

impl CreateNamespaceStatement {
    /// Parse a CREATE NAMESPACE statement from SQL
    ///
    /// Supports syntax:
    /// - CREATE NAMESPACE name
    /// - CREATE NAMESPACE IF NOT EXISTS name
    pub fn parse(sql: &str) -> Result<Self, KalamDbError> {
        let sql_upper = sql.trim().to_uppercase();

        if !sql_upper.starts_with("CREATE NAMESPACE") {
            return Err(KalamDbError::InvalidSql(
                "Expected CREATE NAMESPACE statement".to_string(),
            ));
        }

        let if_not_exists = sql_upper.contains("IF NOT EXISTS");

        // Extract namespace name
        let name_part = if if_not_exists {
            sql.trim()
                .strip_prefix("CREATE NAMESPACE")
                .and_then(|s| s.trim().strip_prefix("IF NOT EXISTS"))
                .or_else(|| {
                    sql.trim()
                        .strip_prefix("create namespace")
                        .and_then(|s| s.trim().strip_prefix("if not exists"))
                })
                .map(|s| s.trim())
        } else {
            sql.trim()
                .strip_prefix("CREATE NAMESPACE")
                .or_else(|| sql.trim().strip_prefix("create namespace"))
                .map(|s| s.trim())
        };

        let name = name_part
            .and_then(|s| s.split_whitespace().next())
            .ok_or_else(|| KalamDbError::InvalidSql("Namespace name is required".to_string()))?;

        Ok(Self {
            name: NamespaceId::new(name),
            if_not_exists,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_create_namespace() {
        let stmt = CreateNamespaceStatement::parse("CREATE NAMESPACE app").unwrap();
        assert_eq!(stmt.name.as_str(), "app");
        assert!(!stmt.if_not_exists);
    }

    #[test]
    fn test_parse_create_namespace_if_not_exists() {
        let stmt = CreateNamespaceStatement::parse("CREATE NAMESPACE IF NOT EXISTS app").unwrap();
        assert_eq!(stmt.name.as_str(), "app");
        assert!(stmt.if_not_exists);
    }

    #[test]
    fn test_parse_create_namespace_lowercase() {
        let stmt = CreateNamespaceStatement::parse("create namespace myapp").unwrap();
        assert_eq!(stmt.name.as_str(), "myapp");
    }

    #[test]
    fn test_parse_create_namespace_missing_name() {
        let result = CreateNamespaceStatement::parse("CREATE NAMESPACE");
        assert!(result.is_err());
    }
}
