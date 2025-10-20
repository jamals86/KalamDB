//! SHOW TABLE STATS statement parser
//!
//! Parses SQL statements like:
//! - SHOW STATS FOR TABLE table_name
//! - SHOW STATS FOR TABLE namespace.table_name

use crate::catalog::{NamespaceId, TableName};
use crate::error::KalamDbError;

/// SHOW TABLE STATS statement
#[derive(Debug, Clone, PartialEq)]
pub struct ShowTableStatsStatement {
    /// Optional namespace (if qualified name used)
    pub namespace_id: Option<NamespaceId>,
    
    /// Table name to show statistics for
    pub table_name: TableName,
}

impl ShowTableStatsStatement {
    /// Parse a SHOW TABLE STATS statement from SQL
    ///
    /// Supports syntax:
    /// - SHOW STATS FOR TABLE table_name
    /// - SHOW STATS FOR TABLE namespace.table_name
    pub fn parse(sql: &str) -> Result<Self, KalamDbError> {
        let sql_trimmed = sql.trim();
        let sql_upper = sql_trimmed.to_uppercase();
        
        if !sql_upper.starts_with("SHOW STATS FOR TABLE") {
            return Err(KalamDbError::InvalidSql(
                "Expected SHOW STATS FOR TABLE statement".to_string(),
            ));
        }

        // Extract table name part
        let table_part = sql_trimmed.strip_prefix("SHOW STATS FOR TABLE")
            .or_else(|| sql_trimmed.strip_prefix("show stats for table"))
            .map(|s| s.trim())
            .ok_or_else(|| {
                KalamDbError::InvalidSql("Invalid SHOW STATS syntax".to_string())
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
    fn test_parse_show_stats() {
        let stmt = ShowTableStatsStatement::parse("SHOW STATS FOR TABLE users").unwrap();
        assert!(stmt.namespace_id.is_none());
        assert_eq!(stmt.table_name.as_str(), "users");
    }

    #[test]
    fn test_parse_show_stats_qualified() {
        let stmt = ShowTableStatsStatement::parse("SHOW STATS FOR TABLE app.users").unwrap();
        assert_eq!(stmt.namespace_id.unwrap().as_str(), "app");
        assert_eq!(stmt.table_name.as_str(), "users");
    }

    #[test]
    fn test_parse_show_stats_lowercase() {
        let stmt = ShowTableStatsStatement::parse("show stats for table myapp.messages").unwrap();
        assert_eq!(stmt.namespace_id.unwrap().as_str(), "myapp");
        assert_eq!(stmt.table_name.as_str(), "messages");
    }

    #[test]
    fn test_parse_show_stats_missing_name() {
        let result = ShowTableStatsStatement::parse("SHOW STATS FOR TABLE");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_qualified_name() {
        let result = ShowTableStatsStatement::parse("SHOW STATS FOR TABLE a.b.c");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_statement() {
        let result = ShowTableStatsStatement::parse("SHOW STATISTICS FOR TABLE users");
        assert!(result.is_err());
    }
}
