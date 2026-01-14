//! System.audit_log table schema
//!
//! This module defines the schema for the system.audit_log table.
//! - TableDefinition: Source of truth for columns, types, comments
//! - Arrow schema: Derived from TableDefinition, memoized via OnceLock

use datafusion::arrow::datatypes::SchemaRef;
use kalamdb_commons::datatypes::KalamDataType;
use kalamdb_commons::schemas::{
    ColumnDefault, ColumnDefinition, TableDefinition, TableOptions, TableType,
};
use kalamdb_commons::{NamespaceId, SystemTable, TableName};
use std::sync::OnceLock;

/// System audit_log table schema definition
///
/// Provides typed access to the table definition and Arrow schema.
/// Contains the full TableDefinition as the single source of truth.
#[derive(Debug, Clone, Copy)]
pub struct AuditLogsTableSchema;

impl AuditLogsTableSchema {
    /// Get the TableDefinition for system.audit_log
    ///
    /// This is the single source of truth for:
    /// - Column definitions (names, types, nullability)
    /// - Column ordering (ordinal_position)
    /// - Column comments/descriptions
    ///
    /// Schema:
    /// - audit_id TEXT PRIMARY KEY
    /// - timestamp TIMESTAMP NOT NULL
    /// - actor_user_id TEXT NOT NULL
    /// - actor_username TEXT NOT NULL
    /// - action TEXT NOT NULL
    /// - target TEXT NOT NULL
    /// - details TEXT (nullable)
    /// - ip_address TEXT (nullable)
    pub fn definition() -> TableDefinition {
        let columns = vec![
            ColumnDefinition::new(
                1,
                "audit_id",
                1,
                KalamDataType::Text,
                false,
                true,
                false,
                ColumnDefault::None,
                Some("Audit log entry identifier".to_string()),
            ),
            ColumnDefinition::new(
                2,
                "timestamp",
                2,
                KalamDataType::Timestamp,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Event timestamp".to_string()),
            ),
            ColumnDefinition::new(
                3,
                "actor_user_id",
                3,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("User ID who performed the action".to_string()),
            ),
            ColumnDefinition::new(
                4,
                "actor_username",
                4,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Username who performed the action".to_string()),
            ),
            ColumnDefinition::new(
                5,
                "action",
                5,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Action performed (CREATE, UPDATE, DELETE, LOGIN, etc.)".to_string()),
            ),
            ColumnDefinition::new(
                6,
                "target",
                6,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Target of the action (table name, user ID, etc.)".to_string()),
            ),
            ColumnDefinition::new(
                7,
                "details",
                7,
                KalamDataType::Text,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Additional details about the action (JSON)".to_string()),
            ),
            ColumnDefinition::new(
                8,
                "ip_address",
                8,
                KalamDataType::Text,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("IP address of the client".to_string()),
            ),
        ];

        TableDefinition::new(
            NamespaceId::system(),
            TableName::new(SystemTable::AuditLog.table_name()),
            TableType::System,
            columns,
            TableOptions::system(),
            Some("System audit log for security and compliance tracking".to_string()),
        )
        .expect("Failed to create system.audit_log table definition")
    }

    /// Get the cached Arrow schema for the system.audit_log table
    pub fn schema() -> SchemaRef {
        static SCHEMA: OnceLock<SchemaRef> = OnceLock::new();
        SCHEMA
            .get_or_init(|| {
                Self::definition()
                    .to_arrow_schema()
                    .expect("Failed to convert audit_log TableDefinition to Arrow schema")
            })
            .clone()
    }

    /// Get the table name
    pub fn table_name() -> &'static str {
        SystemTable::AuditLog.table_name()
    }

    /// Get the column family name in RocksDB
    pub fn column_family_name() -> Option<&'static str> {
        SystemTable::AuditLog.column_family_name()
    }

    /// Get the partition key for storage
    pub fn partition() -> Option<&'static str> {
        SystemTable::AuditLog.column_family_name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::arrow::datatypes::{DataType, TimeUnit};
    use std::sync::Arc;

    #[test]
    fn test_audit_logs_table_schema() {
        let schema = AuditLogsTableSchema::schema();
        assert_eq!(schema.fields().len(), 8);
        assert_eq!(schema.field(0).name(), "audit_id");
        assert_eq!(schema.field(1).name(), "timestamp");
        assert_eq!(schema.field(2).name(), "actor_user_id");
        assert_eq!(schema.field(3).name(), "actor_username");
        assert_eq!(schema.field(4).name(), "action");
        assert_eq!(schema.field(5).name(), "target");
        assert_eq!(schema.field(6).name(), "details");
        assert_eq!(schema.field(7).name(), "ip_address");
    }

    #[test]
    fn test_audit_logs_table_name() {
        assert_eq!(AuditLogsTableSchema::table_name(), "audit_log");
        assert_eq!(
            AuditLogsTableSchema::column_family_name(),
            SystemTable::AuditLog.column_family_name()
        );
    }

    #[test]
    fn test_schema_caching() {
        let schema1 = AuditLogsTableSchema::schema();
        let schema2 = AuditLogsTableSchema::schema();
        assert!(Arc::ptr_eq(&schema1, &schema2));
    }

    #[test]
    fn test_non_nullable_fields() {
        let schema = AuditLogsTableSchema::schema();
        assert!(!schema.field(0).is_nullable()); // audit_id
        assert!(!schema.field(1).is_nullable()); // timestamp
        assert!(!schema.field(2).is_nullable()); // actor_user_id
        assert!(!schema.field(3).is_nullable()); // actor_username
        assert!(!schema.field(4).is_nullable()); // action
        assert!(!schema.field(5).is_nullable()); // target
    }

    #[test]
    fn test_nullable_fields() {
        let schema = AuditLogsTableSchema::schema();
        assert!(schema.field(6).is_nullable()); // details
        assert!(schema.field(7).is_nullable()); // ip_address
    }

    #[test]
    fn test_timestamp_field_type() {
        let schema = AuditLogsTableSchema::schema();
        match schema.field(1).data_type() {
            DataType::Timestamp(TimeUnit::Millisecond, None) => {}
            _ => panic!("timestamp field should be Timestamp(Millisecond, None)"),
        }
    }

    #[test]
    fn test_definition() {
        let def = AuditLogsTableSchema::definition();
        assert_eq!(def.namespace_id.as_str(), "system");
        assert_eq!(def.table_name.as_str(), "audit_log");
        assert_eq!(def.columns.len(), 8);
    }
}
