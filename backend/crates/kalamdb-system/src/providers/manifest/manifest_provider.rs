//! System.manifest table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.manifest table.
//! Exposes manifest cache entries as a queryable system table.

use super::{create_manifest_indexes, ManifestTableSchema};
use crate::error::{SystemError, SystemResultExt};
use crate::providers::base::SimpleSystemTableScan;
use crate::system_table_trait::SystemTableProviderExt;
use async_trait::async_trait;
use datafusion::arrow::array::RecordBatch;
use kalamdb_commons::{ManifestId, StorageKey, TableId};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::Result as DataFusionResult;
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use crate::providers::manifest::ManifestCacheEntry;
use kalamdb_commons::RecordBatchBuilder;
use kalamdb_store::entity_store::EntityStore;
use kalamdb_store::{IndexedEntityStore, StorageBackend};
use std::any::Any;
use std::sync::{Arc, RwLock};

/// Callback type for checking if a cache key is in hot memory
pub type InMemoryChecker = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// System.manifest table provider using IndexedEntityStore architecture
pub struct ManifestTableProvider {
    store: IndexedEntityStore<ManifestId, ManifestCacheEntry>,
    /// Optional callback to check if a cache key is in hot memory (injected from kalamdb-core)
    in_memory_checker: RwLock<Option<InMemoryChecker>>,
}

impl std::fmt::Debug for ManifestTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManifestTableProvider").finish()
    }
}

impl ManifestTableProvider {
    /// Create a new manifest table provider with indexes
    ///
    /// # Arguments
    /// * `backend` - Storage backend (RocksDB or mock)
    ///
    /// # Returns
    /// A new ManifestTableProvider instance
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        let store = IndexedEntityStore::new(
            backend,
            crate::SystemTable::Manifest.column_family_name().expect("Manifest is a table"),
            create_manifest_indexes(),
        );

        Self {
            store,
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

    /// Access the underlying store for direct operations
    pub fn store(&self) -> &IndexedEntityStore<ManifestId, ManifestCacheEntry> {
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
        let iter = self
            .store
            .scan_iterator(None, None)
            .map_err(|e| SystemError::Storage(e.to_string()))?;

        // Extract data into vectors
        let mut cache_keys = Vec::new();
        let mut namespace_ids = Vec::new();
        let mut table_names = Vec::new();
        let mut scopes = Vec::new();
        let mut etags = Vec::new();
        let mut last_refreshed_vals = Vec::new();
        let mut last_accessed_vals = Vec::new();
        let mut in_memory_vals = Vec::new();
        let mut sync_states = Vec::new();
        let mut manifest_jsons = Vec::new();

        // Build arrays by iterating entries once
        for entry in iter {
            let (manifest_id, entry) = entry.map_err(|e| SystemError::Storage(e.to_string()))?;
            let cache_key_str = manifest_id.as_str();
            let is_hot = self.is_in_memory(&cache_key_str);

            // Serialize manifest_json before moving entry fields
            let manifest_json_str = entry.manifest_json();

            cache_keys.push(Some(cache_key_str));
            namespace_ids.push(Some(manifest_id.table_id().namespace_id().as_str().to_string()));
            table_names.push(Some(manifest_id.table_id().table_name().as_str().to_string()));
            scopes.push(Some(manifest_id.scope_str()));
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

    /// Iterator over pending manifest IDs (from the pending-write index).
    pub fn pending_manifest_ids_iter(
        &self,
        prefix: Option<&[u8]>,
        limit: Option<usize>,
    ) -> Result<Box<dyn Iterator<Item = Result<ManifestId, SystemError>> + Send + '_>, SystemError> {
        let iter = self
            .store
            .scan_index_keys_iter(0, prefix, limit)
            .map_err(|e| SystemError::Storage(e.to_string()))?;

        let mapped = iter.map(|res| res.map_err(|e| SystemError::Storage(e.to_string())));
        Ok(Box::new(mapped))
    }

    /// Return all pending manifest IDs (collected).
    pub fn pending_manifest_ids(&self) -> Result<Vec<ManifestId>, SystemError> {
        let iter = self.pending_manifest_ids_iter(None, None)?;
        let mut results = Vec::new();
        for item in iter {
            results.push(item?);
        }

        Ok(results)
    }

    /// Return pending manifest IDs for a specific table.
    pub fn pending_manifest_ids_for_table(
        &self,
        table_id: &TableId,
    ) -> Result<Vec<ManifestId>, SystemError> {
        let prefix_bytes = ManifestId::table_prefix(table_id);
        let iter = self.pending_manifest_ids_iter(Some(&prefix_bytes), None)?;
        let mut results = Vec::new();
        for item in iter {
            results.push(item?);
        }

        Ok(results)
    }

    /// Check if a specific manifest is pending (by key).
    pub fn pending_exists(&self, manifest_id: &ManifestId) -> Result<bool, SystemError> {
        let prefix_bytes = manifest_id.storage_key();
        self.store
            .exists_by_index(0, &prefix_bytes)
            .map_err(|e| SystemError::Storage(e.to_string()))
    }

    /// Count pending manifests via index scan.
    pub fn pending_count(&self) -> Result<usize, SystemError> {
        let iter = self.pending_manifest_ids_iter(None, None)?;
        let mut count = 0usize;
        for item in iter {
            item?;
            count += 1;
        }

        Ok(count)
    }
}

impl SimpleSystemTableScan<ManifestId, ManifestCacheEntry> for ManifestTableProvider {
    fn table_name(&self) -> &str {
        ManifestTableSchema::table_name()
    }

    fn arrow_schema(&self) -> SchemaRef {
        ManifestTableSchema::schema()
    }

    fn scan_all_to_batch(&self) -> Result<RecordBatch, SystemError> {
        self.scan_to_record_batch()
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
        state: &dyn datafusion::catalog::Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        // Use the common SimpleSystemTableScan implementation
        self.base_simple_scan(state, projection, filters, limit).await
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
    use crate::providers::manifest::{Manifest, ManifestCacheEntry, SyncState};
    use kalamdb_commons::{NamespaceId, TableId, TableName};
    use kalamdb_store::entity_store::EntityStore;
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
