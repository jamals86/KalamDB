//! Table row models for user, shared, and stream tables.
//!
//! This module contains models for dynamic table rows:
//! - `UserTableRow`: Row in a user table (user-owned data)
//! - `SharedTableRow`: Row in a shared table (cross-user data with access control)
//! - `StreamTableRow`: Row in a stream table (ephemeral time-series data)
//!
//! All table row models use JSON serialization for flexibility with dynamic fields.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Row in a user table (user-owned data).
///
/// User tables have dynamic schemas - the `fields` map contains user-defined columns.
/// System columns (`_updated`, `_deleted`) are injected during serialization.
///
/// ## Fields
/// - `fields`: User-defined fields as a JSON object
/// - `_updated`: System column - timestamp of last modification (ISO 8601)
/// - `_deleted`: System column - soft delete flag
///
/// ## Example
///
/// ```rust,ignore
/// use kalamdb_core::models::UserTableRow;
/// use serde_json::json;
///
/// let row = UserTableRow {
///     fields: json!({
///         "name": "Alice",
///         "age": 30,
///         "city": "NYC"
///     }).as_object().unwrap().clone(),
///     _updated: "2025-10-27T10:00:00Z".to_string(),
///     _deleted: false,
/// };
/// ```
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct UserTableRow {
    #[serde(flatten)]
    pub fields: Map<String, Value>, // User-defined dynamic fields
    pub _updated: String, // System column: ISO 8601 timestamp
    pub _deleted: bool,   // System column: soft delete flag
}

/// Row in a shared table (cross-user data with access control).
///
/// Shared tables are similar to user tables but with additional access control.
/// The `access_level` determines who can read/write the row.
///
/// ## Fields
/// - `fields`: User-defined fields as a JSON object
/// - `access_level`: Access level (public, private, restricted)
/// - `_updated`: System column - timestamp of last modification (ISO 8601)
/// - `_deleted`: System column - soft delete flag
///
/// ## Example
///
/// ```rust,ignore
/// use kalamdb_core::models::SharedTableRow;
/// use serde_json::json;
///
/// let row = SharedTableRow {
///     fields: json!({
///         "title": "Shared Document",
///         "content": "..."
///     }).as_object().unwrap().clone(),
///     access_level: "public".to_string(),
///     _updated: "2025-10-27T10:00:00Z".to_string(),
///     _deleted: false,
/// };
/// ```
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SharedTableRow {
    #[serde(flatten)]
    pub fields: Map<String, Value>, // User-defined dynamic fields
    pub access_level: kalamdb_commons::TableAccess, // public, private, restricted
    pub _updated: String,                           // System column: ISO 8601 timestamp
    pub _deleted: bool,                             // System column: soft delete flag
}

/// Row in a stream table (ephemeral time-series data).
///
/// Stream tables store ephemeral events with TTL-based eviction.
/// Rows are automatically deleted after the TTL expires.
///
/// ## Fields
/// - `fields`: User-defined fields as a JSON object
/// - `ttl_seconds`: Time-to-live in seconds (after which row is evicted)
/// - `inserted_at`: Timestamp when row was inserted (ISO 8601)
/// - `_updated`: System column - timestamp of last modification (ISO 8601)
/// - `_deleted`: System column - soft delete flag
///
/// ## Example
///
/// ```rust,ignore
/// use kalamdb_core::models::StreamTableRow;
/// use serde_json::json;
///
/// let row = StreamTableRow {
///     fields: json!({
///         "event_type": "click",
///         "user_id": "u_123",
///         "timestamp": 1698422400
///     }).as_object().unwrap().clone(),
///     ttl_seconds: 3600, // 1 hour
///     inserted_at: "2025-10-27T10:00:00Z".to_string(),
///     _updated: "2025-10-27T10:00:00Z".to_string(),
///     _deleted: false,
/// };
/// ```
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StreamTableRow {
    #[serde(flatten)]
    pub fields: Map<String, Value>, // User-defined dynamic fields
    pub ttl_seconds: u64,    // Time-to-live in seconds
    pub inserted_at: String, // ISO 8601 timestamp
    pub _updated: String,    // System column: ISO 8601 timestamp
    pub _deleted: bool,      // System column: soft delete flag
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_user_table_row_serialization() {
        let row = UserTableRow {
            fields: json!({
                "name": "Alice",
                "age": 30
            })
            .as_object()
            .unwrap()
            .clone(),
            _updated: "2025-10-27T10:00:00Z".to_string(),
            _deleted: false,
        };

        // Test JSON serialization
        let json_str = serde_json::to_string(&row).unwrap();
        let deserialized: UserTableRow = serde_json::from_str(&json_str).unwrap();
        assert_eq!(row, deserialized);

        // Verify fields are flattened in JSON
        let json_value: Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(json_value["name"], "Alice");
        assert_eq!(json_value["age"], 30);
        assert_eq!(json_value["_updated"], "2025-10-27T10:00:00Z");
        assert_eq!(json_value["_deleted"], false);
    }

    #[test]
    fn test_shared_table_row_serialization() {
        let row = SharedTableRow {
            fields: json!({
                "title": "Document",
                "content": "Test content"
            })
            .as_object()
            .unwrap()
            .clone(),
            access_level: kalamdb_commons::TableAccess::Public,
            _updated: "2025-10-27T10:00:00Z".to_string(),
            _deleted: false,
        };

        // Test JSON serialization
        let json_str = serde_json::to_string(&row).unwrap();
        let deserialized: SharedTableRow = serde_json::from_str(&json_str).unwrap();
        assert_eq!(row, deserialized);
    }

    #[test]
    fn test_stream_table_row_serialization() {
        let row = StreamTableRow {
            fields: json!({
                "event": "click",
                "user": "u_123"
            })
            .as_object()
            .unwrap()
            .clone(),
            ttl_seconds: 3600,
            inserted_at: "2025-10-27T10:00:00Z".to_string(),
            _updated: "2025-10-27T10:00:00Z".to_string(),
            _deleted: false,
        };

        // Test JSON serialization
        let json_str = serde_json::to_string(&row).unwrap();
        let deserialized: StreamTableRow = serde_json::from_str(&json_str).unwrap();
        assert_eq!(row, deserialized);
    }
}
