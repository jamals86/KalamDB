//! System.manifest table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.manifest table.
//! Exposes manifest cache entries as a queryable system table.

use super::{create_manifest_indexes, ManifestTableSchema};
use crate::error::{SystemError, SystemResultExt};
use crate::providers::base::{extract_filter_value, SimpleProviderDefinition};
use crate::providers::manifest::ManifestCacheEntry;
use datafusion::arrow::array::RecordBatch;
use datafusion::logical_expr::Expr;
use kalamdb_commons::{ManifestId, StorageKey, TableId};
use kalamdb_store::entity_store::EntityStore;
use kalamdb_store::{IndexedEntityStore, StorageBackend};
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
        let entries = self.scan_entries(None)?;
        self.build_batch_from_entries(entries)
    }

    /// Collect manifest entries with optional early limit.
    fn scan_entries(
        &self,
        limit: Option<usize>,
    ) -> Result<Vec<(ManifestId, ManifestCacheEntry)>, SystemError> {
        let iter = self
            .store
            .scan_iterator(None, None)
            .map_err(|e| SystemError::Storage(e.to_string()))?;

        let mut entries = Vec::with_capacity(limit.unwrap_or(256));
        let mut count = 0usize;
        for row in iter {
            if limit.is_some_and(|lim| count >= lim) {
                break;
            }
            entries.push(row.map_err(|e| SystemError::Storage(e.to_string()))?);
            count += 1;
        }
        Ok(entries)
    }

    /// Build a RecordBatch from materialized manifest entries.
    fn build_batch_from_entries(
        &self,
        entries: Vec<(ManifestId, ManifestCacheEntry)>,
    ) -> Result<RecordBatch, SystemError> {
        crate::build_record_batch!(
            schema: ManifestTableSchema::schema(),
            entries: entries,
            columns: [
                cache_keys => OptionalString(|entry| Some(entry.0.as_str())),
                namespace_ids => OptionalString(|entry| Some(entry.0.table_id().namespace_id().as_str())),
                table_names => OptionalString(|entry| Some(entry.0.table_id().table_name().as_str())),
                scopes => OptionalString(|entry| Some(entry.0.scope_str())),
                etags => OptionalString(|entry| entry.1.etag.as_deref()),
                last_refreshed_vals => Timestamp(|entry| Some(entry.1.last_refreshed_millis())),
                // last_accessed = last_refreshed (moka manages TTI internally).
                last_accessed_vals => Timestamp(|entry| Some(entry.1.last_refreshed_millis())),
                in_memory_vals => OptionalBoolean(|entry| Some(self.is_in_memory(&entry.0.as_str()))),
                sync_states => OptionalString(|entry| Some(entry.1.sync_state.to_string())),
                manifest_jsons => OptionalString(|entry| Some(entry.1.manifest_json()))
            ]
        )
        .into_arrow_error("Failed to create RecordBatch")
    }

    /// Build a RecordBatch from a single manifest entry (point lookup result).
    fn build_single_entry_batch(
        &self,
        manifest_id: &ManifestId,
        entry: &ManifestCacheEntry,
    ) -> Result<RecordBatch, SystemError> {
        self.build_batch_from_entries(vec![(manifest_id.clone(), entry.clone())])
    }

    /// Scan manifest entries with a limit (early termination).
    fn scan_to_record_batch_limited(&self, limit: usize) -> Result<RecordBatch, SystemError> {
        let entries = self.scan_entries(Some(limit))?;
        self.build_batch_from_entries(entries)
    }

    /// Iterator over pending manifest IDs (from the pending-write index).
    pub fn pending_manifest_ids_iter(
        &self,
        prefix: Option<&[u8]>,
        limit: Option<usize>,
    ) -> Result<Box<dyn Iterator<Item = Result<ManifestId, SystemError>> + Send + '_>, SystemError>
    {
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
    fn scan_to_batch_filtered(
        &self,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> Result<RecordBatch, SystemError> {
        // Check for primary key equality filter → O(1) point lookup
        if let Some(cache_key_str) = extract_filter_value(filters, "cache_key") {
            let manifest_id = ManifestId::from(cache_key_str.as_str());
            if let Ok(Some(entry)) = self.store.get(&manifest_id) {
                return self.build_single_entry_batch(&manifest_id, &entry);
            }
            // Empty result
            return Ok(RecordBatch::new_empty(ManifestTableSchema::schema()));
        }

        // With limit: use iterator with early termination
        if let Some(lim) = limit {
            return self.scan_to_record_batch_limited(lim);
        }

        // No filters/limit: full scan
        self.scan_to_record_batch()
    }

    fn provider_definition() -> SimpleProviderDefinition {
        SimpleProviderDefinition {
            table_name: ManifestTableSchema::table_name(),
            schema: ManifestTableSchema::schema,
        }
    }
}

crate::impl_simple_system_table_provider!(
    provider = ManifestTableProvider,
    key = ManifestId,
    value = ManifestCacheEntry,
    definition = provider_definition,
    scan_all = scan_to_record_batch,
    scan_filtered = scan_to_batch_filtered
);

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
        let entry =
            ManifestCacheEntry::new(manifest, Some("etag123".to_string()), 1000, SyncState::InSync);

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
