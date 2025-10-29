//! System.storages table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.storages table.
//! Uses the new EntityStore architecture with StorageId keys.

use super::super::SystemTableProviderExt;
use super::{new_storages_store, StoragesStore, StoragesTableSchema};
use crate::error::KalamDbError;
use async_trait::async_trait;
use datafusion::arrow::array::{ArrayRef, RecordBatch, StringBuilder, TimestampMillisecondArray};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_store::StorageBackend;
use kalamdb_commons::system::Storage;
use kalamdb_commons::StorageId;
use kalamdb_store::EntityStoreV2;
use std::any::Any;
use std::sync::Arc;

/// System.storages table provider using EntityStore architecture
pub struct StoragesTableProvider {
    store: StoragesStore,
    schema: SchemaRef,
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
            schema: StoragesTableSchema::schema(),
        }
    }

    /// Create a new storage entry
    pub fn create_storage(&self, storage: Storage) -> Result<(), KalamDbError> {
        self.store.put(&storage.storage_id, &storage)?;
        Ok(())
    }

    /// Alias for create_storage (for backward compatibility)
    pub fn insert_storage(&self, storage: Storage) -> Result<(), KalamDbError> {
        self.create_storage(storage)
    }

    /// Get a storage by ID
    pub fn get_storage_by_id(&self, storage_id: &StorageId) -> Result<Option<Storage>, KalamDbError> {
        Ok(self.store.get(storage_id)?)
    }

    /// Alias for get_storage_by_id (for backward compatibility)
    pub fn get_storage(&self, storage_id: &StorageId) -> Result<Option<Storage>, KalamDbError> {
        self.get_storage_by_id(storage_id)
    }

    /// Update an existing storage entry
    pub fn update_storage(&self, storage: Storage) -> Result<(), KalamDbError> {
        // Check if storage exists
        if self.store.get(&storage.storage_id)?.is_none() {
            return Err(KalamDbError::NotFound(format!(
                "Storage not found: {}",
                storage.storage_id
            )));
        }

        self.store.put(&storage.storage_id, &storage)?;
        Ok(())
    }

    /// Delete a storage entry
    pub fn delete_storage(&self, storage_id: &StorageId) -> Result<(), KalamDbError> {
        self.store.delete(storage_id)?;
        Ok(())
    }

    /// List all storages
    pub fn list_storages(&self) -> Result<Vec<Storage>, KalamDbError> {
        let storages = self.store.scan_all()?;
        Ok(storages.into_iter().map(|(_, s)| s).collect())
    }

    /// Scan all storages and return as RecordBatch
    pub fn scan_all_storages(&self) -> Result<RecordBatch, KalamDbError> {
        let storages = self.store.scan_all()?;

        let mut storage_ids = StringBuilder::new();
        let mut storage_names = StringBuilder::new();
        let mut descriptions = StringBuilder::new();
        let mut storage_types = StringBuilder::new();
        let mut base_directories = StringBuilder::new();
        let mut credentials = StringBuilder::new();
        let mut shared_tables_templates = StringBuilder::new();
        let mut user_tables_templates = StringBuilder::new();
        let mut created_ats = Vec::new();
        let mut updated_ats = Vec::new();

        for (_key, storage) in storages {
            storage_ids.append_value(storage.storage_id.as_str());
            storage_names.append_value(&storage.storage_name);
            descriptions.append_option(storage.description.as_deref());
            storage_types.append_value(&storage.storage_type);
            base_directories.append_value(&storage.base_directory);
            credentials.append_option(storage.credentials.as_deref());
            shared_tables_templates.append_value(&storage.shared_tables_template);
            user_tables_templates.append_value(&storage.user_tables_template);
            created_ats.push(Some(storage.created_at));
            updated_ats.push(Some(storage.updated_at));
        }

        let batch = RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(storage_ids.finish()) as ArrayRef,
                Arc::new(storage_names.finish()) as ArrayRef,
                Arc::new(descriptions.finish()) as ArrayRef,
                Arc::new(storage_types.finish()) as ArrayRef,
                Arc::new(base_directories.finish()) as ArrayRef,
                Arc::new(credentials.finish()) as ArrayRef,
                Arc::new(shared_tables_templates.finish()) as ArrayRef,
                Arc::new(user_tables_templates.finish()) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(created_ats)) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(updated_ats)) as ArrayRef,
            ],
        )
        .map_err(|e| KalamDbError::Other(format!("Arrow error: {}", e)))?;

        Ok(batch)
    }
}

#[async_trait]
impl TableProvider for StoragesTableProvider {
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
        let batch = self.scan_all_storages().map_err(|e| {
            DataFusionError::Execution(format!("Failed to build storages batch: {}", e))
        })?;
        let partitions = vec![vec![batch]];
        let table = MemTable::try_new(schema, partitions)
            .map_err(|e| DataFusionError::Execution(format!("Failed to create MemTable: {}", e)))?;
        table.scan(_state, projection, &[], _limit).await
    }
}

impl SystemTableProviderExt for StoragesTableProvider {
    fn table_name(&self) -> &'static str {
        "system.storages"
    }

    fn schema_ref(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn load_batch(&self) -> Result<RecordBatch, KalamDbError> {
        self.scan_all_storages()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_store::InMemoryBackend;

    fn create_test_provider() -> StoragesTableProvider {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        StoragesTableProvider::new(backend)
    }

    fn create_test_storage(storage_id: &str, name: &str) -> Storage {
        Storage {
            storage_id: StorageId::new(storage_id),
            storage_name: name.to_string(),
            description: Some("Test storage".to_string()),
            storage_type: "filesystem".to_string(),
            base_directory: "/data".to_string(),
            credentials: None,
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

        let storage_id = StorageId::new("local");
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
        let storage_id = StorageId::new("local");
        let retrieved = provider.get_storage(&storage_id).unwrap().unwrap();
        assert_eq!(retrieved.description, Some("Updated description".to_string()));
    }

    #[test]
    fn test_delete_storage() {
        let provider = create_test_provider();
        let storage = create_test_storage("local", "Local Storage");

        provider.create_storage(storage).unwrap();
        
        let storage_id = StorageId::new("local");
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
        assert_eq!(batch.num_columns(), 10);
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
        assert!(plan.schema().fields().len() > 0);
    }
}
