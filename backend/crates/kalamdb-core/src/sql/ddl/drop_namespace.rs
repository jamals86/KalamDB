//! DROP NAMESPACE statement parser
//!
//! Parses SQL statements like:
//! - DROP NAMESPACE app
//! - DROP NAMESPACE IF EXISTS app

use crate::catalog::NamespaceId;
use crate::error::KalamDbError;

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
    pub fn parse(sql: &str) -> Result<Self, KalamDbError> {
        let sql_upper = sql.trim().to_uppercase();
        
        if !sql_upper.starts_with("DROP NAMESPACE") {
            return Err(KalamDbError::InvalidSql(
                "Expected DROP NAMESPACE statement".to_string(),
            ));
        }

        let if_exists = sql_upper.contains("IF EXISTS");
        
        // Extract namespace name
        let name_part = if if_exists {
            sql.trim()
                .strip_prefix("DROP NAMESPACE")
                .and_then(|s| s.trim().strip_prefix("IF EXISTS"))
                .or_else(|| {
                    sql.trim()
                        .strip_prefix("drop namespace")
                        .and_then(|s| s.trim().strip_prefix("if exists"))
                })
                .map(|s| s.trim())
        } else {
            sql.trim()
                .strip_prefix("DROP NAMESPACE")
                .or_else(|| sql.trim().strip_prefix("drop namespace"))
                .map(|s| s.trim())
        };

        let name = name_part
            .and_then(|s| s.split_whitespace().next())
            .ok_or_else(|| {
                KalamDbError::InvalidSql("Namespace name is required".to_string())
            })?;

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
