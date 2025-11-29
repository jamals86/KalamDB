//! Filter evaluation for live query subscriptions
//!
//! This module provides SQL WHERE clause expression evaluation for filtering
//! live query notifications. Only subscribers whose filters match
//! the changed row data will receive notifications.
//!
//! The filter expression (Expr) is parsed once during subscription registration
//! and stored in SubscriptionState. This module provides evaluation functions
//! to match row data against the expression.

use crate::error::KalamDbError;
use datafusion::scalar::ScalarValue;
use datafusion::sql::sqlparser::ast::{BinaryOperator, Expr, Statement, Value};
use datafusion::sql::sqlparser::dialect::PostgreSqlDialect;
use datafusion::sql::sqlparser::parser::Parser;
use kalamdb_commons::models::Row;

/// Parse a WHERE clause string into an Expr AST
///
/// # Arguments
///
/// * `where_clause` - SQL WHERE clause (e.g., "user_id = 'user1' AND read = false")
///
/// # Returns
///
/// Parsed expression ready for evaluation
pub fn parse_where_clause(where_clause: &str) -> Result<Expr, KalamDbError> {
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
    match &statements[0] {
        Statement::Query(query) => {
            if let Some(selection) = &query.body.as_select().and_then(|s| s.selection.as_ref()) {
                Ok((*selection).clone())
            } else {
                Err(KalamDbError::InvalidOperation(
                    "No WHERE clause found".to_string(),
                ))
            }
        }
        _ => Err(KalamDbError::InvalidOperation(
            "Invalid WHERE clause syntax".to_string(),
        )),
    }
}

/// Maximum recursion depth for expression evaluation (prevents stack overflow)
const MAX_EXPR_DEPTH: usize = 64;

/// Evaluate a filter expression against row data
///
/// # Arguments
///
/// * `expr` - The filter expression to evaluate
/// * `row_data` - Row object representing the row
///
/// # Returns
///
/// `true` if the row matches the filter, `false` otherwise
pub fn matches(expr: &Expr, row_data: &Row) -> Result<bool, KalamDbError> {
    evaluate_expr(expr, row_data, 0)
}

/// Recursively evaluate an expression against row data
fn evaluate_expr(expr: &Expr, row_data: &Row, depth: usize) -> Result<bool, KalamDbError> {
    if depth > MAX_EXPR_DEPTH {
        return Err(KalamDbError::InvalidOperation(
            "Filter expression too deeply nested (max 64 levels)".to_string(),
        ));
    }
    match expr {
        // Binary operations: AND, OR, =, !=, <, >, <=, >=
        Expr::BinaryOp { left, op, right } => match op {
            BinaryOperator::And => {
                let left_result = evaluate_expr(left, row_data, depth + 1)?;
                let right_result = evaluate_expr(right, row_data, depth + 1)?;
                Ok(left_result && right_result)
            }
            BinaryOperator::Or => {
                let left_result = evaluate_expr(left, row_data, depth + 1)?;
                let right_result = evaluate_expr(right, row_data, depth + 1)?;
                Ok(left_result || right_result)
            }
            BinaryOperator::Eq => evaluate_comparison(left, right, row_data, scalars_equal),
            BinaryOperator::NotEq => {
                evaluate_comparison(left, right, row_data, |a, b| !scalars_equal(a, b))
            }
            BinaryOperator::Lt => {
                let left_value = extract_value(left, row_data)?;
                let right_value = extract_value(right, row_data)?;
                compare_numeric(&left_value, &right_value, "<")
            }
            BinaryOperator::Gt => {
                let left_value = extract_value(left, row_data)?;
                let right_value = extract_value(right, row_data)?;
                compare_numeric(&left_value, &right_value, ">")
            }
            BinaryOperator::LtEq => {
                let left_value = extract_value(left, row_data)?;
                let right_value = extract_value(right, row_data)?;
                compare_numeric(&left_value, &right_value, "<=")
            }
            BinaryOperator::GtEq => {
                let left_value = extract_value(left, row_data)?;
                let right_value = extract_value(right, row_data)?;
                compare_numeric(&left_value, &right_value, ">=")
            }
            _ => Err(KalamDbError::InvalidOperation(format!(
                "Unsupported operator: {:?}",
                op
            ))),
        },

        // Parentheses: (expression)
        Expr::Nested(inner) => evaluate_expr(inner, row_data, depth + 1),

        // NOT expression
        Expr::UnaryOp { op, expr } => match op {
            datafusion::sql::sqlparser::ast::UnaryOperator::Not => {
                let result = evaluate_expr(expr, row_data, depth + 1)?;
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
    left: &Expr,
    right: &Expr,
    row_data: &Row,
    comparator: F,
) -> Result<bool, KalamDbError>
where
    F: Fn(&ScalarValue, &ScalarValue) -> bool,
{
    let left_value = extract_value(left, row_data)?;
    let right_value = extract_value(right, row_data)?;

    Ok(comparator(&left_value, &right_value))
}

/// Check if two scalars are equal, handling numeric type coercion
fn scalars_equal(left: &ScalarValue, right: &ScalarValue) -> bool {
    if left == right {
        return true;
    }

    // Try numeric coercion
    if let (Some(l), Some(r)) = (as_f64(left), as_f64(right)) {
        return (l - r).abs() < f64::EPSILON;
    }

    false
}

/// Helper to convert ScalarValue to f64 if possible
fn as_f64(v: &ScalarValue) -> Option<f64> {
    match v {
        ScalarValue::Int8(Some(v)) => Some(*v as f64),
        ScalarValue::Int16(Some(v)) => Some(*v as f64),
        ScalarValue::Int32(Some(v)) => Some(*v as f64),
        ScalarValue::Int64(Some(v)) => Some(*v as f64),
        ScalarValue::UInt8(Some(v)) => Some(*v as f64),
        ScalarValue::UInt16(Some(v)) => Some(*v as f64),
        ScalarValue::UInt32(Some(v)) => Some(*v as f64),
        ScalarValue::UInt64(Some(v)) => Some(*v as f64),
        ScalarValue::Float32(Some(v)) => Some(*v as f64),
        ScalarValue::Float64(Some(v)) => Some(*v),
        _ => None,
    }
}

/// Helper to compare two ScalarValues for numeric comparisons
fn compare_numeric(
    left: &ScalarValue,
    right: &ScalarValue,
    op: &str,
) -> Result<bool, KalamDbError> {
    let left_num = as_f64(left).ok_or_else(|| {
        KalamDbError::InvalidOperation(format!("Cannot convert {:?} to number", left))
    })?;

    let right_num = as_f64(right).ok_or_else(|| {
        KalamDbError::InvalidOperation(format!("Cannot convert {:?} to number", right))
    })?;

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
/// - Literals (e.g., 'user1', 123, true) → convert to ScalarValue
fn extract_value(expr: &Expr, row_data: &Row) -> Result<ScalarValue, KalamDbError> {
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

        // Literal value: convert to ScalarValue
        Expr::Value(v) => match &v.value {
            Value::SingleQuotedString(s) | Value::DoubleQuotedString(s) => {
                Ok(ScalarValue::Utf8(Some(s.clone())))
            }
            Value::Number(n, _) => {
                // Try parsing as i64 first, then f64
                if let Ok(i) = n.parse::<i64>() {
                    Ok(ScalarValue::Int64(Some(i)))
                } else if let Ok(f) = n.parse::<f64>() {
                    Ok(ScalarValue::Float64(Some(f)))
                } else {
                    Err(KalamDbError::InvalidOperation(format!(
                        "Invalid number: {}",
                        n
                    )))
                }
            }
            Value::Boolean(b) => Ok(ScalarValue::Boolean(Some(*b))),
            Value::Null => Ok(ScalarValue::Null),
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn to_row(value: serde_json::Value) -> Row {
        let object = value
            .as_object()
            .expect("test rows must be JSON objects")
            .clone();

        let mut values = BTreeMap::new();
        for (key, value) in object {
            let scalar = match value {
                serde_json::Value::Null => ScalarValue::Null,
                serde_json::Value::Bool(b) => ScalarValue::Boolean(Some(b)),
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        ScalarValue::Int64(Some(i))
                    } else if let Some(u) = n.as_u64() {
                        ScalarValue::UInt64(Some(u))
                    } else if let Some(f) = n.as_f64() {
                        ScalarValue::Float64(Some(f))
                    } else {
                        panic!("unsupported numeric literal in test row")
                    }
                }
                serde_json::Value::String(s) => ScalarValue::Utf8(Some(s)),
                other => panic!("unsupported value in test row: {:?}", other),
            };
            values.insert(key, scalar);
        }

        Row::new(values)
    }

    #[test]
    fn test_simple_equality_filter() {
        let expr = parse_where_clause("user_id = 'user1'").unwrap();

        let matching_row = to_row(json!({"user_id": "user1", "text": "Hello"}));
        assert!(matches(&expr, &matching_row).unwrap());

        let non_matching_row = to_row(json!({"user_id": "user2", "text": "Hello"}));
        assert!(!matches(&expr, &non_matching_row).unwrap());
    }

    #[test]
    fn test_and_filter() {
        let expr = parse_where_clause("user_id = 'user1' AND read = false").unwrap();

        let matching_row = to_row(json!({"user_id": "user1", "read": false}));
        assert!(matches(&expr, &matching_row).unwrap());

        let non_matching_row1 = to_row(json!({"user_id": "user2", "read": false}));
        assert!(!matches(&expr, &non_matching_row1).unwrap());

        let non_matching_row2 = to_row(json!({"user_id": "user1", "read": true}));
        assert!(!matches(&expr, &non_matching_row2).unwrap());
    }

    #[test]
    fn test_or_filter() {
        let expr = parse_where_clause("status = 'active' OR status = 'pending'").unwrap();

        let matching_row1 = to_row(json!({"status": "active"}));
        assert!(matches(&expr, &matching_row1).unwrap());

        let matching_row2 = to_row(json!({"status": "pending"}));
        assert!(matches(&expr, &matching_row2).unwrap());

        let non_matching_row = to_row(json!({"status": "completed"}));
        assert!(!matches(&expr, &non_matching_row).unwrap());
    }

    #[test]
    fn test_numeric_comparison() {
        let expr = parse_where_clause("age >= 18").unwrap();

        let matching_row = to_row(json!({"age": 25}));
        assert!(matches(&expr, &matching_row).unwrap());

        let non_matching_row = to_row(json!({"age": 15}));
        assert!(!matches(&expr, &non_matching_row).unwrap());

        let edge_case = to_row(json!({"age": 18}));
        assert!(matches(&expr, &edge_case).unwrap());
    }

    #[test]
    fn test_not_filter() {
        let expr = parse_where_clause("NOT (deleted = true)").unwrap();

        let matching_row = to_row(json!({"deleted": false}));
        assert!(matches(&expr, &matching_row).unwrap());

        let non_matching_row = to_row(json!({"deleted": true}));
        assert!(!matches(&expr, &non_matching_row).unwrap());
    }

    #[test]
    fn test_complex_filter() {
        let expr =
            parse_where_clause("(user_id = 'user1' OR user_id = 'user2') AND read = false")
                .unwrap();

        let matching_row1 = to_row(json!({"user_id": "user1", "read": false}));
        assert!(matches(&expr, &matching_row1).unwrap());

        let matching_row2 = to_row(json!({"user_id": "user2", "read": false}));
        assert!(matches(&expr, &matching_row2).unwrap());

        let non_matching_row1 = to_row(json!({"user_id": "user3", "read": false}));
        assert!(!matches(&expr, &non_matching_row1).unwrap());

        let non_matching_row2 = to_row(json!({"user_id": "user1", "read": true}));
        assert!(!matches(&expr, &non_matching_row2).unwrap());
    }
}
