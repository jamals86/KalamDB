//! SQL query parsing utilities for live queries and subscriptions
//!
//! Uses sqlparser-rs for safe SQL parsing to prevent SQL injection attacks
//! and ensure proper handling of edge cases.

use sqlparser::ast::{
    Expr, Query, Select, SelectItem, SetExpr, Statement, TableFactor, TableWithJoins,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

/// Error type for query parsing
#[derive(Debug, Clone)]
pub enum QueryParseError {
    /// Failed to parse SQL
    ParseError(String),
    /// Invalid SQL for the operation
    InvalidSql(String),
}

impl std::fmt::Display for QueryParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParseError(msg) => write!(f, "Parse error: {}", msg),
            Self::InvalidSql(msg) => write!(f, "Invalid SQL: {}", msg),
        }
    }
}

impl std::error::Error for QueryParseError {}

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
    pub fn extract_table_name(query: &str) -> Result<String, QueryParseError> {
        let dialect = GenericDialect {};
        let statements = Parser::parse_sql(&dialect, query).map_err(|e| {
            QueryParseError::ParseError(format!("Failed to parse SQL: {}", e))
        })?;

        if statements.is_empty() {
            return Err(QueryParseError::InvalidSql("Empty SQL statement".to_string()));
        }

        let statement = &statements[0];
        match statement {
            Statement::Query(query) => Self::extract_table_from_query(query),
            _ => Err(QueryParseError::InvalidSql(
                "Only SELECT queries are supported for subscriptions".to_string(),
            )),
        }
    }

    /// Extract table name from a parsed Query AST
    fn extract_table_from_query(query: &Query) -> Result<String, QueryParseError> {
        match query.body.as_ref() {
            SetExpr::Select(select) => Self::extract_table_from_select(select),
            _ => Err(QueryParseError::InvalidSql(
                "Unsupported query type for subscriptions".to_string(),
            )),
        }
    }

    /// Extract table name from a SELECT statement
    fn extract_table_from_select(select: &Select) -> Result<String, QueryParseError> {
        if select.from.is_empty() {
            return Err(QueryParseError::InvalidSql(
                "No FROM clause in SELECT".to_string(),
            ));
        }

        let table_with_joins = &select.from[0];
        Self::extract_table_from_table_with_joins(table_with_joins)
    }

    /// Extract table name from TableWithJoins
    fn extract_table_from_table_with_joins(
        table_with_joins: &TableWithJoins,
    ) -> Result<String, QueryParseError> {
        match &table_with_joins.relation {
            TableFactor::Table { name, .. } => {
                use sqlparser::ast::ObjectNamePart;
                // Handle schema-qualified names (namespace.table)
                let parts: Vec<String> = name
                    .0
                    .iter()
                    .filter_map(|part| match part {
                        ObjectNamePart::Identifier(ident) => Some(ident.value.clone()),
                        _ => None,
                    })
                    .collect();

                Ok(parts.join("."))
            },
            _ => Err(QueryParseError::InvalidSql(
                "Unsupported table type in FROM clause".to_string(),
            )),
        }
    }

    /// Extract WHERE clause from SQL query
    ///
    /// Returns the WHERE clause as a string for filter matching.
    /// Uses sqlparser-rs to safely parse and extract the clause.
    pub fn extract_where_clause(query: &str) -> Result<Option<String>, QueryParseError> {
        let dialect = GenericDialect {};
        let statements = Parser::parse_sql(&dialect, query).map_err(|e| {
            QueryParseError::ParseError(format!("Failed to parse SQL: {}", e))
        })?;

        if statements.is_empty() {
            return Err(QueryParseError::InvalidSql("Empty SQL statement".to_string()));
        }

        let statement = &statements[0];
        match statement {
            Statement::Query(query) => Self::extract_where_from_query(query),
            _ => Err(QueryParseError::InvalidSql(
                "Only SELECT queries are supported".to_string(),
            )),
        }
    }

    /// Extract WHERE clause from a parsed Query AST
    fn extract_where_from_query(query: &Query) -> Result<Option<String>, QueryParseError> {
        match query.body.as_ref() {
            SetExpr::Select(select) => Ok(select.selection.as_ref().map(|expr| format!("{}", expr))),
            _ => Ok(None),
        }
    }

    /// Extract projection columns from SQL query
    ///
    /// Returns a list of column names requested in the SELECT clause.
    /// Returns None for SELECT * queries.
    pub fn extract_projections(query: &str) -> Result<Option<Vec<String>>, QueryParseError> {
        let dialect = GenericDialect {};
        let statements = Parser::parse_sql(&dialect, query).map_err(|e| {
            QueryParseError::ParseError(format!("Failed to parse SQL: {}", e))
        })?;

        if statements.is_empty() {
            return Err(QueryParseError::InvalidSql("Empty SQL statement".to_string()));
        }

        let statement = &statements[0];
        match statement {
            Statement::Query(query) => Self::extract_projections_from_query(query),
            _ => Err(QueryParseError::InvalidSql(
                "Only SELECT queries are supported".to_string(),
            )),
        }
    }

    /// Resolve placeholders like CURRENT_USER() in WHERE clause
    pub fn resolve_where_clause_placeholders(where_clause: &str, user_id: &kalamdb_commons::models::UserId) -> String {
        where_clause.replace("CURRENT_USER()", &format!("'{}'", user_id))
    }

    /// Extract projection columns from a parsed Query AST
    fn extract_projections_from_query(query: &Query) -> Result<Option<Vec<String>>, QueryParseError> {
        match query.body.as_ref() {
            SetExpr::Select(select) => Self::extract_projections_from_select(select),
            _ => Ok(None),
        }
    }

    /// Extract projections from a SELECT statement
    fn extract_projections_from_select(
        select: &Select,
    ) -> Result<Option<Vec<String>>, QueryParseError> {
        let mut columns = Vec::new();
        let mut has_wildcard = false;

        for item in &select.projection {
            match item {
                SelectItem::Wildcard(_) => {
                    has_wildcard = true;
                    break;
                },
                SelectItem::UnnamedExpr(expr) => {
                    columns.push(Self::expr_to_column_name(expr));
                },
                SelectItem::ExprWithAlias { expr: _, alias } => {
                    columns.push(alias.value.clone());
                },
                SelectItem::QualifiedWildcard(_, _) => {
                    has_wildcard = true;
                    break;
                },
            }
        }

        if has_wildcard {
            Ok(None) // SELECT * or table.* returns None
        } else {
            Ok(Some(columns))
        }
    }

    /// Convert an expression to a column name string
    fn expr_to_column_name(expr: &Expr) -> String {
        match expr {
            Expr::Identifier(ident) => ident.value.clone(),
            Expr::CompoundIdentifier(parts) => {
                parts.iter().map(|p| p.value.clone()).collect::<Vec<_>>().join(".")
            },
            _ => format!("{}", expr),
        }
    }

    /// Parse parameterized WHERE clause expression
    ///
    /// Converts expressions like "user_id = $1" into a structured format
    /// for parameter substitution.
    pub fn parse_parameterized_expr(
        expr_str: &str,
    ) -> Result<(String, Vec<String>), QueryParseError> {
        let dialect = GenericDialect {};

        // Wrap in a dummy SELECT to parse the expression
        let dummy_query = format!("SELECT * FROM dummy WHERE {}", expr_str);
        let statements = Parser::parse_sql(&dialect, &dummy_query).map_err(|e| {
            QueryParseError::ParseError(format!("Failed to parse expression: {}", e))
        })?;

        if statements.is_empty() {
            return Err(QueryParseError::InvalidSql(
                "Failed to parse expression".to_string(),
            ));
        }

        // Extract parameters ($1, $2, etc.)
        let mut params = Vec::new();
        Self::extract_parameters_from_expr_str(expr_str, &mut params);

        Ok((expr_str.to_string(), params))
    }

    /// Extract parameter placeholders from expression string
    fn extract_parameters_from_expr_str(expr_str: &str, params: &mut Vec<String>) {
        let re = regex::Regex::new(r"\$\d+").unwrap();
        for cap in re.captures_iter(expr_str) {
            if let Some(param) = cap.get(0) {
                let param_str = param.as_str().to_string();
                if !params.contains(&param_str) {
                    params.push(param_str);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_table_name_simple() {
        let result = QueryParser::extract_table_name("SELECT * FROM users");
        assert_eq!(result.unwrap(), "users");
    }

    #[test]
    fn test_extract_table_name_qualified() {
        let result = QueryParser::extract_table_name("SELECT * FROM chat.messages");
        assert_eq!(result.unwrap(), "chat.messages");
    }

    #[test]
    fn test_extract_where_clause() {
        let result = QueryParser::extract_where_clause("SELECT * FROM users WHERE id = 1").unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("id"));
    }

    #[test]
    fn test_extract_projections_wildcard() {
        let result = QueryParser::extract_projections("SELECT * FROM users").unwrap();
        assert!(result.is_none()); // Wildcard returns None
    }

    #[test]
    fn test_extract_projections_specific() {
        let result =
            QueryParser::extract_projections("SELECT id, name, email FROM users").unwrap();
        assert!(result.is_some());
        let cols = result.unwrap();
        assert_eq!(cols, vec!["id", "name", "email"]);
    }
}
