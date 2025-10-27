//! System.users table schema
//!
//! This module defines the schema for the system.users table.

use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use std::sync::Arc;

/// System users table schema
pub struct UsersTable;

impl UsersTable {
    /// Get the schema for the system.users table
    pub fn schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("user_id", DataType::Utf8, false),
            Field::new("username", DataType::Utf8, false),
            Field::new("password_hash", DataType::Utf8, false),
            Field::new("role", DataType::Utf8, false),
            Field::new("email", DataType::Utf8, true),
            Field::new("auth_type", DataType::Utf8, false),
            Field::new("auth_data", DataType::Utf8, true),
            Field::new("api_key", DataType::Utf8, true),
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
    }

    /// Get the table name
    pub fn table_name() -> &'static str {
        "users"
    }

    /// Get the column family name
    pub fn column_family_name() -> String {
        format!("system_{}", Self::table_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_users_table_schema() {
        let schema = UsersTable::schema();
        assert_eq!(schema.fields().len(), 12);
        assert_eq!(schema.field(0).name(), "user_id");
        assert_eq!(schema.field(1).name(), "username");
        assert_eq!(schema.field(2).name(), "password_hash");
        assert_eq!(schema.field(3).name(), "role");
        assert_eq!(schema.field(4).name(), "email");
        assert_eq!(schema.field(5).name(), "auth_type");
        assert_eq!(schema.field(6).name(), "auth_data");
        assert_eq!(schema.field(7).name(), "api_key");
        assert_eq!(schema.field(8).name(), "created_at");
        assert_eq!(schema.field(9).name(), "updated_at");
        assert_eq!(schema.field(10).name(), "last_seen");
        assert_eq!(schema.field(11).name(), "deleted_at");
    }

    #[test]
    fn test_users_table_name() {
        assert_eq!(UsersTable::table_name(), "users");
        assert_eq!(UsersTable::column_family_name(), "system_users");
    }
}
