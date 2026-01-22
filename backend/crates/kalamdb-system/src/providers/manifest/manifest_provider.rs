//! System.manifest table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.manifest table.
//! Exposes manifest cache entries as a queryable system table.

use super::{new_manifest_store, ManifestStore, ManifestTableSchema};
use crate::error::{SystemError, SystemResultExt};
use crate::system_table_trait::SystemTableProviderExt;
use async_trait::async_trait;
use datafusion::arrow::array::RecordBatch;
use kalamdb_commons::ManifestId;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::StorageKey;
use kalamdb_commons::storage::StorageError;
use kalamdb_commons::types::ManifestCacheEntry;
use kalamdb_commons::RecordBatchBuilder;
use kalamdb_store::entity_store::{EntityStore, KSerializable};
use kalamdb_store::StorageBackend;
use std::any::Any;
use std::sync::{Arc, RwLock};

/// Callback type for checking if a cache key is in hot memory
pub type InMemoryChecker = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// System.manifest table provider using EntityStore architecture
pub struct ManifestTableProvider {
    store: ManifestStore,
    /// Optional callback to check if a cache key is in hot memory (injected from kalamdb-core)
    in_memory_checker: RwLock<Option<InMemoryChecker>>,
}

impl std::fmt::Debug for ManifestTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManifestTableProvider").finish()
    }
}

impl ManifestTableProvider {
    /// Create a new manifest table provider
    ///
    /// # Arguments
    /// * `backend` - Storage backend (RocksDB or mock)
    ///
    /// # Returns
    /// A new ManifestTableProvider instance
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self {
            store: new_manifest_store(backend),
            in_memory_checker: RwLock::new(None),
        }
    }

    /// Set the in-memory checker callback
    ///
    /// This callback is injected from kalamdb-core to check if a cache key
    /// is currently in the hot cache (RAM).
    pub fn set_in_memory_checker(&self, checker: InMemoryChecker) {
        if let Ok(mut guard) = self.in_memory_checker.write() {
            *guard = Some(checker);
        }
    }

    /// Access the underlying ManifestStore
    pub fn store(&self) -> &ManifestStore {
        &self.store
    }

    /// Check if a cache key is in hot memory
    fn is_in_memory(&self, cache_key: &str) -> bool {
        if let Ok(guard) = self.in_memory_checker.read() {
            if let Some(ref checker) = *guard {
                return checker(cache_key);
            }
        }
        false // Default to false if no checker is set
    }

    /// Scan all manifest cache entries and return as RecordBatch
    ///
    /// This is the main method used by DataFusion to read the table.
    ///
    /// Uses schema-driven array building for optimal performance and correctness.
    pub fn scan_to_record_batch(&self) -> Result<RecordBatch, SystemError> {
        let partition = self.store.partition();
        let iter = self
            .store
            .backend()
            .scan(&partition, None, None, Some(100000))?;

        let mut entries: Vec<(Vec<u8>, ManifestCacheEntry)> = Vec::new();
        for (key_bytes, value_bytes) in iter {
            match ManifestCacheEntry::decode(&value_bytes) {
                Ok(entry) => entries.push((key_bytes, entry)),
                Err(StorageError::SerializationError(err)) => {
                    log::warn!(
                        "Corrupted manifest cache entry skipped: {}",
                        err
                    );
                    if let Ok(cache_key) = ManifestId::from_storage_key(&key_bytes) {
                        let _ = self.store().delete(&cache_key);
                    }
                }
                Err(err) => return Err(SystemError::Storage(err.to_string())),
            }
        }

        // Extract data into vectors
        let mut cache_keys = Vec::with_capacity(entries.len());
        let mut namespace_ids = Vec::with_capacity(entries.len());
        let mut table_names = Vec::with_capacity(entries.len());
        let mut scopes = Vec::with_capacity(entries.len());
        let mut etags = Vec::with_capacity(entries.len());
        let mut last_refreshed_vals = Vec::with_capacity(entries.len());
        let mut last_accessed_vals = Vec::with_capacity(entries.len());
        let mut in_memory_vals = Vec::with_capacity(entries.len());
        let mut sync_states = Vec::with_capacity(entries.len());
        let mut manifest_jsons = Vec::with_capacity(entries.len());

        // Build arrays by iterating entries once
        for (key_bytes, entry) in entries {
            let cache_key = String::from_utf8_lossy(&key_bytes);

            // Parse cache key (format: namespace:table:scope)
            let mut parts = cache_key.splitn(3, ':');
            let namespace = parts.next();
            let table = parts.next();
            let scope = parts.next();
            if namespace.is_none() || table.is_none() || scope.is_none() {
                log::warn!("Invalid cache key format: {}", cache_key);
                continue;
            }

            let cache_key_str = cache_key.to_string();
            let is_hot = self.is_in_memory(&cache_key_str);

            // Serialize manifest_json before moving entry fields
            let manifest_json_str = entry.manifest_json();

            cache_keys.push(Some(cache_key_str));
            namespace_ids.push(Some(namespace.unwrap().to_string()));
            table_names.push(Some(table.unwrap().to_string()));
            scopes.push(Some(scope.unwrap().to_string()));
            etags.push(entry.etag);
            last_refreshed_vals.push(Some(entry.last_refreshed * 1000)); // Convert to milliseconds
                                                                         // last_accessed = last_refreshed (moka manages TTI internally, we can't get actual access time)
            last_accessed_vals.push(Some(entry.last_refreshed * 1000));
            in_memory_vals.push(Some(is_hot));
            sync_states.push(Some(entry.sync_state.to_string()));
            manifest_jsons.push(Some(manifest_json_str));
        }

        // Build batch using RecordBatchBuilder
        let mut builder = RecordBatchBuilder::new(ManifestTableSchema::schema());
        builder
            .add_string_column_owned(cache_keys)
            .add_string_column_owned(namespace_ids)
            .add_string_column_owned(table_names)
            .add_string_column_owned(scopes)
            .add_string_column_owned(etags)
            .add_timestamp_micros_column(last_refreshed_vals)
            .add_timestamp_micros_column(last_accessed_vals)
            .add_boolean_column(in_memory_vals)
            .add_string_column_owned(sync_states)
            .add_string_column_owned(manifest_jsons);

        builder.build().into_arrow_error("Failed to create RecordBatch")
    }
}

#[async_trait]
impl TableProvider for ManifestTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        ManifestTableSchema::schema()
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }

    async fn scan(
        &self,
        _state: &dyn datafusion::catalog::Session,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        use datafusion::datasource::MemTable;
        let schema = ManifestTableSchema::schema();
        let batch = self.scan_to_record_batch().map_err(|e| {
            DataFusionError::Execution(format!("Failed to build manifest batch: {}", e))
        })?;
        let partitions = vec![vec![batch]];
        let table = MemTable::try_new(schema, partitions)
            .map_err(|e| DataFusionError::Execution(format!("Failed to create MemTable: {}", e)))?;
        table.scan(_state, projection, &[], _limit).await
    }
}

impl SystemTableProviderExt for ManifestTableProvider {
    fn table_name(&self) -> &str {
        ManifestTableSchema::table_name()
    }

    fn schema_ref(&self) -> SchemaRef {
        ManifestTableSchema::schema()
    }

    fn load_batch(&self) -> Result<RecordBatch, SystemError> {
        self.scan_to_record_batch()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::types::{Manifest, ManifestCacheEntry, SyncState};
    use kalamdb_commons::{NamespaceId, TableId, TableName};
    use kalamdb_store::test_utils::InMemoryBackend;

    #[tokio::test]
    async fn test_empty_manifest_table() {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let provider = ManifestTableProvider::new(backend);

        let batch = provider.scan_to_record_batch().unwrap();
        assert_eq!(batch.num_rows(), 0);
        assert_eq!(batch.num_columns(), 10); // Updated column count
    }

    #[tokio::test]
    async fn test_manifest_table_with_entries() {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let provider = ManifestTableProvider::new(backend.clone());

        // Insert test entries directly via store
        let table_id = TableId::new(NamespaceId::new("ns1"), TableName::new("tbl1"));
        let manifest = Manifest::new(table_id, None);
        let entry = ManifestCacheEntry::new(
            manifest,
            Some("etag123".to_string()),
            1000,
            SyncState::InSync,
        );

        let key = ManifestId::from("ns1:tbl1:shared");
        provider.store.put(&key, &entry).unwrap();

        let batch = provider.scan_to_record_batch().unwrap();
        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), 10);

        // Verify column names match schema
        let schema = batch.schema();
        assert_eq!(schema.field(0).name(), "cache_key");
        assert_eq!(schema.field(1).name(), "namespace_id");
        assert_eq!(schema.field(2).name(), "table_name");
        assert_eq!(schema.field(3).name(), "scope");
        assert_eq!(schema.field(4).name(), "etag");
        assert_eq!(schema.field(7).name(), "in_memory");
        assert_eq!(schema.field(9).name(), "manifest_json");
    }
}
