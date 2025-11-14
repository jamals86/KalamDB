//! Filter compilation and evaluation for live query subscriptions
//!
//! This module provides SQL WHERE clause parsing, compilation, and evaluation
//! for filtering live query notifications. Only subscribers whose filters match
//! the changed row data will receive notifications.
//!
//! # Architecture
//!
//! ```text
//! Subscription Registration
//!         ↓
//! Parse SQL → Extract WHERE clause
//!         ↓
//! Compile to FilterPredicate
//!         ↓
//! Cache in LiveQueryManager
//!         ↓
//! On Change: Evaluate row_data → Send if matches
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! // Subscription query: SELECT * FROM messages WHERE user_id = 'user1' AND read = false
//!
//! let filter = FilterCompiler::compile("user_id = 'user1' AND read = false")?;
//!
//! // Later, when row changes:
//! let row_data = json!({"user_id": "user1", "read": false, "text": "Hello"});
//! if filter.matches(&row_data)? {
//!     // Send notification to subscriber
//! }
/// ```

use crate::error::KalamDbError;
use datafusion::sql::sqlparser::ast::{BinaryOperator, Expr, Statement, Value};
use datafusion::sql::sqlparser::dialect::PostgreSqlDialect;
use datafusion::sql::sqlparser::parser::Parser;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;

/// Compiled filter predicate that can be evaluated against row data
#[derive(Debug, Clone)]
pub struct FilterPredicate {
    /// Original WHERE clause SQL
    sql: String,

    /// Parsed expression tree
    expr: Expr,
}

impl FilterPredicate {
    /// Create a new filter predicate from a WHERE clause
    ///
    /// # Arguments
    ///
    /// * `where_clause` - SQL WHERE clause (e.g., "user_id = 'user1' AND read = false")
    ///
    /// # Returns
    ///
    /// Compiled filter predicate ready for evaluation
    pub fn new(where_clause: &str) -> Result<Self, KalamDbError> {
        // Parse as a SELECT with WHERE to extract the expression
        let sql = format!("SELECT * FROM t WHERE {}", where_clause);

        let dialect = PostgreSqlDialect {};
        let statements = Parser::parse_sql(&dialect, &sql).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to parse WHERE clause: {}", e))
        })?;

        if statements.is_empty() {
            return Err(KalamDbError::InvalidOperation(
                "Empty WHERE clause".to_string(),
            ));
        }

        // Extract WHERE expression from parsed SELECT
        let expr = match &statements[0] {
            Statement::Query(query) => {
                if let Some(selection) = &query.body.as_select().and_then(|s| s.selection.as_ref())
                {
                    (*selection).clone()
                } else {
                    return Err(KalamDbError::InvalidOperation(
                        "No WHERE clause found".to_string(),
                    ));
                }
            }
            _ => {
                return Err(KalamDbError::InvalidOperation(
                    "Invalid WHERE clause syntax".to_string(),
                ));
            }
        };

        Ok(Self {
            sql: where_clause.to_string(),
            expr: expr.clone(),
        })
    }

    /// Evaluate this filter against row data
    ///
    /// # Arguments
    ///
    /// * `row_data` - JSON object representing the row
    ///
    /// # Returns
    ///
    /// `true` if the row matches the filter, `false` otherwise
    pub fn matches(&self, row_data: &JsonValue) -> Result<bool, KalamDbError> {
        self.evaluate_expr(&self.expr, row_data)
    }

    /// Get the original SQL WHERE clause
    pub fn sql(&self) -> &str {
        &self.sql
    }

    /// Recursively evaluate an expression against row data
    fn evaluate_expr(&self, expr: &Expr, row_data: &JsonValue) -> Result<bool, KalamDbError> {
        match expr {
            // Binary operations: AND, OR, =, !=, <, >, <=, >=
            Expr::BinaryOp { left, op, right } => match op {
                BinaryOperator::And => {
                    let left_result = self.evaluate_expr(left, row_data)?;
                    let right_result = self.evaluate_expr(right, row_data)?;
                    Ok(left_result && right_result)
                }
                BinaryOperator::Or => {
                    let left_result = self.evaluate_expr(left, row_data)?;
                    let right_result = self.evaluate_expr(right, row_data)?;
                    Ok(left_result || right_result)
                }
                BinaryOperator::Eq => {
                    self.evaluate_comparison(left, right, row_data, |a, b| a == b)
                }
                BinaryOperator::NotEq => {
                    self.evaluate_comparison(left, right, row_data, |a, b| a != b)
                }
                BinaryOperator::Lt => {
                    let left_value = self.extract_value(left, row_data)?;
                    let right_value = self.extract_value(right, row_data)?;
                    Self::compare_numeric(&left_value, &right_value, "<")
                }
                BinaryOperator::Gt => {
                    let left_value = self.extract_value(left, row_data)?;
                    let right_value = self.extract_value(right, row_data)?;
                    Self::compare_numeric(&left_value, &right_value, ">")
                }
                BinaryOperator::LtEq => {
                    let left_value = self.extract_value(left, row_data)?;
                    let right_value = self.extract_value(right, row_data)?;
                    Self::compare_numeric(&left_value, &right_value, "<=")
                }
                BinaryOperator::GtEq => {
                    let left_value = self.extract_value(left, row_data)?;
                    let right_value = self.extract_value(right, row_data)?;
                    Self::compare_numeric(&left_value, &right_value, ">=")
                }
                _ => Err(KalamDbError::InvalidOperation(format!(
                    "Unsupported operator: {:?}",
                    op
                ))),
            },

            // Parentheses: (expression)
            Expr::Nested(inner) => self.evaluate_expr(inner, row_data),

            // NOT expression
            Expr::UnaryOp { op, expr } => match op {
                datafusion::sql::sqlparser::ast::UnaryOperator::Not => {
                    let result = self.evaluate_expr(expr, row_data)?;
                    Ok(!result)
                }
                _ => Err(KalamDbError::InvalidOperation(format!(
                    "Unsupported unary operator: {:?}",
                    op
                ))),
            },

            _ => Err(KalamDbError::InvalidOperation(format!(
                "Unsupported expression type: {:?}",
                expr
            ))),
        }
    }

    /// Evaluate a comparison operation (=, !=, <, >, <=, >=)
    fn evaluate_comparison<F>(
        &self,
        left: &Expr,
        right: &Expr,
        row_data: &JsonValue,
        comparator: F,
    ) -> Result<bool, KalamDbError>
    where
        F: Fn(&JsonValue, &JsonValue) -> bool,
    {
        let left_value = self.extract_value(left, row_data)?;
        let right_value = self.extract_value(right, row_data)?;

        // For numeric comparisons, need special handling since JsonValue doesn't impl PartialOrd
        Ok(comparator(&left_value, &right_value))
    }

    /// Helper to compare two JSON values for numeric comparisons
    fn compare_numeric(
        left: &JsonValue,
        right: &JsonValue,
        op: &str,
    ) -> Result<bool, KalamDbError> {
        // Try to extract numbers
        let left_num = match left {
            JsonValue::Number(n) => n.as_f64().ok_or_else(|| {
                KalamDbError::InvalidOperation("Invalid number in comparison".to_string())
            })?,
            _ => {
                return Err(KalamDbError::InvalidOperation(format!(
                    "Cannot compare non-numeric values with {}",
                    op
                )))
            }
        };

        let right_num = match right {
            JsonValue::Number(n) => n.as_f64().ok_or_else(|| {
                KalamDbError::InvalidOperation("Invalid number in comparison".to_string())
            })?,
            _ => {
                return Err(KalamDbError::InvalidOperation(format!(
                    "Cannot compare non-numeric values with {}",
                    op
                )))
            }
        };

        Ok(match op {
            "<" => left_num < right_num,
            ">" => left_num > right_num,
            "<=" => left_num <= right_num,
            ">=" => left_num >= right_num,
            _ => {
                return Err(KalamDbError::InvalidOperation(format!(
                    "Unknown operator: {}",
                    op
                )))
            }
        })
    }

    /// Extract a value from an expression
    ///
    /// Handles:
    /// - Column references (e.g., user_id) → lookup in row_data
    /// - Literals (e.g., 'user1', 123, true) → convert to JsonValue
    fn extract_value(&self, expr: &Expr, row_data: &JsonValue) -> Result<JsonValue, KalamDbError> {
        match expr {
            // Column reference: lookup in row_data
            Expr::Identifier(ident) => {
                let column_name = ident.value.as_str();
                row_data.get(column_name).cloned().ok_or_else(|| {
                    KalamDbError::InvalidOperation(format!(
                        "Column not found in row data: {}",
                        column_name
                    ))
                })
            }

            // Literal value: convert to JsonValue
            Expr::Value(v) => match &v.value {
                Value::SingleQuotedString(s) | Value::DoubleQuotedString(s) => {
                    Ok(JsonValue::String(s.clone()))
                }
                Value::Number(n, _) => {
                    // Try parsing as i64 first, then f64
                    if let Ok(i) = n.parse::<i64>() {
                        Ok(JsonValue::Number(i.into()))
                    } else if let Ok(f) = n.parse::<f64>() {
                        Ok(serde_json::Number::from_f64(f)
                            .map(JsonValue::Number)
                            .unwrap_or(JsonValue::Null))
                    } else {
                        Err(KalamDbError::InvalidOperation(format!(
                            "Invalid number: {}",
                            n
                        )))
                    }
                }
                Value::Boolean(b) => Ok(JsonValue::Bool(*b)),
                Value::Null => Ok(JsonValue::Null),
                _ => Err(KalamDbError::InvalidOperation(format!(
                    "Unsupported literal type: {:?}",
                    v
                ))),
            },

            _ => Err(KalamDbError::InvalidOperation(format!(
                "Unsupported expression in value extraction: {:?}",
                expr
            ))),
        }
    }
}

/// Filter cache for live query subscriptions
///
/// Stores compiled filter predicates keyed by live_id for fast evaluation.
pub struct FilterCache {
    filters: HashMap<String, Arc<FilterPredicate>>,
}

impl FilterCache {
    /// Create a new empty filter cache
    pub fn new() -> Self {
        Self {
            filters: HashMap::new(),
        }
    }

    /// Add or update a filter in the cache
    ///
    /// # Arguments
    ///
    /// * `live_id` - Live query identifier
    /// * `where_clause` - SQL WHERE clause to compile
    pub fn insert(&mut self, live_id: String, where_clause: &str) -> Result<(), KalamDbError> {
        let predicate = FilterPredicate::new(where_clause)?;
        self.filters.insert(live_id, Arc::new(predicate));
        Ok(())
    }

    /// Get a filter from the cache
    pub fn get(&self, live_id: &str) -> Option<Arc<FilterPredicate>> {
        self.filters.get(live_id).cloned()
    }

    /// Remove a filter from the cache
    pub fn remove(&mut self, live_id: &str) -> Option<Arc<FilterPredicate>> {
        self.filters.remove(live_id)
    }

    /// Get the number of cached filters
    pub fn len(&self) -> usize {
        self.filters.len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.filters.is_empty()
    }

    /// Clear all filters from the cache
    pub fn clear(&mut self) {
        self.filters.clear();
    }
}

impl Default for FilterCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_simple_equality_filter() {
        let filter = FilterPredicate::new("user_id = 'user1'").unwrap();

        let matching_row = json!({"user_id": "user1", "text": "Hello"});
        assert!(filter.matches(&matching_row).unwrap());

        let non_matching_row = json!({"user_id": "user2", "text": "Hello"});
        assert!(!filter.matches(&non_matching_row).unwrap());
    }

    #[test]
    fn test_and_filter() {
        let filter = FilterPredicate::new("user_id = 'user1' AND read = false").unwrap();

        let matching_row = json!({"user_id": "user1", "read": false});
        assert!(filter.matches(&matching_row).unwrap());

        let non_matching_row1 = json!({"user_id": "user2", "read": false});
        assert!(!filter.matches(&non_matching_row1).unwrap());

        let non_matching_row2 = json!({"user_id": "user1", "read": true});
        assert!(!filter.matches(&non_matching_row2).unwrap());
    }

    #[test]
    fn test_or_filter() {
        let filter = FilterPredicate::new("status = 'active' OR status = 'pending'").unwrap();

        let matching_row1 = json!({"status": "active"});
        assert!(filter.matches(&matching_row1).unwrap());

        let matching_row2 = json!({"status": "pending"});
        assert!(filter.matches(&matching_row2).unwrap());

        let non_matching_row = json!({"status": "completed"});
        assert!(!filter.matches(&non_matching_row).unwrap());
    }

    #[test]
    fn test_numeric_comparison() {
        let filter = FilterPredicate::new("age >= 18").unwrap();

        let matching_row = json!({"age": 25});
        assert!(filter.matches(&matching_row).unwrap());

        let non_matching_row = json!({"age": 15});
        assert!(!filter.matches(&non_matching_row).unwrap());

        let edge_case = json!({"age": 18});
        assert!(filter.matches(&edge_case).unwrap());
    }

    #[test]
    fn test_not_filter() {
        let filter = FilterPredicate::new("NOT (deleted = true)").unwrap();

        let matching_row = json!({"deleted": false});
        assert!(filter.matches(&matching_row).unwrap());

        let non_matching_row = json!({"deleted": true});
        assert!(!filter.matches(&non_matching_row).unwrap());
    }

    #[test]
    fn test_complex_filter() {
        let filter =
            FilterPredicate::new("(user_id = 'user1' OR user_id = 'user2') AND read = false")
                .unwrap();

        let matching_row1 = json!({"user_id": "user1", "read": false});
        assert!(filter.matches(&matching_row1).unwrap());

        let matching_row2 = json!({"user_id": "user2", "read": false});
        assert!(filter.matches(&matching_row2).unwrap());

        let non_matching_row1 = json!({"user_id": "user3", "read": false});
        assert!(!filter.matches(&non_matching_row1).unwrap());

        let non_matching_row2 = json!({"user_id": "user1", "read": true});
        assert!(!filter.matches(&non_matching_row2).unwrap());
    }

    #[test]
    fn test_filter_cache() {
        let mut cache = FilterCache::new();

        cache
            .insert("live1".to_string(), "user_id = 'user1'")
            .unwrap();
        cache.insert("live2".to_string(), "read = false").unwrap();

        assert_eq!(cache.len(), 2);

        let filter1 = cache.get("live1").unwrap();
        let row = json!({"user_id": "user1"});
        assert!(filter1.matches(&row).unwrap());

        cache.remove("live1");
        assert_eq!(cache.len(), 1);
        assert!(cache.get("live1").is_none());

        cache.clear();
        assert!(cache.is_empty());
    }
}
