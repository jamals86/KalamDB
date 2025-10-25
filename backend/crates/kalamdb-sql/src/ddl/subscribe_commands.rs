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

use super::parsing::parse_table_reference;
use super::DdlResult;
use kalamdb_commons::{NamespaceId, TableName};

/// SUBSCRIBE TO statement for live query subscriptions.
///
/// This command initiates a live query subscription via WebSocket.
#[derive(Debug, Clone, PartialEq)]
pub struct SubscribeStatement {
    /// Namespace name (e.g., "app")
    pub namespace: NamespaceId,
    /// Table name (e.g., "messages")
    pub table_name: TableName,
    /// Optional WHERE clause filter (e.g., "user_id = CURRENT_USER()")
    pub where_clause: Option<String>,
    /// Optional subscription options (e.g., last_rows=10)
    pub options: SubscribeOptions,
}

/// Options for SUBSCRIBE TO command.
#[derive(Debug, Clone, PartialEq)]
pub struct SubscribeOptions {
    /// Number of recent rows to fetch initially (default: 0, no initial fetch)
    pub last_rows: Option<usize>,
}

impl Default for SubscribeOptions {
    fn default() -> Self {
        Self { last_rows: None }
    }
}

impl SubscribeStatement {
    /// Parse SUBSCRIBE TO command from SQL string.
    ///
    /// # Arguments
    ///
    /// * `sql` - SQL string containing SUBSCRIBE TO command
    ///
    /// # Returns
    ///
    /// * `Ok(SubscribeStatement)` - Parsed statement
    /// * `Err(String)` - Parse error with descriptive message
    ///
    /// # Examples
    ///
    /// ```
    /// use kalamdb_sql::ddl::subscribe_commands::SubscribeStatement;
    ///
    /// let stmt = SubscribeStatement::parse("SUBSCRIBE TO app.messages").unwrap();
    /// assert_eq!(stmt.namespace, "app");
    /// assert_eq!(stmt.table_name, "messages");
    /// ```
    pub fn parse(sql: &str) -> DdlResult<Self> {
        let sql = sql.trim().trim_end_matches(';').trim();

        // Check for SUBSCRIBE TO prefix
        if !sql.to_uppercase().starts_with("SUBSCRIBE TO ") {
            return Err("Expected 'SUBSCRIBE TO' command".to_string());
        }

        // Remove "SUBSCRIBE TO " prefix
        let remainder = sql[13..].trim(); // "SUBSCRIBE TO " is 13 characters

        // Split on WHERE and OPTIONS keywords
        let where_pos = remainder
            .to_uppercase()
            .find(" WHERE ")
            .unwrap_or(remainder.len());
        let options_pos = remainder
            .to_uppercase()
            .find(" OPTIONS ")
            .unwrap_or(remainder.len());

        // Extract table name (up to WHERE or OPTIONS)
        let table_part = &remainder[..where_pos.min(options_pos)].trim();
        let (namespace_opt, table_name) = parse_table_reference(table_part)?;

        // Require namespace
        let namespace = namespace_opt
            .ok_or_else(|| "Expected namespace.table format (e.g., app.messages)".to_string())?;

        // Extract WHERE clause if present
        let where_clause = if where_pos < remainder.len() {
            let where_start = where_pos + 7; // " WHERE " is 7 characters
            let where_end = if options_pos < remainder.len() {
                options_pos
            } else {
                remainder.len()
            };
            Some(remainder[where_start..where_end].trim().to_string())
        } else {
            None
        };

        // Extract OPTIONS clause if present
        let options = if options_pos < remainder.len() {
            let options_str = &remainder[options_pos + 9..]; // " OPTIONS " is 9 characters
            parse_subscribe_options(options_str)?
        } else {
            SubscribeOptions::default()
        };

        Ok(SubscribeStatement {
            namespace: NamespaceId::from(namespace),
            table_name: TableName::from(table_name),
            where_clause,
            options,
        })
    }

    /// Convert to SQL SELECT statement for DataFusion query execution.
    ///
    /// This method translates the SUBSCRIBE TO command into a standard SELECT
    /// statement that can be used for initial data fetching and live query filtering.
    ///
    /// # Examples
    ///
    /// ```
    /// use kalamdb_sql::ddl::subscribe_commands::SubscribeStatement;
    ///
    /// let stmt = SubscribeStatement::parse("SUBSCRIBE TO app.messages WHERE user_id = 'alice'").unwrap();
    /// let select_sql = stmt.to_select_sql();
    /// assert_eq!(select_sql, "SELECT * FROM app.messages WHERE user_id = 'alice'");
    /// ```
    pub fn to_select_sql(&self) -> String {
        let mut sql = format!("SELECT * FROM {}.{}", self.namespace, self.table_name);

        if let Some(ref where_clause) = self.where_clause {
            sql.push_str(&format!(" WHERE {}", where_clause));
        }

        sql
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
        assert!(stmt.where_clause.is_none());
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
            SubscribeStatement::parse("SUBSCRIBE TO app.messages WHERE user_id = CURRENT_USER()")
                .unwrap();
        assert_eq!(stmt.namespace, NamespaceId::from("app"));
        assert_eq!(stmt.table_name, TableName::from("messages"));
        assert_eq!(stmt.where_clause.unwrap(), "user_id = CURRENT_USER()");
    }

    #[test]
    fn test_parse_subscribe_with_options() {
        let stmt =
            SubscribeStatement::parse("SUBSCRIBE TO app.messages OPTIONS (last_rows=10)").unwrap();
        assert_eq!(stmt.namespace, NamespaceId::from("app"));
        assert_eq!(stmt.table_name, TableName::from("messages"));
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
        assert_eq!(stmt.where_clause.unwrap(), "user_id = 'alice'");
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
    fn test_to_select_sql_ignores_options() {
        let stmt =
            SubscribeStatement::parse("SUBSCRIBE TO app.messages OPTIONS (last_rows=10)").unwrap();
        assert_eq!(stmt.to_select_sql(), "SELECT * FROM app.messages");
    }
}
