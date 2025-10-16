//! SQL request model for the REST API
//!
//! This module defines the structure for SQL query requests sent to the `/api/sql` endpoint.

use serde::{Deserialize, Serialize};

/// Request payload for SQL execution via REST API
///
/// # Example
/// ```json
/// {
///   "sql": "SELECT * FROM messages WHERE user_id = CURRENT_USER()"
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlRequest {
    /// The SQL statement(s) to execute. Multiple statements can be separated by semicolons.
    pub sql: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sql_request_serialization() {
        let request = SqlRequest {
            sql: "SELECT * FROM users".to_string(),
        };
        
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("SELECT * FROM users"));
        
        let deserialized: SqlRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.sql, "SELECT * FROM users");
    }
    
    #[test]
    fn test_sql_request_with_multiple_statements() {
        let request = SqlRequest {
            sql: "INSERT INTO users VALUES (1, 'Alice'); SELECT * FROM users;".to_string(),
        };
        
        let json = serde_json::to_string(&request).unwrap();
        let deserialized: SqlRequest = serde_json::from_str(&json).unwrap();
        assert!(deserialized.sql.contains("INSERT"));
        assert!(deserialized.sql.contains("SELECT"));
    }
}
