//! WebSocket notification models
//!
//! This module defines the structure for live query change notifications sent to clients.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Notification message sent to clients for live query updates
///
/// # Example Initial Data
/// ```json
/// {
///   "type": "initial_data",
///   "subscription_id": "sub-1",
///   "rows": [
///     {"id": 1, "message": "Hello"},
///     {"id": 2, "message": "World"}
///   ]
/// }
/// ```
///
/// # Example Change Notification (INSERT)
/// ```json
/// {
///   "type": "change",
///   "subscription_id": "sub-1",
///   "change_type": "insert",
///   "rows": [
///     {"id": 3, "message": "New message"}
///   ]
/// }
/// ```
///
/// # Example Change Notification (UPDATE)
/// ```json
/// {
///   "type": "change",
///   "subscription_id": "sub-1",
///   "change_type": "update",
///   "rows": [
///     {"id": 2, "message": "Updated message"}
///   ],
///   "old_values": [
///     {"id": 2, "message": "World"}
///   ]
/// }
/// ```
///
/// # Example Change Notification (DELETE)
/// ```json
/// {
///   "type": "change",
///   "subscription_id": "sub-1",
///   "change_type": "delete",
///   "old_values": [
///     {"id": 1, "message": "Hello"}
///   ]
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Notification {
    /// Initial data fetch response (sent after subscription registration)
    InitialData {
        /// The subscription ID this notification is for
        subscription_id: String,
        
        /// The initial rows matching the query
        rows: Vec<HashMap<String, serde_json::Value>>,
    },
    
    /// Change notification for INSERT/UPDATE/DELETE operations
    Change {
        /// The subscription ID this notification is for
        subscription_id: String,
        
        /// Type of change: "insert", "update", or "delete"
        change_type: ChangeType,
        
        /// New/current row values (for INSERT and UPDATE)
        #[serde(skip_serializing_if = "Option::is_none")]
        rows: Option<Vec<HashMap<String, serde_json::Value>>>,
        
        /// Previous row values (for UPDATE and DELETE)
        #[serde(skip_serializing_if = "Option::is_none")]
        old_values: Option<Vec<HashMap<String, serde_json::Value>>>,
    },
    
    /// Error notification (e.g., subscription query failed)
    Error {
        /// The subscription ID this error is for
        subscription_id: String,
        
        /// Error code
        code: String,
        
        /// Error message
        message: String,
    },
}

/// Type of change that occurred in the database
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChangeType {
    /// New row(s) inserted
    Insert,
    
    /// Existing row(s) updated
    Update,
    
    /// Row(s) deleted
    Delete,
}

impl Notification {
    /// Create an initial data notification
    pub fn initial_data(subscription_id: String, rows: Vec<HashMap<String, serde_json::Value>>) -> Self {
        Self::InitialData {
            subscription_id,
            rows,
        }
    }
    
    /// Create an INSERT change notification
    pub fn insert(subscription_id: String, rows: Vec<HashMap<String, serde_json::Value>>) -> Self {
        Self::Change {
            subscription_id,
            change_type: ChangeType::Insert,
            rows: Some(rows),
            old_values: None,
        }
    }
    
    /// Create an UPDATE change notification
    pub fn update(
        subscription_id: String,
        new_rows: Vec<HashMap<String, serde_json::Value>>,
        old_rows: Vec<HashMap<String, serde_json::Value>>,
    ) -> Self {
        Self::Change {
            subscription_id,
            change_type: ChangeType::Update,
            rows: Some(new_rows),
            old_values: Some(old_rows),
        }
    }
    
    /// Create a DELETE change notification
    pub fn delete(subscription_id: String, old_rows: Vec<HashMap<String, serde_json::Value>>) -> Self {
        Self::Change {
            subscription_id,
            change_type: ChangeType::Delete,
            rows: None,
            old_values: Some(old_rows),
        }
    }
    
    /// Create an error notification
    pub fn error(subscription_id: String, code: String, message: String) -> Self {
        Self::Error {
            subscription_id,
            code,
            message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_data_notification() {
        let mut row = HashMap::new();
        row.insert("id".to_string(), serde_json::json!(1));
        row.insert("message".to_string(), serde_json::json!("Hello"));
        
        let notification = Notification::initial_data("sub-1".to_string(), vec![row]);
        
        let json = serde_json::to_string(&notification).unwrap();
        assert!(json.contains("initial_data"));
        assert!(json.contains("sub-1"));
        assert!(json.contains("Hello"));
    }
    
    #[test]
    fn test_insert_notification() {
        let mut row = HashMap::new();
        row.insert("id".to_string(), serde_json::json!(2));
        
        let notification = Notification::insert("sub-1".to_string(), vec![row]);
        
        let json = serde_json::to_string(&notification).unwrap();
        assert!(json.contains("change"));
        assert!(json.contains("insert"));
        assert!(!json.contains("old_values"));
    }
    
    #[test]
    fn test_update_notification() {
        let mut new_row = HashMap::new();
        new_row.insert("id".to_string(), serde_json::json!(1));
        new_row.insert("message".to_string(), serde_json::json!("Updated"));
        
        let mut old_row = HashMap::new();
        old_row.insert("id".to_string(), serde_json::json!(1));
        old_row.insert("message".to_string(), serde_json::json!("Original"));
        
        let notification = Notification::update("sub-1".to_string(), vec![new_row], vec![old_row]);
        
        let json = serde_json::to_string(&notification).unwrap();
        assert!(json.contains("update"));
        assert!(json.contains("Updated"));
        assert!(json.contains("Original"));
    }
    
    #[test]
    fn test_delete_notification() {
        let mut old_row = HashMap::new();
        old_row.insert("id".to_string(), serde_json::json!(1));
        
        let notification = Notification::delete("sub-1".to_string(), vec![old_row]);
        
        let json = serde_json::to_string(&notification).unwrap();
        assert!(json.contains("delete"));
        assert!(json.contains("old_values"));
        assert!(!json.contains("\"rows\""));
    }
    
    #[test]
    fn test_error_notification() {
        let notification = Notification::error(
            "sub-1".to_string(),
            "INVALID_QUERY".to_string(),
            "Query syntax error".to_string(),
        );
        
        let json = serde_json::to_string(&notification).unwrap();
        assert!(json.contains("error"));
        assert!(json.contains("INVALID_QUERY"));
    }
}
