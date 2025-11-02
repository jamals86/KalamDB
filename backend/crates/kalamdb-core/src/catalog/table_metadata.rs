//! Table metadata entity
//!
//! Defines metadata for user, shared, and stream tables in KalamDB.

use crate::flush::FlushPolicy;
use chrono::{DateTime, Utc};
use kalamdb_commons::models::{NamespaceId, StorageId, TableName};
use kalamdb_commons::schemas::TableType;
use serde::{Deserialize, Serialize};

/// Table metadata entity
///
/// Contains all metadata for a table, including its type, namespace, storage reference,
/// and flush policy.
///
/// # Examples
///
/// ```
/// use kalamdb_core::catalog::{TableMetadata, TableName, TableType, NamespaceId};
/// use kalamdb_commons::models::StorageId;
/// use kalamdb_core::flush::FlushPolicy;
///
/// let metadata = TableMetadata::new(
///     TableName::new("messages"),
///     TableType::User,
///     NamespaceId::new("app"),
///     Some(StorageId::new("local")),
/// );
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableMetadata {
    /// Table identifier within namespace
    pub table_name: TableName,

    /// Type of table (User, Shared, System, Stream)
    pub table_type: TableType,

    /// Parent namespace
    pub namespace: NamespaceId,

    /// When the table was created
    pub created_at: DateTime<Utc>,

    /// Reference to storage configuration in system.storages
    /// Used to resolve storage path templates dynamically via TableCache
    pub storage_id: Option<StorageId>,

    /// When to flush buffered data to Parquet
    pub flush_policy: FlushPolicy,

    /// Current schema version number
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,

    /// How long to keep deleted rows (in hours)
    pub deleted_retention_hours: Option<u32>,
}

fn default_schema_version() -> u32 {
    1
}

impl TableMetadata {
    /// Create new table metadata
    pub fn new(
        table_name: TableName,
        table_type: TableType,
        namespace: NamespaceId,
        storage_id: Option<StorageId>,
    ) -> Self {
        Self {
            table_name,
            table_type,
            namespace,
            created_at: Utc::now(),
            storage_id,
            flush_policy: FlushPolicy::default(),
            schema_version: 1,
            deleted_retention_hours: None,
        }
    }

    /// Validate table name format
    ///
    /// Name must match regex: ^[a-z][a-z0-9_]*$
    pub fn validate_table_name(name: &str) -> Result<(), String> {
        if !name.chars().next().is_some_and(|c| c.is_ascii_lowercase()) {
            return Err("Table name must start with a lowercase letter".to_string());
        }

        if !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(
                "Table name can only contain lowercase letters, digits, and underscores"
                    .to_string(),
            );
        }

        Ok(())
    }

    /// Get the column family name for this table
    ///
    /// Format: {table_type}_table:{namespace}:{table_name}
    pub fn column_family_name(&self) -> String {
        format!(
            "{}_table:{}:{}",
            self.table_type.as_str(),
            self.namespace.as_str(),
            self.table_name.as_str()
        )
    }

    /// Get the schema directory path
    ///
    /// Format: conf/{namespace}/schemas/{table_name}/
    pub fn schema_directory(&self) -> String {
        format!(
            "conf/{}/schemas/{}/",
            self.namespace.as_str(),
            self.table_name.as_str()
        )
    }

    /// Set the flush policy
    pub fn with_flush_policy(mut self, policy: FlushPolicy) -> Self {
        self.flush_policy = policy;
        self
    }

    /// Set the deleted retention hours
    pub fn with_deleted_retention(mut self, hours: u32) -> Self {
        self.deleted_retention_hours = Some(hours);
        self
    }

    /// Increment the schema version
    pub fn increment_schema_version(&mut self) {
        self.schema_version += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_metadata_creation() {
        let metadata = TableMetadata::new(
            TableName::new("messages"),
            TableType::User,
            NamespaceId::new("app"),
            "/data/${user_id}/messages".to_string(),
        );

        assert_eq!(metadata.table_name.as_str(), "messages");
        assert_eq!(metadata.table_type, TableType::User);
        assert_eq!(metadata.namespace.as_str(), "app");
        assert_eq!(metadata.storage_location, "/data/${user_id}/messages");
        assert_eq!(metadata.schema_version, 1);
        assert!(metadata.deleted_retention_hours.is_none());
    }

    #[test]
    fn test_validate_table_name_valid() {
        assert!(TableMetadata::validate_table_name("messages").is_ok());
        assert!(TableMetadata::validate_table_name("user_events").is_ok());
        assert!(TableMetadata::validate_table_name("table123").is_ok());
    }

    #[test]
    fn test_validate_table_name_invalid() {
        assert!(TableMetadata::validate_table_name("Messages").is_err());
        assert!(TableMetadata::validate_table_name("123table").is_err());
        assert!(TableMetadata::validate_table_name("table-name").is_err());
    }

    #[test]
    fn test_column_family_name() {
        let metadata = TableMetadata::new(
            TableName::new("messages"),
            TableType::User,
            NamespaceId::new("app"),
            "/data".to_string(),
        );

        assert_eq!(metadata.column_family_name(), "user_table:app:messages");
    }

    #[test]
    fn test_schema_directory() {
        let metadata = TableMetadata::new(
            TableName::new("events"),
            TableType::Shared,
            NamespaceId::new("analytics"),
            "/data".to_string(),
        );

        assert_eq!(
            metadata.schema_directory(),
            "conf/analytics/schemas/events/"
        );
    }

    #[test]
    fn test_with_flush_policy() {
        let policy = FlushPolicy::TimeInterval {
            interval_seconds: 300,
        };
        let metadata = TableMetadata::new(
            TableName::new("test"),
            TableType::User,
            NamespaceId::new("app"),
            "/data".to_string(),
        )
        .with_flush_policy(policy.clone());

        assert_eq!(metadata.flush_policy, policy);
    }

    #[test]
    fn test_with_deleted_retention() {
        let metadata = TableMetadata::new(
            TableName::new("test"),
            TableType::User,
            NamespaceId::new("app"),
            "/data".to_string(),
        )
        .with_deleted_retention(72);

        assert_eq!(metadata.deleted_retention_hours, Some(72));
    }

    #[test]
    fn test_increment_schema_version() {
        let mut metadata = TableMetadata::new(
            TableName::new("test"),
            TableType::User,
            NamespaceId::new("app"),
            "/data".to_string(),
        );

        assert_eq!(metadata.schema_version, 1);
        metadata.increment_schema_version();
        assert_eq!(metadata.schema_version, 2);
        metadata.increment_schema_version();
        assert_eq!(metadata.schema_version, 3);
    }
}
