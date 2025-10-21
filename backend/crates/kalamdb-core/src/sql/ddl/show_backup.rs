//! SHOW BACKUP statement parser
//!
//! Parses SQL statements like:
//! - SHOW BACKUP FOR DATABASE app
//! - SHOW BACKUPS FOR DATABASE app

use crate::catalog::NamespaceId;
use crate::error::KalamDbError;

/// SHOW BACKUP statement
#[derive(Debug, Clone, PartialEq)]
pub struct ShowBackupStatement {
    /// Namespace ID to show backups for
    pub namespace_id: NamespaceId,
}

impl ShowBackupStatement {
    /// Parse a SHOW BACKUP statement from SQL
    ///
    /// Supports syntax:
    /// - SHOW BACKUP FOR DATABASE namespace
    /// - SHOW BACKUPS FOR DATABASE namespace
    pub fn parse(sql: &str) -> Result<Self, KalamDbError> {
        let sql_trimmed = sql.trim();
        let sql_upper = sql_trimmed.to_uppercase();

        // Support both BACKUP and BACKUPS
        let has_backup = sql_upper.starts_with("SHOW BACKUP FOR DATABASE")
            || sql_upper.starts_with("SHOW BACKUPS FOR DATABASE");

        if !has_backup {
            return Err(KalamDbError::InvalidSql(
                "Expected SHOW BACKUP FOR DATABASE statement".to_string(),
            ));
        }

        // Extract namespace name
        let namespace_name = if sql_upper.starts_with("SHOW BACKUPS FOR DATABASE") {
            sql_trimmed
                .strip_prefix("SHOW BACKUPS FOR DATABASE")
                .or_else(|| sql_trimmed.strip_prefix("show backups for database"))
                .map(|s| s.trim())
        } else {
            sql_trimmed
                .strip_prefix("SHOW BACKUP FOR DATABASE")
                .or_else(|| sql_trimmed.strip_prefix("show backup for database"))
                .map(|s| s.trim())
        };

        let namespace_name = namespace_name
            .and_then(|s| s.split_whitespace().next())
            .ok_or_else(|| KalamDbError::InvalidSql("Namespace name is required".to_string()))?;

        Ok(Self {
            namespace_id: NamespaceId::new(namespace_name),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_show_backup() {
        let stmt = ShowBackupStatement::parse("SHOW BACKUP FOR DATABASE app").unwrap();
        assert_eq!(stmt.namespace_id.as_str(), "app");
    }

    #[test]
    fn test_parse_show_backups() {
        let stmt = ShowBackupStatement::parse("SHOW BACKUPS FOR DATABASE app").unwrap();
        assert_eq!(stmt.namespace_id.as_str(), "app");
    }

    #[test]
    fn test_parse_show_backup_lowercase() {
        let stmt = ShowBackupStatement::parse("show backup for database myapp").unwrap();
        assert_eq!(stmt.namespace_id.as_str(), "myapp");
    }

    #[test]
    fn test_parse_show_backup_missing_namespace() {
        let result = ShowBackupStatement::parse("SHOW BACKUP FOR DATABASE");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_show_backup_invalid_syntax() {
        let result = ShowBackupStatement::parse("SHOW BACKUP app");
        assert!(result.is_err());
    }
}
