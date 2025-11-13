//! System.users table schema
//!
//! This module defines the Arrow schema for the system.users table.
//! Uses OnceLock for zero-overhead static schema caching.

use datafusion::arrow::datatypes::SchemaRef;
use std::sync::OnceLock;

use crate::system_table_definitions::users_table_definition;

/// Static schema cache for the users table
static USERS_SCHEMA: OnceLock<SchemaRef> = OnceLock::new();

/// System users table schema definition
pub struct UsersTableSchema;

impl UsersTableSchema {
    /// Get the cached schema for the system.users table
    ///
    /// Uses OnceLock to ensure the schema is created exactly once and reused
    /// across all providers without synchronization overhead.
    ///
    /// Schema is built from TableDefinition which ensures columns are ordered
    /// by ordinal_position (Phase 4 requirement: T062-T065).
    pub fn schema() -> SchemaRef {
        USERS_SCHEMA
            .get_or_init(|| {
                users_table_definition()
                    .to_arrow_schema()
                    .expect("Failed to convert users TableDefinition to Arrow schema")
            })
            .clone()
    }

    /// Get the table name
    pub fn table_name() -> &'static str {
        kalamdb_commons::SystemTable::Users.table_name()
    }

    /// Get the column family name in RocksDB
    pub fn column_family_name() -> &'static str {
        kalamdb_commons::SystemTable::Users.column_family_name()
    }

    /// Get the partition key for storage
    pub fn partition() -> &'static str {
        kalamdb_commons::SystemTable::Users.column_family_name()
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
        assert_eq!(schema.fields().len(), 13);

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
