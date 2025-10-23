//! DESCRIBE TABLE statement parser
//!
//! Parses SQL statements like:
//! - DESCRIBE TABLE table_name
//! - DESCRIBE TABLE namespace.table_name
//! - DESC TABLE table_name
//! - DESCRIBE TABLE table_name HISTORY (show schema versions)

use crate::ddl::DdlResult;

use kalamdb_commons::models::{NamespaceId, TableName};

/// DESCRIBE TABLE statement
#[derive(Debug, Clone, PartialEq)]
pub struct DescribeTableStatement {
    /// Optional namespace (if qualified name used)
    pub namespace_id: Option<NamespaceId>,

    /// Table name to describe
    pub table_name: TableName,

    /// If true, show schema history (all versions)
    pub show_history: bool,
}

impl DescribeTableStatement {
    /// Parse a DESCRIBE TABLE statement from SQL
    ///
    /// Supports syntax:
    /// - DESCRIBE TABLE table_name
    /// - DESCRIBE TABLE namespace.table_name
    /// - DESC TABLE table_name (shorthand)
    /// - DESCRIBE TABLE table_name HISTORY
    pub fn parse(sql: &str) -> DdlResult<Self> {
        let sql = sql.trim();
        let sql_upper = sql.to_uppercase();

        // Check for DESCRIBE TABLE or DESC TABLE prefix (case-insensitive)
        let rest = if sql_upper.strip_prefix("DESCRIBE TABLE ").is_some() {
            sql[15..].trim() // Use original casing for table names
        } else if sql_upper.strip_prefix("DESC TABLE ").is_some() {
            sql[11..].trim() // Use original casing for table names
        } else {
            return Err("Expected DESCRIBE TABLE or DESC TABLE".to_string());
        };

        // Check for HISTORY keyword at the end (case-insensitive)
        let rest_upper = rest.to_uppercase();
        let (table_ref, show_history) = if rest_upper.ends_with(" HISTORY") {
            (rest[..rest.len() - 8].trim(), true)
        } else {
            (rest, false)
        };

        // Split on . for qualified names (namespace.table)
        let parts: Vec<&str> = table_ref.split('.').collect();

        match parts.len() {
            // Simple table name (no namespace)
            1 => Ok(Self {
                namespace_id: None,
                table_name: TableName::new(parts[0].trim()),
                show_history,
            }),
            // Qualified name (namespace.table)
            2 => Ok(Self {
                namespace_id: Some(NamespaceId::new(parts[0].trim())),
                table_name: TableName::new(parts[1].trim()),
                show_history,
            }),
            _ => Err(format!("Invalid table reference: {}", table_ref)),
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

    #[test]
    fn test_describe_table_history() {
        let stmt = DescribeTableStatement::parse("DESCRIBE TABLE users HISTORY").unwrap();
        assert!(stmt.namespace_id.is_none());
        assert_eq!(stmt.table_name.as_str(), "users");
        assert!(stmt.show_history);
    }

    #[test]
    fn test_desc_table_history() {
        let stmt = DescribeTableStatement::parse("DESC TABLE users HISTORY").unwrap();
        assert!(stmt.namespace_id.is_none());
        assert_eq!(stmt.table_name.as_str(), "users");
        assert!(stmt.show_history);
    }

    #[test]
    fn test_describe_qualified_table_history() {
        let stmt = DescribeTableStatement::parse("DESCRIBE TABLE myapp.messages HISTORY").unwrap();
        assert_eq!(stmt.namespace_id.unwrap().as_str(), "myapp");
        assert_eq!(stmt.table_name.as_str(), "messages");
        assert!(stmt.show_history);
    }

    #[test]
    fn test_describe_table_no_history() {
        let stmt = DescribeTableStatement::parse("DESCRIBE TABLE users").unwrap();
        assert!(!stmt.show_history);
    }
}
