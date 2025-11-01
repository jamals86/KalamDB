//! System.audit_log table schema
//!
//! This module defines the Arrow schema for the system.audit_log table.
//! Uses OnceLock for zero-overhead static schema caching.

use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use std::sync::{Arc, OnceLock};

/// Static schema cache for the audit_log table
static AUDIT_LOGS_SCHEMA: OnceLock<Arc<Schema>> = OnceLock::new();

/// System audit_log table schema definition
pub struct AuditLogsTableSchema;

impl AuditLogsTableSchema {
    /// Get the cached schema for the system.audit_log table
    ///
    /// Uses OnceLock to ensure the schema is created exactly once and reused
    /// across all providers without synchronization overhead.
    pub fn schema() -> SchemaRef {
        AUDIT_LOGS_SCHEMA
            .get_or_init(|| {
                Arc::new(Schema::new(vec![
                    Field::new("audit_id", DataType::Utf8, false),
                    Field::new(
                        "timestamp",
                        DataType::Timestamp(TimeUnit::Millisecond, None),
                        false,
                    ),
                    Field::new("actor_user_id", DataType::Utf8, false),
                    Field::new("actor_username", DataType::Utf8, false),
                    Field::new("action", DataType::Utf8, false),
                    Field::new("target", DataType::Utf8, false),
                    Field::new("details", DataType::Utf8, true),
                    Field::new("ip_address", DataType::Utf8, true),
                ]))
            })
            .clone()
    }

    /// Get the table name
    pub fn table_name() -> &'static str {
        kalamdb_commons::SystemTable::AuditLog.table_name()
    }

    /// Get the column family name in RocksDB
    pub fn column_family_name() -> &'static str {
        kalamdb_commons::SystemTable::AuditLog.column_family_name()
    }

    /// Get the partition key for storage
    pub fn partition() -> &'static str {
        kalamdb_commons::SystemTable::AuditLog.column_family_name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            "system_audit_log"
        );
    }

    #[test]
    fn test_schema_caching() {
        // Verify schema is the same instance (Arc pointer equality)
        let schema1 = AuditLogsTableSchema::schema();
        let schema2 = AuditLogsTableSchema::schema();
        assert!(Arc::ptr_eq(&schema1, &schema2));
    }

    #[test]
    fn test_non_nullable_fields() {
        let schema = AuditLogsTableSchema::schema();
        // audit_id, timestamp, actor_user_id, actor_username, action, target are required
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
        // details and ip_address are optional
        assert!(schema.field(6).is_nullable()); // details
        assert!(schema.field(7).is_nullable()); // ip_address
    }

    #[test]
    fn test_timestamp_field_type() {
        let schema = AuditLogsTableSchema::schema();
        match schema.field(1).data_type() {
            DataType::Timestamp(TimeUnit::Millisecond, None) => {} // Expected
            _ => panic!("timestamp field should be Timestamp(Millisecond, None)"),
        }
    }
}
