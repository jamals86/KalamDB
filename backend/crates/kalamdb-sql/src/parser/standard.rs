//! Standard SQL parser wrapping sqlparser-rs.
//!
//! This module provides parsing for ANSI SQL, PostgreSQL, and MySQL syntax
//! using the battle-tested sqlparser-rs library.
//!
//! ## Features
//!
//! - **Multi-dialect support**: PostgreSQL, MySQL, and Generic SQL dialects
//! - **Type-safe AST**: Leverage sqlparser-rs's strongly-typed statement representations
//! - **Compatibility**: Accept common syntax variants from major databases
//!
//! ## Usage
//!
//! ```rust
//! use kalamdb_sql::parser::standard::parse_sql;
//!
//! let statements = parse_sql("SELECT * FROM users WHERE id = 1")?;
//! ```

use sqlparser::ast::Statement;
use sqlparser::dialect::{Dialect, GenericDialect, MySqlDialect, PostgreSqlDialect};
use sqlparser::parser::Parser;

/// Parse SQL statement(s) using the appropriate dialect.
///
/// Attempts to parse with PostgreSQL dialect first (our preferred syntax),
/// then falls back to MySQL and Generic dialects for compatibility.
///
/// # Arguments
///
/// * `sql` - The SQL string to parse
///
/// # Returns
///
/// A vector of parsed statements (supports multiple statements separated by semicolons)
///
/// # Errors
///
/// Returns parsing errors if the SQL syntax is invalid in all attempted dialects
pub fn parse_sql(sql: &str) -> Result<Vec<Statement>, String> {
    // Try PostgreSQL dialect first (our preferred syntax)
    if let Ok(statements) = try_parse_with_dialect(sql, &PostgreSqlDialect {}) {
        return Ok(statements);
    }

    // Try MySQL dialect for compatibility
    if let Ok(statements) = try_parse_with_dialect(sql, &MySqlDialect {}) {
        return Ok(statements);
    }

    // Fall back to generic dialect
    try_parse_with_dialect(sql, &GenericDialect {})
        .map_err(|e| format!("Failed to parse SQL: {}", e))
}

/// Attempt to parse SQL with a specific dialect.
fn try_parse_with_dialect(sql: &str, dialect: &dyn Dialect) -> Result<Vec<Statement>, sqlparser::parser::ParserError> {
    Parser::parse_sql(dialect, sql)
}

/// Parse a single SQL statement (convenience function).
///
/// # Arguments
///
/// * `sql` - The SQL string to parse (must contain exactly one statement)
///
/// # Returns
///
/// The parsed statement
///
/// # Errors
///
/// Returns an error if:
/// - The SQL contains zero or multiple statements
/// - The SQL syntax is invalid
pub fn parse_single_statement(sql: &str) -> Result<Statement, String> {
    let statements = parse_sql(sql)?;
    
    if statements.len() != 1 {
        return Err(format!(
            "Expected exactly one statement, got {}",
            statements.len()
        ));
    }
    
    Ok(statements.into_iter().next().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_select() {
        let sql = "SELECT * FROM users WHERE id = 1";
        let result = parse_sql(sql);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn test_parse_multiple_statements() {
        let sql = "SELECT * FROM users; SELECT * FROM posts;";
        let result = parse_sql(sql);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }

    #[test]
    fn test_parse_insert() {
        let sql = "INSERT INTO users (name, email) VALUES ('John', 'john@example.com')";
        let result = parse_sql(sql);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_update() {
        let sql = "UPDATE users SET name = 'Jane' WHERE id = 1";
        let result = parse_sql(sql);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_delete() {
        let sql = "DELETE FROM users WHERE id = 1";
        let result = parse_sql(sql);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_create_table() {
        let sql = "CREATE TABLE users (id INT PRIMARY KEY, name TEXT)";
        let result = parse_sql(sql);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_invalid_sql() {
        let sql = "SELECT FROM WHERE";
        let result = parse_sql(sql);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_single_statement_success() {
        let sql = "SELECT * FROM users";
        let result = parse_single_statement(sql);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_single_statement_multiple_error() {
        let sql = "SELECT * FROM users; SELECT * FROM posts;";
        let result = parse_single_statement(sql);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Expected exactly one statement"));
    }
}
