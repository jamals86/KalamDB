//! DESCRIBE TABLE statement parser
//!
//! Parses SQL statements like:
//! - DESCRIBE TABLE table_name
//! - DESCRIBE TABLE namespace.table_name
//! - DESC TABLE table_name

use crate::catalog::{NamespaceId, TableName};
use crate::error::KalamDbError;

/// DESCRIBE TABLE statement
#[derive(Debug, Clone, PartialEq)]
pub struct DescribeTableStatement {
    /// Optional namespace (if qualified name used)
    pub namespace_id: Option<NamespaceId>,
    
    /// Table name to describe
    pub table_name: TableName,
}

impl DescribeTableStatement {
    /// Parse a DESCRIBE TABLE statement from SQL
    ///
    /// Supports syntax:
    /// - DESCRIBE TABLE table_name
    /// - DESCRIBE TABLE namespace.table_name
    /// - DESC TABLE table_name (shorthand)
    pub fn parse(sql: &str) -> Result<Self, KalamDbError> {
        let sql_trimmed = sql.trim();
        let sql_upper = sql_trimmed.to_uppercase();
        
        // Support both DESCRIBE and DESC
        let has_describe = sql_upper.starts_with("DESCRIBE TABLE") || sql_upper.starts_with("DESC TABLE");
        
        if !has_describe {
            return Err(KalamDbError::InvalidSql(
                "Expected DESCRIBE TABLE or DESC TABLE statement".to_string(),
            ));
        }

        // Extract table name part
        let table_part = if sql_upper.starts_with("DESCRIBE TABLE") {
            sql_trimmed.strip_prefix("DESCRIBE TABLE")
                .or_else(|| sql_trimmed.strip_prefix("describe table"))
                .map(|s| s.trim())
        } else {
            sql_trimmed.strip_prefix("DESC TABLE")
                .or_else(|| sql_trimmed.strip_prefix("desc table"))
                .map(|s| s.trim())
        };

        let table_part = table_part.ok_or_else(|| {
            KalamDbError::InvalidSql("Invalid DESCRIBE TABLE syntax".to_string())
        })?;

        let table_identifier = table_part.split_whitespace().next()
            .ok_or_else(|| {
                KalamDbError::InvalidSql("Table name is required".to_string())
            })?;

        // Check for qualified name (namespace.table)
        if table_identifier.contains('.') {
            let parts: Vec<&str> = table_identifier.split('.').collect();
            if parts.len() != 2 {
                return Err(KalamDbError::InvalidSql(
                    "Invalid qualified table name format".to_string(),
                ));
            }

            Ok(Self {
                namespace_id: Some(NamespaceId::new(parts[0])),
                table_name: TableName::new(parts[1]),
            })
        } else {
            // Unqualified name
            Ok(Self {
                namespace_id: None,
                table_name: TableName::new(table_identifier),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_describe_table() {
        let stmt = DescribeTableStatement::parse("DESCRIBE TABLE users").unwrap();
        assert!(stmt.namespace_id.is_none());
        assert_eq!(stmt.table_name.as_str(), "users");
    }

    #[test]
    fn test_parse_describe_table_qualified() {
        let stmt = DescribeTableStatement::parse("DESCRIBE TABLE app.users").unwrap();
        assert_eq!(stmt.namespace_id.unwrap().as_str(), "app");
        assert_eq!(stmt.table_name.as_str(), "users");
    }

    #[test]
    fn test_parse_desc_table() {
        let stmt = DescribeTableStatement::parse("DESC TABLE users").unwrap();
        assert!(stmt.namespace_id.is_none());
        assert_eq!(stmt.table_name.as_str(), "users");
    }

    #[test]
    fn test_parse_describe_table_lowercase() {
        let stmt = DescribeTableStatement::parse("describe table myapp.messages").unwrap();
        assert_eq!(stmt.namespace_id.unwrap().as_str(), "myapp");
        assert_eq!(stmt.table_name.as_str(), "messages");
    }

    #[test]
    fn test_parse_describe_table_missing_name() {
        let result = DescribeTableStatement::parse("DESCRIBE TABLE");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_qualified_name() {
        let result = DescribeTableStatement::parse("DESCRIBE TABLE a.b.c");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_statement() {
        let result = DescribeTableStatement::parse("DESCRIBE NAMESPACE app");
        assert!(result.is_err());
    }
}
