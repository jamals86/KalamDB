//! Unified CREATE TABLE parser for USER, SHARED, and STREAM tables
//!
//! This module provides a single parser that handles all three table types,
//! eliminating code duplication and ensuring consistent parsing logic.

use crate::compatibility::map_sql_type_to_arrow;
use crate::ddl::DdlResult;
use arrow::datatypes::{Field, Schema};
use kalamdb_commons::models::{NamespaceId, StorageId, TableName, TableType};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sqlparser::ast::{ColumnDef, Statement};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;
use std::sync::Arc;

// Compile regexes once at startup
static FLUSH_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)\s+FLUSH(\s+(ROWS|SECONDS|INTERVAL|ROW_THRESHOLD)\s+\d+[a-z]*)+"#).unwrap()
});

static TABLE_TYPE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)\s+TABLE_TYPE\s+['\"]?[a-z0-9_]+['\"]?"#).unwrap()
});

static OWNER_ID_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)\s+OWNER_ID\s+['\"][^'\"]+['\"]"#).unwrap()
});

static STORAGE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)\s+STORAGE\s+['\"]?[a-z0-9_]+['\"]?"#).unwrap()
});

static USE_USER_STORAGE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)\s+USE_USER_STORAGE(\s+['\"][^'\"]+['\"]|\s+TRUE|\s+FALSE)?"#).unwrap()
});

static TTL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)\s+TTL\s+\d+[a-z]*"#).unwrap()
});

static INTERVAL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)FLUSH\s+INTERVAL\s+(\d+)s").unwrap()
});

static ROW_THRESHOLD_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)ROW_THRESHOLD\s+(\d+)").unwrap()
});

static ROWS_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)FLUSH\s+ROWS\s+(\d+)").unwrap()
});

static SECONDS_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)FLUSH\s+SECONDS\s+(\d+)").unwrap()
});

static STORAGE_CLAUSE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)STORAGE\s+(?:'([^']+)'|"([^"]+)"|([a-z0-9_]+))"#).unwrap()
});

static USE_USER_STORAGE_MATCH_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)USE_USER_STORAGE(\s+TRUE)?").unwrap()
});

static TTL_MATCH_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)TTL\s+(\d+)").unwrap()
});

/// Common flush policy for all table types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FlushPolicy {
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

impl FlushPolicy {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            FlushPolicy::RowLimit { row_limit } => {
                if *row_limit == 0 {
                    return Err("Row limit must be greater than 0".to_string());
                }
                if *row_limit >= 1_000_000 {
                    return Err("Row limit must be less than 1,000,000".to_string());
                }
                Ok(())
            }
            FlushPolicy::TimeInterval { interval_seconds } => {
                if *interval_seconds == 0 {
                    return Err("Interval must be greater than 0".to_string());
                }
                if *interval_seconds >= 86_400 {
                    return Err("Interval must be less than 86400 seconds (24 hours)".to_string());
                }
                Ok(())
            }
            FlushPolicy::Combined {
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

impl Default for FlushPolicy {
    fn default() -> Self {
        FlushPolicy::RowLimit { row_limit: 10_000 }
    }
}

/// Unified CREATE TABLE statement that works for USER, SHARED, and STREAM tables
#[derive(Debug, Clone, PartialEq)]
pub struct CreateTableStatement {
    /// Table name (without namespace prefix)
    pub table_name: TableName,
    /// Namespace ID
    pub namespace_id: NamespaceId,
    /// Table type (User, Shared, or Stream)
    pub table_type: TableType,
    /// Arrow schema for the table
    pub schema: Arc<Schema>,
    /// Storage ID - References system.storages (defaults to 'local')
    pub storage_id: Option<StorageId>,
    /// Use user-specific storage - Allow per-user storage override (USER tables only)
    pub use_user_storage: bool,
    /// Flush policy
    pub flush_policy: Option<FlushPolicy>,
    /// Deleted row retention in hours
    pub deleted_retention_hours: Option<u32>,
    /// TTL for stream tables in seconds (STREAM tables only)
    pub ttl_seconds: Option<u64>,
    /// If true, don't error if table already exists
    pub if_not_exists: bool,
}

impl CreateTableStatement {
    /// Parse a CREATE TABLE statement from SQL
    ///
    /// Automatically detects table type from keywords:
    /// - CREATE USER TABLE ...
    /// - CREATE SHARED TABLE ...  
    /// - CREATE STREAM TABLE ...
    /// - CREATE TABLE ... (defaults to SHARED)
    pub fn parse(sql: &str, current_namespace: &NamespaceId) -> DdlResult<Self> {
        // Detect table type from SQL
        let table_type = Self::detect_table_type(sql);

        // Preprocess SQL to remove KalamDB-specific keywords
        let mut normalized_sql = sql.to_string();
        
        // Remove table type keywords
        normalized_sql = normalized_sql
            .replace("USER TABLE", "TABLE")
            .replace("SHARED TABLE", "TABLE")
            .replace("STREAM TABLE", "TABLE")
            .replace(['\n', '\r'], " ");

        // Remove KalamDB-specific clauses before parsing with sqlparser
        // FLUSH clause can have multiple keywords, so match the entire clause
        let flush_re = Regex::new(r#"(?i)\s+FLUSH(\s+(ROWS|SECONDS|INTERVAL|ROW_THRESHOLD)\s+\d+[a-z]*)+"#).unwrap();
        normalized_sql = flush_re.replace_all(&normalized_sql, "").to_string();
        
        // Remove other KalamDB-specific clauses
        let clauses_to_remove = vec![
            r#"(?i)\s+TABLE_TYPE\s+['\"]?[a-z0-9_]+['\"]?"#,
            r#"(?i)\s+OWNER_ID\s+['\"][^'\"]+['\"]"#,
            r#"(?i)\s+STORAGE\s+['\"]?[a-z0-9_]+['\"]?"#,
            r#"(?i)\s+USE_USER_STORAGE(\s+['\"][^'\"]+['\"]|\s+TRUE|\s+FALSE)?"#,
            r#"(?i)\s+TTL\s+\d+[a-z]*"#,
        ];

        for pattern in clauses_to_remove {
            let re = Regex::new(pattern).unwrap();
            normalized_sql = re.replace_all(&normalized_sql, "").to_string();
        }

        // Parse with sqlparser - use PostgreSQL dialect for better TEXT support
        let dialect = PostgreSqlDialect {};
        let statements = Parser::parse_sql(&dialect, &normalized_sql)
            .map_err(|e| format!("sql parser error: {}", e))?;

        if statements.is_empty() {
            return Err("No SQL statement found".to_string());
        }

        let stmt = &statements[0];
        Self::parse_statement(stmt, current_namespace, sql, table_type)
    }

    /// Detect table type from SQL keywords
    fn detect_table_type(sql: &str) -> TableType {
        let sql_upper = sql.to_uppercase();
        
        if sql_upper.contains("CREATE USER TABLE") || sql_upper.contains("TABLE_TYPE 'USER'") || sql_upper.contains("TABLE_TYPE \"USER\"") {
            TableType::User
        } else if sql_upper.contains("CREATE STREAM TABLE") || sql_upper.contains("TABLE_TYPE 'STREAM'") || sql_upper.contains("TABLE_TYPE \"STREAM\"") {
            TableType::Stream
        } else {
            // Default to SHARED if not specified
            TableType::Shared
        }
    }

    /// Parse the CREATE TABLE statement
    fn parse_statement(
        stmt: &Statement,
        current_namespace: &NamespaceId,
        original_sql: &str,
        table_type: TableType,
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

                // Parse common clauses from original SQL
                let flush_policy = Self::parse_flush_policy(original_sql)?;
                let storage_id = Self::parse_storage_clause(original_sql)?;
                let use_user_storage = Self::parse_use_user_storage(original_sql);
                let ttl_seconds = Self::parse_ttl(original_sql)?;
                let deleted_retention_hours = None; // TODO: Parse from SQL if needed

                Ok(CreateTableStatement {
                    table_name: TableName::new(table_name),
                    namespace_id,
                    table_type,
                    schema,
                    storage_id,
                    use_user_storage,
                    flush_policy,
                    deleted_retention_hours,
                    ttl_seconds,
                    if_not_exists: *if_not_exists,
                })
            }
            _ => Err("Expected CREATE TABLE statement".to_string()),
        }
    }

    /// Extract table name from object name
    fn extract_table_name(name: &sqlparser::ast::ObjectName) -> DdlResult<String> {
        let parts = &name.0;
        if parts.is_empty() {
            return Err("Empty table name".to_string());
        }
        Ok(parts.last().unwrap().value.clone())
    }

    /// Extract namespace from object name if qualified
    fn extract_namespace(name: &sqlparser::ast::ObjectName) -> Option<String> {
        let parts = &name.0;
        if parts.len() >= 2 {
            Some(parts[0].value.clone())
        } else {
            None
        }
    }

    /// Parse schema from column definitions
    fn parse_schema(columns: &[ColumnDef]) -> DdlResult<Arc<Schema>> {
        if columns.is_empty() {
            return Err("Table must have at least one column".to_string());
        }

        let mut fields = Vec::new();
        for col in columns {
            let field_name = col.name.value.clone();
            let data_type = map_sql_type_to_arrow(&col.data_type)
                .map_err(|e| e.to_string())?;
            
            let nullable = !col.options.iter().any(|opt| {
                matches!(opt.option, sqlparser::ast::ColumnOption::NotNull)
            });

            fields.push(Field::new(field_name, data_type, nullable));
        }

        Ok(Arc::new(Schema::new(fields)))
    }

    /// Parse FLUSH policy from SQL
    fn parse_flush_policy(sql: &str) -> DdlResult<Option<FlushPolicy>> {
        // Try new syntax: FLUSH INTERVAL <seconds>s ROW_THRESHOLD <count>
        let interval_re = Regex::new(r"(?i)FLUSH\s+INTERVAL\s+(\d+)s").unwrap();
        let row_threshold_re = Regex::new(r"(?i)ROW_THRESHOLD\s+(\d+)").unwrap();

        let interval = interval_re.captures(sql).and_then(|caps| {
            caps[1].parse::<u32>().ok()
        });

        let row_threshold = row_threshold_re.captures(sql).and_then(|caps| {
            caps[1].parse::<u32>().ok()
        });

        match (interval, row_threshold) {
            (Some(secs), Some(rows)) => Ok(Some(FlushPolicy::Combined {
                row_limit: rows,
                interval_seconds: secs,
            })),
            (Some(secs), None) => Ok(Some(FlushPolicy::TimeInterval {
                interval_seconds: secs,
            })),
            (None, Some(rows)) => Ok(Some(FlushPolicy::RowLimit { row_limit: rows })),
            (None, None) => {
                // Try legacy syntax: FLUSH ROWS <count> or FLUSH SECONDS <secs>
                let rows_re = Regex::new(r"(?i)FLUSH\s+ROWS\s+(\d+)").unwrap();
                if let Some(caps) = rows_re.captures(sql) {
                    let count: u32 = caps[1]
                        .parse()
                        .map_err(|_| "Invalid FLUSH ROWS value".to_string())?;
                    return Ok(Some(FlushPolicy::RowLimit { row_limit: count }));
                }

                let seconds_re = Regex::new(r"(?i)FLUSH\s+SECONDS\s+(\d+)").unwrap();
                if let Some(caps) = seconds_re.captures(sql) {
                    let secs: u32 = caps[1]
                        .parse()
                        .map_err(|_| "Invalid FLUSH SECONDS value".to_string())?;
                    return Ok(Some(FlushPolicy::TimeInterval {
                        interval_seconds: secs,
                    }));
                }

                Ok(None)
            }
        }
    }

    /// Parse STORAGE clause from SQL
    fn parse_storage_clause(sql: &str) -> DdlResult<Option<StorageId>> {
        let storage_re =
            Regex::new(r#"(?i)STORAGE\s+(?:'([^']+)'|"([^"]+)"|([a-z0-9_]+))"#).unwrap();

        if let Some(caps) = storage_re.captures(sql) {
            let storage_id_str = caps
                .get(1)
                .or_else(|| caps.get(2))
                .or_else(|| caps.get(3))
                .map(|m| m.as_str().to_string())
                .ok_or_else(|| "Invalid STORAGE clause".to_string())?;
            Ok(Some(StorageId::new(storage_id_str)))
        } else {
            Ok(None)
        }
    }

    /// Parse USE_USER_STORAGE flag from SQL
    fn parse_use_user_storage(sql: &str) -> bool {
        let use_user_storage_re = Regex::new(r"(?i)USE_USER_STORAGE(\s+TRUE)?").unwrap();
        use_user_storage_re.is_match(sql)
    }

    /// Parse TTL from SQL (for STREAM tables)
    fn parse_ttl(sql: &str) -> DdlResult<Option<u64>> {
        let ttl_re = Regex::new(r"(?i)TTL\s+(\d+)").unwrap();
        if let Some(caps) = ttl_re.captures(sql) {
            let ttl: u64 = caps[1]
                .parse()
                .map_err(|_| "Invalid TTL value".to_string())?;
            Ok(Some(ttl))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_namespace() -> NamespaceId {
        NamespaceId::new("test_namespace".to_string())
    }

    #[test]
    fn test_parse_user_table() {
        let sql = "CREATE USER TABLE messages (id BIGINT, content TEXT)";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        assert_eq!(stmt.table_name.as_str(), "messages");
        assert_eq!(stmt.table_type, TableType::User);
        assert_eq!(stmt.schema.fields().len(), 2);
    }

    #[test]
    fn test_parse_shared_table() {
        let sql = "CREATE SHARED TABLE config (key TEXT, value TEXT)";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        assert_eq!(stmt.table_name.as_str(), "config");
        assert_eq!(stmt.table_type, TableType::Shared);
    }

    #[test]
    fn test_parse_stream_table() {
        let sql = "CREATE STREAM TABLE events (id BIGINT, data TEXT) TTL 3600";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        assert_eq!(stmt.table_name.as_str(), "events");
        assert_eq!(stmt.table_type, TableType::Stream);
        assert_eq!(stmt.ttl_seconds, Some(3600));
    }

    #[test]
    fn test_parse_default_to_shared() {
        let sql = "CREATE TABLE config (key TEXT, value TEXT)";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        assert_eq!(stmt.table_type, TableType::Shared);
    }

    #[test]
    fn test_parse_with_storage() {
        let sql = "CREATE USER TABLE messages (id BIGINT) STORAGE s3_prod";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        assert_eq!(stmt.storage_id, Some(StorageId::new("s3_prod")));
    }

    #[test]
    fn test_parse_with_flush_policy() {
        let sql = "CREATE USER TABLE messages (id BIGINT) FLUSH INTERVAL 60s ROW_THRESHOLD 1000";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        match stmt.flush_policy {
            Some(FlushPolicy::Combined {
                row_limit,
                interval_seconds,
            }) => {
                assert_eq!(row_limit, 1000);
                assert_eq!(interval_seconds, 60);
            }
            _ => panic!("Expected Combined flush policy"),
        }
    }

    #[test]
    fn test_parse_if_not_exists() {
        let sql = "CREATE TABLE IF NOT EXISTS config (key TEXT)";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        assert!(stmt.if_not_exists);
    }
}
