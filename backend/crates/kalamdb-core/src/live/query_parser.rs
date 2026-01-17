//! SQL query parsing utilities for live queries
//!
//! Uses sqlparser-rs (via DataFusion) for safe SQL parsing to prevent
//! SQL injection attacks and ensure proper handling of edge cases.

use crate::error::KalamDbError;
use kalamdb_commons::models::UserId;
use sqlparser::ast::{
    Expr, Query, Select, SelectItem, SetExpr, Statement, TableFactor, TableWithJoins,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

/// Utilities for parsing SQL queries for live subscriptions
pub struct QueryParser;

impl QueryParser {
    /// Extract table name from SQL query using sqlparser-rs
    ///
    /// This uses proper SQL parsing to safely extract the table name,
    /// preventing SQL injection attacks and handling edge cases like:
    /// - Quoted identifiers
    /// - Schema-qualified names (schema.table)
    /// - Complex FROM clauses
    ///
    /// # Security
    /// Uses sqlparser-rs instead of string manipulation to prevent
    /// SQL injection attacks in live query subscriptions.
    pub fn extract_table_name(query: &str) -> Result<String, KalamDbError> {
        let dialect = GenericDialect {};
        let statements = Parser::parse_sql(&dialect, query)
            .map_err(|e| KalamDbError::InvalidSql(format!("Failed to parse SQL: {}", e)))?;

        if statements.is_empty() {
            return Err(KalamDbError::InvalidSql("Empty SQL statement".to_string()));
        }

        let statement = &statements[0];
        match statement {
            Statement::Query(query) => Self::extract_table_from_query(query),
            _ => Err(KalamDbError::InvalidSql(
                "Only SELECT queries are supported for subscriptions".to_string(),
            )),
        }
    }

    /// Extract table name from a parsed Query AST
    fn extract_table_from_query(query: &Query) -> Result<String, KalamDbError> {
        match query.body.as_ref() {
            SetExpr::Select(select) => Self::extract_table_from_select(select),
            _ => Err(KalamDbError::InvalidSql(
                "Only simple SELECT queries are supported for subscriptions".to_string(),
            )),
        }
    }

    /// Extract table name from SELECT statement
    fn extract_table_from_select(select: &Select) -> Result<String, KalamDbError> {
        if select.from.is_empty() {
            return Err(KalamDbError::InvalidSql("Query must contain FROM clause".to_string()));
        }

        // Get the first table in FROM clause
        let table_with_joins = &select.from[0];
        Self::extract_table_from_table_with_joins(table_with_joins)
    }

    /// Extract table name from TableWithJoins
    fn extract_table_from_table_with_joins(twj: &TableWithJoins) -> Result<String, KalamDbError> {
        match &twj.relation {
            TableFactor::Table { name, .. } => {
                // name is an ObjectName which is Vec<ObjectNamePart>
                // Join with dots for schema-qualified names
                use sqlparser::ast::ObjectNamePart;
                let table_name = name
                    .0
                    .iter()
                    .filter_map(|part| match part {
                        ObjectNamePart::Identifier(ident) => Some(ident.value.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(".");
                Ok(table_name)
            },
            _ => Err(KalamDbError::InvalidSql(
                "Unsupported table reference type for subscriptions".to_string(),
            )),
        }
    }

    /// Extract WHERE clause from SQL query
    ///
    /// Returns None if no WHERE clause exists.
    /// Uses sqlparser-rs for proper SQL parsing.
    pub fn extract_where_clause(query: &str) -> Option<String> {
        let dialect = GenericDialect {};
        let statements = Parser::parse_sql(&dialect, query).ok()?;

        if statements.is_empty() {
            return None;
        }

        match &statements[0] {
            Statement::Query(query) => Self::extract_where_from_query(query),
            _ => None,
        }
    }

    /// Extract WHERE clause from parsed Query
    fn extract_where_from_query(query: &Query) -> Option<String> {
        match query.body.as_ref() {
            SetExpr::Select(select) => select.selection.as_ref().map(|expr| expr.to_string()),
            _ => None,
        }
    }

    /// Resolve placeholders in WHERE clause (e.g., CURRENT_USER())
    ///
    /// # Security
    /// Single quotes in the user ID are escaped to prevent SQL injection.
    pub fn resolve_where_clause_placeholders(clause: &str, user_id: &UserId) -> String {
        // SECURITY: Escape single quotes to prevent SQL injection
        // Example: O'Brien -> O''Brien (SQL standard escaping)
        let safe_user_id = user_id.as_str().replace('\'', "''");
        let replacement = format!("'{}'", safe_user_id);
        clause
            .replace("CURRENT_USER()", &replacement)
            .replace("current_user()", &replacement)
    }

    /// Extract column projections from SQL query
    ///
    /// Returns None if SELECT * (all columns), otherwise returns the list of column names.
    /// Uses sqlparser-rs for proper SQL parsing.
    ///
    /// Handles:
    /// - SELECT * FROM ... -> None (all columns)
    /// - SELECT col1, col2 FROM ... -> Some(["col1", "col2"])
    /// - SELECT col1 as alias FROM ... -> Some(["col1"]) (uses original name, not alias)
    pub fn extract_projections(query: &str) -> Option<Vec<String>> {
        let dialect = GenericDialect {};
        let statements = Parser::parse_sql(&dialect, query).ok()?;

        if statements.is_empty() {
            return None;
        }

        match &statements[0] {
            Statement::Query(query) => Self::extract_projections_from_query(query),
            _ => None,
        }
    }

    /// Extract projections from parsed Query
    fn extract_projections_from_query(query: &Query) -> Option<Vec<String>> {
        match query.body.as_ref() {
            SetExpr::Select(select) => Self::extract_projections_from_select(select),
            _ => None,
        }
    }

    /// Extract column names from SELECT clause
    fn extract_projections_from_select(select: &Select) -> Option<Vec<String>> {
        let mut columns = Vec::new();

        for item in &select.projection {
            match item {
                SelectItem::Wildcard(_) => return None, // SELECT *
                SelectItem::UnnamedExpr(expr) => {
                    if let Some(col_name) = Self::extract_column_name_from_expr(expr) {
                        columns.push(col_name);
                    }
                },
                SelectItem::ExprWithAlias { expr, .. } => {
                    // Use original column name, not alias
                    if let Some(col_name) = Self::extract_column_name_from_expr(expr) {
                        columns.push(col_name);
                    }
                },
                SelectItem::QualifiedWildcard(_, _) => return None, // table.*
            }
        }

        if columns.is_empty() {
            None
        } else {
            Some(columns)
        }
    }

    /// Extract column name from expression (handles table.column)
    fn extract_column_name_from_expr(expr: &Expr) -> Option<String> {
        match expr {
            Expr::Identifier(ident) => Some(ident.value.clone()),
            Expr::CompoundIdentifier(idents) => {
                // table.column -> take the column name (last part)
                idents.last().map(|ident| ident.value.clone())
            },
            _ => None, // Complex expressions not supported for projections
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_table_name() {
        let table_name =
            QueryParser::extract_table_name("SELECT * FROM user1.messages WHERE id > 0").unwrap();
        assert_eq!(table_name, "user1.messages");

        let table_name = QueryParser::extract_table_name("select id from test.users").unwrap();
        assert_eq!(table_name, "test.users");

        let table_name =
            QueryParser::extract_table_name("SELECT * FROM \"ns.my_table\" WHERE x = 1").unwrap();
        assert_eq!(table_name, "ns.my_table");
    }

    #[test]
    fn test_extract_table_name_simple() {
        let table_name = QueryParser::extract_table_name("SELECT * FROM users").unwrap();
        assert_eq!(table_name, "users");
    }

    #[test]
    fn test_extract_table_name_with_joins() {
        // Should extract the first table from FROM clause
        let table_name = QueryParser::extract_table_name(
            "SELECT * FROM orders o JOIN users u ON o.user_id = u.id",
        )
        .unwrap();
        assert_eq!(table_name, "orders");
    }

    #[test]
    fn test_extract_table_name_invalid() {
        // Missing FROM clause
        let result = QueryParser::extract_table_name("SELECT * users");
        assert!(result.is_err());

        // Non-SELECT statement
        let result = QueryParser::extract_table_name("INSERT INTO users VALUES (1)");
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_where_clause() {
        let clause =
            QueryParser::extract_where_clause("SELECT * FROM t WHERE id > 5 ORDER BY id").unwrap();
        assert_eq!(clause, "id > 5");

        let clause = QueryParser::extract_where_clause("SELECT * FROM t WHERE x = 1").unwrap();
        assert_eq!(clause, "x = 1");

        let clause = QueryParser::extract_where_clause("SELECT * FROM t");
        assert!(clause.is_none());
    }

    #[test]
    fn test_extract_where_clause_complex() {
        let clause = QueryParser::extract_where_clause(
            "SELECT * FROM t WHERE a = 1 AND b = 'hello' ORDER BY id LIMIT 10",
        )
        .unwrap();
        assert_eq!(clause, "a = 1 AND b = 'hello'");
    }

    #[test]
    fn test_resolve_placeholders() {
        let user_id = UserId::new("alice");
        let clause = "user_id = CURRENT_USER()";
        let resolved = QueryParser::resolve_where_clause_placeholders(clause, &user_id);
        assert_eq!(resolved, "user_id = 'alice'");
    }

    #[test]
    fn test_extract_projections_star() {
        let projections = QueryParser::extract_projections("SELECT * FROM test.users");
        assert!(projections.is_none());
    }

    #[test]
    fn test_extract_projections_single_column() {
        let projections = QueryParser::extract_projections("SELECT id FROM test.users").unwrap();
        assert_eq!(projections, vec!["id"]);
    }

    #[test]
    fn test_extract_projections_multiple_columns() {
        let projections =
            QueryParser::extract_projections("SELECT id, name, email FROM test.users").unwrap();
        assert_eq!(projections, vec!["id", "name", "email"]);
    }

    #[test]
    fn test_extract_projections_with_alias() {
        let projections =
            QueryParser::extract_projections("SELECT id, name AS user_name FROM test.users")
                .unwrap();
        assert_eq!(projections, vec!["id", "name"]);
    }

    #[test]
    fn test_extract_projections_with_where() {
        let projections = QueryParser::extract_projections(
            "SELECT id, message FROM chat.messages WHERE user_id = 'alice'",
        )
        .unwrap();
        assert_eq!(projections, vec!["id", "message"]);
    }

    #[test]
    fn test_extract_projections_qualified_wildcard() {
        // table.* should return None (all columns)
        let projections = QueryParser::extract_projections("SELECT users.* FROM users");
        assert!(projections.is_none());
    }
}
