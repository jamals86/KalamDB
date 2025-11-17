//! SUBSCRIBE TO command parser for live query subscriptions.
//!
//! **Purpose**: Enable SQL-based syntax for creating live query subscriptions via WebSocket.
//!
//! **Syntax**:
//! ```sql
//! SUBSCRIBE TO [namespace.]table_name [WHERE condition] [OPTIONS (last_rows=N)];
//! ```
//!
//! **Examples**:
//! ```sql
//! -- Basic subscription
//! SUBSCRIBE TO app.messages;
//!
//! -- With WHERE clause filter
//! SUBSCRIBE TO app.messages WHERE user_id = CURRENT_USER();
//!
//! -- With initial data fetch (last 10 rows)
//! SUBSCRIBE TO app.messages WHERE user_id = CURRENT_USER() OPTIONS (last_rows=10);
//!
//! -- Shared table subscription
//! SUBSCRIBE TO shared.announcements WHERE priority > 5;
//! ```
//!
//! **Integration**:
//! When executed via /api/sql endpoint, this command returns metadata instructing
//! the client to establish a WebSocket connection with the appropriate subscription message.
//!
//! **Response Format**:
//! ```json
//! {
//!   "status": "subscription_required",
//!   "ws_url": "ws://localhost:8080/ws",
//!   "subscription": {
//!     "id": "auto-generated-id",
//!     "sql": "SELECT * FROM app.messages WHERE user_id = CURRENT_USER()",
//!     "options": {"last_rows": 10}
//!   }
//! }
//! ```

//! SUBSCRIBE TO command parser for live query subscriptions.
//!
//! **Purpose**: Enable SQL-based syntax for creating live query subscriptions via WebSocket.
//!
//! **Syntax**:
//! ```sql
//! SUBSCRIBE TO [namespace.]table_name [WHERE condition] [OPTIONS (last_rows=N)];
//! ```
//!
//! **Examples**:
//! ```sql
//! -- Basic subscription
//! SUBSCRIBE TO app.messages;
//!
//! -- With WHERE clause filter
//! SUBSCRIBE TO app.messages WHERE user_id = CURRENT_USER();
//!
//! -- With initial data fetch (last 10 rows)
//! SUBSCRIBE TO app.messages WHERE user_id = CURRENT_USER() OPTIONS (last_rows=10);
//!
//! -- Shared table subscription
//! SUBSCRIBE TO shared.announcements WHERE priority > 5;
//! ```
//!
//! **Integration**:
//! When executed via /api/sql endpoint, this command returns metadata instructing
//! the client to establish a WebSocket connection with the appropriate subscription message.
//!
//! **Response Format**:
//! ```json
//! {
//!   "status": "subscription_required",
//!   "ws_url": "ws://localhost:8080/ws",
//!   "subscription": {
//!     "id": "auto-generated-id",
//!     "sql": "SELECT * FROM app.messages WHERE user_id = CURRENT_USER()",
//!     "options": {"last_rows": 10}
//!   }
//! }
//! ```

use super::DdlResult;
use kalamdb_commons::{NamespaceId, TableName};
use sqlparser::ast::{ObjectName, ObjectNamePart, SetExpr, Statement, TableFactor};
use sqlparser::dialect::{GenericDialect, PostgreSqlDialect};
use sqlparser::parser::Parser;

/// SUBSCRIBE TO statement for live query subscriptions.
///
/// This command initiates a live query subscription via WebSocket.
#[derive(Debug, Clone, PartialEq)]
pub struct SubscribeStatement {
    /// Full SELECT query (e.g., "SELECT event_type FROM app.messages WHERE user_id = 'alice'")
    pub select_query: String,
    /// Namespace name (e.g., "app") - extracted from query
    pub namespace: NamespaceId,
    /// Table name (e.g., "messages") - extracted from query
    pub table_name: TableName,
    /// Optional subscription options (e.g., last_rows=10)
    pub options: SubscribeOptions,
}

/// Options for SUBSCRIBE TO command.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SubscribeOptions {
    /// Number of recent rows to fetch initially (default: 0, no initial fetch)
    pub last_rows: Option<usize>,
}

impl SubscribeStatement {
    /// Parse SUBSCRIBE TO command from SQL string using sqlparser-rs.
    ///
    /// # Supported Formats
    ///
    /// 1. `SUBSCRIBE TO namespace.table [WHERE ...] [OPTIONS (...)]`
    ///    - Expands to: `SELECT * FROM namespace.table [WHERE ...]`
    ///
    /// 2. `SUBSCRIBE TO SELECT columns FROM namespace.table [WHERE ...] [OPTIONS (...)]`
    ///    - Uses custom SELECT query as-is
    ///
    /// # Examples
    ///
    /// ```
    /// use kalamdb_sql::ddl::subscribe_commands::SubscribeStatement;
    ///
    /// // Basic subscription (SELECT * FROM)
    /// let stmt = SubscribeStatement::parse("SUBSCRIBE TO app.messages").unwrap();
    /// assert_eq!(stmt.select_query, "SELECT * FROM app.messages");
    ///
    /// // Custom column selection
    /// let stmt = SubscribeStatement::parse("SUBSCRIBE TO SELECT event_type FROM app.messages").unwrap();
    /// assert_eq!(stmt.select_query, "SELECT event_type FROM app.messages");
    /// ```
    pub fn parse(sql: &str) -> DdlResult<Self> {
        let sql = sql.trim().trim_end_matches(';').trim();

        // Check for SUBSCRIBE TO prefix
        if !sql.to_uppercase().starts_with("SUBSCRIBE TO ") {
            return Err("Expected 'SUBSCRIBE TO' command".to_string());
        }

        // Extract OPTIONS clause first
        let (sql_without_options, options) = Self::extract_options_clause(sql)?;

        // Check if user provided custom SELECT query
        let select_sql = if sql_without_options
            .to_uppercase()
            .contains("SUBSCRIBE TO SELECT")
        {
            // Format: SUBSCRIBE TO SELECT columns FROM table [WHERE ...]
            // Just strip "SUBSCRIBE TO " prefix (case-insensitive)
            let upper = sql_without_options.to_uppercase();
            if let Some(pos) = upper.find("SUBSCRIBE TO ") {
                sql_without_options[pos + 13..].trim().to_string() // "SUBSCRIBE TO ".len() == 13
            } else {
                return Err("Invalid SUBSCRIBE TO SELECT syntax".to_string());
            }
        } else {
            // Format: SUBSCRIBE TO table [WHERE ...]
            // Convert to SELECT * FROM table [WHERE ...]
            Self::replace_subscribe_with_select(&sql_without_options)
        };

        // Normalize certain function call syntaxes that sqlparser sees as keywords
        // e.g., PostgreSQL treats CURRENT_USER as a special keyword (no parentheses)
        let current_user_re = regex::Regex::new(r"(?i)CURRENT_USER\s*\(\s*\)").unwrap();
        let select_sql = current_user_re
            .replace_all(&select_sql, "CURRENT_USER")
            .into_owned();

        // Parse the SELECT statement using sqlparser
        let dialect = PostgreSqlDialect {};
        let mut ast = match Parser::parse_sql(&dialect, &select_sql) {
            Ok(ast) => ast,
            Err(e) => return Err(format!("Failed to parse SUBSCRIBE TO as SELECT: {}", e)),
        };

        if ast.len() != 1 {
            return Err("Expected exactly one SUBSCRIBE TO statement".to_string());
        }

        let statement = ast.remove(0);
        let Statement::Query(query) = statement else {
            return Err("SUBSCRIBE TO must parse as SELECT query".to_string());
        };

        // Extract table name from FROM clause
        let SetExpr::Select(select_box) = *query.body else {
            return Err("SUBSCRIBE TO requires simple SELECT structure".to_string());
        };

        if select_box.from.len() != 1 {
            return Err("SUBSCRIBE TO requires exactly one table".to_string());
        }

        let table_factor = &select_box.from[0].relation;
        let TableFactor::Table { name, .. } = table_factor else {
            return Err("SUBSCRIBE TO requires direct table reference".to_string());
        };

        // Extract namespace.table from ObjectName
        let (namespace, table_name) = Self::extract_namespace_table(name)?;

        Ok(SubscribeStatement {
            select_query: select_sql,
            namespace: NamespaceId::from(namespace),
            table_name: TableName::from(table_name),
            options,
        })
    }

    /// Extract OPTIONS clause from SUBSCRIBE TO SQL, return modified SQL and parsed options.
    ///
    /// Uses sqlparser tokenizer to find OPTIONS keyword, avoiding false matches in strings.
    fn extract_options_clause(sql: &str) -> DdlResult<(String, SubscribeOptions)> {
        use sqlparser::tokenizer::{Token, Tokenizer};

        let dialect = GenericDialect {};
        let mut tokenizer = Tokenizer::new(&dialect, sql);

        // Tokenize to get list of tokens
        let tokens = tokenizer
            .tokenize()
            .map_err(|e| format!("Failed to tokenize SUBSCRIBE TO: {}", e))?;

        // Find OPTIONS keyword and calculate its byte position in original SQL
        let mut byte_pos = 0;
        let mut options_byte_pos = None;

        for token in &tokens {
            let token_str = token.to_string();

            // Check if this token is OPTIONS keyword
            if let Token::Word(word) = token {
                if word.value.to_uppercase() == "OPTIONS" {
                    options_byte_pos = Some(byte_pos);
                    break;
                }
            }

            // Advance byte position (account for token length + spaces)
            // This is approximate but works for our purpose since we'll search for the keyword
            if let Some(pos) = sql[byte_pos..].find(&token_str) {
                byte_pos += pos + token_str.len();
            } else {
                byte_pos += token_str.len() + 1; // +1 for space
            }
        }

        // If no OPTIONS found, return SQL as-is (will be processed later)
        let Some(_) = options_byte_pos else {
            return Ok((sql.to_string(), SubscribeOptions::default()));
        };

        // Find actual OPTIONS keyword position in SQL (case-insensitive)
        let sql_upper = sql.to_uppercase();
        let options_idx = sql_upper
            .rfind(" OPTIONS ")
            .or_else(|| sql_upper.rfind(" OPTIONS("))
            .ok_or_else(|| "OPTIONS keyword not found in SQL".to_string())?;

        // Split SQL at OPTIONS
        let before_options = sql[..options_idx].trim();
        let after_options_start = options_idx + " OPTIONS".len(); // " OPTIONS".len() == 8
        let after_options = sql[after_options_start..].trim();

        // Parse OPTIONS (don't modify SQL here, will be processed later)
        let options = parse_subscribe_options(after_options)?;

        Ok((before_options.to_string(), options))
    }

    /// Replace SUBSCRIBE TO with SELECT * FROM (simple string replacement for no-OPTIONS case)
    fn replace_subscribe_with_select(sql: &str) -> String {
        // Case-insensitive replacement
        let upper = sql.to_uppercase();
        if let Some(pos) = upper.find("SUBSCRIBE TO") {
            let mut result = String::new();
            result.push_str(&sql[..pos]);
            result.push_str("SELECT * FROM");
            result.push_str(&sql[pos + 12..]); // "SUBSCRIBE TO".len() == 12
            result
        } else {
            sql.to_string()
        }
    }

    /// Extract namespace and table name from ObjectName.
    fn extract_namespace_table(name: &ObjectName) -> DdlResult<(String, String)> {
        let parts: Vec<String> = name
            .0
            .iter()
            .filter_map(|part| match part {
                ObjectNamePart::Identifier(ident) => Some(ident.value.clone()),
                _ => None,
            })
            .collect();

        if parts.len() == 2 {
            Ok((parts[0].clone(), parts[1].clone()))
        } else if parts.len() == 1 {
            Err("Expected namespace.table format (e.g., app.messages)".to_string())
        } else {
            Err(format!(
                "Invalid table reference: expected namespace.table, got {}",
                name
            ))
        }
    }

    /// Get the SELECT query for execution.
    ///
    /// This returns the full SELECT statement that should be executed for the subscription.
    ///
    /// # Examples
    ///
    /// ```
    /// use kalamdb_sql::ddl::subscribe_commands::SubscribeStatement;
    ///
    /// let stmt = SubscribeStatement::parse("SUBSCRIBE TO app.messages WHERE user_id = 'alice'").unwrap();
    /// assert_eq!(stmt.to_select_sql(), "SELECT * FROM app.messages WHERE user_id = 'alice'");
    ///
    /// let stmt = SubscribeStatement::parse("SUBSCRIBE TO SELECT event_type FROM app.messages").unwrap();
    /// assert_eq!(stmt.to_select_sql(), "SELECT event_type FROM app.messages");
    /// ```
    pub fn to_select_sql(&self) -> String {
        self.select_query.clone()
    }
}

/// Parse OPTIONS clause for SUBSCRIBE TO command.
///
/// Expected format: `(last_rows=N)`
fn parse_subscribe_options(options_str: &str) -> DdlResult<SubscribeOptions> {
    let options_str = options_str.trim();

    // Expect options wrapped in parentheses
    if !options_str.starts_with('(') || !options_str.ends_with(')') {
        return Err(
            "OPTIONS clause must be wrapped in parentheses, e.g., OPTIONS (last_rows=10)"
                .to_string(),
        );
    }

    let inner = &options_str[1..options_str.len() - 1].trim();

    // Parse key=value pairs
    let mut last_rows = None;

    for part in inner.split(',') {
        let part = part.trim();
        if let Some((key, value)) = part.split_once('=') {
            let key = key.trim().to_lowercase();
            let value = value.trim();

            match key.as_str() {
                "last_rows" => {
                    last_rows = Some(
                        value
                            .parse::<usize>()
                            .map_err(|_| format!("Invalid last_rows value: {}", value))?,
                    );
                }
                _ => {
                    return Err(format!("Unknown subscription option: {}", key));
                }
            }
        } else {
            return Err(format!("Invalid option format: {}", part));
        }
    }

    Ok(SubscribeOptions { last_rows })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_subscribe() {
        let stmt = SubscribeStatement::parse("SUBSCRIBE TO app.messages").unwrap();
        assert_eq!(stmt.namespace, NamespaceId::from("app"));
        assert_eq!(stmt.table_name, TableName::from("messages"));
        assert_eq!(stmt.select_query, "SELECT * FROM app.messages");
        assert!(stmt.options.last_rows.is_none());
    }

    #[test]
    fn test_parse_subscribe_with_semicolon() {
        let stmt = SubscribeStatement::parse("SUBSCRIBE TO app.messages;").unwrap();
        assert_eq!(stmt.namespace, NamespaceId::from("app"));
        assert_eq!(stmt.table_name, TableName::from("messages"));
    }

    #[test]
    fn test_parse_subscribe_case_insensitive() {
        let stmt = SubscribeStatement::parse("subscribe to app.messages").unwrap();
        assert_eq!(stmt.namespace, NamespaceId::from("app"));
        assert_eq!(stmt.table_name, TableName::from("messages"));
    }

    #[test]
    fn test_parse_subscribe_with_where_clause() {
        let stmt =
            SubscribeStatement::parse("SUBSCRIBE TO app.messages WHERE user_id = 'alice'").unwrap();
        assert_eq!(stmt.namespace, NamespaceId::from("app"));
        assert_eq!(stmt.table_name, TableName::from("messages"));
        assert_eq!(
            stmt.select_query,
            "SELECT * FROM app.messages WHERE user_id = 'alice'"
        );
    }

    #[test]
    fn test_parse_subscribe_with_options() {
        let stmt =
            SubscribeStatement::parse("SUBSCRIBE TO app.messages OPTIONS (last_rows=10)").unwrap();
        assert_eq!(stmt.namespace, NamespaceId::from("app"));
        assert_eq!(stmt.table_name, TableName::from("messages"));
        assert_eq!(stmt.select_query, "SELECT * FROM app.messages");
        assert_eq!(stmt.options.last_rows, Some(10));
    }

    #[test]
    fn test_parse_subscribe_with_where_and_options() {
        let stmt = SubscribeStatement::parse(
            "SUBSCRIBE TO app.messages WHERE user_id = 'alice' OPTIONS (last_rows=20)",
        )
        .unwrap();
        assert_eq!(stmt.namespace, NamespaceId::from("app"));
        assert_eq!(stmt.table_name, TableName::from("messages"));
        assert_eq!(
            stmt.select_query,
            "SELECT * FROM app.messages WHERE user_id = 'alice'"
        );
        assert_eq!(stmt.options.last_rows, Some(20));
    }

    #[test]
    fn test_parse_subscribe_missing_namespace() {
        let result = SubscribeStatement::parse("SUBSCRIBE TO messages");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Expected namespace.table format"));
    }

    #[test]
    fn test_parse_subscribe_custom_columns() {
        let stmt =
            SubscribeStatement::parse("SUBSCRIBE TO SELECT event_type FROM app.messages").unwrap();
        assert_eq!(stmt.namespace, NamespaceId::from("app"));
        assert_eq!(stmt.table_name, TableName::from("messages"));
        assert_eq!(stmt.select_query, "SELECT event_type FROM app.messages");
        assert!(stmt.options.last_rows.is_none());
    }

    #[test]
    fn test_parse_subscribe_custom_columns_with_where() {
        let stmt = SubscribeStatement::parse(
            "SUBSCRIBE TO SELECT event_type, user_id FROM app.messages WHERE conversation_id = 1",
        )
        .unwrap();
        assert_eq!(stmt.namespace, NamespaceId::from("app"));
        assert_eq!(stmt.table_name, TableName::from("messages"));
        assert_eq!(
            stmt.select_query,
            "SELECT event_type, user_id FROM app.messages WHERE conversation_id = 1"
        );
    }

    #[test]
    fn test_parse_subscribe_custom_columns_with_options() {
        let stmt = SubscribeStatement::parse(
            "SUBSCRIBE TO SELECT event_type FROM chat.typing_events OPTIONS (last_rows=20)",
        )
        .unwrap();
        assert_eq!(stmt.namespace, NamespaceId::from("chat"));
        assert_eq!(stmt.table_name, TableName::from("typing_events"));
        assert_eq!(
            stmt.select_query,
            "SELECT event_type FROM chat.typing_events"
        );
        assert_eq!(stmt.options.last_rows, Some(20));
    }

    #[test]
    fn test_parse_subscribe_invalid_syntax() {
        let result = SubscribeStatement::parse("SUBSCRIBE messages");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Expected 'SUBSCRIBE TO'"));
    }

    #[test]
    fn test_parse_subscribe_invalid_options() {
        let result = SubscribeStatement::parse("SUBSCRIBE TO app.messages OPTIONS last_rows=10");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("parentheses"));
    }

    #[test]
    fn test_parse_subscribe_invalid_option_value() {
        let result = SubscribeStatement::parse("SUBSCRIBE TO app.messages OPTIONS (last_rows=abc)");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid last_rows value"));
    }

    #[test]
    fn test_to_select_sql_basic() {
        let stmt = SubscribeStatement::parse("SUBSCRIBE TO app.messages").unwrap();
        assert_eq!(stmt.to_select_sql(), "SELECT * FROM app.messages");
    }

    #[test]
    fn test_to_select_sql_with_where() {
        let stmt =
            SubscribeStatement::parse("SUBSCRIBE TO app.messages WHERE user_id = 'alice'").unwrap();
        assert_eq!(
            stmt.to_select_sql(),
            "SELECT * FROM app.messages WHERE user_id = 'alice'"
        );
    }

    #[test]
    fn test_to_select_sql_custom_columns() {
        let stmt =
            SubscribeStatement::parse("SUBSCRIBE TO SELECT event_type FROM app.messages").unwrap();
        assert_eq!(stmt.to_select_sql(), "SELECT event_type FROM app.messages");
    }

    #[test]
    fn test_parse_subscribe_malformed_table_reference() {
        // This now works because we support custom SELECT queries!
        // "SUBSCRIBE TO select * from admin_ops_test.users" is interpreted as:
        // "SUBSCRIBE TO SELECT * FROM admin_ops_test.users"
        let result = SubscribeStatement::parse("SUBSCRIBE TO select * from admin_ops_test.users");
        assert!(result.is_ok());
        let stmt = result.unwrap();
        assert_eq!(stmt.select_query, "select * from admin_ops_test.users");
    }

    #[test]
    fn test_parse_subscribe_table_reference_with_spaces() {
        // Test that table references with spaces are rejected (sqlparser will handle this)
        let result = SubscribeStatement::parse("SUBSCRIBE TO my table");
        assert!(result.is_err());
        // sqlparser will parse "my" as table name and fail on unexpected "table" keyword
        // or require namespace.table format
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_subscribe_invalid_option_format() {
        let result = SubscribeStatement::parse("SUBSCRIBE TO app.messages OPTIONS invalid");
        assert!(result.is_err());
    }
}
