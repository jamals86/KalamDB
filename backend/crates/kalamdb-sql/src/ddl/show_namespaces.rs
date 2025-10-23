//! SHOW NAMESPACES statement parser
//!
//! Parses SQL statements like:
//! - SHOW NAMESPACES

use crate::ddl::{parsing, DdlResult};

/// SHOW NAMESPACES statement
#[derive(Debug, Clone, PartialEq)]
pub struct ShowNamespacesStatement;

impl ShowNamespacesStatement {
    /// Parse a SHOW NAMESPACES statement from SQL
    pub fn parse(sql: &str) -> DdlResult<Self> {
        parsing::parse_simple_statement(sql, "SHOW NAMESPACES")?;
        Ok(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_show_namespaces() {
        let stmt = ShowNamespacesStatement::parse("SHOW NAMESPACES").unwrap();
        assert_eq!(stmt, ShowNamespacesStatement);
    }

    #[test]
    fn test_parse_show_namespaces_lowercase() {
        let stmt = ShowNamespacesStatement::parse("show namespaces").unwrap();
        assert_eq!(stmt, ShowNamespacesStatement);
    }

    #[test]
    fn test_parse_show_namespaces_invalid() {
        let result = ShowNamespacesStatement::parse("SHOW TABLES");
        assert!(result.is_err());
    }
}
