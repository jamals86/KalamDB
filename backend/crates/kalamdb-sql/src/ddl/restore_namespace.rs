//! RESTORE DATABASE statement parser
//!
//! Parses SQL statements like:
//! - RESTORE DATABASE app FROM 'path/to/backup'
//! - RESTORE DATABASE IF NOT EXISTS app FROM '/backups/app'

use crate::ddl::DdlResult;

use kalamdb_commons::models::NamespaceId;

/// RESTORE DATABASE statement
#[derive(Debug, Clone, PartialEq)]
pub struct RestoreDatabaseStatement {
    /// Namespace ID to restore
    pub namespace_id: NamespaceId,

    /// Backup source path
    pub backup_path: String,

    /// If true, don't error if namespace already exists
    pub if_not_exists: bool,
}

impl RestoreDatabaseStatement {
    /// Parse a RESTORE DATABASE statement from SQL
    ///
    /// Supports syntax:
    /// - RESTORE DATABASE namespace FROM 'path'
    /// - RESTORE DATABASE IF NOT EXISTS namespace FROM 'path'
    pub fn parse(sql: &str) -> DdlResult<Self> {
        let sql_trimmed = sql.trim();
        let sql_upper = sql_trimmed.to_uppercase();

        if !sql_upper.starts_with("RESTORE DATABASE") {
            return Err("Expected RESTORE DATABASE statement".to_string());
        }

        let if_not_exists = sql_upper.contains("IF NOT EXISTS");

        // Extract namespace name and path
        let remaining = if if_not_exists {
            sql_trimmed
                .strip_prefix("RESTORE DATABASE")
                .and_then(|s| s.trim().strip_prefix("IF NOT EXISTS"))
                .or_else(|| {
                    sql_trimmed
                        .strip_prefix("restore database")
                        .and_then(|s| s.trim().strip_prefix("if not exists"))
                })
                .map(|s| s.trim())
        } else {
            sql_trimmed
                .strip_prefix("RESTORE DATABASE")
                .or_else(|| sql_trimmed.strip_prefix("restore database"))
                .map(|s| s.trim())
        };

        let remaining = remaining.ok_or_else(|| "Invalid RESTORE DATABASE syntax".to_string())?;

        // Split by FROM keyword
        let from_upper = remaining.to_uppercase();
        let from_pos = from_upper
            .find(" FROM ")
            .ok_or_else(|| "Expected FROM clause in RESTORE DATABASE".to_string())?;

        let namespace_name = remaining[..from_pos].trim();
        if namespace_name.is_empty() {
            return Err("Namespace name is required".to_string());
        }

        let path_part = remaining[from_pos + 6..].trim(); // Skip " FROM "

        // Extract path from quotes
        let backup_path = if path_part.starts_with('\'') && path_part.ends_with('\'') {
            path_part[1..path_part.len() - 1].to_string()
        } else if path_part.starts_with('"') && path_part.ends_with('"') {
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
            if_not_exists,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_restore_database() {
        let stmt =
            RestoreDatabaseStatement::parse("RESTORE DATABASE app FROM '/backups/app'").unwrap();
        assert_eq!(stmt.namespace_id.as_str(), "app");
        assert_eq!(stmt.backup_path, "/backups/app");
        assert!(!stmt.if_not_exists);
    }

    #[test]
    fn test_parse_restore_database_if_not_exists() {
        let stmt = RestoreDatabaseStatement::parse(
            "RESTORE DATABASE IF NOT EXISTS app FROM '/backups/app'",
        )
        .unwrap();
        assert_eq!(stmt.namespace_id.as_str(), "app");
        assert_eq!(stmt.backup_path, "/backups/app");
        assert!(stmt.if_not_exists);
    }

    #[test]
    fn test_parse_restore_database_double_quotes() {
        let stmt =
            RestoreDatabaseStatement::parse("RESTORE DATABASE app FROM \"/backups/app\"").unwrap();
        assert_eq!(stmt.backup_path, "/backups/app");
    }

    #[test]
    fn test_parse_restore_database_lowercase() {
        let stmt = RestoreDatabaseStatement::parse("restore database myapp from '/backups/myapp'")
            .unwrap();
        assert_eq!(stmt.namespace_id.as_str(), "myapp");
        assert_eq!(stmt.backup_path, "/backups/myapp");
    }

    #[test]
    fn test_parse_restore_database_missing_namespace() {
        let result = RestoreDatabaseStatement::parse("RESTORE DATABASE FROM '/backups/app'");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_restore_database_missing_from() {
        let result = RestoreDatabaseStatement::parse("RESTORE DATABASE app");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_restore_database_unquoted_path() {
        let result = RestoreDatabaseStatement::parse("RESTORE DATABASE app FROM /backups/app");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_restore_database_empty_path() {
        let result = RestoreDatabaseStatement::parse("RESTORE DATABASE app FROM ''");
        assert!(result.is_err());
    }
}
