//! Data models for the 7 system tables
//!
//! These structs represent rows in KalamDB's system tables stored in RocksDB.

use serde::{Deserialize, Serialize};

/// User in system_users table
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct User {
    pub user_id: String,
    pub username: String,
    pub email: String,
    pub created_at: i64, // Unix timestamp
    /// Storage mode: 'table' (use table's storage) or 'region' (use user's storage)
    pub storage_mode: Option<String>, // T163c: ENUM ('table', 'region'), NULL defaults to 'table'
    /// Storage ID for user-level storage (used when storage_mode='region')
    pub storage_id: Option<String>, // T163c: FK to system.storages
    /// API key for authentication (auto-generated UUID v4)
    pub apikey: String, // Feature 006: X-API-KEY authentication
    /// User role for authorization (admin, user, readonly)
    pub role: String, // Feature 006: Role-based access control
}

/// Live query subscription in system_live_queries table
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LiveQuery {
    pub live_id: String, // PK: {user_id}-{unique_conn_id}-{table_name}-{query_id}
    pub connection_id: String,
    #[serde(default)]
    pub namespace_id: String,
    pub table_name: String,
    pub query_id: String,
    pub user_id: String,
    pub query: String,
    pub options: String, // JSON
    pub created_at: i64,
    #[serde(default)]
    pub last_update: i64,
    pub changes: i64,
    pub node: String,
}

/// Background job in system_jobs table
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Job {
    pub job_id: String, // PK
    pub job_type: String,
    pub status: String, // "running", "completed", "failed"
    #[serde(default)]
    pub table_name: Option<String>,
    #[serde(default)]
    pub parameters: Vec<String>, // JSON array
    pub result: Option<String>,
    pub trace: Option<String>,
    #[serde(default)]
    pub memory_used: Option<i64>, // bytes
    #[serde(default)]
    pub cpu_used: Option<i64>, // microseconds
    #[serde(default)]
    pub created_at: i64,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub node_id: String,
    pub error_message: Option<String>,
}

/// Namespace in system_namespaces table
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Namespace {
    pub namespace_id: String, // PK
    pub name: String,
    pub created_at: i64,
    pub options: String, // JSON
    pub table_count: i32,
}

/// Table metadata in system_tables table
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Table {
    pub table_id: String, // PK
    pub table_name: String,
    pub namespace: String,
    pub table_type: String, // "user", "shared", "system", "stream"
    pub created_at: i64,
    pub storage_location: String,   //TODO: Remove in favor of storage_id
    pub storage_id: Option<String>, // T167: FK to system.storages
    pub use_user_storage: bool,     // T168: Allow per-user storage override
    pub flush_policy: String,       // JSON
    pub schema_version: i32,
    pub deleted_retention_hours: i32,
}

/// Table schema version in system_table_schemas table
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TableSchema {
    pub schema_id: String, // PK
    pub table_id: String,
    pub version: i32,
    pub arrow_schema: String, // Arrow schema as JSON
    pub created_at: i64,
    pub changes: String, // JSON array of schema changes
}

/// Storage configuration in system_storages table
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Storage {
    pub storage_id: String, // PK
    pub storage_name: String,
    pub description: Option<String>,
    pub storage_type: String, // "filesystem" or "s3"
    pub base_directory: String,
    #[serde(default)]
    pub credentials: Option<String>,
    pub shared_tables_template: String,
    pub user_tables_template: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_serialization() {
        let user = User {
            user_id: "user123".to_string(),
            username: "alice".to_string(),
            email: "alice@example.com".to_string(),
            created_at: 1697500000,
            storage_mode: Some("table".to_string()),
            storage_id: None,
            apikey: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            role: "user".to_string(),
        };

        let json = serde_json::to_string(&user).unwrap();
        let deserialized: User = serde_json::from_str(&json).unwrap();
        assert_eq!(user, deserialized);
    }

    #[test]
    fn test_namespace_serialization() {
        let ns = Namespace {
            namespace_id: "ns1".to_string(),
            name: "my_app".to_string(),
            created_at: 1697500000,
            options: "{}".to_string(),
            table_count: 0,
        };

        let json = serde_json::to_string(&ns).unwrap();
        let deserialized: Namespace = serde_json::from_str(&json).unwrap();
        assert_eq!(ns, deserialized);
    }

    #[test]
    fn test_table_schema_serialization() {
        let schema = TableSchema {
            schema_id: "schema1".to_string(),
            table_id: "table1".to_string(),
            version: 1,
            arrow_schema: "{\"fields\":[]}".to_string(),
            created_at: 1697500000,
            changes: "[]".to_string(),
        };

        let json = serde_json::to_string(&schema).unwrap();
        let deserialized: TableSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(schema, deserialized);
    }
}
