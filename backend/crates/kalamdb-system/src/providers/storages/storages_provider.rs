//! System.storages table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.storages table.
//! Uses the new EntityStore architecture with StorageId keys.

use super::{new_storages_store, StoragesStore, StoragesTableSchema};
use crate::error::{SystemError, SystemResultExt};
use crate::providers::base::{extract_filter_value, SimpleSystemTableScan};
use crate::providers::storages::models::Storage;
use crate::system_table_trait::SystemTableProviderExt;
use async_trait::async_trait;
use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::Result as DataFusionResult;
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::{RecordBatchBuilder, StorageId};
use kalamdb_store::entity_store::{EntityStore, EntityStoreAsync};
use kalamdb_store::StorageBackend;
use std::any::Any;
use std::sync::Arc;

/// System.storages table provider using EntityStore architecture
pub struct StoragesTableProvider {
    store: StoragesStore,
}

impl std::fmt::Debug for StoragesTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoragesTableProvider").finish()
    }
}

impl StoragesTableProvider {
    /// Create a new storages table provider
    ///
    /// # Arguments
    /// * `backend` - Storage backend (RocksDB or mock)
    ///
    /// # Returns
    /// A new StoragesTableProvider instance
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self {
            store: new_storages_store(backend),
        }
    }

    /// Create a new storage entry
    pub fn create_storage(&self, storage: Storage) -> Result<(), SystemError> {
        self.store.put(&storage.storage_id, &storage)?;
        Ok(())
    }

    /// Async version of `create_storage()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn create_storage_async(&self, storage: Storage) -> Result<(), SystemError> {
        self.store
            .put_async(&storage.storage_id, &storage)
            .await
            .into_system_error("put_async error")?;
        Ok(())
    }

    /// Alias for create_storage (for backward compatibility)
    pub fn insert_storage(&self, storage: Storage) -> Result<(), SystemError> {
        self.create_storage(storage)
    }

    /// Get a storage by ID
    pub fn get_storage_by_id(
        &self,
        storage_id: &StorageId,
    ) -> Result<Option<Storage>, SystemError> {
        Ok(self.store.get(storage_id)?)
    }

    /// Async version of `get_storage_by_id()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn get_storage_by_id_async(
        &self,
        storage_id: &StorageId,
    ) -> Result<Option<Storage>, SystemError> {
        self.store.get_async(storage_id).await.into_system_error("get_async error")
    }

    /// Alias for get_storage_by_id (for backward compatibility)
    pub fn get_storage(&self, storage_id: &StorageId) -> Result<Option<Storage>, SystemError> {
        self.get_storage_by_id(storage_id)
    }

    /// Async version of `get_storage()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn get_storage_async(
        &self,
        storage_id: &StorageId,
    ) -> Result<Option<Storage>, SystemError> {
        self.get_storage_by_id_async(storage_id).await
    }

    /// Update an existing storage entry
    pub fn update_storage(&self, storage: Storage) -> Result<(), SystemError> {
        // Check if storage exists
        if self.store.get(&storage.storage_id)?.is_none() {
            return Err(SystemError::NotFound(format!(
                "Storage not found: {}",
                storage.storage_id
            )));
        }

        self.store.put(&storage.storage_id, &storage)?;
        Ok(())
    }

    /// Async version of `update_storage()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn update_storage_async(&self, storage: Storage) -> Result<(), SystemError> {
        // Check if storage exists
        if self.store.get_async(&storage.storage_id).await?.is_none() {
            return Err(SystemError::NotFound(format!(
                "Storage not found: {}",
                storage.storage_id
            )));
        }

        self.store
            .put_async(&storage.storage_id, &storage)
            .await
            .into_system_error("put_async error")?;
        Ok(())
    }

    /// Delete a storage entry
    pub fn delete_storage(&self, storage_id: &StorageId) -> Result<(), SystemError> {
        self.store.delete(storage_id)?;
        Ok(())
    }

    /// Async version of `delete_storage()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn delete_storage_async(&self, storage_id: &StorageId) -> Result<(), SystemError> {
        self.store
            .delete_async(storage_id)
            .await
            .into_system_error("delete_async error")?;
        Ok(())
    }

    /// List all storages
    pub fn list_storages(&self) -> Result<Vec<Storage>, SystemError> {
        let iter = self.store.scan_iterator(None, None)?;
        let mut storages = Vec::new();
        for item in iter {
            let (_, s) = item?;
            storages.push(s);
        }
        Ok(storages)
    }

    /// Async version of `list_storages()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn list_storages_async(&self) -> Result<Vec<Storage>, SystemError> {
        let results: Vec<(Vec<u8>, Storage)> = self
            .store
            .scan_all_async(None, None, None)
            .await
            .into_system_error("scan_all_async error")?;
        Ok(results.into_iter().map(|(_, s)| s).collect())
    }

    /// Build a RecordBatch from a list of (StorageId, Storage) pairs
    fn build_storages_batch(
        &self,
        entries: Vec<(StorageId, Storage)>,
    ) -> Result<RecordBatch, SystemError> {
        let mut storage_ids = Vec::with_capacity(entries.len());
        let mut storage_names = Vec::with_capacity(entries.len());
        let mut descriptions = Vec::with_capacity(entries.len());
        let mut storage_types = Vec::with_capacity(entries.len());
        let mut base_directories = Vec::with_capacity(entries.len());
        let mut credentials = Vec::with_capacity(entries.len());
        let mut config_jsons = Vec::with_capacity(entries.len());
        let mut shared_templates = Vec::with_capacity(entries.len());
        let mut user_templates = Vec::with_capacity(entries.len());
        let mut created_ats = Vec::with_capacity(entries.len());
        let mut updated_ats = Vec::with_capacity(entries.len());

        for (_key, storage) in entries {
            storage_ids.push(Some(storage.storage_id.to_string()));
            storage_names.push(Some(storage.storage_name));
            descriptions.push(storage.description);
            storage_types.push(Some(storage.storage_type.to_string()));
            base_directories.push(Some(storage.base_directory));
            credentials.push(storage.credentials);
            config_jsons.push(storage.config_json);
            shared_templates.push(Some(storage.shared_tables_template));
            user_templates.push(Some(storage.user_tables_template));
            created_ats.push(Some(storage.created_at));
            updated_ats.push(Some(storage.updated_at));
        }

        let mut builder = RecordBatchBuilder::new(StoragesTableSchema::schema());
        builder
            .add_string_column_owned(storage_ids)
            .add_string_column_owned(storage_names)
            .add_string_column_owned(descriptions)
            .add_string_column_owned(storage_types)
            .add_string_column_owned(base_directories)
            .add_string_column_owned(credentials)
            .add_string_column_owned(config_jsons)
            .add_string_column_owned(shared_templates)
            .add_string_column_owned(user_templates)
            .add_timestamp_micros_column(created_ats)
            .add_timestamp_micros_column(updated_ats);

        let batch = builder.build().into_arrow_error("Failed to create RecordBatch")?;
        Ok(batch)
    }

    /// Scan all storages and return as RecordBatch
    pub fn scan_all_storages(&self) -> Result<RecordBatch, SystemError> {
        let iter = self.store.scan_iterator(None, None)?;
        let mut entries = Vec::new();
        for item in iter {
            entries.push(item?);
        }
        entries.sort_by(|a, b| {
            let storage_a = &a.1;
            let storage_b = &b.1;
            if storage_a.storage_id.is_local() {
                std::cmp::Ordering::Less
            } else if storage_b.storage_id.is_local() {
                std::cmp::Ordering::Greater
            } else {
                storage_a.storage_id.as_str().cmp(storage_b.storage_id.as_str())
            }
        });

        self.build_storages_batch(entries)
    }
}

impl SimpleSystemTableScan<StorageId, Storage> for StoragesTableProvider {
    fn table_name(&self) -> &str {
        "system.storages"
    }

    fn arrow_schema(&self) -> SchemaRef {
        StoragesTableSchema::schema()
    }

    fn scan_all_to_batch(&self) -> Result<RecordBatch, SystemError> {
        self.scan_all_storages()
    }

    fn scan_to_batch(
        &self,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> Result<RecordBatch, SystemError> {
        // Check for primary key equality filter → O(1) point lookup
        if let Some(storage_id_str) = extract_filter_value(filters, "storage_id") {
            let storage_id = StorageId::new(&storage_id_str);
            if let Some(storage) = self.store.get(&storage_id)? {
                return self.build_storages_batch(vec![(storage_id, storage)]);
            }
            return self.build_storages_batch(vec![]);
        }

        // With limit: use iterator with early termination (skip sort)
        if let Some(lim) = limit {
            let iter = self.store.scan_iterator(None, None)?;
            let mut entries = Vec::with_capacity(lim.min(1000));
            for item in iter {
                entries.push(item?);
                if entries.len() >= lim {
                    break;
                }
            }
            return self.build_storages_batch(entries);
        }

        // No limit: full scan with sort (default behavior)
        self.scan_all_storages()
    }
}

#[async_trait]
impl TableProvider for StoragesTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        StoragesTableSchema::schema()
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

impl SystemTableProviderExt for StoragesTableProvider {
    fn table_name(&self) -> &str {
        StoragesTableSchema::table_name()
    }

    fn schema_ref(&self) -> SchemaRef {
        StoragesTableSchema::schema()
    }

    fn load_batch(&self) -> Result<RecordBatch, SystemError> {
        self.scan_all_storages()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StorageType;
    use kalamdb_store::test_utils::InMemoryBackend;

    fn create_test_provider() -> StoragesTableProvider {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        StoragesTableProvider::new(backend)
    }

    fn create_test_storage(storage_id: &str, name: &str) -> Storage {
        Storage {
            storage_id: StorageId::new(storage_id),
            storage_name: name.to_string(),
            description: Some("Test storage".to_string()),
            storage_type: StorageType::Filesystem,
            base_directory: "/data".to_string(),
            credentials: None,
            config_json: None,
            shared_tables_template: "{base}/shared/{namespace}/{table}".to_string(),
            user_tables_template: "{base}/user/{namespace}/{table}/{user_id}".to_string(),
            created_at: 1000,
            updated_at: 1000,
        }
    }

    #[test]
    fn test_create_and_get_storage() {
        let provider = create_test_provider();
        let storage = create_test_storage("local", "Local Storage");

        provider.create_storage(storage.clone()).unwrap();

        let storage_id = StorageId::local();
        let retrieved = provider.get_storage(&storage_id).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.storage_id, storage_id);
        assert_eq!(retrieved.storage_name, "Local Storage");
    }

    #[test]
    fn test_update_storage() {
        let provider = create_test_provider();
        let mut storage = create_test_storage("local", "Local Storage");
        provider.create_storage(storage.clone()).unwrap();

        // Update
        storage.description = Some("Updated description".to_string());
        provider.update_storage(storage.clone()).unwrap();

        // Verify
        let storage_id = StorageId::local();
        let retrieved = provider.get_storage(&storage_id).unwrap().unwrap();
        assert_eq!(retrieved.description, Some("Updated description".to_string()));
    }

    #[test]
    fn test_delete_storage() {
        let provider = create_test_provider();
        let storage = create_test_storage("local", "Local Storage");

        provider.create_storage(storage).unwrap();

        let storage_id = StorageId::local();
        provider.delete_storage(&storage_id).unwrap();

        let retrieved = provider.get_storage(&storage_id).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_list_storages() {
        let provider = create_test_provider();

        // Insert multiple storages
        for i in 1..=3 {
            let storage = create_test_storage(&format!("storage{}", i), &format!("Storage {}", i));
            provider.create_storage(storage).unwrap();
        }

        // List
        let storages = provider.list_storages().unwrap();
        assert_eq!(storages.len(), 3);
    }

    #[test]
    fn test_scan_all_storages() {
        let provider = create_test_provider();

        // Insert test data
        let storage = create_test_storage("local", "Local Storage");
        provider.create_storage(storage).unwrap();

        // Scan
        let batch = provider.scan_all_storages().unwrap();
        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), 11); // storage_id, storage_name, description, storage_type, base_directory, credentials, config_json, shared_tables_template, user_tables_template, created_at, updated_at
    }

    #[tokio::test]
    async fn test_datafusion_scan() {
        let provider = create_test_provider();

        // Insert test data
        let storage = create_test_storage("local", "Local Storage");
        provider.create_storage(storage).unwrap();

        // Create DataFusion session
        let ctx = datafusion::execution::context::SessionContext::new();
        let state = ctx.state();

        // Scan via DataFusion
        let plan = provider.scan(&state, None, &[], None).await.unwrap();
        assert!(!plan.schema().fields().is_empty());
    }
}
