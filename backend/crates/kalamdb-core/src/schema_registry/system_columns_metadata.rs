//! System column injection logic
//!
//! This module handles injection of system columns into table schemas.
//!
//! **NOTE**: This is legacy code. New code should use SystemColumnsService instead.

use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use std::sync::Arc;

/// System columns that are automatically added to user and shared tables
///
/// **DEPRECATED**: Use SystemColumnsService for adding system columns.
pub struct SystemColumns;

impl SystemColumns {
    /// Get the _seq system column (Snowflake ID with embedded timestamp)
    ///
    /// Type: BIGINT (Int64)
    pub fn seq_column() -> Field {
        Field::new("_seq", DataType::Int64, false)
    }

    /// Get the _deleted system column
    ///
    /// Type: BOOLEAN (default false)
    pub fn deleted_column() -> Field {
        Field::new("_deleted", DataType::Boolean, false)
    }

    /// Inject system columns into a schema
    ///
    /// Adds _seq and _deleted columns for user/shared tables.
    /// System and stream tables don't get these columns.
    pub fn inject_into_schema(schema: SchemaRef, include_system_columns: bool) -> SchemaRef {
        if !include_system_columns {
            return schema;
        }

        let mut fields: Vec<Field> = Vec::new();
        for field in schema.fields().iter() {
            fields.push((**field).clone());
        }

        // Add system columns at the end
        fields.push(Self::seq_column());
        fields.push(Self::deleted_column());

        Arc::new(Schema::new(fields))
    }

    /// Check if a column is a system column
    pub fn is_system_column(column_name: &str) -> bool {
        matches!(column_name, "_seq" | "_deleted")
    }

    /// Get all system column names
    pub fn column_names() -> Vec<&'static str> {
        vec!["_seq", "_deleted"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::datatypes::Field;

    #[test]
    fn test_system_columns() {
        let seq = SystemColumns::seq_column();
        assert_eq!(seq.name(), "_seq");
        assert_eq!(seq.data_type(), &DataType::Int64);

        let deleted = SystemColumns::deleted_column();
        assert_eq!(deleted.name(), "_deleted");
        assert_eq!(deleted.data_type(), &DataType::Boolean);
    }

    #[test]
    fn test_inject_into_schema() {
        let user_schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("message", DataType::Utf8, true),
        ]));

        // Inject system columns
        let with_system = SystemColumns::inject_into_schema(user_schema.clone(), true);
        assert_eq!(with_system.fields().len(), 4);
        assert_eq!(with_system.field(2).name(), "_seq");
        assert_eq!(with_system.field(3).name(), "_deleted");

        // Don't inject system columns
        let without_system = SystemColumns::inject_into_schema(user_schema.clone(), false);
        assert_eq!(without_system.fields().len(), 2);
    }

    #[test]
    fn test_is_system_column() {
        assert!(SystemColumns::is_system_column("_seq"));
        assert!(SystemColumns::is_system_column("_deleted"));
        assert!(!SystemColumns::is_system_column("id"));
        assert!(!SystemColumns::is_system_column("message"));
    }

    #[test]
    fn test_column_names() {
        let names = SystemColumns::column_names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"_seq"));
        assert!(names.contains(&"_deleted"));
    }
}
