//! SQL request model
//!
//! This module defines the structure for SQL query requests sent to the `/v1/api/sql` endpoint.

use kalamdb_core::providers::arrow_json_conversion::SerializationMode;
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
///
/// # Example (with namespace)
/// ```json
/// {
///   "sql": "SELECT * FROM messages",
///   "namespace_id": "chat"
/// }
/// ```
///
/// # Example (with typed serialization mode for SDKs)
/// ```json
/// {
///   "sql": "SELECT * FROM messages",
///   "serialization_mode": "typed"
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRequest {
    /// The SQL statement(s) to execute. Multiple statements can be separated by semicolons.
    pub sql: String,

    /// Optional query parameters for parameterized queries ($1, $2, ...).
    /// Parameters are validated (max 50 params, 512KB each) before execution.
    /// Supported types: null, bool, integers, floats, strings, timestamps.
    #[serde(default)]
    pub params: Option<Vec<JsonValue>>,

    /// Optional namespace ID for unqualified table names.
    /// When set, queries like `SELECT * FROM users` resolve to `namespace_id.users`.
    /// Set via `USE namespace` command in CLI clients.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace_id: Option<String>,

    /// Serialization mode for response data.
    ///
    /// - `simple` (default for REST API): Plain JSON values, Int64/UInt64 as strings
    ///   Example: `{"id": "123", "name": "Alice"}`
    ///
    /// - `typed`: Values with type wrappers and formatted timestamps
    ///   Example: `{"id": {"Int64": "123"}, "created_at": {"TimestampMicrosecond": {"value": 123, "formatted": "2025-12-14T..."}}}`
    ///
    /// Use `typed` mode for type-safe SDKs (kalam-link) that need type information.
    #[serde(default = "default_rest_serialization_mode")]
    pub serialization_mode: SerializationMode,
}

fn default_rest_serialization_mode() -> SerializationMode {
    SerializationMode::Simple
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_request_serialization() {
        let request = QueryRequest {
            sql: "SELECT * FROM users".to_string(),
            params: None,
            namespace_id: None,
            serialization_mode: SerializationMode::Simple,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("SELECT * FROM users"));

        let deserialized: QueryRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.sql, "SELECT * FROM users");
        assert!(deserialized.params.is_none());
    }

    #[test]
    fn test_query_request_with_multiple_statements() {
        let request = QueryRequest {
            sql: "INSERT INTO users VALUES (1, 'Alice'); SELECT * FROM users;".to_string(),
            params: None,
            namespace_id: None,
            serialization_mode: SerializationMode::Simple,
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: QueryRequest = serde_json::from_str(&json).unwrap();
        assert!(deserialized.sql.contains("INSERT"));
        assert!(deserialized.sql.contains("SELECT"));
    }

    #[test]
    fn test_query_request_with_parameters() {
        let request = QueryRequest {
            sql: "SELECT * FROM users WHERE id = $1 AND status = $2".to_string(),
            params: Some(vec![serde_json::json!(42), serde_json::json!("active")]),
            namespace_id: None,
            serialization_mode: SerializationMode::Simple,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("$1"));
        assert!(json.contains("params"));

        let deserialized: QueryRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.params.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_query_request_params_optional() {
        // Test that params field is optional (serde default)
        let json = r#"{"sql": "SELECT * FROM users"}"#;
        let deserialized: QueryRequest = serde_json::from_str(json).unwrap();
        assert_eq!(deserialized.sql, "SELECT * FROM users");
        assert!(deserialized.params.is_none());
        assert_eq!(deserialized.serialization_mode, SerializationMode::Simple);
    }

    #[test]
    fn test_query_request_with_namespace() {
        let json = r#"{"sql": "SELECT * FROM messages", "namespace_id": "chat"}"#;
        let deserialized: QueryRequest = serde_json::from_str(json).unwrap();
        assert_eq!(deserialized.sql, "SELECT * FROM messages");
        assert_eq!(deserialized.namespace_id, Some("chat".to_string()));
    }

    #[test]
    fn test_query_request_with_typed_serialization_mode() {
        let json = r#"{"sql": "SELECT * FROM messages", "serialization_mode": "typed"}"#;
        let deserialized: QueryRequest = serde_json::from_str(json).unwrap();
        assert_eq!(deserialized.sql, "SELECT * FROM messages");
        assert_eq!(deserialized.serialization_mode, SerializationMode::Typed);
    }

    #[test]
    fn test_query_request_serialization_mode_defaults_to_simple() {
        let json = r#"{"sql": "SELECT * FROM messages"}"#;
        let deserialized: QueryRequest = serde_json::from_str(json).unwrap();
        assert_eq!(deserialized.serialization_mode, SerializationMode::Simple);
    }
}
