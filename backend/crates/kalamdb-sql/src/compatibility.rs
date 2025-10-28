//! SQL dialect compatibility helpers.
//!
//! This module provides utilities for mapping PostgreSQL/MySQL specific
//! data types into Arrow data types that KalamDB understands.  Centralising
//! these conversions keeps the CREATE TABLE parsers in sync across crates.

use arrow::datatypes::{DataType, IntervalUnit, TimeUnit};
use sqlparser::ast::{DataType as SQLDataType, ObjectName};

/// Map a parsed `sqlparser` data type into an Arrow data type while accounting
/// for PostgreSQL/MySQL aliases (e.g. `SERIAL`, `INT4`, `AUTO_INCREMENT`).
pub fn map_sql_type_to_arrow(sql_type: &SQLDataType) -> Result<DataType, String> {
    use SQLDataType::*;

    let dtype = match sql_type {
        // Signed integers ----------------------------------------------------
        SmallInt(_) | Int2(_) => DataType::Int16,
        Int(_) | Integer(_) | Int4(_) => DataType::Int32,
        MediumInt(_) => DataType::Int32,
        BigInt(_) | Int8(_) | Int64 => DataType::Int64,
        TinyInt(_) => DataType::Int8,

        // Unsigned integers --------------------------------------------------
        UnsignedInteger => DataType::UInt32,

        // Floating point -----------------------------------------------------
        Float(_) | Real | Float4 => DataType::Float32,
        SQLDataType::Double(_) | DoublePrecision | Float8 | Float64 => DataType::Float64,

        // Boolean ------------------------------------------------------------
        Boolean | Bool => DataType::Boolean,

        // Character / string -------------------------------------------------
        Character(_)
        | Char(_)
        | CharacterVarying(_)
        | CharVarying(_)
        | Varchar(_)
        | Nvarchar(_)
        | CharacterLargeObject(_)
        | CharLargeObject(_)
        | Clob(_)
        | Text
        | String(_)
        | JSON
        | JSONB => DataType::Utf8,

        // Binary -------------------------------------------------------------
        Binary(_) | Varbinary(_) | Blob(_) | Bytes(_) | Bytea => DataType::Binary,

        // Temporal -----------------------------------------------------------
        Date => DataType::Date32,
        Timestamp(_, _) | Datetime(_) => DataType::Timestamp(TimeUnit::Millisecond, None),
        Time(_, _) => DataType::Time64(TimeUnit::Nanosecond),
        Interval => DataType::Interval(IntervalUnit::MonthDayNano),

        // Custom or dialect specific identifiers ----------------------------
        Custom(name, _) => map_custom_type(name)?,

        // Struct / collection types -----------------------------------------
        Array(_) | Enum(_, _) | Set(_) | Struct(_, _) => DataType::Utf8,

        // Otherwise, leave unsupported so callers can surface a friendly error
        Numeric(_) | Decimal(_) | Dec(_) | BigNumeric(_) | BigDecimal(_) | Uuid | Regclass
        | Unspecified | _ => {
            return Err(format!("Unsupported data type: {:?}", sql_type));
        }
    };

    Ok(dtype)
}

fn map_custom_type(name: &ObjectName) -> Result<DataType, String> {
    let ident = name
        .0
        .iter()
        .map(|id| id.to_string().to_lowercase())
        .collect::<Vec<_>>()
        .join(".");

    let dtype = match ident.as_str() {
        // PostgreSQL serial aliases
        "serial" | "serial4" => DataType::Int32,
        "bigserial" | "serial8" => DataType::Int64,
        "smallserial" | "serial2" => DataType::Int16,
        // Postgres integer aliases
        "int1" => DataType::Int8,
        "int2" => DataType::Int16,
        "int4" => DataType::Int32,
        "int8" => DataType::Int64,
        // MySQL aliases
        "signed" => DataType::Int32,
        "unsigned" => DataType::UInt32,
        // Fallback to treating unknown custom types as UTF8 strings
        other if other.ends_with("text") || other.ends_with("string") => DataType::Utf8,
        other => {
            return Err(format!("Unsupported custom data type '{}'", other));
        }
    };

    Ok(dtype)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::ast::Ident;

    fn custom(name: &str) -> SQLDataType {
        SQLDataType::Custom(ObjectName(vec![Ident::new(name)]), vec![])
    }

    #[test]
    fn maps_postgres_serial_types() {
        assert_eq!(
            map_sql_type_to_arrow(&custom("serial")).unwrap(),
            DataType::Int32
        );
        assert_eq!(
            map_sql_type_to_arrow(&custom("serial8")).unwrap(),
            DataType::Int64
        );
        assert_eq!(
            map_sql_type_to_arrow(&custom("smallserial")).unwrap(),
            DataType::Int16
        );
    }

    #[test]
    fn maps_unsigned_variants() {
        assert_eq!(
            map_sql_type_to_arrow(&SQLDataType::UnsignedInt(None)).unwrap(),
            DataType::UInt32
        );
        assert_eq!(
            map_sql_type_to_arrow(&SQLDataType::UnsignedBigInt(None)).unwrap(),
            DataType::UInt64
        );
    }

    #[test]
    fn rejects_unknown_custom_types() {
        let err = map_sql_type_to_arrow(&custom("geography")).unwrap_err();
        assert!(err.to_string().contains("Unsupported custom data type"));
    }
}

/// Database error message style configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ErrorStyle {
    /// PostgreSQL-style errors (default)
    /// Examples:
    /// - "ERROR: relation \"users\" does not exist"
    /// - "ERROR: column \"age\" does not exist"
    /// - "ERROR: syntax error at or near \"FROM\""
    #[default]
    PostgreSQL,

    /// MySQL-style errors
    /// Examples:
    /// - "ERROR 1146 (42S02): Table 'db.users' doesn't exist"
    /// - "ERROR 1054 (42S22): Unknown column 'age' in 'field list'"
    MySQL,
}

/// Format an error message in PostgreSQL style
///
/// # Examples
///
/// ```
/// use kalamdb_sql::compatibility::format_postgres_error;
///
/// let msg = format_postgres_error("relation \"users\" does not exist");
/// assert_eq!(msg, "ERROR: relation \"users\" does not exist");
/// ```
pub fn format_postgres_error(message: &str) -> String {
    format!("ERROR: {}", message)
}

/// Format a table not found error in PostgreSQL style
///
/// # Examples
///
/// ```
/// use kalamdb_sql::compatibility::format_postgres_table_not_found;
///
/// let msg = format_postgres_table_not_found("users");
/// assert_eq!(msg, "ERROR: relation \"users\" does not exist");
/// ```
pub fn format_postgres_table_not_found(table_name: &str) -> String {
    format!("ERROR: relation \"{}\" does not exist", table_name)
}

/// Format a column not found error in PostgreSQL style
///
/// # Examples
///
/// ```
/// use kalamdb_sql::compatibility::format_postgres_column_not_found;
///
/// let msg = format_postgres_column_not_found("age");
/// assert_eq!(msg, "ERROR: column \"age\" does not exist");
/// ```
pub fn format_postgres_column_not_found(column_name: &str) -> String {
    format!("ERROR: column \"{}\" does not exist", column_name)
}

/// Format a syntax error in PostgreSQL style
///
/// # Examples
///
/// ```
/// use kalamdb_sql::compatibility::format_postgres_syntax_error;
///
/// let msg = format_postgres_syntax_error("FROM");
/// assert_eq!(msg, "ERROR: syntax error at or near \"FROM\"");
/// ```
pub fn format_postgres_syntax_error(token: &str) -> String {
    format!("ERROR: syntax error at or near \"{}\"", token)
}

/// Format an error message in MySQL style
///
/// # Examples
///
/// ```
/// use kalamdb_sql::compatibility::format_mysql_error;
///
/// let msg = format_mysql_error(1146, "42S02", "Table 'db.users' doesn't exist");
/// assert_eq!(msg, "ERROR 1146 (42S02): Table 'db.users' doesn't exist");
/// ```
pub fn format_mysql_error(error_code: u16, sqlstate: &str, message: &str) -> String {
    format!("ERROR {} ({}): {}", error_code, sqlstate, message)
}

/// Format a table not found error in MySQL style
///
/// # Examples
///
/// ```
/// use kalamdb_sql::compatibility::format_mysql_table_not_found;
///
/// let msg = format_mysql_table_not_found("db", "users");
/// assert_eq!(msg, "ERROR 1146 (42S02): Table 'db.users' doesn't exist");
/// ```
pub fn format_mysql_table_not_found(database: &str, table_name: &str) -> String {
    format!(
        "ERROR 1146 (42S02): Table '{}.{}' doesn't exist",
        database, table_name
    )
}

/// Format a column not found error in MySQL style
///
/// # Examples
///
/// ```
/// use kalamdb_sql::compatibility::format_mysql_column_not_found;
///
/// let msg = format_mysql_column_not_found("age");
/// assert_eq!(msg, "ERROR 1054 (42S22): Unknown column 'age' in 'field list'");
/// ```
pub fn format_mysql_column_not_found(column_name: &str) -> String {
    format!(
        "ERROR 1054 (42S22): Unknown column '{}' in 'field list'",
        column_name
    )
}

/// Format a syntax error in MySQL style
///
/// # Examples
///
/// ```
/// use kalamdb_sql::compatibility::format_mysql_syntax_error;
///
/// let msg = format_mysql_syntax_error("FROM", 1);
/// assert_eq!(msg, "ERROR 1064 (42000): You have an error in your SQL syntax; check the manual that corresponds to your MySQL server version for the right syntax to use near 'FROM' at line 1");
/// ```
pub fn format_mysql_syntax_error(token: &str, line: usize) -> String {
    format!(
        "ERROR 1064 (42000): You have an error in your SQL syntax; check the manual that corresponds to your MySQL server version for the right syntax to use near '{}' at line {}",
        token, line
    )
}

#[cfg(test)]
mod error_formatting_tests {
    use super::*;

    #[test]
    fn test_postgres_table_not_found() {
        assert_eq!(
            format_postgres_table_not_found("users"),
            "ERROR: relation \"users\" does not exist"
        );
    }

    #[test]
    fn test_postgres_column_not_found() {
        assert_eq!(
            format_postgres_column_not_found("age"),
            "ERROR: column \"age\" does not exist"
        );
    }

    #[test]
    fn test_postgres_syntax_error() {
        assert_eq!(
            format_postgres_syntax_error("FROM"),
            "ERROR: syntax error at or near \"FROM\""
        );
    }

    #[test]
    fn test_mysql_table_not_found() {
        assert_eq!(
            format_mysql_table_not_found("mydb", "users"),
            "ERROR 1146 (42S02): Table 'mydb.users' doesn't exist"
        );
    }

    #[test]
    fn test_mysql_column_not_found() {
        assert_eq!(
            format_mysql_column_not_found("age"),
            "ERROR 1054 (42S22): Unknown column 'age' in 'field list'"
        );
    }

    #[test]
    fn test_mysql_syntax_error() {
        assert_eq!(
            format_mysql_syntax_error("FROM", 1),
            "ERROR 1064 (42000): You have an error in your SQL syntax; check the manual that corresponds to your MySQL server version for the right syntax to use near 'FROM' at line 1"
        );
    }
}
