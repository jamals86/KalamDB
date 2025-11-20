//! SQL response model
//!
//! This module defines the structure for SQL execution responses from the `/v1/api/sql` endpoint.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Execution status enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ResponseStatus {
    Success,
    Error,
}

impl std::fmt::Display for ResponseStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResponseStatus::Success => write!(f, "success"),
            ResponseStatus::Error => write!(f, "error"),
        }
    }
}

/// Response from SQL execution via REST API
///
/// Contains execution status, results, timing information, and any errors that occurred.
///
/// # Example Success Response
/// ```json
/// {
///   "status": "success",
///   "results": [
///     {
///       "rows": [
///         {"id": 1, "name": "Alice"},
///         {"id": 2, "name": "Bob"}
///       ],
///       "row_count": 2,
///       "columns": ["id", "name"]
///     }
///   ],
///   "took": 15.0,
///   "error": null
/// }
/// ```
///
/// # Example Error Response
/// ```json
/// {
///   "status": "error",
///   "results": [],
///   "took": 5.0,
///   "error": {
///     "code": "INVALID_SQL",
///     "message": "Syntax error near 'SELCT'"
///   }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlResponse {
    /// Overall execution status: "success" or "error"
    pub status: ResponseStatus,

    /// Array of result sets, one per executed statement
    pub results: Vec<QueryResult>,

    /// Total execution time in milliseconds (with fractional precision)
    pub took: f64,

    /// Error details if status is "error", otherwise null
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorDetail>,
}

/// Individual query result within a SQL response
///
/// Each executed SQL statement produces one QueryResult.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// The result rows as JSON objects (each row is a key-value map)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<Vec<HashMap<String, serde_json::Value>>>,

    /// Number of rows affected (for INSERT/UPDATE/DELETE) or returned (for SELECT)
    pub row_count: usize,

    /// Column names in the result set
    pub columns: Vec<String>,

    /// Optional message for non-query statements (e.g., "Table created successfully")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Error details for failed SQL execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetail {
    /// Error code (e.g., "INVALID_SQL", "TABLE_NOT_FOUND", "PERMISSION_DENIED")
    pub code: String,

    /// Human-readable error message
    pub message: String,

    /// Optional detailed context (e.g., line number, column position)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl SqlResponse {
    /// Create a successful response with results
    pub fn success(results: Vec<QueryResult>, took: f64) -> Self {
        Self {
            status: ResponseStatus::Success,
            results,
            took,
            error: None,
        }
    }

    /// Create an error response
    pub fn error(code: &str, message: &str, took: f64) -> Self {
        Self {
            status: ResponseStatus::Error,
            results: Vec::new(),
            took,
            error: Some(ErrorDetail {
                code: code.to_string(),
                message: message.to_string(),
                details: None,
            }),
        }
    }

    /// Create an error response with additional details
    pub fn error_with_details(code: &str, message: &str, details: &str, took: f64) -> Self {
        Self {
            status: ResponseStatus::Error,
            results: Vec::new(),
            took,
            error: Some(ErrorDetail {
                code: code.to_string(),
                message: message.to_string(),
                details: Some(details.to_string()),
            }),
        }
    }
}

impl QueryResult {
    /// Create a result for a SELECT query with rows
    pub fn with_rows(rows: Vec<HashMap<String, serde_json::Value>>, columns: Vec<String>) -> Self {
        let row_count = rows.len();
        Self {
            rows: Some(rows),
            row_count,
            columns,
            message: None,
        }
    }

    /// Create a result for a DML statement (INSERT/UPDATE/DELETE)
    pub fn with_affected_rows(row_count: usize, message: Option<String>) -> Self {
        Self {
            rows: None,
            row_count,
            columns: Vec::new(),
            message,
        }
    }

    /// Create a result for a DDL statement (CREATE/ALTER/DROP)
    pub fn with_message(message: String) -> Self {
        Self {
            rows: None,
            row_count: 0,
            columns: Vec::new(),
            message: Some(message),
        }
    }

    /// Create a result for a SUBSCRIBE TO statement
    ///
    /// Returns subscription metadata as a single row result
    pub fn subscription(subscription_data: serde_json::Value) -> Self {
        // Convert subscription JSON to a single row
        let mut row = HashMap::new();
        if let serde_json::Value::Object(map) = subscription_data {
            for (key, value) in map {
                row.insert(key, value);
            }
        }

        Self {
            rows: Some(vec![row]),
            row_count: 1,
            columns: vec![
                "status".to_string(),
                "ws_url".to_string(),
                "subscription".to_string(),
                "message".to_string(),
            ],
            message: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_success_response_serialization() {
        let mut row1 = HashMap::new();
        row1.insert("id".to_string(), serde_json::json!(1));
        row1.insert("name".to_string(), serde_json::json!("Alice"));

        let result = QueryResult::with_rows(vec![row1], vec!["id".to_string(), "name".to_string()]);

        let response = SqlResponse::success(vec![result], 15.0);

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("success"));
        assert!(json.contains("Alice"));
        assert!(json.contains("15"));
    }

    #[test]
    fn test_error_response_serialization() {
        let response = SqlResponse::error("INVALID_SQL", "Syntax error", 5.0);

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("error"));
        assert!(json.contains("INVALID_SQL"));
        assert!(json.contains("Syntax error"));
    }

    #[test]
    fn test_query_result_with_message() {
        let result = QueryResult::with_message("Table created successfully".to_string());

        assert_eq!(result.row_count, 0);
        assert!(result.rows.is_none());
        assert_eq!(
            result.message,
            Some("Table created successfully".to_string())
        );
    }

    #[test]
    fn test_query_result_with_affected_rows() {
        let result = QueryResult::with_affected_rows(5, Some("5 rows inserted".to_string()));

        assert_eq!(result.row_count, 5);
        assert!(result.rows.is_none());
        assert_eq!(result.message, Some("5 rows inserted".to_string()));
    }
}
