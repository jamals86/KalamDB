//! System.manifest table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.manifest table.
//! Exposes manifest cache entries as a queryable system table.

use super::{new_manifest_store, ManifestStore, ManifestTableSchema};
use crate::error::SystemError;
use crate::system_table_trait::SystemTableProviderExt;
use async_trait::async_trait;
use datafusion::arrow::array::{
    ArrayRef, Int64Array, RecordBatch, StringBuilder, TimestampMillisecondArray,
};
use datafusion::arrow::datatypes::SchemaRef;
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
        let entries = self.store.scan_all()?;
        let row_count = entries.len();

        // Pre-allocate builders based on schema field order
        let mut cache_keys = StringBuilder::with_capacity(row_count, row_count * 32);
        let mut namespace_ids = StringBuilder::with_capacity(row_count, row_count * 16);
        let mut table_names = StringBuilder::with_capacity(row_count, row_count * 16);
        let mut scopes = StringBuilder::with_capacity(row_count, row_count * 16);
        let mut etags = StringBuilder::with_capacity(row_count, row_count * 16);
        let mut last_refreshed_vals = Vec::with_capacity(row_count);
        let mut last_accessed_vals = Vec::with_capacity(row_count);
        let mut ttl_vals = Vec::with_capacity(row_count);
        let mut source_paths = StringBuilder::with_capacity(row_count, row_count * 64);
        let mut sync_states = StringBuilder::with_capacity(row_count, row_count * 8);

        // Build arrays by iterating entries once
        for (key_bytes, entry) in entries {
            let cache_key = String::from_utf8_lossy(&key_bytes);

            // Parse cache key (format: namespace:table:scope)
            let parts: Vec<&str> = cache_key.split(':').collect();
            if parts.len() != 3 {
                log::warn!("Invalid cache key format: {}", cache_key);
                continue;
            }

            cache_keys.append_value(&cache_key);
            namespace_ids.append_value(parts[0]);
            table_names.append_value(parts[1]);
            scopes.append_value(parts[2]);
            etags.append_option(entry.etag.as_deref());
            last_refreshed_vals.push(Some(entry.last_refreshed * 1000)); // Convert to milliseconds
            last_accessed_vals.push(Some(entry.last_refreshed * 1000)); // Fallback to last_refreshed
            ttl_vals.push(Some(0i64)); // Use 0 to indicate "see config"
            source_paths.append_value(&entry.source_path);
            sync_states.append_value(entry.sync_state.to_string());
        }

        // Build RecordBatch with schema-ordered columns (order matches ManifestTableSchema)
        RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(cache_keys.finish()) as ArrayRef,
                Arc::new(namespace_ids.finish()) as ArrayRef,
                Arc::new(table_names.finish()) as ArrayRef,
                Arc::new(scopes.finish()) as ArrayRef,
                Arc::new(etags.finish()) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(last_refreshed_vals)) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(last_accessed_vals)) as ArrayRef,
                Arc::new(Int64Array::from(ttl_vals)) as ArrayRef,
                Arc::new(source_paths.finish()) as ArrayRef,
                Arc::new(sync_states.finish()) as ArrayRef,
            ],
        )
        .map_err(|e| SystemError::Other(format!("Arrow error: {}", e)))
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
