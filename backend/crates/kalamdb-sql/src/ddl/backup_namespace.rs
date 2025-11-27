//! BACKUP DATABASE statement parser
//!
//! Parses SQL statements like:
//! - BACKUP DATABASE app TO 'path/to/backup'
//! - BACKUP DATABASE IF EXISTS app TO '/backups/app'

use crate::ddl::DdlResult;

use kalamdb_commons::models::NamespaceId;

/// BACKUP DATABASE statement
#[derive(Debug, Clone, PartialEq)]
pub struct BackupDatabaseStatement {
    /// Namespace ID to backup
    pub namespace_id: NamespaceId,

    /// Backup destination path
    pub backup_path: String,

    /// If true, don't error if namespace doesn't exist
    pub if_exists: bool,
}

impl BackupDatabaseStatement {
    /// Parse a BACKUP DATABASE statement from SQL
    ///
    /// Supports syntax:
    /// - BACKUP DATABASE namespace TO 'path'
    /// - BACKUP DATABASE IF EXISTS namespace TO 'path'
    pub fn parse(sql: &str) -> DdlResult<Self> {
        let sql_trimmed = sql.trim();
        let sql_upper = sql_trimmed.to_uppercase();

        if !sql_upper.starts_with("BACKUP DATABASE") {
            return Err("Expected BACKUP DATABASE statement".to_string());
        }

        let if_exists = sql_upper.contains("IF EXISTS");

        // Extract namespace name and path
        let remaining = if if_exists {
            sql_trimmed
                .strip_prefix("BACKUP DATABASE")
                .and_then(|s| s.trim().strip_prefix("IF EXISTS"))
                .or_else(|| {
                    sql_trimmed
                        .strip_prefix("backup database")
                        .and_then(|s| s.trim().strip_prefix("if exists"))
                })
                .map(|s| s.trim())
        } else {
            sql_trimmed
                .strip_prefix("BACKUP DATABASE")
                .or_else(|| sql_trimmed.strip_prefix("backup database"))
                .map(|s| s.trim())
        };

        let remaining = remaining.ok_or_else(|| "Invalid BACKUP DATABASE syntax".to_string())?;

        // Split by TO keyword
        let to_upper = remaining.to_uppercase();
        let to_pos = to_upper
            .find(" TO ")
            .ok_or_else(|| "Expected TO clause in BACKUP DATABASE".to_string())?;

        let namespace_name = remaining[..to_pos].trim();
        if namespace_name.is_empty() {
            return Err("Namespace name is required".to_string());
        }

        let path_part = remaining[to_pos + 4..].trim(); // Skip " TO "

        // Extract path from quotes (single or double)
        let backup_path = if (path_part.starts_with('\'') && path_part.ends_with('\''))
            || (path_part.starts_with('"') && path_part.ends_with('"'))
        {
            path_part[1..path_part.len() - 1].to_string()
        } else {
            return Err("Backup path must be quoted".to_string());
        };

        if backup_path.is_empty() {
            return Err("Backup path cannot be empty".to_string());
        }

        Ok(Self {
            namespace_id: NamespaceId::new(namespace_name),
            backup_path,
            if_exists,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_backup_database() {
        let stmt = BackupDatabaseStatement::parse("BACKUP DATABASE app TO '/backups/app'").unwrap();
        assert_eq!(stmt.namespace_id.as_str(), "app");
        assert_eq!(stmt.backup_path, "/backups/app");
        assert!(!stmt.if_exists);
    }

    #[test]
    fn test_parse_backup_database_if_exists() {
        let stmt =
            BackupDatabaseStatement::parse("BACKUP DATABASE IF EXISTS app TO '/backups/app'")
                .unwrap();
        assert_eq!(stmt.namespace_id.as_str(), "app");
        assert_eq!(stmt.backup_path, "/backups/app");
        assert!(stmt.if_exists);
    }

    #[test]
    fn test_parse_backup_database_double_quotes() {
        let stmt =
            BackupDatabaseStatement::parse("BACKUP DATABASE app TO \"/backups/app\"").unwrap();
        assert_eq!(stmt.backup_path, "/backups/app");
    }

    #[test]
    fn test_parse_backup_database_lowercase() {
        let stmt =
            BackupDatabaseStatement::parse("backup database myapp to '/backups/myapp'").unwrap();
        assert_eq!(stmt.namespace_id.as_str(), "myapp");
        assert_eq!(stmt.backup_path, "/backups/myapp");
    }

    #[test]
    fn test_parse_backup_database_missing_namespace() {
        let result = BackupDatabaseStatement::parse("BACKUP DATABASE TO '/backups/app'");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_backup_database_missing_to() {
        let result = BackupDatabaseStatement::parse("BACKUP DATABASE app");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_backup_database_unquoted_path() {
        let result = BackupDatabaseStatement::parse("BACKUP DATABASE app TO /backups/app");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_backup_database_empty_path() {
        let result = BackupDatabaseStatement::parse("BACKUP DATABASE app TO ''");
        assert!(result.is_err());
    }
}
