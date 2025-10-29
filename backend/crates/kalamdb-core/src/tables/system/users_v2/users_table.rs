//! System.users table schema
//!
//! This module defines the Arrow schema for the system.users table.
//! Uses OnceLock for zero-overhead static schema caching.

use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use std::sync::{Arc, OnceLock};

/// Static schema cache for the users table
static USERS_SCHEMA: OnceLock<Arc<Schema>> = OnceLock::new();

/// System users table schema definition
pub struct UsersTableSchema;

impl UsersTableSchema {
    /// Get the cached schema for the system.users table
    /// 
    /// Uses OnceLock to ensure the schema is created exactly once and reused
    /// across all providers without synchronization overhead.
    pub fn schema() -> SchemaRef {
        USERS_SCHEMA
            .get_or_init(|| {
                Arc::new(Schema::new(vec![
                    Field::new("user_id", DataType::Utf8, false),
                    Field::new("username", DataType::Utf8, false),
                    Field::new("password_hash", DataType::Utf8, false),
                    Field::new("role", DataType::Utf8, false),
                    Field::new("email", DataType::Utf8, true),
                    Field::new("auth_type", DataType::Utf8, false),
                    Field::new("auth_data", DataType::Utf8, true),
                    Field::new(
                        "created_at",
                        DataType::Timestamp(TimeUnit::Millisecond, None),
                        false,
                    ),
                    Field::new(
                        "updated_at",
                        DataType::Timestamp(TimeUnit::Millisecond, None),
                        false,
                    ),
                    Field::new(
                        "last_seen",
                        DataType::Timestamp(TimeUnit::Millisecond, None),
                        true,
                    ),
                    Field::new(
                        "deleted_at",
                        DataType::Timestamp(TimeUnit::Millisecond, None),
                        true,
                    ),
                ]))
            })
            .clone()
    }

    /// Get the table name
    pub fn table_name() -> &'static str {
        "users"
    }

    /// Get the column family name in RocksDB
    pub fn column_family_name() -> &'static str {
        "system_users"
    }

    /// Get the partition key for storage
    pub fn partition() -> &'static str {
        "system_users"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_users_table_schema() {
        let schema = UsersTableSchema::schema();
        assert_eq!(schema.fields().len(), 11);
        assert_eq!(schema.field(0).name(), "user_id");
        assert_eq!(schema.field(1).name(), "username");
        assert_eq!(schema.field(2).name(), "password_hash");
        assert_eq!(schema.field(3).name(), "role");
        assert_eq!(schema.field(4).name(), "email");
        assert_eq!(schema.field(5).name(), "auth_type");
        assert_eq!(schema.field(6).name(), "auth_data");
        assert_eq!(schema.field(7).name(), "created_at");
        assert_eq!(schema.field(8).name(), "updated_at");
        assert_eq!(schema.field(9).name(), "last_seen");
        assert_eq!(schema.field(10).name(), "deleted_at");
    }

    #[test]
    fn test_users_table_name() {
        assert_eq!(UsersTableSchema::table_name(), "users");
        assert_eq!(UsersTableSchema::column_family_name(), "system_users");
    }

    #[test]
    fn test_schema_caching() {
        // Verify schema is the same instance (Arc pointer equality)
        let schema1 = UsersTableSchema::schema();
        let schema2 = UsersTableSchema::schema();
        assert!(Arc::ptr_eq(&schema1, &schema2));
    }
}
