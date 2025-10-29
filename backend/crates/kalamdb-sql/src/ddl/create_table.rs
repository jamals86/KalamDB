//! Unified CREATE TABLE parser for USER, SHARED, and STREAM tables
//!
//! This module provides a single parser that handles all three table types,
//! eliminating code duplication and ensuring consistent parsing logic.

use crate::compatibility::map_sql_type_to_arrow;
use crate::ddl::DdlResult;
use arrow::datatypes::{Field, Schema};
use kalamdb_commons::models::{
    ColumnDefault, NamespaceId, StorageId, TableAccess, TableName, TableType,
};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sqlparser::ast::{ColumnDef, Statement};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;
use std::collections::HashMap;
use std::sync::Arc;

// Compile regexes once at startup
static FLUSH_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)\s+FLUSH(\s+(ROWS|SECONDS|INTERVAL|ROW_THRESHOLD)\s+\d+[a-z]*)+"#).unwrap()
});

static TABLE_TYPE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)\s+TABLE_TYPE\s+['\"]?[a-z0-9_]+['\"]?"#).unwrap());

static OWNER_ID_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)\s+OWNER_ID\s+['\"][^'\"]+['\"]"#).unwrap());

static STORAGE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)\s+STORAGE\s+['\"]?[a-z0-9_]+['\"]?"#).unwrap());

static USE_USER_STORAGE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)\s+USE_USER_STORAGE(\s+['\"][^'\"]+['\"]|\s+TRUE|\s+FALSE)?"#).unwrap()
});

static TTL_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?i)\s+TTL\s+\d+[a-z]*"#).unwrap());

static ACCESS_LEVEL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)\s+ACCESS\s+LEVEL\s+['\"]?(public|private|restricted)['\"]?"#).unwrap()
});

// MySQL compatibility: strip AUTO_INCREMENT from column definitions
static AUTO_INCREMENT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)\s+AUTO_INCREMENT"#).unwrap());

static INTERVAL_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)FLUSH\s+INTERVAL\s+(\d+)s").unwrap());

static ROW_THRESHOLD_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)ROW_THRESHOLD\s+(\d+)").unwrap());

static ROWS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)FLUSH\s+ROWS\s+(\d+)").unwrap());

static SECONDS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)FLUSH\s+SECONDS\s+(\d+)").unwrap());

static STORAGE_CLAUSE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)STORAGE\s+(?:'([^']+)'|"([^"]+)"|([a-z0-9_]+))"#).unwrap());

static USE_USER_STORAGE_MATCH_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)USE_USER_STORAGE(\s+TRUE)?").unwrap());

static TTL_MATCH_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)TTL\s+(\d+)").unwrap());

static ACCESS_LEVEL_MATCH_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)ACCESS\s+LEVEL\s+(?:'([^']+)'|"([^"]+)"|([a-z]+))"#).unwrap());

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
    /// Column default values (column_name -> default_spec)
    pub column_defaults: HashMap<String, ColumnDefault>,
    /// PRIMARY KEY column name (if detected)
    pub primary_key_column: Option<String>,
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
    /// Access level for SHARED tables (public, private, restricted)
    /// Defaults to private. Only applicable to SHARED tables.
    pub access_level: Option<TableAccess>,
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

        // Preprocess SQL to remove KalamDB-specific keywords (optimized)
        let mut normalized_sql = sql
            .replace("USER TABLE", "TABLE")
            .replace("SHARED TABLE", "TABLE")
            .replace("STREAM TABLE", "TABLE")
            .replace(['\n', '\r'], " ");

        // Remove KalamDB-specific clauses using pre-compiled regexes
        normalized_sql = FLUSH_RE.replace_all(&normalized_sql, "").into_owned();
        normalized_sql = TABLE_TYPE_RE.replace_all(&normalized_sql, "").into_owned();
        normalized_sql = OWNER_ID_RE.replace_all(&normalized_sql, "").into_owned();
        normalized_sql = STORAGE_RE.replace_all(&normalized_sql, "").into_owned();
        normalized_sql = USE_USER_STORAGE_RE
            .replace_all(&normalized_sql, "")
            .into_owned();
        normalized_sql = TTL_RE.replace_all(&normalized_sql, "").into_owned();
        normalized_sql = ACCESS_LEVEL_RE
            .replace_all(&normalized_sql, "")
            .into_owned();
        // Strip non-Postgres AUTO_INCREMENT for compatibility
        normalized_sql = AUTO_INCREMENT_RE
            .replace_all(&normalized_sql, "")
            .into_owned();

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

    /// Detect table type from SQL keywords (optimized with minimal allocations)
    fn detect_table_type(sql: &str) -> TableType {
        // Fast path: check for common keywords first without full uppercase conversion
        let sql_upper = sql.to_uppercase();

        if sql_upper.contains("CREATE USER TABLE")
            || sql_upper.contains("TABLE_TYPE 'USER'")
            || sql_upper.contains("TABLE_TYPE \"USER\"")
        {
            TableType::User
        } else if sql_upper.contains("CREATE STREAM TABLE")
            || sql_upper.contains("TABLE_TYPE 'STREAM'")
            || sql_upper.contains("TABLE_TYPE \"STREAM\"")
        {
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
            Statement::CreateTable(create_table) => {
                let name = &create_table.name;
                let columns = &create_table.columns;
                let if_not_exists = create_table.if_not_exists;

                // Extract table name
                let table_name = Self::extract_table_name(name)?;

                // Extract namespace from qualified name, or use current_namespace
                let namespace_id = if let Some(ns) = Self::extract_namespace(name) {
                    NamespaceId::new(ns)
                } else {
                    current_namespace.clone()
                };

                // Parse schema from columns
                let (schema, column_defaults, primary_key_column) =
                    Self::parse_schema_and_defaults(columns)?;

                // Parse common clauses from original SQL
                let flush_policy = Self::parse_flush_policy(original_sql)?;
                let storage_id = Self::parse_storage_clause(original_sql)?;
                let use_user_storage = Self::parse_use_user_storage(original_sql);
                let ttl_seconds = Self::parse_ttl(original_sql)?;
                let deleted_retention_hours = None; // TODO: Parse from SQL if needed
                let access_level = Self::parse_access_level(original_sql, &table_type)?;

                Ok(CreateTableStatement {
                    table_name: TableName::new(table_name),
                    namespace_id,
                    table_type,
                    schema,
                    column_defaults,
                    primary_key_column,
                    storage_id,
                    use_user_storage,
                    flush_policy,
                    deleted_retention_hours,
                    ttl_seconds,
                    if_not_exists,
                    access_level,
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
        Ok(parts.last().unwrap().to_string())
    }

    /// Extract namespace from object name if qualified
    fn extract_namespace(name: &sqlparser::ast::ObjectName) -> Option<String> {
        let parts = &name.0;
        if parts.len() >= 2 {
            Some(parts[0].to_string())
        } else {
            None
        }
    }

    /// Parse schema and column defaults from column definitions
    fn parse_schema_and_defaults(
        columns: &[ColumnDef],
    ) -> DdlResult<(Arc<Schema>, HashMap<String, ColumnDefault>, Option<String>)> {
        if columns.is_empty() {
            return Err("Table must have at least one column".to_string());
        }

        let mut fields = Vec::new();
        let mut column_defaults = HashMap::new();
        let mut primary_key_column: Option<String> = None;

        for col in columns {
            let field_name = col.name.value.clone();
            let data_type = map_sql_type_to_arrow(&col.data_type).map_err(|e| e.to_string())?;

            let nullable = !col
                .options
                .iter()
                .any(|opt| matches!(opt.option, sqlparser::ast::ColumnOption::NotNull));

            // Check if this column is PRIMARY KEY
            let is_primary_key = col.options.iter().any(|opt| {
                matches!(
                    opt.option,
                    sqlparser::ast::ColumnOption::Unique {
                        is_primary: true,
                        ..
                    }
                )
            });

            if is_primary_key {
                if primary_key_column.is_some() {
                    return Err("Multiple PRIMARY KEY columns not supported".to_string());
                }
                primary_key_column = Some(field_name.clone());
            }

            // Parse DEFAULT clause if present
            for option in &col.options {
                if let sqlparser::ast::ColumnOption::Default(expr) = &option.option {
                    let default_value = Self::parse_default_expression(expr)?;
                    column_defaults.insert(field_name.clone(), default_value);
                }
            }

            fields.push(Field::new(field_name, data_type, nullable));
        }

        Ok((
            Arc::new(Schema::new(fields)),
            column_defaults,
            primary_key_column,
        ))
    }

    /// Parse a DEFAULT expression into a ColumnDefault
    fn parse_default_expression(expr: &sqlparser::ast::Expr) -> DdlResult<ColumnDefault> {
        use sqlparser::ast::Expr;

        match expr {
            // Function call: DEFAULT NOW(), DEFAULT SNOWFLAKE_ID(), etc.
            Expr::Function(func) => {
                let func_name = func.name.to_string().to_uppercase();

                // Validate function has no arguments
                let has_args = match &func.args {
                    sqlparser::ast::FunctionArguments::None => false,
                    sqlparser::ast::FunctionArguments::Subquery(_) => true,
                    sqlparser::ast::FunctionArguments::List(list) => !list.args.is_empty(),
                };

                if has_args {
                    return Err(format!(
                        "DEFAULT function {}() should not have arguments",
                        func_name
                    ));
                }

                Ok(ColumnDefault::FunctionCall(func_name))
            }

            // Literal value: DEFAULT 'text', DEFAULT 42, DEFAULT TRUE
            Expr::Value(value_with_span) => {
                let literal_str = match &value_with_span.value {
                    sqlparser::ast::Value::SingleQuotedString(s) => s.clone(),
                    sqlparser::ast::Value::DoubleQuotedString(s) => s.clone(),
                    sqlparser::ast::Value::Number(n, _) => n.clone(),
                    sqlparser::ast::Value::Boolean(b) => b.to_string(),
                    sqlparser::ast::Value::Null => return Ok(ColumnDefault::None),
                    _ => {
                        return Err(format!(
                            "Unsupported DEFAULT literal: {:?}",
                            value_with_span
                        ))
                    }
                };
                Ok(ColumnDefault::Literal(literal_str))
            }

            _ => Err(format!("Unsupported DEFAULT expression: {:?}", expr)),
        }
    }

    /// Parse FLUSH policy from SQL (optimized with pre-compiled regexes)
    fn parse_flush_policy(sql: &str) -> DdlResult<Option<FlushPolicy>> {
        // Try new syntax: FLUSH INTERVAL <seconds>s ROW_THRESHOLD <count>
        let interval = INTERVAL_RE
            .captures(sql)
            .and_then(|caps| caps[1].parse::<u32>().ok());

        let row_threshold = ROW_THRESHOLD_RE
            .captures(sql)
            .and_then(|caps| caps[1].parse::<u32>().ok());

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
                if let Some(caps) = ROWS_RE.captures(sql) {
                    let count: u32 = caps[1]
                        .parse()
                        .map_err(|_| "Invalid FLUSH ROWS value".to_string())?;
                    return Ok(Some(FlushPolicy::RowLimit { row_limit: count }));
                }

                if let Some(caps) = SECONDS_RE.captures(sql) {
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

    /// Parse STORAGE clause from SQL (optimized)
    fn parse_storage_clause(sql: &str) -> DdlResult<Option<StorageId>> {
        if let Some(caps) = STORAGE_CLAUSE_RE.captures(sql) {
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

    /// Parse USE_USER_STORAGE flag from SQL (optimized)
    #[inline]
    fn parse_use_user_storage(sql: &str) -> bool {
        USE_USER_STORAGE_MATCH_RE.is_match(sql)
    }

    /// Parse TTL from SQL (for STREAM tables, optimized)
    fn parse_ttl(sql: &str) -> DdlResult<Option<u64>> {
        if let Some(caps) = TTL_MATCH_RE.captures(sql) {
            let ttl: u64 = caps[1]
                .parse()
                .map_err(|_| "Invalid TTL value".to_string())?;
            Ok(Some(ttl))
        } else {
            Ok(None)
        }
    }

    /// Parse ACCESS LEVEL from SQL (for SHARED tables only)
    fn parse_access_level(sql: &str, table_type: &TableType) -> DdlResult<Option<TableAccess>> {
        // ACCESS LEVEL is only valid for SHARED tables
        if *table_type != TableType::Shared {
            // If ACCESS LEVEL is specified for non-SHARED table, that's an error
            if ACCESS_LEVEL_MATCH_RE.is_match(sql) {
                return Err("ACCESS LEVEL can only be specified for SHARED tables".to_string());
            }
            return Ok(None);
        }

        // Extract access level from SQL
        if let Some(caps) = ACCESS_LEVEL_MATCH_RE.captures(sql) {
            // Try each capture group (single quote, double quote, or unquoted)
            let access_level_str = caps
                .get(1)
                .or_else(|| caps.get(2))
                .or_else(|| caps.get(3))
                .map(|m| m.as_str().to_lowercase())
                .ok_or_else(|| "Invalid ACCESS LEVEL syntax".to_string())?;

            // Convert string to TableAccess enum (validates automatically via From<&str>)
            let access_level: TableAccess = access_level_str.as_str().into();
            Ok(Some(access_level))
        } else {
            // Default to private for SHARED tables
            Ok(Some(TableAccess::Private))
        }
    }

    /// Validate DEFAULT functions (T530, T531, T532)
    ///
    /// Validates:
    /// 1. Function exists (would be registered with DataFusion)
    /// 2. Function return type matches column data type
    /// 3. Special function restrictions:
    ///    - NOW() only on TIMESTAMP columns
    ///    - SNOWFLAKE_ID() only on BIGINT columns
    ///    - UUID_V7() and ULID() only on STRING/TEXT/VARCHAR columns
    ///
    /// Note: This is a static validation. Full DataFusion validation happens at execution time.
    pub fn validate_default_functions(&self) -> DdlResult<()> {
        use arrow::datatypes::DataType;

        // Known DEFAULT functions (this should match what's registered in DataFusion)
        let known_functions = [
            "NOW",
            "SNOWFLAKE_ID",
            "UUID_V7",
            "ULID",
            "CURRENT_USER",
            "CURRENT_TIMESTAMP",
        ];

        for (column_name, default_value) in &self.column_defaults {
            if let ColumnDefault::FunctionCall(func_name) = default_value {
                let func_upper = func_name.to_uppercase();

                // T530: Validate function exists
                if !known_functions.contains(&func_upper.as_str()) {
                    return Err(format!(
                        "Unknown DEFAULT function '{}' for column '{}'. Known functions: {}",
                        func_name,
                        column_name,
                        known_functions.join(", ")
                    ));
                }

                // Find the column's data type
                let field = self
                    .schema
                    .field_with_name(column_name)
                    .map_err(|_| format!("Column '{}' not found in schema", column_name))?;

                let data_type = field.data_type();

                // T531 & T532: Validate function return type matches column type
                match func_upper.as_str() {
                    "NOW" | "CURRENT_TIMESTAMP" => {
                        // NOW() and CURRENT_TIMESTAMP() return TIMESTAMP
                        if !matches!(data_type, DataType::Timestamp(_, _)) {
                            return Err(format!(
                                "DEFAULT {}() can only be used on TIMESTAMP columns, but column '{}' has type {:?}",
                                func_upper, column_name, data_type
                            ));
                        }
                    }
                    "SNOWFLAKE_ID" => {
                        // SNOWFLAKE_ID() returns BIGINT (Int64)
                        if !matches!(data_type, DataType::Int64) {
                            return Err(format!(
                                "DEFAULT SNOWFLAKE_ID() can only be used on BIGINT columns, but column '{}' has type {:?}",
                                column_name, data_type
                            ));
                        }
                    }
                    "UUID_V7" | "ULID" => {
                        // UUID_V7() and ULID() return STRING (Utf8 or LargeUtf8)
                        if !matches!(data_type, DataType::Utf8 | DataType::LargeUtf8) {
                            return Err(format!(
                                "DEFAULT {}() can only be used on STRING/TEXT/VARCHAR columns, but column '{}' has type {:?}",
                                func_upper, column_name, data_type
                            ));
                        }
                    }
                    "CURRENT_USER" => {
                        // CURRENT_USER() returns STRING
                        if !matches!(data_type, DataType::Utf8 | DataType::LargeUtf8) {
                            return Err(format!(
                                "DEFAULT CURRENT_USER() can only be used on STRING/TEXT/VARCHAR columns, but column '{}' has type {:?}",
                                column_name, data_type
                            ));
                        }
                    }
                    _ => {
                        // This should never happen due to the known_functions check above
                        // But we include it for completeness
                    }
                }
            }
        }

        Ok(())
    }

    /// Validate PRIMARY KEY requirements (T550, T551, T552)
    ///
    /// Validates:
    /// 1. PRIMARY KEY exists for USER, SHARED, and STREAM tables
    /// 2. PRIMARY KEY type is BIGINT or STRING (TEXT/VARCHAR)
    ///
    /// SYSTEM tables are excluded from this requirement
    pub fn validate_primary_key(&self) -> DdlResult<()> {
        use arrow::datatypes::DataType;

        // SYSTEM tables don't require PRIMARY KEY
        if self.table_type == TableType::System {
            return Ok(());
        }

        // T550, T552: Check PRIMARY KEY exists
        let pk_column_name = self.primary_key_column.as_ref()
            .ok_or_else(|| format!(
                "PRIMARY KEY is required for {:?} tables. All USER, SHARED, and STREAM tables must declare a PRIMARY KEY column.",
                self.table_type
            ))?;

        // T551: Check PRIMARY KEY type is BIGINT or STRING
        let field = self.schema.field_with_name(pk_column_name).map_err(|_| {
            format!(
                "PRIMARY KEY column '{}' not found in schema",
                pk_column_name
            )
        })?;

        let data_type = field.data_type();

        match data_type {
            DataType::Int64 => Ok(()), // BIGINT
            DataType::Utf8 | DataType::LargeUtf8 => Ok(()), // STRING/TEXT/VARCHAR
            _ => Err(format!(
                "PRIMARY KEY '{}' has invalid type {:?}. Only BIGINT or STRING (TEXT/VARCHAR) types are allowed for PRIMARY KEY columns.",
                pk_column_name,
                data_type
            ))
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

    // T530: Test DEFAULT function validation - unknown function
    #[test]
    fn test_validate_default_unknown_function() {
        let sql =
            "CREATE USER TABLE users (id BIGINT, created_at TIMESTAMP DEFAULT UNKNOWN_FUNC())";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        let result = stmt.validate_default_functions();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown DEFAULT function"));
    }

    // T531 & T532: Test NOW() type validation - must be on TIMESTAMP
    #[test]
    fn test_validate_now_on_timestamp_success() {
        let sql = "CREATE USER TABLE users (id BIGINT, created_at TIMESTAMP DEFAULT NOW())";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        let result = stmt.validate_default_functions();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_now_on_non_timestamp_fails() {
        let sql = "CREATE USER TABLE users (id BIGINT, name TEXT DEFAULT NOW())";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        let result = stmt.validate_default_functions();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("can only be used on TIMESTAMP columns"));
    }

    // T532: Test SNOWFLAKE_ID() type validation - must be on BIGINT
    #[test]
    fn test_validate_snowflake_id_on_bigint_success() {
        let sql = "CREATE USER TABLE users (id BIGINT DEFAULT SNOWFLAKE_ID(), name TEXT)";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        let result = stmt.validate_default_functions();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_snowflake_id_on_non_bigint_fails() {
        let sql = "CREATE USER TABLE users (id TEXT DEFAULT SNOWFLAKE_ID(), name TEXT)";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        let result = stmt.validate_default_functions();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("can only be used on BIGINT columns"));
    }

    // T532: Test UUID_V7() type validation - must be on STRING/TEXT
    #[test]
    fn test_validate_uuid_v7_on_text_success() {
        let sql = "CREATE USER TABLE users (id TEXT DEFAULT UUID_V7(), name TEXT)";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        let result = stmt.validate_default_functions();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_uuid_v7_on_non_text_fails() {
        let sql = "CREATE USER TABLE users (id BIGINT DEFAULT UUID_V7(), name TEXT)";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        let result = stmt.validate_default_functions();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("can only be used on STRING/TEXT/VARCHAR columns"));
    }

    // T532: Test ULID() type validation - must be on STRING/TEXT
    #[test]
    fn test_validate_ulid_on_text_success() {
        let sql = "CREATE USER TABLE users (id TEXT DEFAULT ULID(), name TEXT)";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        let result = stmt.validate_default_functions();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_ulid_on_non_text_fails() {
        let sql = "CREATE USER TABLE users (id BIGINT DEFAULT ULID(), name TEXT)";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        let result = stmt.validate_default_functions();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("can only be used on STRING/TEXT/VARCHAR columns"));
    }

    // Test multiple DEFAULT functions in one table
    #[test]
    fn test_validate_multiple_default_functions() {
        let sql = "CREATE USER TABLE users (
            id BIGINT DEFAULT SNOWFLAKE_ID(),
            uuid TEXT DEFAULT UUID_V7(),
            created_at TIMESTAMP DEFAULT NOW()
        )";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        let result = stmt.validate_default_functions();
        assert!(result.is_ok());
    }

    // T550-T553: PRIMARY KEY validation tests

    // Test PRIMARY KEY detection
    #[test]
    fn test_primary_key_detection() {
        let sql = "CREATE USER TABLE users (id BIGINT PRIMARY KEY, name TEXT)";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        assert_eq!(stmt.primary_key_column, Some("id".to_string()));
    }

    // T550, T552: Test PRIMARY KEY required for USER table
    #[test]
    fn test_primary_key_required_user_table() {
        let sql = "CREATE USER TABLE users (id BIGINT, name TEXT)";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        let result = stmt.validate_primary_key();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("PRIMARY KEY is required"));
    }

    // T550, T552: Test PRIMARY KEY required for SHARED table
    #[test]
    fn test_primary_key_required_shared_table() {
        let sql = "CREATE SHARED TABLE config (key TEXT, value TEXT)";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        let result = stmt.validate_primary_key();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("PRIMARY KEY is required"));
    }

    // T550, T552: Test PRIMARY KEY required for STREAM table
    #[test]
    fn test_primary_key_required_stream_table() {
        let sql = "CREATE STREAM TABLE events (id BIGINT, data TEXT)";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        let result = stmt.validate_primary_key();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("PRIMARY KEY is required"));
    }

    // T551: Test PRIMARY KEY BIGINT allowed
    #[test]
    fn test_primary_key_bigint_allowed() {
        let sql = "CREATE USER TABLE users (id BIGINT PRIMARY KEY, name TEXT)";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        let result = stmt.validate_primary_key();
        assert!(result.is_ok());
    }

    // T551: Test PRIMARY KEY STRING allowed
    #[test]
    fn test_primary_key_string_allowed() {
        let sql = "CREATE USER TABLE users (id TEXT PRIMARY KEY, name TEXT)";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        let result = stmt.validate_primary_key();
        assert!(result.is_ok());
    }

    // T551: Test PRIMARY KEY invalid type rejected
    #[test]
    fn test_primary_key_invalid_type_rejected() {
        let sql = "CREATE USER TABLE users (id BOOLEAN PRIMARY KEY, name TEXT)";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        let result = stmt.validate_primary_key();
        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(err_msg.contains("has invalid type"));
        assert!(err_msg.contains("Only BIGINT or STRING"));
    }

    // Test PRIMARY KEY with DEFAULT SNOWFLAKE_ID()
    #[test]
    fn test_primary_key_with_default_snowflake_id() {
        let sql =
            "CREATE USER TABLE users (id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(), name TEXT)";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        // Both validations should pass
        assert!(stmt.validate_primary_key().is_ok());
        assert!(stmt.validate_default_functions().is_ok());
    }

    // Test PRIMARY KEY with DEFAULT UUID_V7()
    #[test]
    fn test_primary_key_with_default_uuid_v7() {
        let sql =
            "CREATE USER TABLE events (event_id TEXT PRIMARY KEY DEFAULT UUID_V7(), data TEXT)";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        // Both validations should pass
        assert!(stmt.validate_primary_key().is_ok());
        assert!(stmt.validate_default_functions().is_ok());
    }

    // Test PRIMARY KEY with DEFAULT ULID()
    #[test]
    fn test_primary_key_with_default_ulid() {
        let sql =
            "CREATE USER TABLE requests (request_id TEXT PRIMARY KEY DEFAULT ULID(), data TEXT)";
        let stmt = CreateTableStatement::parse(sql, &test_namespace()).unwrap();

        // Both validations should pass
        assert!(stmt.validate_primary_key().is_ok());
        assert!(stmt.validate_default_functions().is_ok());
    }
}
