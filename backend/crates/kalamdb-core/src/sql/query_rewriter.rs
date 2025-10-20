//! Query rewriter for automatic _deleted filter injection
//!
//! This module rewrites SQL queries to automatically add WHERE _deleted = false
//! for user and shared tables.

use datafusion::sql::sqlparser::ast::{
    BinaryOperator, Expr, Ident, Select, SetExpr, Statement, Value,
};
use datafusion::sql::sqlparser::dialect::GenericDialect;
use datafusion::sql::sqlparser::parser::Parser;

/// Query rewriter for automatic filter injection
pub struct QueryRewriter;

impl QueryRewriter {
    /// Rewrite a SQL query to add _deleted = false filter
    ///
    /// This is applied to SELECT queries on user and shared tables.
    /// System and stream tables are not filtered.
    pub fn rewrite_query(sql: &str) -> Result<String, String> {
        let dialect = GenericDialect {};

        // Parse the SQL
        let statements =
            Parser::parse_sql(&dialect, sql).map_err(|e| format!("Failed to parse SQL: {}", e))?;

        if statements.is_empty() {
            return Err("No SQL statement found".to_string());
        }

        // Only rewrite SELECT statements
        let statement = &statements[0];
        match statement {
            Statement::Query(query) => {
                let mut rewritten_query = (**query).clone();
                Self::add_deleted_filter(&mut rewritten_query)?;
                Ok(format!("{}", Statement::Query(Box::new(rewritten_query))))
            }
            _ => {
                // Non-SELECT statements are returned as-is
                Ok(sql.to_string())
            }
        }
    }

    /// Add _deleted = false filter to a query
    fn add_deleted_filter(
        query: &mut datafusion::sql::sqlparser::ast::Query,
    ) -> Result<(), String> {
        // Only process simple SELECT queries for now
        if let SetExpr::Select(select) = &mut *query.body {
            // Check if the query is on a table that needs filtering
            if Self::should_add_filter(select) {
                Self::inject_deleted_filter(select);
            }
        }

        Ok(())
    }

    /// Check if a SELECT query should have _deleted filter added
    fn should_add_filter(select: &Select) -> bool {
        // For now, add to all SELECT queries
        // In the future, we'll check the table type (exclude system and stream tables)

        // Don't add filter if it already exists
        if let Some(ref selection) = select.selection {
            if Self::has_deleted_filter(selection) {
                return false;
            }
        }

        true
    }

    /// Check if an expression already contains _deleted filter
    fn has_deleted_filter(expr: &Expr) -> bool {
        match expr {
            Expr::BinaryOp { left, op: _, right } => {
                Self::has_deleted_filter(left) || Self::has_deleted_filter(right)
            }
            Expr::Identifier(ident) => ident.value == "_deleted",
            _ => false,
        }
    }

    /// Inject _deleted = false filter into SELECT
    fn inject_deleted_filter(select: &mut Select) {
        let deleted_filter = Expr::BinaryOp {
            left: Box::new(Expr::Identifier(Ident::new("_deleted"))),
            op: BinaryOperator::Eq,
            right: Box::new(Expr::Value(Value::Boolean(false))),
        };

        // Combine with existing WHERE clause if present
        select.selection = Some(match select.selection.take() {
            Some(existing) => Expr::BinaryOp {
                left: Box::new(existing),
                op: BinaryOperator::And,
                right: Box::new(deleted_filter),
            },
            None => deleted_filter,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rewrite_simple_select() {
        let sql = "SELECT * FROM messages";
        let rewritten = QueryRewriter::rewrite_query(sql).unwrap();

        // Verify _deleted filter was added
        assert!(rewritten.contains("_deleted"));
        assert!(rewritten.contains("false"));
    }

    #[test]
    fn test_rewrite_with_existing_where() {
        let sql = "SELECT * FROM messages WHERE user_id = 'user1'";
        let rewritten = QueryRewriter::rewrite_query(sql).unwrap();

        // Verify _deleted filter was added with AND
        assert!(rewritten.contains("_deleted"));
        assert!(rewritten.contains("AND"));
    }

    #[test]
    fn test_no_duplicate_filter() {
        let sql = "SELECT * FROM messages WHERE _deleted = false";
        let rewritten = QueryRewriter::rewrite_query(sql).unwrap();

        // Verify no duplicate filter
        let count = rewritten.matches("_deleted").count();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_non_select_passthrough() {
        let sql = "INSERT INTO messages (content) VALUES ('test')";
        let rewritten = QueryRewriter::rewrite_query(sql).unwrap();

        // Verify INSERT is not modified
        assert!(!rewritten.contains("_deleted"));
    }
}
