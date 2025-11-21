use super::types::CreateTableStatement;
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use kalamdb_commons::models::{NamespaceId, StorageId, TableAccess, TableName};
use kalamdb_commons::schemas::policy::FlushPolicy;
use kalamdb_commons::schemas::{ColumnDefault, TableType};
use once_cell::sync::Lazy;
use regex::Regex;
use sqlparser::ast::{
    ColumnOption, CreateTable, DataType as SqlDataType, Statement, TableConstraint,
};
use std::collections::HashMap;
use std::sync::Arc;

static RE_ALPHANUMERIC: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-zA-Z0-9_]+$").unwrap());
static RE_STORAGE_ID: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap());
static RE_FLUSH_ROWS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\s+FLUSH\s+(?:ROWS|ROW_THRESHOLD)\s+(\d+)").unwrap());
static RE_FLUSH_INTERVAL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\s+FLUSH\s+INTERVAL\s+(\d+)s?").unwrap());
static RE_CREATE_TYPE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^\s*CREATE\s+(USER|SHARED|STREAM)\s+TABLE").unwrap());
static RE_TTL: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\s+TTL\s+(\d+)").unwrap());
static RE_STORAGE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\s+STORAGE\s+'([^']+)'").unwrap());

impl CreateTableStatement {
    /// Pre-process SQL to handle custom syntax:
    /// 1. FLUSH ROWS/INTERVAL
    /// 2. CREATE [USER|SHARED|STREAM] TABLE -> CREATE TABLE
    /// 3. TTL <seconds>
    /// 4. STORAGE '<storage_id>'
    fn preprocess_create_table(
        sql: &str,
    ) -> (
        String,
        Option<u64>,
        Option<u64>,
        Option<TableType>,
        Option<u64>,
        Option<StorageId>,
    ) {
        let mut sql_string = sql.to_string();
        let mut rows = None;
        let mut interval = None;
        let mut table_type = None;
        let mut ttl_seconds = None;
        let mut storage_id = None;

        // 1. Handle FLUSH options
        if let Some(caps) = RE_FLUSH_ROWS.captures(&sql_string) {
            if let Some(m) = caps.get(1) {
                if let Ok(val) = m.as_str().parse::<u64>() {
                    rows = Some(val);
                }
            }
            sql_string = RE_FLUSH_ROWS.replace(&sql_string, "").to_string();
        }

        if let Some(caps) = RE_FLUSH_INTERVAL.captures(&sql_string) {
            if let Some(m) = caps.get(1) {
                if let Ok(val) = m.as_str().parse::<u64>() {
                    interval = Some(val);
                }
            }
            sql_string = RE_FLUSH_INTERVAL.replace(&sql_string, "").to_string();
        }

        // Handle TTL
        if let Some(caps) = RE_TTL.captures(&sql_string) {
            if let Some(m) = caps.get(1) {
                if let Ok(val) = m.as_str().parse::<u64>() {
                    ttl_seconds = Some(val);
                }
            }
            sql_string = RE_TTL.replace(&sql_string, "").to_string();
        }

        // Handle STORAGE
        if let Some(caps) = RE_STORAGE.captures(&sql_string) {
            if let Some(m) = caps.get(1) {
                storage_id = Some(StorageId::from(m.as_str()));
            }
            sql_string = RE_STORAGE.replace(&sql_string, "").to_string();
        }

        // 2. Handle Table Type syntax
        if let Some(caps) = RE_CREATE_TYPE.captures(&sql_string) {
            if let Some(m) = caps.get(1) {
                let type_str = m.as_str().to_uppercase();
                match type_str.as_str() {
                    "USER" => table_type = Some(TableType::User),
                    "SHARED" => table_type = Some(TableType::Shared),
                    "STREAM" => table_type = Some(TableType::Stream),
                    _ => {}
                }
            }
            // Replace with CREATE TABLE
            sql_string = RE_CREATE_TYPE
                .replace(&sql_string, "CREATE TABLE")
                .to_string();
        }

        (
            sql_string,
            rows,
            interval,
            table_type,
            ttl_seconds,
            storage_id,
        )
    }

    /// Parse a SQL statement into a CreateTableStatement
    pub fn parse(sql: &str, default_namespace: &str) -> Result<Self, String> {
        // Pre-process SQL to handle custom syntax
        let (
            cleaned_sql,
            flush_rows_custom,
            flush_interval_custom,
            explicit_type,
            ttl_custom,
            storage_custom,
        ) = Self::preprocess_create_table(sql);

        let dialect = sqlparser::dialect::GenericDialect {};
        let mut statements = sqlparser::parser::Parser::parse_sql(&dialect, &cleaned_sql)
            .map_err(|e| e.to_string())?;
        if statements.len() != 1 {
            return Err("Expected exactly one statement".to_string());
        }
        let statement = statements.remove(0);

        match statement {
            Statement::CreateTable(CreateTable {
                name,
                columns,
                constraints,
                table_options,
                if_not_exists,
                ..
            }) => {
                // 1. Parse table name and namespace
                let (namespace_id, table_name) = if name.0.len() == 1 {
                    (
                        NamespaceId::from(default_namespace),
                        TableName::from(name.0[0].to_string().as_str()),
                    )
                } else if name.0.len() == 2 {
                    (
                        NamespaceId::from(name.0[0].to_string().as_str()),
                        TableName::from(name.0[1].to_string().as_str()),
                    )
                } else {
                    return Err("Invalid table name format. Expected 'table_name' or 'namespace.table_name'".to_string());
                };

                // Validate names
                if !RE_ALPHANUMERIC.is_match(namespace_id.as_str()) {
                    return Err(format!(
                        "Invalid namespace name '{}'. Only alphanumeric characters and underscores are allowed.",
                        namespace_id
                    ));
                }
                if !RE_ALPHANUMERIC.is_match(table_name.as_str()) {
                    return Err(format!(
                        "Invalid table name '{}'. Only alphanumeric characters and underscores are allowed.",
                        table_name
                    ));
                }

                // 2. Parse options (TYPE, STORAGE, FLUSH_POLICY, etc.)
                let mut table_type = explicit_type.unwrap_or(TableType::User);
                let mut storage_id = storage_custom;
                let mut use_user_storage = false;
                let mut flush_policy = None;
                let mut deleted_retention_hours = None;
                let mut ttl_seconds = ttl_custom;
                let mut access_level = None;

                // Handle options (was with_options)
                let options_vec = match table_options {
                    sqlparser::ast::CreateTableOptions::With(opts) => opts,
                    sqlparser::ast::CreateTableOptions::Options(opts) => opts,
                    _ => vec![],
                };

                for option in options_vec {
                    if let sqlparser::ast::SqlOption::KeyValue { key, value } = option {
                        let key_str = key.value.to_uppercase();
                        let value_str = value.to_string().replace('\'', ""); // Remove quotes

                        match key_str.as_str() {
                            "TYPE" => {
                                table_type = match value_str.to_uppercase().as_str() {
                                    "USER" => TableType::User,
                                    "SHARED" => TableType::Shared,
                                    "STREAM" => TableType::Stream,
                                    _ => {
                                        return Err(format!(
                                        "Invalid table TYPE '{}'. Supported: USER, SHARED, STREAM",
                                        value_str
                                    ))
                                    }
                                };
                            }
                            "STORAGE_ID" => {
                                if !RE_STORAGE_ID.is_match(&value_str) {
                                    return Err(format!("Invalid STORAGE_ID '{}'. Only alphanumeric, underscore, and hyphen allowed.", value_str));
                                }
                                storage_id = Some(StorageId::from(value_str));
                            }
                            "USE_USER_STORAGE" => {
                                use_user_storage = value_str.to_uppercase() == "TRUE";
                            }
                            "FLUSH_POLICY" => {
                                // Format: "rows:1000" or "interval:60" or "rows:1000,interval:60"
                                let parts: Vec<&str> = value_str.split(',').collect();
                                let mut rows = 0;
                                let mut interval = 0;

                                for part in parts {
                                    let kv: Vec<&str> = part.split(':').collect();
                                    if kv.len() != 2 {
                                        return Err(format!("Invalid FLUSH_POLICY format '{}'. Expected 'key:value'", part));
                                    }
                                    match kv[0].to_uppercase().as_str() {
                                        "ROWS" => {
                                            rows = kv[1]
                                                .parse()
                                                .map_err(|_| "Invalid row limit in FLUSH_POLICY")?;
                                        }
                                        "INTERVAL" => {
                                            interval = kv[1]
                                                .parse()
                                                .map_err(|_| "Invalid interval in FLUSH_POLICY")?;
                                        }
                                        _ => {
                                            return Err(format!(
                                                "Unknown FLUSH_POLICY key '{}'",
                                                kv[0]
                                            ))
                                        }
                                    }
                                }

                                let policy = if rows > 0 && interval > 0 {
                                    FlushPolicy::Combined {
                                        row_limit: rows,
                                        interval_seconds: interval,
                                    }
                                } else if rows > 0 {
                                    FlushPolicy::RowLimit { row_limit: rows }
                                } else if interval > 0 {
                                    FlushPolicy::TimeInterval {
                                        interval_seconds: interval,
                                    }
                                } else {
                                    return Err(
                                        "FLUSH_POLICY must specify 'rows' or 'interval' > 0"
                                            .to_string(),
                                    );
                                };

                                // Validate policy immediately
                                policy.validate()?;
                                flush_policy = Some(policy);
                            }
                            "DELETED_RETENTION_HOURS" => {
                                let hours: u32 = value_str
                                    .parse()
                                    .map_err(|_| "Invalid DELETED_RETENTION_HOURS")?;
                                deleted_retention_hours = Some(hours);
                            }
                            "TTL_SECONDS" => {
                                let seconds: u64 =
                                    value_str.parse().map_err(|_| "Invalid TTL_SECONDS")?;
                                ttl_seconds = Some(seconds);
                            }
                            "ACCESS_LEVEL" => {
                                access_level = match value_str.to_uppercase().as_str() {
                                    "PUBLIC" => Some(TableAccess::Public),
                                    "PRIVATE" => Some(TableAccess::Private),
                                    "RESTRICTED" => Some(TableAccess::Restricted),
                                    _ => return Err(format!("Invalid ACCESS_LEVEL '{}'. Supported: PUBLIC, PRIVATE, RESTRICTED", value_str)),
                                };
                            }
                            _ => return Err(format!("Unknown table option '{}'", key_str)),
                        }
                    }
                }

                // Merge extracted flush options if not already set by WITH clause
                if flush_policy.is_none()
                    && (flush_rows_custom.is_some() || flush_interval_custom.is_some())
                {
                    let rows = flush_rows_custom.unwrap_or(0) as u32;
                    let interval = flush_interval_custom.unwrap_or(0) as u32;

                    let policy = if rows > 0 && interval > 0 {
                        FlushPolicy::Combined {
                            row_limit: rows,
                            interval_seconds: interval,
                        }
                    } else if rows > 0 {
                        FlushPolicy::RowLimit { row_limit: rows }
                    } else if interval > 0 {
                        FlushPolicy::TimeInterval {
                            interval_seconds: interval,
                        }
                    } else {
                        // Should not happen given the check above
                        return Err("Invalid flush options".to_string());
                    };

                    policy.validate()?;
                    flush_policy = Some(policy);
                }

                // 3. Validate options based on table type
                if table_type == TableType::Stream && ttl_seconds.is_none() {
                    return Err("STREAM tables must specify 'TTL_SECONDS'".to_string());
                }
                if table_type != TableType::Stream && ttl_seconds.is_some() {
                    return Err("TTL_SECONDS is only supported for STREAM tables".to_string());
                }
                if table_type != TableType::Shared && access_level.is_some() {
                    return Err("ACCESS_LEVEL is only supported for SHARED tables".to_string());
                }
                if table_type != TableType::User && use_user_storage {
                    return Err("USE_USER_STORAGE is only supported for USER tables".to_string());
                }

                // 4. Parse columns and constraints
                let mut arrow_fields = Vec::new();
                let mut column_defaults = HashMap::new();
                let mut primary_key_column = None;

                // Check table constraints for PRIMARY KEY
                for constraint in constraints {
                    match constraint {
                        TableConstraint::Unique { .. } => {
                            // Assuming if it's unique and we don't have is_primary, we check if it's intended as PK?
                            // Or maybe sqlparser 0.58.0 has `primary` field?
                            // Let's assume for now we only support PK via ColumnOption or if we can detect it here.
                            // If `is_primary` is gone, maybe `TableConstraint::PrimaryKey` exists?
                            // Let's check if TableConstraint has PrimaryKey variant.
                            // If so, we should match that instead.
                            // But for now, I'll just comment out the PK check from table constraints if I can't find the field.
                            // Wait, I should check if `TableConstraint::PrimaryKey` exists.
                            // I'll assume it does if `Unique` doesn't have `is_primary`.
                        }
                        TableConstraint::PrimaryKey { columns, .. } => {
                            if columns.len() != 1 {
                                return Err(
                                    "Composite PRIMARY KEYs are not supported yet".to_string()
                                );
                            }
                            if primary_key_column.is_some() {
                                return Err("Multiple PRIMARY KEY definitions found".to_string());
                            }
                            // Handle OrderByExpr
                            let col_expr = &columns[0].column.expr;
                            if let sqlparser::ast::Expr::Identifier(ident) = col_expr {
                                primary_key_column = Some(ident.value.clone());
                            } else {
                                return Err(
                                    "Complex expressions in PRIMARY KEY not supported".to_string()
                                );
                            }
                        }
                        _ => {}
                    }
                }

                for col in columns {
                    let col_name = col.name.value;
                    if !RE_ALPHANUMERIC.is_match(&col_name) {
                        return Err(format!(
                            "Invalid column name '{}'. Only alphanumeric characters and underscores are allowed.",
                            col_name
                        ));
                    }

                    let (data_type, is_nullable) = convert_sql_type_to_arrow(&col.data_type)?;

                    // Check column options (PRIMARY KEY, DEFAULT, NOT NULL)
                    let mut col_is_nullable = is_nullable; // Default from type mapping

                    for option in col.options {
                        match option.option {
                            ColumnOption::Unique { is_primary, .. } => {
                                if is_primary {
                                    if primary_key_column.is_some() {
                                        return Err(
                                            "Multiple PRIMARY KEY definitions found".to_string()
                                        );
                                    }
                                    primary_key_column = Some(col_name.clone());
                                    col_is_nullable = false; // PKs cannot be null
                                }
                            }
                            ColumnOption::NotNull => {
                                col_is_nullable = false;
                            }
                            ColumnOption::Null => {
                                col_is_nullable = true;
                            }
                            ColumnOption::Default(expr) => {
                                let default_spec = match expr {
                                    // Handle function calls
                                    sqlparser::ast::Expr::Function(func) => {
                                        let name = func.name.to_string().to_uppercase();
                                        match name.as_str() {
                                            "NOW" | "CURRENT_TIMESTAMP" | "SNOWFLAKE_ID"
                                            | "UUID_V7" | "ULID" | "CURRENT_USER" => {
                                                ColumnDefault::function(&name, vec![])
                                            }
                                            _ => {
                                                // Fallback to string literal for unknown functions
                                                ColumnDefault::literal(serde_json::Value::String(
                                                    func.to_string(),
                                                ))
                                            }
                                        }
                                    }
                                    // Handle literals
                                    sqlparser::ast::Expr::Value(val) => match &val.value {
                                        sqlparser::ast::Value::Number(n, _) => {
                                            if let Ok(i) = n.parse::<i64>() {
                                                ColumnDefault::literal(serde_json::Value::Number(
                                                    i.into(),
                                                ))
                                            } else if let Ok(f) = n.parse::<f64>() {
                                                ColumnDefault::literal(serde_json::json!(f))
                                            } else {
                                                ColumnDefault::literal(serde_json::Value::String(
                                                    n.clone(),
                                                ))
                                            }
                                        }
                                        sqlparser::ast::Value::SingleQuotedString(s)
                                        | sqlparser::ast::Value::DoubleQuotedString(s) => {
                                            ColumnDefault::literal(serde_json::Value::String(
                                                s.clone(),
                                            ))
                                        }
                                        sqlparser::ast::Value::Boolean(b) => {
                                            ColumnDefault::literal(serde_json::Value::Bool(*b))
                                        }
                                        sqlparser::ast::Value::Null => {
                                            ColumnDefault::literal(serde_json::Value::Null)
                                        }
                                        _ => ColumnDefault::literal(serde_json::Value::String(
                                            val.to_string(),
                                        )),
                                    },
                                    // Handle identifiers (e.g. CURRENT_TIMESTAMP without parens)
                                    sqlparser::ast::Expr::Identifier(ident) => {
                                        let s = ident.value.to_uppercase();
                                        if s == "CURRENT_TIMESTAMP" {
                                            ColumnDefault::function("NOW", vec![])
                                        } else if s == "NULL" {
                                            ColumnDefault::literal(serde_json::Value::Null)
                                        } else {
                                            ColumnDefault::literal(serde_json::Value::String(
                                                ident.value,
                                            ))
                                        }
                                    }
                                    _ => {
                                        let default_val = expr.to_string();
                                        if default_val.to_uppercase() == "NULL" {
                                            ColumnDefault::literal(serde_json::Value::Null)
                                        } else if default_val.to_uppercase() == "CURRENT_TIMESTAMP"
                                            || default_val.to_uppercase() == "NOW()"
                                        {
                                            ColumnDefault::function("NOW", vec![])
                                        } else {
                                            // Strip quotes if present
                                            let val = default_val.trim_matches('\'').to_string();
                                            ColumnDefault::literal(serde_json::Value::String(val))
                                        }
                                    }
                                };
                                column_defaults.insert(col_name.clone(), default_spec);
                            }
                            ColumnOption::DialectSpecific(tokens) => {
                                // Check for AUTO_INCREMENT
                                let s = tokens
                                    .iter()
                                    .map(|t| t.to_string())
                                    .collect::<Vec<_>>()
                                    .join(" ");
                                println!(
                                    "DEBUG: DialectSpecific tokens for column {}: {}",
                                    col_name, s
                                );
                                if s.to_uppercase().contains("AUTO_INCREMENT") {
                                    println!(
                                        "DEBUG: Detected AUTO_INCREMENT for column {}",
                                        col_name
                                    );
                                    // Set default to SNOWFLAKE_ID()
                                    column_defaults.insert(
                                        col_name.clone(),
                                        ColumnDefault::function("SNOWFLAKE_ID", vec![]),
                                    );
                                }
                            }
                            _ => {} // Ignore other options for now
                        }
                    }

                    arrow_fields.push(Field::new(&col_name, data_type, col_is_nullable));
                }

                if arrow_fields.is_empty() {
                    return Err("Table must have at least one column".to_string());
                }

                // Ensure PK column exists and is not null
                if let Some(ref pk) = primary_key_column {
                    let mut found = false;
                    for field in &mut arrow_fields {
                        if field.name() == pk {
                            found = true;
                            // Force PK to be non-nullable
                            if field.is_nullable() {
                                *field = Field::new(pk, field.data_type().clone(), false);
                            }
                            break;
                        }
                    }
                    if !found {
                        return Err(format!(
                            "PRIMARY KEY column '{}' not found in column list",
                            pk
                        ));
                    }
                }

                Ok(CreateTableStatement {
                    table_name,
                    namespace_id,
                    table_type,
                    schema: Arc::new(Schema::new(arrow_fields)),
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
            _ => Err("Not a CREATE TABLE statement".to_string()),
        }
    }
}

fn convert_sql_type_to_arrow(sql_type: &SqlDataType) -> Result<(DataType, bool), String> {
    match sql_type {
        SqlDataType::Boolean => Ok((DataType::Boolean, true)),
        SqlDataType::TinyInt(_) => Ok((DataType::Int8, true)),
        SqlDataType::SmallInt(_) => Ok((DataType::Int16, true)),
        SqlDataType::Int(_) | SqlDataType::Integer(_) => Ok((DataType::Int32, true)),
        SqlDataType::BigInt(_) => Ok((DataType::Int64, true)),
        SqlDataType::Float(_) => Ok((DataType::Float32, true)),
        SqlDataType::Double(_) | SqlDataType::Real => Ok((DataType::Float64, true)),
        SqlDataType::Text
        | SqlDataType::String(_)
        | SqlDataType::Varchar(_)
        | SqlDataType::Char(_) => Ok((DataType::Utf8, true)),
        SqlDataType::Timestamp(precision, _) => {
            // Default to Microsecond precision if not specified
            // Note: DataFusion often prefers Nanosecond, but we use Microsecond for compatibility
            let unit = match precision {
                Some(p) if *p <= 3 => TimeUnit::Millisecond,
                Some(p) if *p <= 6 => TimeUnit::Microsecond,
                Some(_) => TimeUnit::Nanosecond,
                None => TimeUnit::Microsecond,
            };
            Ok((DataType::Timestamp(unit, None), true))
        }
        SqlDataType::Date => Ok((DataType::Date32, true)),
        SqlDataType::Binary(_) | SqlDataType::Blob(_) | SqlDataType::Bytea => {
            Ok((DataType::Binary, true))
        }
        _ => Err(format!("Unsupported SQL data type: {:?}", sql_type)),
    }
}
