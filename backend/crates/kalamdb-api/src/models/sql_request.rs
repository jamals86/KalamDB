//! SQL request model
//!
//! This module defines the structure for SQL query requests sent to the `/v1/api/sql` endpoint.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Request payload for SQL execution via REST API
///
/// # Example (without parameters)
/// ```json
/// {
///   "sql": "SELECT * FROM messages WHERE user_id = CURRENT_USER()"
/// }
/// ```
///
/// # Example (with parameters)
/// ```json
/// {
///   "sql": "SELECT * FROM users WHERE id = $1 AND status = $2",
///   "params": [42, "active"]
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlRequest {
    /// The SQL statement(s) to execute. Multiple statements can be separated by semicolons.
    pub sql: String,

    /// Optional query parameters for parameterized queries ($1, $2, ...).
    /// Parameters are validated (max 50 params, 512KB each) before execution.
    /// Supported types: null, bool, integers, floats, strings, timestamps.
    #[serde(default)]
    pub params: Option<Vec<JsonValue>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sql_request_serialization() {
        let request = SqlRequest {
            sql: "SELECT * FROM users".to_string(),
            params: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("SELECT * FROM users"));

        let deserialized: SqlRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.sql, "SELECT * FROM users");
        assert!(deserialized.params.is_none());
    }

    #[test]
    fn test_sql_request_with_multiple_statements() {
        let request = SqlRequest {
            sql: "INSERT INTO users VALUES (1, 'Alice'); SELECT * FROM users;".to_string(),
            params: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: SqlRequest = serde_json::from_str(&json).unwrap();
        assert!(deserialized.sql.contains("INSERT"));
        assert!(deserialized.sql.contains("SELECT"));
    }

    #[test]
    fn test_sql_request_with_parameters() {
        let request = SqlRequest {
            sql: "SELECT * FROM users WHERE id = $1 AND status = $2".to_string(),
            params: Some(vec![serde_json::json!(42), serde_json::json!("active")]),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("$1"));
        assert!(json.contains("params"));

        let deserialized: SqlRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.params.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_sql_request_params_optional() {
        // Test that params field is optional (serde default)
        let json = r#"{"sql": "SELECT * FROM users"}"#;
        let deserialized: SqlRequest = serde_json::from_str(json).unwrap();
        assert_eq!(deserialized.sql, "SELECT * FROM users");
        assert!(deserialized.params.is_none());
    }
}
