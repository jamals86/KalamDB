//! CREATE USER TABLE SQL parser
//!
//! Parses CREATE USER TABLE statements with schema, LOCATION clause,
//! LOCATION REFERENCE, FLUSH POLICY, and deleted_retention options.

use crate::compatibility::map_sql_type_to_arrow;
use crate::ddl::DdlResult;

use arrow::datatypes::{Field, Schema};
use kalamdb_commons::models::{NamespaceId, TableName};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sqlparser::ast::{ColumnDef, Statement};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;
use std::sync::Arc;

/// Storage location specification for a user table
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StorageLocation {
    /// Direct path template (e.g., "/data/${user_id}/messages")
    Path(String),
    /// Reference to a predefined storage location
    Reference(String),
}

/// Flush policy extracted from CREATE USER TABLE statements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UserTableFlushPolicy {
    /// Flush after N rows inserted
    RowLimit { row_limit: u32 },
    /// Flush every N seconds
    TimeInterval { interval_seconds: u32 },
    /// Flush when either limit is reached
    Combined {
        row_limit: u32,
        interval_seconds: u32,
    },
}

impl UserTableFlushPolicy {
    fn validate(&self) -> Result<(), String> {
        match self {
            UserTableFlushPolicy::RowLimit { row_limit } => {
                if *row_limit == 0 {
                    return Err("Row limit must be greater than 0".to_string());
                }
                if *row_limit >= 1_000_000 {
                    return Err("Row limit must be less than 1,000,000".to_string());
                }
                Ok(())
            }
            UserTableFlushPolicy::TimeInterval { interval_seconds } => {
                if *interval_seconds == 0 {
                    return Err("Interval must be greater than 0".to_string());
                }
                if *interval_seconds >= 86_400 {
                    return Err("Interval must be less than 86400 seconds (24 hours)".to_string());
                }
                Ok(())
            }
            UserTableFlushPolicy::Combined {
                row_limit,
                interval_seconds,
            } => {
                if *row_limit == 0 {
                    return Err("Row limit must be greater than 0".to_string());
                }
                if *row_limit >= 1_000_000 {
                    return Err("Row limit must be less than 1,000,000".to_string());
                }
                if *interval_seconds == 0 {
                    return Err("Interval must be greater than 0".to_string());
                }
                if *interval_seconds >= 86_400 {
                    return Err("Interval must be less than 86400 seconds (24 hours)".to_string());
                }
                Ok(())
            }
        }
    }
}

impl Default for UserTableFlushPolicy {
    fn default() -> Self {
        UserTableFlushPolicy::RowLimit { row_limit: 10_000 }
    }
}

/// CREATE USER TABLE statement
#[derive(Debug, Clone, PartialEq)]
pub struct CreateUserTableStatement {
    /// Table name (without namespace prefix)
    pub table_name: TableName,
    /// Namespace ID
    pub namespace_id: NamespaceId,
    /// Arrow schema for the table
    pub schema: Arc<Schema>,
    /// Storage location (path or reference)
    pub storage_location: Option<StorageLocation>,
    /// Storage ID (T167) - References system.storages
    pub storage_id: Option<String>,
    /// Use user-specific storage (T168) - Allow per-user storage override
    pub use_user_storage: bool,
    /// Flush policy
    pub flush_policy: Option<UserTableFlushPolicy>,
    /// Deleted row retention in hours
    pub deleted_retention_hours: Option<u32>,
    /// If true, don't error if table already exists
    pub if_not_exists: bool,
}

impl CreateUserTableStatement {
    /// Parse a CREATE USER TABLE statement from SQL
    pub fn parse(sql: &str, current_namespace: &NamespaceId) -> DdlResult<Self> {
        // Preprocess SQL to remove "USER" keyword and FLUSH clause for standard SQL parser
        // "CREATE USER TABLE ... FLUSH ROWS 100" -> "CREATE TABLE ..."
        let mut normalized_sql = sql
            .replace("USER TABLE", "TABLE")
            // Remove FLUSH clauses (will be parsed separately from original SQL)
            .replace(['\n', '\r'], " "); // Normalize line breaks first

        // Remove FLUSH clause using regex
        use regex::Regex;
        let flush_re = Regex::new(r"(?i)\s+FLUSH\s+(ROWS|SECONDS|BYTES)\s+\d+").unwrap();
        normalized_sql = flush_re.replace_all(&normalized_sql, "").to_string();

        // Remove KalamDB-specific clauses the generic parser does not understand.
        // These are parsed from the original SQL later in the pipeline.
        for pattern in [
            r#"(?i)\s+TABLE_TYPE\s+['\"]?[a-z0-9_]+['\"]?"#,
            r#"(?i)\s+OWNER_ID\s+['\"][^'\"]+['\"]"#,
            r#"(?i)\s+STORAGE\s+['\"]?[a-z0-9_]+['\"]?"#,
            r#"(?i)\s+USE_USER_STORAGE(\s+['\"][^'\"]+['\"]|\s+TRUE|\s+FALSE)?"#,
        ] {
            let re = Regex::new(pattern).unwrap();
            normalized_sql = re.replace_all(&normalized_sql, "").to_string();
        }

        let dialect = GenericDialect {};
        let statements =
            Parser::parse_sql(&dialect, &normalized_sql).map_err(|e| e.to_string())?;

        if statements.is_empty() {
            return Err("No SQL statement found".to_string());
        }

        let stmt = &statements[0];
        Self::parse_statement(stmt, current_namespace, sql)
    }

    /// Parse the CREATE TABLE statement
    fn parse_statement(
        stmt: &Statement,
        current_namespace: &NamespaceId,
        original_sql: &str,
    ) -> DdlResult<Self> {
        match stmt {
            Statement::CreateTable {
                name,
                columns,
                if_not_exists,
                ..
            } => {
                // Extract table name
                let table_name = Self::extract_table_name(name)?;

                // Extract namespace from qualified name, or use current_namespace
                let namespace_id = if let Some(ns) = Self::extract_namespace(name) {
                    NamespaceId::new(ns)
                } else {
                    current_namespace.clone()
                };

                // Parse schema from columns
                let schema = Self::parse_schema(columns)?;

                // Parse FLUSH policy from original SQL (if present)
                let flush_policy = Self::parse_flush_policy(original_sql)?;

                // Parse STORAGE clause from original SQL (T167)
                let storage_id = Self::parse_storage_clause(original_sql)?;

                // Parse USE_USER_STORAGE flag from original SQL (T168)
                let use_user_storage = Self::parse_use_user_storage(original_sql);

                Ok(CreateUserTableStatement {
                    table_name: TableName::new(table_name),
                    namespace_id,
                    schema,
                    storage_location: None,
                    storage_id,
                    use_user_storage,
                    flush_policy,
                    deleted_retention_hours: None,
                    if_not_exists: *if_not_exists,
                })
            }
            _ => Err("Expected CREATE TABLE statement".to_string()),
        }
    }

    /// Parse FLUSH policy from SQL text (T155, T155a, T155b)
    ///
    /// Supported syntax:
    /// - FLUSH INTERVAL <seconds>s             -> TimeInterval
    /// - FLUSH ROW_THRESHOLD <count>           -> RowLimit
    /// - FLUSH INTERVAL <seconds>s ROW_THRESHOLD <count> -> Combined
    ///
    /// Legacy syntax also supported:
    /// - FLUSH ROWS <count>    -> RowLimit
    /// - FLUSH SECONDS <secs>  -> TimeInterval
    fn parse_flush_policy(sql: &str) -> DdlResult<Option<UserTableFlushPolicy>> {
        // New syntax: FLUSH INTERVAL <seconds>s ROW_THRESHOLD <count>
        let interval_re = Regex::new(r"(?i)FLUSH\s+INTERVAL\s+(\d+)s").unwrap();
        let row_threshold_re = Regex::new(r"(?i)ROW_THRESHOLD\s+(\d+)").unwrap();

        let interval_match = interval_re.captures(sql);
        let row_threshold_match = row_threshold_re.captures(sql);

        match (interval_match, row_threshold_match) {
            (Some(interval_caps), Some(row_caps)) => {
                // Both parameters provided - Combined policy
                let interval_seconds: u32 = interval_caps[1]
                    .parse()
                    .map_err(|_| "Invalid FLUSH INTERVAL value".to_string())?;
                let row_limit: u32 = row_caps[1]
                    .parse()
                    .map_err(|_| "Invalid ROW_THRESHOLD value".to_string())?;

                let policy = UserTableFlushPolicy::Combined {
                    interval_seconds,
                    row_limit,
                };

                // T155b: Validate policy parameters
                policy.validate().map_err(|e| e.to_string())?;

                return Ok(Some(policy));
            }
            (Some(interval_caps), None) => {
                // Only interval provided - TimeInterval policy
                let interval_seconds: u32 = interval_caps[1]
                    .parse()
                    .map_err(|_| "Invalid FLUSH INTERVAL value".to_string())?;

                let policy = UserTableFlushPolicy::TimeInterval { interval_seconds };
                policy.validate().map_err(|e| e.to_string())?;

                return Ok(Some(policy));
            }
            (None, Some(row_caps)) => {
                // Only row threshold provided - RowLimit policy
                let row_limit: u32 = row_caps[1]
                    .parse()
                    .map_err(|_| "Invalid ROW_THRESHOLD value".to_string())?;

                let policy = UserTableFlushPolicy::RowLimit { row_limit };
                policy.validate().map_err(|e| e.to_string())?;

                return Ok(Some(policy));
            }
            (None, None) => {
                // No new syntax found - try legacy syntax
            }
        }

        // Legacy syntax: FLUSH ROWS <number>
        let rows_re = Regex::new(r"(?i)FLUSH\s+ROWS\s+(\d+)").unwrap();
        if let Some(caps) = rows_re.captures(sql) {
            let count: u32 = caps[1]
                .parse()
                .map_err(|_| "Invalid FLUSH ROWS value".to_string())?;
            let policy = UserTableFlushPolicy::RowLimit { row_limit: count };
            policy.validate().map_err(|e| e.to_string())?;
            return Ok(Some(policy));
        }

        // Legacy syntax: FLUSH SECONDS <number>
        let seconds_re = Regex::new(r"(?i)FLUSH\s+SECONDS\s+(\d+)").unwrap();
        if let Some(caps) = seconds_re.captures(sql) {
            let secs: u32 = caps[1]
                .parse()
                .map_err(|_| "Invalid FLUSH SECONDS value".to_string())?;
            let policy = UserTableFlushPolicy::TimeInterval {
                interval_seconds: secs,
            };
            policy.validate().map_err(|e| e.to_string())?;
            return Ok(Some(policy));
        }

        // No flush policy specified - this is OK for user tables
        Ok(None)
    }

    /// Parse STORAGE clause from SQL (T167)
    ///
    /// Syntax: STORAGE <storage_id>
    ///
    /// # Examples
    /// - `STORAGE local`
    /// - `STORAGE s3_prod`
    fn parse_storage_clause(sql: &str) -> DdlResult<Option<String>> {
        // Match: STORAGE <identifier> or STORAGE 'identifier' or STORAGE "identifier"
        let storage_re =
            Regex::new(r#"(?i)STORAGE\s+(?:'([^']+)'|"([^"]+)"|([a-z0-9_]+))"#).unwrap();

        if let Some(caps) = storage_re.captures(sql) {
            // Try each capture group (single quote, double quote, or unquoted)
            let storage_id = caps
                .get(1)
                .or_else(|| caps.get(2))
                .or_else(|| caps.get(3))
                .map(|m| m.as_str().to_string())
                .ok_or_else(|| "Invalid STORAGE clause".to_string())?;
            Ok(Some(storage_id))
        } else {
            // T167a: Default to 'local' when omitted
            Ok(None)
        }
    }

    /// Parse USE_USER_STORAGE flag from SQL (T168)
    ///
    /// Syntax: USE_USER_STORAGE or USE_USER_STORAGE TRUE
    ///
    /// # Examples
    /// - `USE_USER_STORAGE` -> true
    /// - `USE_USER_STORAGE TRUE` -> true
    /// - No clause -> false (default)
    fn parse_use_user_storage(sql: &str) -> bool {
        // Match: USE_USER_STORAGE (optionally followed by TRUE)
        let use_user_storage_re = Regex::new(r"(?i)USE_USER_STORAGE(\s+TRUE)?").unwrap();

        use_user_storage_re.is_match(sql)
    }

    /// Extract table name from object name
    fn extract_table_name(name: &sqlparser::ast::ObjectName) -> DdlResult<String> {
        let parts = &name.0;
        if parts.is_empty() {
            return Err("Empty table name".to_string());
        }

        // Take the last part as table name (handles both "table" and "namespace.table")
        Ok(parts.last().unwrap().value.clone())
    }

    /// Extract namespace from object name if qualified, otherwise return None
    fn extract_namespace(name: &sqlparser::ast::ObjectName) -> Option<String> {
        let parts = &name.0;
        if parts.len() >= 2 {
            // Qualified name like "app.messages" - first part is namespace
            Some(parts[0].value.clone())
        } else {
            None
        }
    }

    /// Parse schema from column definitions
    fn parse_schema(columns: &[ColumnDef]) -> DdlResult<Arc<Schema>> {
        let fields: DdlResult<Vec<Field>> = columns
            .iter()
            .map(|col| {
                let data_type =
                    map_sql_type_to_arrow(&col.data_type).map_err(|e| e.to_string())?;
                Ok(Field::new(
                    col.name.value.clone(),
                    data_type,
                    col.options
                        .iter()
                        .any(|opt| matches!(opt.option, sqlparser::ast::ColumnOption::Null)),
                ))
            })
            .collect();

        Ok(Arc::new(Schema::new(fields?)))
    }

    /// Validate the table name follows naming conventions
    pub fn validate_table_name(&self) -> DdlResult<()> {
        let name = self.table_name.as_str();

        // Must start with lowercase letter
        if !name
            .chars()
            .next()
            .map(|c| c.is_ascii_lowercase())
            .unwrap_or(false)
        {
            return Err("Table name must start with a lowercase letter".to_string());
        }

        // Can only contain lowercase letters, digits, and underscores
        if !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(
                "Table name can only contain lowercase letters, digits, and underscores"
            .to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::datatypes::DataType;
    use sqlparser::ast::DataType as SQLDataType;

    #[test]
    fn test_parse_simple_create_table() {
        let sql = "CREATE TABLE messages (id BIGINT, content TEXT, created_at TIMESTAMP)";
        let namespace = NamespaceId::new("app");

        let result = CreateUserTableStatement::parse(sql, &namespace);
        assert!(result.is_ok());

        let stmt = result.unwrap();
        assert_eq!(stmt.table_name.as_str(), "messages");
        assert_eq!(stmt.namespace_id, namespace);
        assert_eq!(stmt.schema.fields().len(), 3);
    }

    #[test]
    fn test_parse_with_if_not_exists() {
        let sql = "CREATE TABLE IF NOT EXISTS events (id BIGINT, name TEXT)";
        let namespace = NamespaceId::new("app");

        let result = CreateUserTableStatement::parse(sql, &namespace);
        assert!(result.is_ok());

        let stmt = result.unwrap();
        assert!(stmt.if_not_exists);
    }

    #[test]
    fn test_validate_table_name() {
        let namespace = NamespaceId::new("app");

        // Valid names
        let valid = CreateUserTableStatement {
            table_name: TableName::new("messages"),
            namespace_id: namespace.clone(),
            schema: Arc::new(Schema::empty()),
            storage_location: None,
            storage_id: None,
            use_user_storage: false,
            flush_policy: None,
            deleted_retention_hours: None,
            if_not_exists: false,
        };
        assert!(valid.validate_table_name().is_ok());

        // Invalid: starts with uppercase
        let invalid = CreateUserTableStatement {
            table_name: TableName::new("Messages"),
            namespace_id: namespace.clone(),
            schema: Arc::new(Schema::empty()),
            storage_location: None,
            storage_id: None,
            use_user_storage: false,
            flush_policy: None,
            deleted_retention_hours: None,
            if_not_exists: false,
        };
        assert!(invalid.validate_table_name().is_err());

        // Invalid: contains special characters
        let invalid2 = CreateUserTableStatement {
            table_name: TableName::new("my-table"),
            namespace_id: namespace.clone(),
            schema: Arc::new(Schema::empty()),
            storage_location: None,
            storage_id: None,
            use_user_storage: false,
            flush_policy: None,
            deleted_retention_hours: None,
            if_not_exists: false,
        };
        assert!(invalid2.validate_table_name().is_err());
    }

    #[test]
    fn test_convert_sql_types() {
        assert_eq!(
            map_sql_type_to_arrow(&SQLDataType::BigInt(None)).unwrap(),
            DataType::Int64
        );
        assert_eq!(
            map_sql_type_to_arrow(&SQLDataType::Text).unwrap(),
            DataType::Utf8
        );
        assert_eq!(
            map_sql_type_to_arrow(&SQLDataType::Boolean).unwrap(),
            DataType::Boolean
        );
    }

    #[test]
    fn test_parse_schema() {
        let columns = vec![
            ColumnDef {
                name: sqlparser::ast::Ident::new("id"),
                data_type: SQLDataType::BigInt(None),
                collation: None,
                options: vec![],
            },
            ColumnDef {
                name: sqlparser::ast::Ident::new("name"),
                data_type: SQLDataType::Text,
                collation: None,
                options: vec![],
            },
        ];

        let schema = CreateUserTableStatement::parse_schema(&columns).unwrap();
        assert_eq!(schema.fields().len(), 2);
        assert_eq!(schema.field(0).name(), "id");
        assert_eq!(schema.field(0).data_type(), &DataType::Int64);
        assert_eq!(schema.field(1).name(), "name");
        assert_eq!(schema.field(1).data_type(), &DataType::Utf8);
    }

    #[test]
    fn test_postgres_serial_support() {
        let sql =
            "CREATE USER TABLE app.pg_orders (id SERIAL PRIMARY KEY, amount DOUBLE PRECISION)";
        let namespace = NamespaceId::new("app");
        let stmt = CreateUserTableStatement::parse(sql, &namespace).unwrap();

        assert_eq!(stmt.schema.field(0).data_type(), &DataType::Int32);
        assert_eq!(stmt.schema.field(1).data_type(), &DataType::Float64);
    }

    #[test]
    fn test_mysql_auto_increment_support() {
        let sql =
            "CREATE USER TABLE app.accounts (id INT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(128))";
        let namespace = NamespaceId::new("app");
        let stmt = CreateUserTableStatement::parse(sql, &namespace).unwrap();

        assert_eq!(stmt.schema.field(0).data_type(), &DataType::Int32);
        assert_eq!(stmt.schema.field(1).data_type(), &DataType::Utf8);
    }
}
