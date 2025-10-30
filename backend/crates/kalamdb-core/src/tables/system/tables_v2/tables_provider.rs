//! System.tables table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.tables table.
//! Uses the new EntityStore architecture with String keys (table_id).

use super::super::SystemTableProviderExt;
use super::{new_tables_store, TablesStore, TablesTableSchema};
use crate::error::KalamDbError;
use async_trait::async_trait;
use datafusion::arrow::array::{
    ArrayRef, BooleanArray, Int32Array, RecordBatch, StringBuilder, TimestampMillisecondArray,
};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::system::SystemTable;
use kalamdb_store::EntityStoreV2;
use kalamdb_store::StorageBackend;
use std::any::Any;
use std::sync::Arc;

/// System.tables table provider using EntityStore architecture
pub struct TablesTableProvider {
    store: TablesStore,
    schema: SchemaRef,
}

impl std::fmt::Debug for TablesTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TablesTableProvider").finish()
    }
}

impl TablesTableProvider {
    /// Create a new tables table provider
    ///
    /// # Arguments
    /// * `backend` - Storage backend (RocksDB or mock)
    ///
    /// # Returns
    /// A new TablesTableProvider instance
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self {
            store: new_tables_store(backend),
            schema: TablesTableSchema::schema(),
        }
    }

    pub fn create_table(&self, table: SystemTable) -> Result<(), KalamDbError> {
        self.store.put(&table.table_id.to_string(), &table)?;
        Ok(())
    }

    pub fn update_table(&self, table: SystemTable) -> Result<(), KalamDbError> {
        // Check if table exists
        if self.store.get(&table.table_id.to_string())?.is_none() {
            return Err(KalamDbError::NotFound(format!(
                "Table not found: {}",
                table.table_id
            )));
        }

        self.store.put(&table.table_id.to_string(), &table)?;
        Ok(())
    }

    /// Delete a table entry
    pub fn delete_table(&self, table_id: &str) -> Result<(), KalamDbError> {
        self.store.delete(&table_id.to_string())?;
        Ok(())
    }

    /// Get a table by ID
    pub fn get_table_by_id(&self, table_id: &str) -> Result<Option<SystemTable>, KalamDbError> {
        Ok(self.store.get(&table_id.to_string())?)
    }

    /// Scan all tables and return as RecordBatch
    pub fn scan_all_tables(&self) -> Result<RecordBatch, KalamDbError> {
        let tables = self.store.scan_all()?;

        let mut table_ids = StringBuilder::new();
        let mut table_names = StringBuilder::new();
        let mut namespaces = StringBuilder::new();
        let mut table_types = StringBuilder::new();
        let mut created_ats = Vec::new();
        let mut storage_locations = StringBuilder::new();
        let mut storage_ids = StringBuilder::new();
        let mut use_user_storages = Vec::new();
        let mut flush_policies = StringBuilder::new();
        let mut schema_versions = Vec::new();
        let mut deleted_retention_hours_vec = Vec::new();
        let mut access_levels = StringBuilder::new();

        for (_key, table) in tables {
            table_ids.append_value(&table.table_id.to_string());
            table_names.append_value(table.table_name.as_str());
            namespaces.append_value(table.namespace.as_str());
            table_types.append_value(table.table_type.as_str());
            created_ats.push(Some(table.created_at));
            storage_locations.append_value(&table.storage_location);
            storage_ids.append_option(table.storage_id.as_ref().map(|s| s.as_str()));
            use_user_storages.push(Some(table.use_user_storage));
            flush_policies.append_value(&table.flush_policy);
            schema_versions.push(Some(table.schema_version));
            deleted_retention_hours_vec.push(Some(table.deleted_retention_hours));
            access_levels.append_option(table.access_level.as_ref().map(|a| a.as_str()));
        }

        let batch = RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(table_ids.finish()) as ArrayRef,
                Arc::new(table_names.finish()) as ArrayRef,
                Arc::new(namespaces.finish()) as ArrayRef,
                Arc::new(table_types.finish()) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(created_ats)) as ArrayRef,
                Arc::new(storage_locations.finish()) as ArrayRef,
                Arc::new(storage_ids.finish()) as ArrayRef,
                Arc::new(BooleanArray::from(use_user_storages)) as ArrayRef,
                Arc::new(flush_policies.finish()) as ArrayRef,
                Arc::new(Int32Array::from(schema_versions)) as ArrayRef,
                Arc::new(Int32Array::from(deleted_retention_hours_vec)) as ArrayRef,
                Arc::new(access_levels.finish()) as ArrayRef,
            ],
        )
        .map_err(|e| KalamDbError::Other(format!("Arrow error: {}", e)))?;

        Ok(batch)
    }
}

#[async_trait]
impl TableProvider for TablesTableProvider {
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
        let batch = self.scan_all_tables().map_err(|e| {
            DataFusionError::Execution(format!("Failed to build tables batch: {}", e))
        })?;
        let partitions = vec![vec![batch]];
        let table = MemTable::try_new(schema, partitions)
            .map_err(|e| DataFusionError::Execution(format!("Failed to create MemTable: {}", e)))?;
        table.scan(_state, projection, &[], _limit).await
    }
}

impl SystemTableProviderExt for TablesTableProvider {
    fn table_name(&self) -> &str {
        "system.tables"
    }

    fn schema_ref(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn load_batch(&self) -> Result<RecordBatch, KalamDbError> {
        self.scan_all_tables()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::{
        NamespaceId, StorageId, TableAccess, TableName, TableType as KalamTableType,
    };
    use kalamdb_store::test_utils::InMemoryBackend;

    fn create_test_provider() -> TablesTableProvider {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        TablesTableProvider::new(backend)
    }

    fn create_test_table(table_id: &str, table_name: &str) -> SystemTable {
        SystemTable {
            table_id: TableId::new(NamespaceId::new("default"), TableName::new(table_name)),
            table_name: TableName::new(table_name),
            namespace: NamespaceId::new("default"),
            table_type: KalamTableType::User,
            created_at: 1000,
            storage_location: "/data".to_string(),
            storage_id: Some(StorageId::new("local")),
            use_user_storage: false,
            flush_policy: "{}".to_string(),
            schema_version: 1,
            deleted_retention_hours: 24,
            access_level: None,
        }
    }

    #[test]
    fn test_create_and_get_table() {
        let provider = create_test_provider();
        let table = create_test_table("table1", "conversations");

        provider.create_table(table.clone()).unwrap();

        let retrieved = provider.get_table_by_id("table1").unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.table_id.as_str(), "default:conversations");
        assert_eq!(retrieved.table_name.as_str(), "conversations");
    }

    #[test]
    fn test_update_table() {
        let provider = create_test_provider();
        let mut table = create_test_table("table1", "conversations");
        provider.create_table(table.clone()).unwrap();

        // Update
        table.schema_version = 2;
        provider.update_table(table.clone()).unwrap();

        // Verify
        let retrieved = provider.get_table_by_id("table1").unwrap().unwrap();
        assert_eq!(retrieved.schema_version, 2);
    }

    #[test]
    fn test_delete_table() {
        let provider = create_test_provider();
        let table = create_test_table("table1", "conversations");

        provider.create_table(table).unwrap();
        provider.delete_table("table1").unwrap();

        let retrieved = provider.get_table_by_id("table1").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_scan_all_tables() {
        let provider = create_test_provider();

        // Insert multiple tables
        for i in 1..=3 {
            let table = create_test_table(&format!("table{}", i), &format!("table{}", i));
            provider.create_table(table).unwrap();
        }

        // Scan
        let batch = provider.scan_all_tables().unwrap();
        assert_eq!(batch.num_rows(), 3);
        assert_eq!(batch.num_columns(), 12);
    }

    #[tokio::test]
    async fn test_datafusion_scan() {
        let provider = create_test_provider();

        // Insert test data
        let table = create_test_table("table1", "conversations");
        provider.create_table(table).unwrap();

        // Create DataFusion session
        let ctx = datafusion::execution::context::SessionContext::new();
        let state = ctx.state();

        // Scan via DataFusion
        let plan = provider.scan(&state, None, &[], None).await.unwrap();
        assert!(plan.schema().fields().len() > 0);
    }
}
