//! System.users table schema
//!
//! This module defines the schema for the system.users table.
//! - TableDefinition: Source of truth for columns, types, comments
//! - Arrow schema: Derived from TableDefinition, memoized via OnceLock

use crate::providers::users::models::User;
use datafusion::arrow::datatypes::SchemaRef;
use kalamdb_commons::schemas::TableDefinition;
use kalamdb_commons::SystemTable;
use std::sync::OnceLock;

/// System users table schema definition
///
/// Provides typed access to the table definition and Arrow schema.
/// Contains the full TableDefinition as the single source of truth.
#[derive(Debug, Clone, Copy)]
pub struct UsersTableSchema;

impl UsersTableSchema {
    /// Get the TableDefinition for system.users
    ///
    /// This is the single source of truth for:
    /// - Column definitions (names, types, nullability)
    /// - Column ordering (ordinal_position)
    /// - Column comments/descriptions
    ///
    /// Schema:
    /// - user_id TEXT PRIMARY KEY
    /// - username TEXT NOT NULL
    /// - password_hash TEXT NOT NULL
    /// - role TEXT NOT NULL
    /// - email TEXT (nullable)
    /// - auth_type TEXT NOT NULL
    /// - auth_data TEXT (nullable)
    /// - storage_mode TEXT NOT NULL
    /// - storage_id TEXT (nullable)
    /// - created_at TIMESTAMP NOT NULL
    /// - updated_at TIMESTAMP NOT NULL
    /// - last_seen TIMESTAMP (nullable)
    /// - deleted_at TIMESTAMP (nullable)
    /// - failed_login_attempts INT NOT NULL
    /// - locked_until TIMESTAMP (nullable)
    /// - last_login_at TIMESTAMP (nullable)
    pub fn definition() -> TableDefinition {
        User::definition()
    }

    /// Get the cached Arrow schema for the system.users table
    ///
    /// Ensures the schema is created exactly once and reused across the process.
    pub fn schema() -> SchemaRef {
        static SCHEMA: OnceLock<SchemaRef> = OnceLock::new();
        SCHEMA
            .get_or_init(|| {
                Self::definition()
                    .to_arrow_schema()
                    .expect("Failed to convert users TableDefinition to Arrow schema")
            })
            .clone()
    }

    /// Get the table name
    pub fn table_name() -> &'static str {
        SystemTable::Users.table_name()
    }

    /// Get the column family name in RocksDB
    pub fn column_family_name() -> &'static str {
        SystemTable::Users.column_family_name().expect("Users is a table, not a view")
    }

    /// Get the partition key for storage
    pub fn partition() -> &'static str {
        Self::column_family_name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_users_table_schema() {
        let schema = UsersTableSchema::schema();
        // Schema built from TableDefinition, verify field count and names are correct
        assert_eq!(schema.fields().len(), 16);

        // Verify fields exist (order guaranteed by TableDefinition's ordinal_position)
        let field_names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
        assert!(field_names.contains(&"user_id"));
        assert!(field_names.contains(&"username"));
        assert!(field_names.contains(&"password_hash"));
        assert!(field_names.contains(&"role"));
        assert!(field_names.contains(&"email"));
        assert!(field_names.contains(&"auth_type"));
        assert!(field_names.contains(&"auth_data"));
        assert!(field_names.contains(&"storage_mode"));
        assert!(field_names.contains(&"storage_id"));
        assert!(field_names.contains(&"created_at"));
        assert!(field_names.contains(&"updated_at"));
        assert!(field_names.contains(&"last_seen"));
        assert!(field_names.contains(&"deleted_at"));
    }

    #[test]
    fn test_users_table_name() {
        assert_eq!(UsersTableSchema::table_name(), "users");
        assert_eq!(
            UsersTableSchema::column_family_name(),
            SystemTable::Users.column_family_name().expect("Users is a table, not a view")
        );
    }

    #[test]
    fn test_schema_caching() {
        // Verify schema is the same instance (Arc pointer equality)
        let schema1 = UsersTableSchema::schema();
        let schema2 = UsersTableSchema::schema();
        assert!(Arc::ptr_eq(&schema1, &schema2));
    }
}
