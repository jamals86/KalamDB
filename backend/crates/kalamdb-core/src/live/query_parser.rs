//! SQL query parsing utilities for live queries

use crate::error::KalamDbError;
use kalamdb_commons::models::UserId;

/// Utilities for parsing SQL queries for live subscriptions
pub struct QueryParser;

impl QueryParser {
    /// Extract table name from SQL query
    ///
    /// This is a simple implementation that looks for "FROM table_name"
    /// TODO: Replace with proper DataFusion SQL parsing
    pub fn extract_table_name(query: &str) -> Result<String, KalamDbError> {
        let query_upper = query.to_uppercase();
        let from_pos = query_upper.find(" FROM ").ok_or_else(|| {
            KalamDbError::InvalidSql("Query must contain FROM clause".to_string())
        })?;

        let after_from = &query[(from_pos + 6)..]; // Skip " FROM "
        let table_name = after_from
            .split_whitespace()
            .next()
            .ok_or_else(|| KalamDbError::InvalidSql("Invalid table name after FROM".to_string()))?
            .trim_matches(|c| c == '"' || c == '\'' || c == '`')
            .to_string();

        Ok(table_name)
    }

    /// Extract WHERE clause from SQL query
    ///
    /// Returns None if no WHERE clause exists.
    /// This is a simple implementation that looks for "WHERE ..."
    pub fn extract_where_clause(query: &str) -> Option<String> {
        let query_upper = query.to_uppercase();
        let where_pos = query_upper.find(" WHERE ")?;

        // Get everything after WHERE, handling potential ORDER BY, LIMIT, etc.
        let after_where = &query[(where_pos + 7)..]; // Skip " WHERE "

        // Find the end of the WHERE clause (before ORDER BY, LIMIT, etc.)
        let end_keywords = [" ORDER BY", " LIMIT", " OFFSET", " GROUP BY"];
        let mut end_pos = after_where.len();

        for keyword in &end_keywords {
            if let Some(pos) = after_where.to_uppercase().find(keyword) {
                if pos < end_pos {
                    end_pos = pos;
                }
            }
        }

        Some(after_where[..end_pos].trim().to_string())
    }

    /// Resolve placeholders in WHERE clause (e.g., CURRENT_USER())
    pub fn resolve_where_clause_placeholders(clause: &str, user_id: &UserId) -> String {
        let replacement = format!("'{}'", user_id.as_str());
        clause
            .replace("CURRENT_USER()", &replacement)
            .replace("current_user()", &replacement)
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
    fn test_extract_where_clause() {
        let clause =
            QueryParser::extract_where_clause("SELECT * FROM t WHERE id > 5 ORDER BY id").unwrap();
        assert_eq!(clause, "id > 5");

        let clause = QueryParser::extract_where_clause("SELECT * FROM t WHERE x=1").unwrap();
        assert_eq!(clause, "x=1");

        let clause = QueryParser::extract_where_clause("SELECT * FROM t");
        assert!(clause.is_none());
    }

    #[test]
    fn test_resolve_placeholders() {
        let user_id = UserId::new("alice");
        let clause = "user_id = CURRENT_USER()";
        let resolved = QueryParser::resolve_where_clause_placeholders(clause, &user_id);
        assert_eq!(resolved, "user_id = 'alice'");
    }
}
