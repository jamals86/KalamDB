//! System.manifest table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.manifest table.
//! Exposes manifest cache entries as a queryable system table.

use super::{new_manifest_store, ManifestStore, ManifestTableSchema};
use crate::error::{SystemError, SystemResultExt};
use crate::system_table_trait::SystemTableProviderExt;
use async_trait::async_trait;
use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::datatypes::SchemaRef;
use kalamdb_commons::RecordBatchBuilder;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_store::entity_store::EntityStore;
use kalamdb_store::StorageBackend;
use std::any::Any;
use std::sync::Arc;

/// System.manifest table provider using EntityStore architecture
pub struct ManifestTableProvider {
    store: ManifestStore,
    schema: SchemaRef,
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
            schema: ManifestTableSchema::schema(),
        }
    }

    /// Scan all manifest cache entries and return as RecordBatch
    ///
    /// This is the main method used by DataFusion to read the table.
    ///
    /// Uses schema-driven array building for optimal performance and correctness.
    pub fn scan_to_record_batch(&self) -> Result<RecordBatch, SystemError> {
        let entries = self.store.scan_all(None, None, None)?;

        // Extract data into vectors
        let mut cache_keys = Vec::with_capacity(entries.len());
        let mut namespace_ids = Vec::with_capacity(entries.len());
        let mut table_names = Vec::with_capacity(entries.len());
        let mut scopes = Vec::with_capacity(entries.len());
        let mut etags = Vec::with_capacity(entries.len());
        let mut last_refreshed_vals = Vec::with_capacity(entries.len());
        let mut last_accessed_vals = Vec::with_capacity(entries.len());
        let mut ttl_vals = Vec::with_capacity(entries.len());
        let mut source_paths = Vec::with_capacity(entries.len());
        let mut sync_states = Vec::with_capacity(entries.len());

        // Build arrays by iterating entries once
        for (key_bytes, entry) in entries {
            let cache_key = String::from_utf8_lossy(&key_bytes);

            // Parse cache key (format: namespace:table:scope)
            let parts: Vec<&str> = cache_key.split(':').collect();
            if parts.len() != 3 {
                log::warn!("Invalid cache key format: {}", cache_key);
                continue;
            }

            cache_keys.push(Some(cache_key.to_string()));
            namespace_ids.push(Some(parts[0].to_string()));
            table_names.push(Some(parts[1].to_string()));
            scopes.push(Some(parts[2].to_string()));
            etags.push(entry.etag);
            last_refreshed_vals.push(Some(entry.last_refreshed * 1000)); // Convert to milliseconds
            last_accessed_vals.push(Some(entry.last_refreshed * 1000)); // Fallback to last_refreshed
            ttl_vals.push(Some(0i64)); // Use 0 to indicate "see config"
            source_paths.push(Some(entry.source_path));
            sync_states.push(Some(entry.sync_state.to_string()));
        }

        // Build batch using RecordBatchBuilder
        let mut builder = RecordBatchBuilder::new(self.schema.clone());
        builder
            .add_string_column_owned(cache_keys)
            .add_string_column_owned(namespace_ids)
            .add_string_column_owned(table_names)
            .add_string_column_owned(scopes)
            .add_string_column_owned(etags)
            .add_timestamp_micros_column(last_refreshed_vals)
            .add_timestamp_micros_column(last_accessed_vals)
            .add_int64_column(ttl_vals)
            .add_string_column_owned(source_paths)
            .add_string_column_owned(sync_states);

        builder.build().into_arrow_error("Failed to create RecordBatch")
    }
}

#[async_trait]
impl TableProvider for ManifestTableProvider {
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
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        use datafusion::datasource::MemTable;
        let schema = self.schema.clone();
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
        self.schema.clone()
    }

    fn load_batch(&self) -> Result<RecordBatch, SystemError> {
        self.scan_to_record_batch()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::types::{ManifestCacheEntry, SyncState};
    use kalamdb_store::test_utils::InMemoryBackend;

    #[tokio::test]
    async fn test_empty_manifest_table() {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let provider = ManifestTableProvider::new(backend);

        let batch = provider.scan_to_record_batch().unwrap();
        assert_eq!(batch.num_rows(), 0);
        assert_eq!(batch.num_columns(), 10);
    }

    #[tokio::test]
    async fn test_manifest_table_with_entries() {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let provider = ManifestTableProvider::new(backend.clone());

        // Insert test entries directly via store
        let entry = ManifestCacheEntry::new(
            r#"{"table_id":"test","scope":"shared"}"#.to_string(),
            Some("etag123".to_string()),
            1000,
            "s3://bucket/ns1/tbl1/manifest.json".to_string(),
            SyncState::InSync,
        );

        use super::super::manifest_store::ManifestCacheKey;
        let key = ManifestCacheKey::from("ns1:tbl1:shared");
        EntityStore::put(&provider.store, &key, &entry).unwrap();

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
    }
}
