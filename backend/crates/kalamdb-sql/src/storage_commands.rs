//! Storage management SQL commands
//!
//! This module provides SQL command parsing for storage management:
//! - CREATE STORAGE: Register a new storage backend
//! - ALTER STORAGE: Modify storage configuration
//! - DROP STORAGE: Remove a storage backend
//! - SHOW STORAGES: List all registered storages

use serde::{Deserialize, Serialize};

/// CREATE STORAGE command
///
/// Syntax:
/// ```sql
/// CREATE STORAGE storage_id
///   TYPE 'filesystem' | 's3'
///   NAME 'storage_name'
///   [DESCRIPTION 'description']
///   BASE_DIRECTORY 'path'
///   SHARED_TABLES_TEMPLATE 'template'
///   USER_TABLES_TEMPLATE 'template';
/// ```
///
/// Example:
/// ```sql
/// CREATE STORAGE s3_prod
///   TYPE 's3'
///   NAME 'Production S3 Storage'
///   DESCRIPTION 'S3 bucket for production data'
///   BASE_DIRECTORY 's3://my-bucket/kalamdb/'
///   SHARED_TABLES_TEMPLATE '{namespace}/{tableName}/'
///   USER_TABLES_TEMPLATE '{namespace}/{tableName}/{userId}/';
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateStorageStatement {
    /// Unique storage identifier
    pub storage_id: String,

    /// Storage type: 'filesystem' or 's3'
    pub storage_type: String,

    /// Human-readable storage name
    pub storage_name: String,

    /// Optional description
    pub description: Option<String>,

    /// Base directory or S3 bucket path
    pub base_directory: String,

    /// Template for shared table paths
    pub shared_tables_template: String,

    /// Template for user table paths
    pub user_tables_template: String,
}

impl CreateStorageStatement {
    /// Parse CREATE STORAGE from SQL
    ///
    /// This is a simple keyword-based parser since sqlparser-rs doesn't support custom DDL.
    pub fn parse(sql: &str) -> Result<Self, String> {
        // Normalize SQL: remove extra whitespace and newlines
        let normalized = sql.split_whitespace().collect::<Vec<_>>().join(" ");
        let sql_upper = normalized.to_uppercase();

        if !sql_upper.starts_with("CREATE STORAGE") {
            return Err("SQL must start with CREATE STORAGE".to_string());
        }

        // Extract storage_id (first token after CREATE STORAGE)
        let storage_id = Self::extract_storage_id(&normalized)?;

        // Extract TYPE (unquoted keyword)
        let storage_type = Self::extract_unquoted_keyword(&normalized, "TYPE")?;
        if storage_type != "filesystem" && storage_type != "s3" {
            return Err(format!(
                "Invalid storage type '{}'. Must be 'filesystem' or 's3'",
                storage_type
            ));
        }

        // Extract NAME
        let storage_name = Self::extract_keyword_value(&normalized, "NAME")?;

        // Extract DESCRIPTION (optional)
        let description = Self::extract_keyword_value(&normalized, "DESCRIPTION").ok();

        // Extract BASE_DIRECTORY - support PATH (filesystem), BUCKET (S3), or BASE_DIRECTORY
        let base_directory = if storage_type == "filesystem" {
            // For filesystem, try PATH first, then BASE_DIRECTORY
            Self::extract_keyword_value(&normalized, "PATH")
                .or_else(|_| Self::extract_keyword_value(&normalized, "BASE_DIRECTORY"))?
        } else if storage_type == "s3" {
            // For S3, try BUCKET+REGION first, then BASE_DIRECTORY
            match Self::extract_keyword_value(&normalized, "BUCKET") {
                Ok(bucket) => {
                    // REGION is optional and informational
                    let _region = Self::extract_keyword_value(&normalized, "REGION").ok();
                    // Ensure bucket has s3:// prefix
                    if bucket.starts_with("s3://") {
                        bucket
                    } else {
                        format!("s3://{}", bucket)
                    }
                }
                Err(_) => {
                    // Fall back to BASE_DIRECTORY
                    Self::extract_keyword_value(&normalized, "BASE_DIRECTORY")?
                }
            }
        } else {
            Self::extract_keyword_value(&normalized, "BASE_DIRECTORY")?
        };

        // Extract SHARED_TABLES_TEMPLATE (optional)
        let shared_tables_template =
            Self::extract_keyword_value(&normalized, "SHARED_TABLES_TEMPLATE")
                .unwrap_or_else(|_| "".to_string());

        // Extract USER_TABLES_TEMPLATE (optional)
        let user_tables_template = Self::extract_keyword_value(&normalized, "USER_TABLES_TEMPLATE")
            .unwrap_or_else(|_| "".to_string());

        Ok(CreateStorageStatement {
            storage_id,
            storage_type,
            storage_name,
            description,
            base_directory,
            shared_tables_template,
            user_tables_template,
        })
    }

    fn extract_storage_id(sql: &str) -> Result<String, String> {
        let parts: Vec<&str> = sql.split_whitespace().collect();
        if parts.len() < 3 {
            return Err("Missing storage_id after CREATE STORAGE".to_string());
        }
        Ok(parts[2].to_string())
    }

    fn extract_unquoted_keyword(sql: &str, keyword: &str) -> Result<String, String> {
        let keyword_upper = keyword.to_uppercase();
        let sql_upper = sql.to_uppercase();

        // Find keyword as a whole word
        let mut search_pos = 0;
        let keyword_pos = loop {
            match sql_upper[search_pos..].find(&keyword_upper) {
                Some(pos) => {
                    let absolute_pos = search_pos + pos;
                    let is_word_start = absolute_pos == 0
                        || sql_upper
                            .chars()
                            .nth(absolute_pos - 1)
                            .unwrap()
                            .is_whitespace();
                    let is_word_end = absolute_pos + keyword_upper.len() >= sql_upper.len()
                        || sql_upper
                            .chars()
                            .nth(absolute_pos + keyword_upper.len())
                            .unwrap()
                            .is_whitespace();

                    if is_word_start && is_word_end {
                        break absolute_pos;
                    } else {
                        search_pos = absolute_pos + 1;
                    }
                }
                None => {
                    return Err(format!("{} keyword not found", keyword));
                }
            }
        };

        let after_keyword = &sql[keyword_pos + keyword.len()..];

        // Check if value is quoted or unquoted
        let trimmed = after_keyword.trim();
        if trimmed.starts_with('\'') {
            // Quoted value - extract it
            let quote_end = trimmed[1..]
                .find('\'')
                .ok_or_else(|| format!("Unclosed quote after {}", keyword))?;
            Ok(trimmed[1..=quote_end].to_string())
        } else {
            // Unquoted value - extract next whitespace-separated token
            let value = trimmed
                .split_whitespace()
                .next()
                .ok_or_else(|| format!("Missing value after {}", keyword))?;
            Ok(value.to_string())
        }
    }

    fn extract_keyword_value(sql: &str, keyword: &str) -> Result<String, String> {
        let keyword_upper = keyword.to_uppercase();
        let sql_upper = sql.to_uppercase();

        // Find keyword as a whole word (surrounded by whitespace or start/end of string)
        let mut search_pos = 0;
        let keyword_pos = loop {
            match sql_upper[search_pos..].find(&keyword_upper) {
                Some(pos) => {
                    let absolute_pos = search_pos + pos;
                    // Check if this is a whole word match
                    let is_word_start = absolute_pos == 0
                        || sql_upper
                            .chars()
                            .nth(absolute_pos - 1)
                            .unwrap()
                            .is_whitespace();
                    let is_word_end = absolute_pos + keyword_upper.len() >= sql_upper.len()
                        || sql_upper
                            .chars()
                            .nth(absolute_pos + keyword_upper.len())
                            .unwrap()
                            .is_whitespace();

                    if is_word_start && is_word_end {
                        break absolute_pos;
                    } else {
                        // Not a whole word, continue searching
                        search_pos = absolute_pos + 1;
                    }
                }
                None => {
                    return Err(format!("{} keyword not found", keyword));
                }
            }
        };

        let after_keyword = &sql[keyword_pos + keyword.len()..];

        // Find the quoted value
        let quote_start = after_keyword
            .find('\'')
            .ok_or_else(|| format!("Missing quoted value after {}", keyword))?;

        let after_quote = &after_keyword[quote_start + 1..];
        let quote_end = after_quote
            .find('\'')
            .ok_or_else(|| format!("Unclosed quote after {}", keyword))?;

        Ok(after_quote[..quote_end].to_string())
    }
}

/// ALTER STORAGE command
///
/// Syntax:
/// ```sql
/// ALTER STORAGE storage_id
///   [SET NAME 'new_name']
///   [SET DESCRIPTION 'new_description']
///   [SET SHARED_TABLES_TEMPLATE 'new_template']
///   [SET USER_TABLES_TEMPLATE 'new_template'];
/// ```
///
/// Example:
/// ```sql
/// ALTER STORAGE local
///   SET NAME 'Local Filesystem'
///   SET DESCRIPTION 'Updated description';
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AlterStorageStatement {
    /// Storage identifier to alter
    pub storage_id: String,

    /// New storage name (if updating)
    pub storage_name: Option<String>,

    /// New description (if updating)
    pub description: Option<String>,

    /// New shared tables template (if updating)
    pub shared_tables_template: Option<String>,

    /// New user tables template (if updating)
    pub user_tables_template: Option<String>,
}

impl AlterStorageStatement {
    /// Parse ALTER STORAGE from SQL
    pub fn parse(sql: &str) -> Result<Self, String> {
        // Normalize SQL: remove extra whitespace and newlines
        let normalized = sql.split_whitespace().collect::<Vec<_>>().join(" ");
        let sql_upper = normalized.to_uppercase();

        if !sql_upper.starts_with("ALTER STORAGE") {
            return Err("SQL must start with ALTER STORAGE".to_string());
        }

        // Extract storage_id
        let parts: Vec<&str> = normalized.split_whitespace().collect();
        if parts.len() < 3 {
            return Err("Missing storage_id after ALTER STORAGE".to_string());
        }
        let storage_id = parts[2].to_string();

        // Extract optional SET clauses
        let storage_name = Self::extract_set_value(&normalized, "NAME").ok();
        let description = Self::extract_set_value(&normalized, "DESCRIPTION").ok();
        let shared_tables_template =
            Self::extract_set_value(&normalized, "SHARED_TABLES_TEMPLATE").ok();
        let user_tables_template =
            Self::extract_set_value(&normalized, "USER_TABLES_TEMPLATE").ok();

        Ok(AlterStorageStatement {
            storage_id,
            storage_name,
            description,
            shared_tables_template,
            user_tables_template,
        })
    }

    fn extract_set_value(sql: &str, keyword: &str) -> Result<String, String> {
        let pattern = format!("SET {}", keyword.to_uppercase());
        let sql_upper = sql.to_uppercase();

        let start_pos = sql_upper
            .find(&pattern)
            .ok_or_else(|| format!("SET {} not found", keyword))?;

        let after_keyword = &sql[start_pos + pattern.len()..];

        // Find the quoted value
        let quote_start = after_keyword
            .find('\'')
            .ok_or_else(|| format!("Missing quoted value after SET {}", keyword))?;

        let after_quote = &after_keyword[quote_start + 1..];
        let quote_end = after_quote
            .find('\'')
            .ok_or_else(|| format!("Unclosed quote after SET {}", keyword))?;

        Ok(after_quote[..quote_end].to_string())
    }
}

/// DROP STORAGE command
///
/// Syntax:
/// ```sql
/// DROP STORAGE storage_id;
/// ```
///
/// Example:
/// ```sql
/// DROP STORAGE old_storage;
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DropStorageStatement {
    /// Storage identifier to drop
    pub storage_id: String,
}

impl DropStorageStatement {
    /// Parse DROP STORAGE from SQL
    pub fn parse(sql: &str) -> Result<Self, String> {
        let sql_upper = sql.to_uppercase();

        if !sql_upper.starts_with("DROP STORAGE") {
            return Err("SQL must start with DROP STORAGE".to_string());
        }

        let parts: Vec<&str> = sql.split_whitespace().collect();
        if parts.len() < 3 {
            return Err("Missing storage_id after DROP STORAGE".to_string());
        }

        let storage_id = parts[2].trim_end_matches(';').to_string();

        Ok(DropStorageStatement { storage_id })
    }
}

/// SHOW STORAGES command
///
/// Syntax:
/// ```sql
/// SHOW STORAGES;
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShowStoragesStatement;

impl ShowStoragesStatement {
    /// Parse SHOW STORAGES from SQL
    pub fn parse(sql: &str) -> Result<Self, String> {
        let sql_upper = sql.trim().to_uppercase();

        if sql_upper != "SHOW STORAGES" && sql_upper != "SHOW STORAGES;" {
            return Err("SQL must be 'SHOW STORAGES'".to_string());
        }

        Ok(ShowStoragesStatement)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_storage_filesystem() {
        let sql = r#"
            CREATE STORAGE local
                TYPE 'filesystem'
                NAME 'Local Storage'
                DESCRIPTION 'Local filesystem storage'
                BASE_DIRECTORY '/data/storage'
                SHARED_TABLES_TEMPLATE '{namespace}/{tableName}/'
                USER_TABLES_TEMPLATE '{namespace}/{tableName}/{userId}/'
        "#;

        let stmt = CreateStorageStatement::parse(sql).unwrap();
        assert_eq!(stmt.storage_id, "local");
        assert_eq!(stmt.storage_type, "filesystem");
        assert_eq!(stmt.storage_name, "Local Storage");
        assert_eq!(
            stmt.description,
            Some("Local filesystem storage".to_string())
        );
        assert_eq!(stmt.base_directory, "/data/storage");
        assert_eq!(stmt.shared_tables_template, "{namespace}/{tableName}/");
        assert_eq!(
            stmt.user_tables_template,
            "{namespace}/{tableName}/{userId}/"
        );
    }

    #[test]
    fn test_create_storage_s3() {
        let sql = r#"
            CREATE STORAGE s3_prod
                TYPE 's3'
                NAME 'S3 Production'
                BASE_DIRECTORY 's3://my-bucket/'
                SHARED_TABLES_TEMPLATE '{namespace}/{tableName}/'
                USER_TABLES_TEMPLATE '{namespace}/{tableName}/{userId}/'
        "#;

        let stmt = CreateStorageStatement::parse(sql).unwrap();
        assert_eq!(stmt.storage_id, "s3_prod");
        assert_eq!(stmt.storage_type, "s3");
        assert_eq!(stmt.base_directory, "s3://my-bucket/");
        assert_eq!(stmt.description, None);
    }

    #[test]
    fn test_create_storage_invalid_type() {
        let sql = r#"
            CREATE STORAGE bad
                TYPE 'invalid'
                NAME 'Bad Storage'
                BASE_DIRECTORY '/data'
                SHARED_TABLES_TEMPLATE '{namespace}/'
                USER_TABLES_TEMPLATE '{namespace}/{userId}/'
        "#;

        let result = CreateStorageStatement::parse(sql);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid storage type"));
    }

    #[test]
    fn test_alter_storage_all_fields() {
        let sql = r#"
            ALTER STORAGE local
                SET NAME 'Updated Local'
                SET DESCRIPTION 'Updated description'
                SET SHARED_TABLES_TEMPLATE '{namespace}/shared/{tableName}/'
                SET USER_TABLES_TEMPLATE '{namespace}/users/{userId}/{tableName}/'
        "#;

        let stmt = AlterStorageStatement::parse(sql).unwrap();
        assert_eq!(stmt.storage_id, "local");
        assert_eq!(stmt.storage_name, Some("Updated Local".to_string()));
        assert_eq!(stmt.description, Some("Updated description".to_string()));
        assert_eq!(
            stmt.shared_tables_template,
            Some("{namespace}/shared/{tableName}/".to_string())
        );
        assert_eq!(
            stmt.user_tables_template,
            Some("{namespace}/users/{userId}/{tableName}/".to_string())
        );
    }

    #[test]
    fn test_alter_storage_partial() {
        let sql = "ALTER STORAGE local SET NAME 'New Name'";

        let stmt = AlterStorageStatement::parse(sql).unwrap();
        assert_eq!(stmt.storage_id, "local");
        assert_eq!(stmt.storage_name, Some("New Name".to_string()));
        assert_eq!(stmt.description, None);
    }

    #[test]
    fn test_drop_storage() {
        let sql = "DROP STORAGE old_storage;";

        let stmt = DropStorageStatement::parse(sql).unwrap();
        assert_eq!(stmt.storage_id, "old_storage");
    }

    #[test]
    fn test_show_storages() {
        let sql = "SHOW STORAGES;";
        let stmt = ShowStoragesStatement::parse(sql).unwrap();
        assert!(matches!(stmt, ShowStoragesStatement));
    }

    #[test]
    fn test_show_storages_no_semicolon() {
        let sql = "SHOW STORAGES";
        let stmt = ShowStoragesStatement::parse(sql).unwrap();
        assert!(matches!(stmt, ShowStoragesStatement));
    }
}
