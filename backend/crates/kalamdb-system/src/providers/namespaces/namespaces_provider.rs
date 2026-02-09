//! System.namespaces table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.namespaces table.
//! Uses the new EntityStore architecture with NamespaceId keys.

use super::{new_namespaces_store, NamespacesStore, NamespacesTableSchema};
use crate::error::{SystemError, SystemResultExt};
use crate::providers::base::{extract_filter_value, SimpleSystemTableScan};
use crate::providers::namespaces::models::Namespace;
use crate::system_table_trait::SystemTableProviderExt;
use async_trait::async_trait;
use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::Result as DataFusionResult;
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::NamespaceId;
use kalamdb_commons::RecordBatchBuilder;
use kalamdb_store::entity_store::{EntityStore, EntityStoreAsync};
use kalamdb_store::StorageBackend;
use std::any::Any;
use std::sync::Arc;

/// System.namespaces table provider using EntityStore architecture
pub struct NamespacesTableProvider {
    store: NamespacesStore,
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

    /// Async version of `create_namespace()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn create_namespace_async(&self, namespace: Namespace) -> Result<(), SystemError> {
        self.store
            .put_async(&namespace.namespace_id, &namespace)
            .await
            .into_system_error("put_async error")?;
        Ok(())
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

    /// Async version of `get_namespace()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn get_namespace_async(
        &self,
        namespace_id: &NamespaceId,
    ) -> Result<Option<Namespace>, SystemError> {
        self.store.get_async(namespace_id).await.into_system_error("get_async error")
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

    /// Async version of `update_namespace()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn update_namespace_async(&self, namespace: Namespace) -> Result<(), SystemError> {
        // Check if namespace exists
        if self.store.get_async(&namespace.namespace_id).await?.is_none() {
            return Err(SystemError::NotFound(format!(
                "Namespace not found: {}",
                namespace.namespace_id
            )));
        }

        self.store
            .put_async(&namespace.namespace_id, &namespace)
            .await
            .into_system_error("put_async error")?;
        Ok(())
    }

    /// Delete a namespace entry
    pub fn delete_namespace(&self, namespace_id: &NamespaceId) -> Result<(), SystemError> {
        self.store.delete(namespace_id)?;
        Ok(())
    }

    /// Async version of `delete_namespace()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn delete_namespace_async(
        &self,
        namespace_id: &NamespaceId,
    ) -> Result<(), SystemError> {
        self.store
            .delete_async(namespace_id)
            .await
            .into_system_error("delete_async error")?;
        Ok(())
    }

    /// List all namespaces
    pub fn list_namespaces(&self) -> Result<Vec<Namespace>, SystemError> {
        let iter = self.store.scan_iterator(None, None)?;
        let mut namespaces = Vec::new();
        for item in iter {
            let (_, ns) = item?;
            namespaces.push(ns);
        }
        Ok(namespaces)
    }

    /// Async version of `list_namespaces()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn list_namespaces_async(&self) -> Result<Vec<Namespace>, SystemError> {
        let results: Vec<(Vec<u8>, Namespace)> = self
            .store
            .scan_all_async(None, None, None)
            .await
            .into_system_error("scan_all_async error")?;
        Ok(results.into_iter().map(|(_, ns)| ns).collect())
    }

    /// Alias for list_namespaces (backward compatibility)
    pub fn scan_all(&self) -> Result<Vec<Namespace>, SystemError> {
        self.list_namespaces()
    }

    /// Async version of `scan_all()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn scan_all_async(&self) -> Result<Vec<Namespace>, SystemError> {
        let results: Vec<(Vec<u8>, Namespace)> = self
            .store
            .scan_all_async(None, None, None)
            .await
            .into_system_error("scan_all_async error")?;
        Ok(results.into_iter().map(|(_, ns)| ns).collect())
    }

    /// Build a RecordBatch from a list of (NamespaceId, Namespace) pairs
    fn build_namespaces_batch(
        &self,
        entries: Vec<(NamespaceId, Namespace)>,
    ) -> Result<RecordBatch, SystemError> {
        let mut namespace_ids = Vec::with_capacity(entries.len());
        let mut names = Vec::with_capacity(entries.len());
        let mut created_ats = Vec::with_capacity(entries.len());
        let mut options = Vec::with_capacity(entries.len());
        let mut table_counts = Vec::with_capacity(entries.len());

        for (_key, ns) in entries {
            namespace_ids.push(Some(ns.namespace_id.as_str().to_string()));
            names.push(Some(ns.name));
            created_ats.push(Some(ns.created_at));
            options.push(ns.options);
            table_counts.push(Some(ns.table_count));
        }

        let mut builder = RecordBatchBuilder::new(NamespacesTableSchema::schema());
        builder
            .add_string_column_owned(namespace_ids)
            .add_string_column_owned(names)
            .add_timestamp_micros_column(created_ats)
            .add_string_column_owned(options)
            .add_int32_column(table_counts);

        let batch = builder.build().into_arrow_error("Failed to create RecordBatch")?;
        Ok(batch)
    }

    /// Scan all namespaces and return as RecordBatch
    pub fn scan_all_namespaces(&self) -> Result<RecordBatch, SystemError> {
        let iter = self.store.scan_iterator(None, None)?;
        let mut entries = Vec::new();
        for item in iter {
            entries.push(item?);
        }
        self.build_namespaces_batch(entries)
    }
}

impl SimpleSystemTableScan<NamespaceId, Namespace> for NamespacesTableProvider {
    fn table_name(&self) -> &str {
        NamespacesTableSchema::table_name()
    }

    fn arrow_schema(&self) -> SchemaRef {
        NamespacesTableSchema::schema()
    }

    fn scan_all_to_batch(&self) -> Result<RecordBatch, SystemError> {
        self.scan_all_namespaces()
    }

    fn scan_to_batch(
        &self,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> Result<RecordBatch, SystemError> {
        // Check for primary key equality filter → O(1) point lookup
        if let Some(ns_id_str) = extract_filter_value(filters, "namespace_id") {
            let namespace_id = NamespaceId::new(&ns_id_str);
            if let Some(ns) = self.store.get(&namespace_id)? {
                return self.build_namespaces_batch(vec![(namespace_id, ns)]);
            }
            return self.build_namespaces_batch(vec![]);
        }

        // Use iterator with early termination on limit
        let iter = self.store.scan_iterator(None, None)?;
        let effective_limit = limit.unwrap_or(100_000);
        let mut entries = Vec::with_capacity(effective_limit.min(1000));
        for item in iter {
            entries.push(item?);
            if entries.len() >= effective_limit {
                break;
            }
        }
        self.build_namespaces_batch(entries)
    }
}

#[async_trait]
impl TableProvider for NamespacesTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        NamespacesTableSchema::schema()
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

impl SystemTableProviderExt for NamespacesTableProvider {
    fn table_name(&self) -> &str {
        NamespacesTableSchema::table_name()
    }

    fn schema_ref(&self) -> SchemaRef {
        NamespacesTableSchema::schema()
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

    fn create_test_namespace(namespace_id: &NamespaceId, name: &str) -> Namespace {
        Namespace {
            namespace_id: namespace_id.clone(),
            name: name.to_string(),
            created_at: 1000,
            options: Some("{}".to_string()),
            table_count: 0,
        }
    }

    #[test]
    fn test_create_and_get_namespace() {
        let provider = create_test_provider();
        let namespace = create_test_namespace(&NamespaceId::new("app"), "app");

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
        let mut namespace = create_test_namespace(&NamespaceId::new("app"), "app");
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
        let namespace = create_test_namespace(&NamespaceId::new("app"), "app");

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
            let namespace = create_test_namespace(
                &NamespaceId::new(&format!("ns{}", i)),
                &format!("namespace{}", i),
            );
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
        let namespace = create_test_namespace(&NamespaceId::new("app"), "app");
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
        let namespace = create_test_namespace(&NamespaceId::new("app"), "app");
        provider.create_namespace(namespace).unwrap();

        // Create DataFusion session
        let ctx = datafusion::execution::context::SessionContext::new();
        let state = ctx.state();

        // Scan via DataFusion
        let plan = provider.scan(&state, None, &[], None).await.unwrap();
        assert!(!plan.schema().fields().is_empty());
    }
}
