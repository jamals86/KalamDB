//! SHOW TABLES statement parser
//!
//! Parses SQL statements like:
//! - SHOW TABLES
//! - SHOW TABLES IN namespace

use crate::ddl::DdlResult;

use kalamdb_commons::models::NamespaceId;

/// SHOW TABLES statement
#[derive(Debug, Clone, PartialEq)]
pub struct ShowTablesStatement {
    /// Optional namespace to filter tables
    pub namespace_id: Option<NamespaceId>,
}

impl ShowTablesStatement {
    /// Parse a SHOW TABLES statement from SQL
    ///
    /// Supports syntax:
    /// - SHOW TABLES
    /// - SHOW TABLES IN namespace
    pub fn parse(sql: &str) -> DdlResult<Self> {
        let sql_trimmed = sql.trim();
        let sql_upper = sql_trimmed.to_uppercase();

        if !sql_upper.starts_with("SHOW TABLES") {
            return Err("Expected SHOW TABLES statement".to_string());
        }

        // Check for IN clause
        if sql_upper.contains(" IN ") {
            // Extract namespace name after IN
            let in_pos = sql_upper.find(" IN ").unwrap();
            let namespace_part = sql_trimmed[in_pos + 4..].trim();

            if namespace_part.is_empty() {
                return Err("Namespace name required after IN".to_string());
            }

            let namespace_name = namespace_part
                .split_whitespace()
                .next()
                .ok_or_else(|| "Namespace name required after IN".to_string())?;

            Ok(Self {
                namespace_id: Some(NamespaceId::new(namespace_name)),
            })
        } else if sql_upper.ends_with(" IN") {
            // Trailing IN without namespace
            Err("Namespace name required after IN".to_string())
        } else {
            // No namespace filter - show all tables
            Ok(Self { namespace_id: None })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_show_tables() {
        let stmt = ShowTablesStatement::parse("SHOW TABLES").unwrap();
        assert!(stmt.namespace_id.is_none());
    }

    #[test]
    fn test_parse_show_tables_in_namespace() {
        let stmt = ShowTablesStatement::parse("SHOW TABLES IN app").unwrap();
        assert_eq!(stmt.namespace_id.unwrap().as_str(), "app");
    }

    #[test]
    fn test_parse_show_tables_lowercase() {
        let stmt = ShowTablesStatement::parse("show tables in myapp").unwrap();
        assert_eq!(stmt.namespace_id.unwrap().as_str(), "myapp");
    }

    #[test]
    fn test_parse_show_tables_missing_namespace() {
        let result = ShowTablesStatement::parse("SHOW TABLES IN");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_statement() {
        let result = ShowTablesStatement::parse("SHOW DATABASES");
        assert!(result.is_err());
    }
}
