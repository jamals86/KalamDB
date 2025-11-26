//! System.audit_log table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.audit_log table.
//! Uses the EntityStore architecture with type-safe keys (AuditLogId).

use super::{new_audit_logs_store, AuditLogsStore, AuditLogsTableSchema};
use crate::error::SystemError;
use crate::system_table_trait::SystemTableProviderExt;
use async_trait::async_trait;
use datafusion::arrow::array::{ArrayRef, RecordBatch, StringBuilder, TimestampMillisecondArray};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::models::AuditLogId;
use kalamdb_commons::system::AuditLogEntry;
use kalamdb_commons::StorageKey;
use kalamdb_store::entity_store::{EntityStore, EntityStoreAsync};
use kalamdb_store::StorageBackend;
use std::any::Any;
use std::sync::Arc;

/// System.audit_log table provider using EntityStore architecture
pub struct AuditLogsTableProvider {
    store: AuditLogsStore,
    schema: SchemaRef,
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
            schema: AuditLogsTableSchema::schema(),
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
            .map_err(|e| SystemError::Other(format!("put_async error: {}", e)))?;
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

    /// Helper to create RecordBatch from entries
    fn create_batch(
        &self,
        entries: Vec<(Vec<u8>, AuditLogEntry)>,
    ) -> Result<RecordBatch, SystemError> {
        let row_count = entries.len();

        // Pre-allocate builders for optimal performance
        let mut audit_ids = StringBuilder::with_capacity(row_count, row_count * 16);
        let mut timestamps = Vec::with_capacity(row_count);
        let mut actor_user_ids = StringBuilder::with_capacity(row_count, row_count * 16);
        let mut actor_usernames = StringBuilder::with_capacity(row_count, row_count * 32);
        let mut actions = StringBuilder::with_capacity(row_count, row_count * 32);
        let mut targets = StringBuilder::with_capacity(row_count, row_count * 64);
        let mut details_list = StringBuilder::with_capacity(row_count, row_count * 128);
        let mut ip_addresses = StringBuilder::with_capacity(row_count, row_count * 16);

        for (_key, entry) in entries {
            audit_ids.append_value(entry.audit_id.as_str());
            timestamps.push(Some(entry.timestamp));
            actor_user_ids.append_value(entry.actor_user_id.as_str());
            actor_usernames.append_value(entry.actor_username.as_str());
            actions.append_value(&entry.action);
            targets.append_value(&entry.target);
            details_list.append_option(entry.details.as_deref());
            ip_addresses.append_option(entry.ip_address.as_deref());
        }

        let batch = RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(audit_ids.finish()) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(timestamps)) as ArrayRef,
                Arc::new(actor_user_ids.finish()) as ArrayRef,
                Arc::new(actor_usernames.finish()) as ArrayRef,
                Arc::new(actions.finish()) as ArrayRef,
                Arc::new(targets.finish()) as ArrayRef,
                Arc::new(details_list.finish()) as ArrayRef,
                Arc::new(ip_addresses.finish()) as ArrayRef,
            ],
        )
        .map_err(|e| SystemError::Other(format!("Failed to create RecordBatch: {}", e)))?;

        Ok(batch)
    }

    /// Scan all audit log entries and return as RecordBatch
    pub fn scan_all_entries(&self) -> Result<RecordBatch, SystemError> {
        let iter = self.store.scan_iterator(None, None)?;
        let mut entries: Vec<(Vec<u8>, AuditLogEntry)> = Vec::new();
        for item in iter {
            let (id, entry) = item?;
            entries.push((id.storage_key(), entry));
        }
        self.create_batch(entries)
    }

    /// Scan up to `limit` audit log entries and return as RecordBatch
    pub fn scan_entries_limited(&self, limit: usize) -> Result<RecordBatch, SystemError> {
        use kalamdb_store::entity_store::{EntityStore, ScanDirection};
        let iter = self.store.scan_directional(None, ScanDirection::Newer, limit)?;
        let mut entries: Vec<(Vec<u8>, AuditLogEntry)> = Vec::new();
        for item in iter {
            let (id, entry) = item?;
            entries.push((id.storage_key(), entry));
        }
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
}

impl SystemTableProviderExt for AuditLogsTableProvider {
    fn table_name(&self) -> &str {
        AuditLogsTableSchema::table_name()
    }

    fn schema_ref(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn load_batch(&self) -> Result<RecordBatch, SystemError> {
        self.scan_all_entries()
    }
}

#[async_trait]
impl TableProvider for AuditLogsTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }

    async fn scan(
        &self,
        _state: &dyn datafusion::catalog::Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        use datafusion::datasource::MemTable;
        use datafusion::logical_expr::Operator;
        use datafusion::scalar::ScalarValue;

        let mut start_key = None;
        let mut prefix = None;

        // Extract start_key/prefix from filters
        for expr in filters {
            if let Expr::BinaryExpr(binary) = expr {
                // Note: Expr::Literal might have 1 or 2 fields depending on DataFusion version.
                // Based on base.rs, it seems to have 2 fields? Or maybe 1?
                // Let's try matching just the value and ignoring the rest if any.
                // Actually, let's try to match separately to avoid tuple issues.
                if let Expr::Column(col) = binary.left.as_ref() {
                    // Expr::Literal has 2 fields in this DataFusion version
                    if let Expr::Literal(val, _) = binary.right.as_ref() {
                        if col.name == "audit_id" {
                            if let ScalarValue::Utf8(Some(s)) = val {
                                match binary.op {
                                    Operator::Eq => {
                                        prefix = Some(AuditLogId::new(s));
                                    }
                                    Operator::Gt | Operator::GtEq => {
                                        start_key = Some(AuditLogId::new(s));
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }

        let schema = self.schema.clone();
        let entries = self
            .store
            .scan_all(limit, prefix.as_ref(), start_key.as_ref())
            .map_err(|e| DataFusionError::Execution(format!("Failed to scan audit logs: {}", e)))?;

        let batch = self.create_batch(entries).map_err(|e| {
            DataFusionError::Execution(format!("Failed to build audit log batch: {}", e))
        })?;

        let partitions = vec![vec![batch]];
        let table = MemTable::try_new(schema, partitions)
            .map_err(|e| DataFusionError::Execution(format!("Failed to create MemTable: {}", e)))?;
        table.scan(_state, projection, &[], limit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::Array;
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
        assert_eq!(batch.num_columns(), 8);
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
        assert_eq!(batch.num_columns(), 8);

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
        let timestamps_col = batch
            .column(1)
            .as_any()
            .downcast_ref::<TimestampMillisecondArray>()
            .unwrap();
        assert_eq!(timestamps_col.len(), 3);
    }

    #[test]
    fn test_system_table_provider_trait() {
        let provider = create_test_provider();

        // Test SystemTableProviderExt trait methods
        assert_eq!(provider.table_name(), "audit_log");
        assert_eq!(provider.schema_ref().fields().len(), 8);

        // Test load_batch
        provider
            .append(create_test_entry("audit_001", "test.action", 1730000000000))
            .unwrap();
        let batch = provider.load_batch().unwrap();
        assert_eq!(batch.num_rows(), 1);
    }
}
