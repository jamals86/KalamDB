//! DROP TABLE statement parser
//!
//! Parses SQL statements like:
//! - DROP USER TABLE messages
//! - DROP SHARED TABLE conversations  
//! - DROP STREAM TABLE events
//! - DROP TABLE IF EXISTS messages

use crate::catalog::{NamespaceId, TableName, TableType};
use crate::error::KalamDbError;

/// DROP TABLE statement
#[derive(Debug, Clone, PartialEq)]
pub struct DropTableStatement {
    /// Table name to drop
    pub table_name: TableName,
    
    /// Namespace ID (defaults to current namespace)
    pub namespace_id: NamespaceId,
    
    /// Table type (User, Shared, or Stream)
    pub table_type: TableType,
    
    /// If true, don't error if table doesn't exist
    pub if_exists: bool,
}

impl DropTableStatement {
    /// Parse a DROP TABLE statement from SQL
    ///
    /// Supports syntax:
    /// - DROP USER TABLE name
    /// - DROP SHARED TABLE name
    /// - DROP STREAM TABLE name
    /// - DROP TABLE IF EXISTS name (defaults to USER TABLE)
    /// - DROP USER TABLE IF EXISTS name
    pub fn parse(sql: &str, current_namespace: &NamespaceId) -> Result<Self, KalamDbError> {
        let sql_upper = sql.trim().to_uppercase();
        
        if !sql_upper.starts_with("DROP") {
            return Err(KalamDbError::InvalidSql(
                "Expected DROP TABLE statement".to_string(),
            ));
        }

        // Determine if IF EXISTS is present
        let if_exists = sql_upper.contains("IF EXISTS");
        
        // Determine table type
        let table_type = if sql_upper.contains("DROP USER TABLE") {
            TableType::User
        } else if sql_upper.contains("DROP SHARED TABLE") {
            TableType::Shared
        } else if sql_upper.contains("DROP STREAM TABLE") {
            TableType::Stream
        } else if sql_upper.contains("DROP TABLE") {
            // Default to USER TABLE if no type specified
            TableType::User
        } else {
            return Err(KalamDbError::InvalidSql(
                "Expected DROP [USER|SHARED|STREAM] TABLE statement".to_string(),
            ));
        };

        // Extract table name
        let name_part = if if_exists {
            // Handle "DROP [TYPE] TABLE IF EXISTS name"
            sql.trim()
                .strip_prefix("DROP USER TABLE")
                .or_else(|| sql.trim().strip_prefix("drop user table"))
                .or_else(|| sql.trim().strip_prefix("DROP SHARED TABLE"))
                .or_else(|| sql.trim().strip_prefix("drop shared table"))
                .or_else(|| sql.trim().strip_prefix("DROP STREAM TABLE"))
                .or_else(|| sql.trim().strip_prefix("drop stream table"))
                .or_else(|| sql.trim().strip_prefix("DROP TABLE"))
                .or_else(|| sql.trim().strip_prefix("drop table"))
                .and_then(|s| {
                    s.trim()
                        .strip_prefix("IF EXISTS")
                        .or_else(|| s.trim().strip_prefix("if exists"))
                })
                .map(|s| s.trim())
        } else {
            // Handle "DROP [TYPE] TABLE name"
            sql.trim()
                .strip_prefix("DROP USER TABLE")
                .or_else(|| sql.trim().strip_prefix("drop user table"))
                .or_else(|| sql.trim().strip_prefix("DROP SHARED TABLE"))
                .or_else(|| sql.trim().strip_prefix("drop shared table"))
                .or_else(|| sql.trim().strip_prefix("DROP STREAM TABLE"))
                .or_else(|| sql.trim().strip_prefix("drop stream table"))
                .or_else(|| sql.trim().strip_prefix("DROP TABLE"))
                .or_else(|| sql.trim().strip_prefix("drop table"))
                .map(|s| s.trim())
        };

        let name = name_part
            .and_then(|s| s.split_whitespace().next())
            .ok_or_else(|| {
                KalamDbError::InvalidSql("Table name is required".to_string())
            })?;

        Ok(Self {
            table_name: TableName::new(name),
            namespace_id: current_namespace.clone(),
            table_type,
            if_exists,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_namespace() -> NamespaceId {
        NamespaceId::new("test_app")
    }

    #[test]
    fn test_parse_drop_user_table() {
        let stmt = DropTableStatement::parse("DROP USER TABLE messages", &test_namespace()).unwrap();
        assert_eq!(stmt.table_name.as_str(), "messages");
        assert_eq!(stmt.table_type, TableType::User);
        assert!(!stmt.if_exists);
    }

    #[test]
    fn test_parse_drop_shared_table() {
        let stmt = DropTableStatement::parse("DROP SHARED TABLE conversations", &test_namespace()).unwrap();
        assert_eq!(stmt.table_name.as_str(), "conversations");
        assert_eq!(stmt.table_type, TableType::Shared);
        assert!(!stmt.if_exists);
    }

    #[test]
    fn test_parse_drop_stream_table() {
        let stmt = DropTableStatement::parse("DROP STREAM TABLE events", &test_namespace()).unwrap();
        assert_eq!(stmt.table_name.as_str(), "events");
        assert_eq!(stmt.table_type, TableType::Stream);
        assert!(!stmt.if_exists);
    }

    #[test]
    fn test_parse_drop_table_defaults_to_user() {
        let stmt = DropTableStatement::parse("DROP TABLE messages", &test_namespace()).unwrap();
        assert_eq!(stmt.table_name.as_str(), "messages");
        assert_eq!(stmt.table_type, TableType::User);
        assert!(!stmt.if_exists);
    }

    #[test]
    fn test_parse_drop_table_if_exists() {
        let stmt = DropTableStatement::parse("DROP USER TABLE IF EXISTS messages", &test_namespace()).unwrap();
        assert_eq!(stmt.table_name.as_str(), "messages");
        assert_eq!(stmt.table_type, TableType::User);
        assert!(stmt.if_exists);
    }

    #[test]
    fn test_parse_drop_shared_table_if_exists() {
        let stmt = DropTableStatement::parse("DROP SHARED TABLE IF EXISTS conversations", &test_namespace()).unwrap();
        assert_eq!(stmt.table_name.as_str(), "conversations");
        assert_eq!(stmt.table_type, TableType::Shared);
        assert!(stmt.if_exists);
    }

    #[test]
    fn test_parse_drop_table_lowercase() {
        let stmt = DropTableStatement::parse("drop user table mydata", &test_namespace()).unwrap();
        assert_eq!(stmt.table_name.as_str(), "mydata");
        assert_eq!(stmt.table_type, TableType::User);
    }

    #[test]
    fn test_parse_drop_table_missing_name() {
        let result = DropTableStatement::parse("DROP USER TABLE", &test_namespace());
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_statement() {
        let result = DropTableStatement::parse("SELECT * FROM messages", &test_namespace());
        assert!(result.is_err());
    }
}
