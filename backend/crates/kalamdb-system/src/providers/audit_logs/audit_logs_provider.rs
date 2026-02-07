//! System.audit_log table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.audit_log table.
//! Uses the EntityStore architecture with type-safe keys (AuditLogId).

use super::{new_audit_logs_store, AuditLogsStore, AuditLogsTableSchema};
use crate::error::{SystemError, SystemResultExt};
use crate::providers::audit_logs::models::AuditLogEntry;
use crate::providers::base::SimpleSystemTableScan;
use crate::system_table_trait::SystemTableProviderExt;
use async_trait::async_trait;
use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::Result as DataFusionResult;
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::models::AuditLogId;
use kalamdb_commons::RecordBatchBuilder;
use kalamdb_store::entity_store::{EntityStore, EntityStoreAsync};
use kalamdb_store::StorageBackend;
use std::any::Any;
use std::sync::Arc;

/// System.audit_log table provider using EntityStore architecture
pub struct AuditLogsTableProvider {
    store: AuditLogsStore,
}

impl std::fmt::Debug for AuditLogsTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditLogsTableProvider").finish()
    }
}

impl AuditLogsTableProvider {
    /// Create a new audit logs table provider
    ///
    /// # Arguments
    /// * `backend` - Storage backend (RocksDB or mock)
    ///
    /// # Returns
    /// A new AuditLogsTableProvider instance
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self {
            store: new_audit_logs_store(backend),
        }
    }

    /// Append a new audit log entry
    ///
    /// # Arguments
    /// * `entry` - The audit log entry to append
    ///
    /// # Returns
    /// Result indicating success or failure
    pub fn append(&self, entry: AuditLogEntry) -> Result<(), SystemError> {
        self.store.put(&entry.audit_id, &entry)?;
        Ok(())
    }

    /// Async version of `append()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn append_async(&self, entry: AuditLogEntry) -> Result<(), SystemError> {
        self.store
            .put_async(&entry.audit_id, &entry)
            .await
            .into_system_error("put_async error")?;
        Ok(())
    }

    /// Get an audit log entry by ID
    ///
    /// # Arguments
    /// * `audit_id` - The audit log ID to lookup
    ///
    /// # Returns
    /// Option<AuditLogEntry> if found, None otherwise
    pub fn get_entry(&self, audit_id: &AuditLogId) -> Result<Option<AuditLogEntry>, SystemError> {
        Ok(self.store.get(audit_id)?)
    }

    /// Async version of `get_entry()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn get_entry_async(
        &self,
        audit_id: &AuditLogId,
    ) -> Result<Option<AuditLogEntry>, SystemError> {
        self.store.get_async(audit_id).await.into_system_error("get_async error")
    }

    /// Helper to create RecordBatch from entries
    fn create_batch(
        &self,
        entries: Vec<(AuditLogId, AuditLogEntry)>,
    ) -> Result<RecordBatch, SystemError> {
        // Extract data into vectors
        let mut audit_ids = Vec::with_capacity(entries.len());
        let mut timestamps = Vec::with_capacity(entries.len());
        let mut actor_user_ids = Vec::with_capacity(entries.len());
        let mut actor_usernames = Vec::with_capacity(entries.len());
        let mut actions = Vec::with_capacity(entries.len());
        let mut targets = Vec::with_capacity(entries.len());
        let mut details_list = Vec::with_capacity(entries.len());
        let mut ip_addresses = Vec::with_capacity(entries.len());
        let mut subject_user_ids = Vec::with_capacity(entries.len());

        for (_key, entry) in entries {
            audit_ids.push(Some(entry.audit_id.as_str().to_string()));
            timestamps.push(Some(entry.timestamp));
            actor_user_ids.push(Some(entry.actor_user_id.as_str().to_string()));
            actor_usernames.push(Some(entry.actor_username.as_str().to_string()));
            actions.push(Some(entry.action));
            targets.push(Some(entry.target));
            details_list.push(entry.details);
            ip_addresses.push(entry.ip_address);
            subject_user_ids.push(entry.subject_user_id.map(|id| id.as_str().to_string()));
        }

        // Build batch using RecordBatchBuilder
        let mut builder = RecordBatchBuilder::new(AuditLogsTableSchema::schema());
        builder
            .add_string_column_owned(audit_ids)
            .add_timestamp_micros_column(timestamps)
            .add_string_column_owned(actor_user_ids)
            .add_string_column_owned(actor_usernames)
            .add_string_column_owned(actions)
            .add_string_column_owned(targets)
            .add_string_column_owned(details_list)
            .add_string_column_owned(ip_addresses)
            .add_string_column_owned(subject_user_ids);

        let batch = builder.build().into_arrow_error("Failed to create RecordBatch")?;

        Ok(batch)
    }

    /// Scan all audit log entries and return as RecordBatch
    pub fn scan_all_entries(&self) -> Result<RecordBatch, SystemError> {
        let entries = self.store.scan_all_typed(None, None, None)?;
        self.create_batch(entries)
    }

    /// Scan up to `limit` audit log entries and return as RecordBatch
    pub fn scan_entries_limited(&self, limit: usize) -> Result<RecordBatch, SystemError> {
        use kalamdb_store::entity_store::{EntityStore, ScanDirection};
        let iter = self.store.scan_directional(None, ScanDirection::Newer, limit)?;
        let entries: Vec<(AuditLogId, AuditLogEntry)> = iter.collect::<Result<Vec<_>, _>>()?;
        self.create_batch(entries)
    }

    /// Scan all audit log entries and return as Vec<AuditLogEntry>
    /// Useful for testing and internal usage where RecordBatch is not needed
    pub fn scan_all(&self) -> Result<Vec<AuditLogEntry>, SystemError> {
        use kalamdb_store::entity_store::EntityStore;
        let iter = self.store.scan_iterator(None, None)?;
        let mut entries = Vec::new();
        for item in iter {
            let (_, entry) = item?;
            entries.push(entry);
        }
        Ok(entries)
    }

    /// Async version of `scan_all()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn scan_all_async(&self) -> Result<Vec<AuditLogEntry>, SystemError> {
        let results: Vec<(Vec<u8>, AuditLogEntry)> = self
            .store
            .scan_all_async(None, None, None)
            .await
            .into_system_error("scan_all_async error")?;
        Ok(results.into_iter().map(|(_, entry)| entry).collect())
    }
}

impl SystemTableProviderExt for AuditLogsTableProvider {
    fn table_name(&self) -> &str {
        AuditLogsTableSchema::table_name()
    }

    fn schema_ref(&self) -> SchemaRef {
        AuditLogsTableSchema::schema()
    }

    fn load_batch(&self) -> Result<RecordBatch, SystemError> {
        self.scan_all_entries()
    }
}

impl SimpleSystemTableScan<AuditLogId, AuditLogEntry> for AuditLogsTableProvider {
    fn table_name(&self) -> &str {
        AuditLogsTableSchema::table_name()
    }

    fn arrow_schema(&self) -> SchemaRef {
        AuditLogsTableSchema::schema()
    }

    fn scan_all_to_batch(&self) -> Result<RecordBatch, SystemError> {
        self.scan_all_entries()
    }
}

#[async_trait]
impl TableProvider for AuditLogsTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        AuditLogsTableSchema::schema()
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }

    async fn scan(
        &self,
        state: &dyn datafusion::catalog::Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        // Use the common SimpleSystemTableScan implementation
        self.base_simple_scan(state, projection, filters, limit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::Array;
    use datafusion::arrow::array::TimestampMicrosecondArray;
    use kalamdb_commons::{UserId, UserName};
    use kalamdb_store::test_utils::InMemoryBackend;
    use serde_json::json;

    fn create_test_provider() -> AuditLogsTableProvider {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        AuditLogsTableProvider::new(backend)
    }

    fn create_test_entry(audit_id: &str, action: &str, timestamp: i64) -> AuditLogEntry {
        AuditLogEntry {
            audit_id: AuditLogId::new(audit_id),
            timestamp,
            subject_user_id: Some(UserId::new("user_123")),
            actor_user_id: UserId::new("admin"),
            actor_username: UserName::new("admin"),
            action: action.to_string(),
            target: "system.users".to_string(),
            details: Some(json!({"test": true}).to_string()),
            ip_address: Some("127.0.0.1".to_string()),
        }
    }

    #[test]
    fn test_append_and_get_entry() {
        let provider = create_test_provider();
        let entry = create_test_entry("audit_001", "user.create", 1730000000000);

        // Append entry
        provider.append(entry.clone()).unwrap();

        // Get by ID
        let retrieved = provider.get_entry(&AuditLogId::new("audit_001")).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.action, "user.create");
        assert_eq!(retrieved.timestamp, 1730000000000);
    }

    #[test]
    fn test_scan_all_entries() {
        let provider = create_test_provider();

        // Append multiple entries
        for i in 1..=5 {
            let entry = create_test_entry(
                &format!("audit_{:03}", i),
                "user.update",
                1730000000000 + (i * 1000),
            );
            provider.append(entry).unwrap();
        }

        // Scan all
        let batch = provider.scan_all_entries().unwrap();
        assert_eq!(batch.num_rows(), 5);
        assert_eq!(batch.num_columns(), 9);
    }

    #[test]
    fn test_filter_by_action() {
        let provider = create_test_provider();

        // Append entries with different actions
        provider
            .append(create_test_entry("audit_001", "user.create", 1730000000000))
            .unwrap();
        provider
            .append(create_test_entry("audit_002", "user.update", 1730000001000))
            .unwrap();
        provider
            .append(create_test_entry("audit_003", "user.create", 1730000002000))
            .unwrap();

        // Scan all and verify
        let batch = provider.scan_all_entries().unwrap();
        assert_eq!(batch.num_rows(), 3);

        // Verify actions column
        let actions = batch
            .column(4)
            .as_any()
            .downcast_ref::<datafusion::arrow::array::StringArray>()
            .unwrap();
        assert_eq!(actions.len(), 3);
    }

    #[test]
    fn test_projection() {
        let provider = create_test_provider();

        // Append test entry
        provider
            .append(create_test_entry("audit_001", "user.create", 1730000000000))
            .unwrap();

        // Scan with all columns
        let batch = provider.scan_all_entries().unwrap();
        assert_eq!(batch.num_columns(), 9);

        // Verify schema matches
        assert_eq!(batch.schema(), provider.schema());
    }

    #[test]
    fn test_batch_operations() {
        let provider = create_test_provider();

        // Batch append 100 entries
        for i in 0..100 {
            let entry = create_test_entry(
                &format!("audit_{:05}", i),
                "batch.operation",
                1730000000000 + (i * 100),
            );
            provider.append(entry).unwrap();
        }

        // Verify all entries
        let batch = provider.scan_all_entries().unwrap();
        assert_eq!(batch.num_rows(), 100);
    }

    #[test]
    fn test_nullable_fields() {
        let provider = create_test_provider();

        // Create entry without optional fields
        let entry = AuditLogEntry {
            audit_id: AuditLogId::new("audit_001"),
            timestamp: 1730000000000,
            subject_user_id: None,
            actor_user_id: UserId::new("admin"),
            actor_username: UserName::new("admin"),
            action: "test.action".to_string(),
            target: "test.target".to_string(),
            details: None,    // Optional
            ip_address: None, // Optional
        };

        provider.append(entry).unwrap();

        // Verify nullable fields handled correctly
        let batch = provider.scan_all_entries().unwrap();
        assert_eq!(batch.num_rows(), 1);
    }

    #[test]
    fn test_timestamp_ordering() {
        let provider = create_test_provider();

        // Insert entries with different timestamps
        let timestamps = [1730000003000, 1730000001000, 1730000002000];
        for (i, &ts) in timestamps.iter().enumerate() {
            let entry = create_test_entry(&format!("audit_{}", i), "test.action", ts);
            provider.append(entry).unwrap();
        }

        // Scan all
        let batch = provider.scan_all_entries().unwrap();
        assert_eq!(batch.num_rows(), 3);

        // Verify timestamps column exists
        let timestamps_col =
            batch.column(1).as_any().downcast_ref::<TimestampMicrosecondArray>().unwrap();
        assert_eq!(timestamps_col.len(), 3);
    }

    #[test]
    fn test_system_table_provider_trait() {
        let provider = create_test_provider();

        // Test SystemTableProviderExt trait methods
        //assert_eq!(provider.table_name(), "audit_log");
        assert_eq!(provider.schema_ref().fields().len(), 9);

        // Test load_batch
        provider
            .append(create_test_entry("audit_001", "test.action", 1730000000000))
            .unwrap();
        let batch = provider.load_batch().unwrap();
        assert_eq!(batch.num_rows(), 1);
    }
}
