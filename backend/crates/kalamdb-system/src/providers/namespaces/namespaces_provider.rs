//! System.namespaces table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.namespaces table.
//! Uses the new EntityStore architecture with NamespaceId keys.

use super::{new_namespaces_store, NamespacesStore, NamespacesTableSchema};
use crate::error::SystemError;
use crate::system_table_trait::SystemTableProviderExt;
use async_trait::async_trait;
use datafusion::arrow::array::{
    ArrayRef, Int32Array, RecordBatch, StringBuilder, TimestampMillisecondArray,
};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::system::Namespace;
use kalamdb_commons::NamespaceId;
use kalamdb_store::entity_store::EntityStore;
use kalamdb_store::StorageBackend;
use std::any::Any;
use std::sync::Arc;

/// System.namespaces table provider using EntityStore architecture
pub struct NamespacesTableProvider {
    store: NamespacesStore,
    schema: SchemaRef,
}

impl std::fmt::Debug for NamespacesTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NamespacesTableProvider").finish()
    }
}

impl NamespacesTableProvider {
    /// Create a new namespaces table provider
    ///
    /// # Arguments
    /// * `backend` - Storage backend (RocksDB or mock)
    ///
    /// # Returns
    /// A new NamespacesTableProvider instance
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self {
            store: new_namespaces_store(backend),
            schema: NamespacesTableSchema::schema(),
        }
    }

    /// Create a new namespace entry
    pub fn create_namespace(&self, namespace: Namespace) -> Result<(), SystemError> {
        self.store.put(&namespace.namespace_id, &namespace)?;
        Ok(())
    }

    /// Alias for create_namespace (for backward compatibility)
    pub fn insert_namespace(&self, namespace: Namespace) -> Result<(), SystemError> {
        self.create_namespace(namespace)
    }

    /// Get a namespace by ID
    pub fn get_namespace_by_id(
        &self,
        namespace_id: &NamespaceId,
    ) -> Result<Option<Namespace>, SystemError> {
        Ok(self.store.get(namespace_id)?)
    }

    /// Alias for get_namespace_by_id (for backward compatibility)
    pub fn get_namespace(
        &self,
        namespace_id: &NamespaceId,
    ) -> Result<Option<Namespace>, SystemError> {
        self.get_namespace_by_id(namespace_id)
    }

    /// Update an existing namespace entry
    pub fn update_namespace(&self, namespace: Namespace) -> Result<(), SystemError> {
        // Check if namespace exists
        if self.store.get(&namespace.namespace_id)?.is_none() {
            return Err(SystemError::NotFound(format!(
                "Namespace not found: {}",
                namespace.namespace_id
            )));
        }

        self.store.put(&namespace.namespace_id, &namespace)?;
        Ok(())
    }

    /// Delete a namespace entry
    pub fn delete_namespace(&self, namespace_id: &NamespaceId) -> Result<(), SystemError> {
        self.store.delete(namespace_id)?;
        Ok(())
    }

    /// List all namespaces
    pub fn list_namespaces(&self) -> Result<Vec<Namespace>, SystemError> {
        let namespaces = self.store.scan_all()?;
        Ok(namespaces.into_iter().map(|(_, ns)| ns).collect())
    }

    /// Alias for list_namespaces (backward compatibility)
    pub fn scan_all(&self) -> Result<Vec<Namespace>, SystemError> {
        self.list_namespaces()
    }

    /// Scan all namespaces and return as RecordBatch
    pub fn scan_all_namespaces(&self) -> Result<RecordBatch, SystemError> {
        let namespaces = self.store.scan_all()?;
        let row_count = namespaces.len();

        // Pre-allocate builders for optimal performance
        let mut namespace_ids = StringBuilder::with_capacity(row_count, row_count * 16);
        let mut names = StringBuilder::with_capacity(row_count, row_count * 32);
        let mut created_ats = Vec::with_capacity(row_count);
        let mut options = StringBuilder::with_capacity(row_count, row_count * 64);
        let mut table_counts = Vec::with_capacity(row_count);

        for (_key, ns) in namespaces {
            namespace_ids.append_value(ns.namespace_id.as_str());
            names.append_value(&ns.name);
            created_ats.push(Some(ns.created_at));
            options.append_option(ns.options.as_deref());
            table_counts.push(ns.table_count);
        }

        let batch = RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(namespace_ids.finish()) as ArrayRef,
                Arc::new(names.finish()) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(created_ats)) as ArrayRef,
                Arc::new(options.finish()) as ArrayRef,
                Arc::new(Int32Array::from(table_counts)) as ArrayRef,
            ],
        )
        .map_err(|e| SystemError::Other(format!("Arrow error: {}", e)))?;

        Ok(batch)
    }
}

#[async_trait]
impl TableProvider for NamespacesTableProvider {
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
        let batch = self.scan_all_namespaces().map_err(|e| {
            DataFusionError::Execution(format!("Failed to build namespaces batch: {}", e))
        })?;
        let partitions = vec![vec![batch]];
        let table = MemTable::try_new(schema, partitions)
            .map_err(|e| DataFusionError::Execution(format!("Failed to create MemTable: {}", e)))?;
        table.scan(_state, projection, &[], _limit).await
    }
}

impl SystemTableProviderExt for NamespacesTableProvider {
    fn table_name(&self) -> &str {
        "system.namespaces"
    }

    fn schema_ref(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn load_batch(&self) -> Result<RecordBatch, SystemError> {
        self.scan_all_namespaces()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_store::test_utils::InMemoryBackend;

    fn create_test_provider() -> NamespacesTableProvider {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        NamespacesTableProvider::new(backend)
    }

    fn create_test_namespace(namespace_id: &str, name: &str) -> Namespace {
        Namespace {
            namespace_id: NamespaceId::new(namespace_id),
            name: name.to_string(),
            created_at: 1000,
            options: Some("{}".to_string()),
            table_count: 0,
        }
    }

    #[test]
    fn test_create_and_get_namespace() {
        let provider = create_test_provider();
        let namespace = create_test_namespace("app", "app");

        provider.create_namespace(namespace.clone()).unwrap();

        let namespace_id = NamespaceId::new("app");
        let retrieved = provider.get_namespace(&namespace_id).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.namespace_id, namespace_id);
        assert_eq!(retrieved.name, "app");
    }

    #[test]
    fn test_update_namespace() {
        let provider = create_test_provider();
        let mut namespace = create_test_namespace("app", "app");
        provider.create_namespace(namespace.clone()).unwrap();

        // Update
        namespace.table_count = 5;
        provider.update_namespace(namespace.clone()).unwrap();

        // Verify
        let namespace_id = NamespaceId::new("app");
        let retrieved = provider.get_namespace(&namespace_id).unwrap().unwrap();
        assert_eq!(retrieved.table_count, 5);
    }

    #[test]
    fn test_delete_namespace() {
        let provider = create_test_provider();
        let namespace = create_test_namespace("app", "app");

        provider.create_namespace(namespace).unwrap();

        let namespace_id = NamespaceId::new("app");
        provider.delete_namespace(&namespace_id).unwrap();

        let retrieved = provider.get_namespace(&namespace_id).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_list_namespaces() {
        let provider = create_test_provider();

        // Insert multiple namespaces
        for i in 1..=3 {
            let namespace = create_test_namespace(&format!("ns{}", i), &format!("namespace{}", i));
            provider.create_namespace(namespace).unwrap();
        }

        // List
        let namespaces = provider.list_namespaces().unwrap();
        assert_eq!(namespaces.len(), 3);
    }

    #[test]
    fn test_scan_all_namespaces() {
        let provider = create_test_provider();

        // Insert test data
        let namespace = create_test_namespace("app", "app");
        provider.create_namespace(namespace).unwrap();

        // Scan
        let batch = provider.scan_all_namespaces().unwrap();
        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), 5);
    }

    #[tokio::test]
    async fn test_datafusion_scan() {
        let provider = create_test_provider();

        // Insert test data
        let namespace = create_test_namespace("app", "app");
        provider.create_namespace(namespace).unwrap();

        // Create DataFusion session
        let ctx = datafusion::execution::context::SessionContext::new();
        let state = ctx.state();

        // Scan via DataFusion
        let plan = provider.scan(&state, None, &[], None).await.unwrap();
        assert!(plan.schema().fields().len() > 0);
    }
}
