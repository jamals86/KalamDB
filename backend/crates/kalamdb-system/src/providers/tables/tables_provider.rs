//! System.tables table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.tables table.
//! Uses the new EntityStore architecture with TableId keys and TableDefinition values.

use super::{new_tables_store, TablesStore, TablesTableSchema};
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
use kalamdb_commons::models::TableId;
use kalamdb_commons::schemas::TableDefinition;
use kalamdb_store::entity_store::{EntityStore, EntityStoreAsync};
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

    pub fn create_table(
        &self,
        table_id: &TableId,
        table_def: &TableDefinition,
    ) -> Result<(), SystemError> {
        self.store.put(table_id, table_def)?;
        Ok(())
    }

    /// Async version of `create_table()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn create_table_async(
        &self,
        table_id: &TableId,
        table_def: &TableDefinition,
    ) -> Result<(), SystemError> {
        self.store
            .put_async(table_id, table_def)
            .await
            .map_err(|e| SystemError::Other(format!("put_async error: {}", e)))?;
        Ok(())
    }

    pub fn update_table(
        &self,
        table_id: &TableId,
        table_def: &TableDefinition,
    ) -> Result<(), SystemError> {
        // Check if table exists
        if self.store.get(table_id)?.is_none() {
            return Err(SystemError::NotFound(format!(
                "Table not found: {}",
                table_id
            )));
        }

        self.store.put(table_id, table_def)?;
        Ok(())
    }

    /// Async version of `update_table()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn update_table_async(
        &self,
        table_id: &TableId,
        table_def: &TableDefinition,
    ) -> Result<(), SystemError> {
        // Check if table exists
        if self.store.get_async(table_id).await?.is_none() {
            return Err(SystemError::NotFound(format!(
                "Table not found: {}",
                table_id
            )));
        }

        self.store
            .put_async(table_id, table_def)
            .await
            .map_err(|e| SystemError::Other(format!("put_async error: {}", e)))?;
        Ok(())
    }

    /// Delete a table entry
    pub fn delete_table(&self, table_id: &TableId) -> Result<(), SystemError> {
        self.store.delete(table_id)?;
        Ok(())
    }

    /// Async version of `delete_table()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn delete_table_async(&self, table_id: &TableId) -> Result<(), SystemError> {
        self.store
            .delete_async(table_id)
            .await
            .map_err(|e| SystemError::Other(format!("delete_async error: {}", e)))?;
        Ok(())
    }

    /// Get a table by ID
    pub fn get_table_by_id(
        &self,
        table_id: &TableId,
    ) -> Result<Option<TableDefinition>, SystemError> {
        Ok(self.store.get(table_id)?)
    }

    /// Async version of `get_table_by_id()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn get_table_by_id_async(
        &self,
        table_id: &TableId,
    ) -> Result<Option<TableDefinition>, SystemError> {
        self.store
            .get_async(table_id)
            .await
            .map_err(|e| SystemError::Other(format!("get_async error: {}", e)))
    }

    /// List all table definitions
    pub fn list_tables(&self) -> Result<Vec<TableDefinition>, SystemError> {
        let iter = self.store.scan_iterator(None, None)?;
        let mut tables = Vec::new();
        for item in iter {
            let (_, table_def) = item?;
            tables.push(table_def);
        }
        Ok(tables)
    }

    /// Async version of `list_tables()` - offloads to blocking thread pool.
    ///
    /// Use this in async contexts to avoid blocking the Tokio runtime.
    pub async fn list_tables_async(&self) -> Result<Vec<TableDefinition>, SystemError> {
        let results: Vec<(Vec<u8>, TableDefinition)> = self
            .store
            .scan_all_async(None, None, None)
            .await
            .map_err(|e| SystemError::Other(format!("scan_all_async error: {}", e)))?;
        Ok(results.into_iter().map(|(_, td)| td).collect())
    }

    /// Alias for list_tables (backward compatibility)
    pub fn scan_all(&self) -> Result<Vec<TableDefinition>, SystemError> {
        self.list_tables()
    }

    /// Scan all tables and return as RecordBatch
    pub fn scan_all_tables(&self) -> Result<RecordBatch, SystemError> {
        use kalamdb_store::entity_store::EntityStore;

        let iter = self.store.scan_iterator(None, None)?;
        let mut tables = Vec::new();
        for item in iter {
            tables.push(item?);
        }
        let row_count = tables.len();

        // Pre-allocate builders for optimal performance
        let mut table_ids = StringBuilder::with_capacity(row_count, row_count * 32);
        let mut table_names = StringBuilder::with_capacity(row_count, row_count * 16);
        let mut namespaces = StringBuilder::with_capacity(row_count, row_count * 16); // corresponds to column name 'namespace_id'
        let mut table_types = StringBuilder::with_capacity(row_count, row_count * 16);
        let mut created_ats = Vec::with_capacity(row_count);
        let mut schema_versions = Vec::with_capacity(row_count);
        let mut table_comments = StringBuilder::with_capacity(row_count, row_count * 64);
        let mut updated_ats = Vec::with_capacity(row_count);
        let mut options_json = StringBuilder::with_capacity(row_count, row_count * 128);
        let mut access_levels = StringBuilder::with_capacity(row_count, row_count * 16);

        for (_table_id, table_def) in tables {
            // Convert TableId to string format: "namespace:table_name"
            let table_id_str = format!(
                "{}:{}",
                table_def.namespace_id.as_str(),
                table_def.table_name.as_str()
            );
            table_ids.append_value(&table_id_str);
            table_names.append_value(table_def.table_name.as_str());
            namespaces.append_value(table_def.namespace_id.as_str());
            table_types.append_value(table_def.table_type.as_str());
            created_ats.push(Some(table_def.created_at.timestamp_millis()));
            schema_versions.push(Some(table_def.schema_version as i32));
            table_comments.append_option(table_def.table_comment.as_deref());
            updated_ats.push(Some(table_def.updated_at.timestamp_millis()));
            // Serialize TableOptions enum to JSON
            match serde_json::to_string(&table_def.table_options) {
                Ok(json) => options_json.append_value(&json),
                Err(e) => options_json.append_value(format!(
                    "{{\"error\":\"failed to serialize options: {}\"}}",
                    e
                )),
            }
            // Access Level (only for Shared tables, null otherwise)
            use kalamdb_commons::schemas::TableOptions;
            if let TableOptions::Shared(opts) = &table_def.table_options {
                if let Some(access) = &opts.access_level {
                    access_levels.append_value(access.as_str());
                } else {
                    access_levels.append_null();
                }
            } else {
                access_levels.append_null();
            }
        }

        let batch = RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(table_ids.finish()) as ArrayRef,   // 1 table_id
                Arc::new(table_names.finish()) as ArrayRef, // 2 table_name
                Arc::new(namespaces.finish()) as ArrayRef,  // 3 namespace_id
                Arc::new(table_types.finish()) as ArrayRef, // 4 table_type
                Arc::new(TimestampMillisecondArray::from(created_ats)) as ArrayRef, // 5 created_at
                Arc::new(Int32Array::from(schema_versions)) as ArrayRef, // 6 schema_version
                Arc::new(table_comments.finish()) as ArrayRef, // 7 table_comment
                Arc::new(TimestampMillisecondArray::from(updated_ats)) as ArrayRef, // 8 updated_at
                Arc::new(options_json.finish()) as ArrayRef, // 9 options
                Arc::new(access_levels.finish()) as ArrayRef, // 10 access_level
            ],
        )
        .map_err(|e| SystemError::Other(format!("Arrow error: {}", e)))?;

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

    fn load_batch(&self) -> Result<RecordBatch, SystemError> {
        self.scan_all_tables()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::datatypes::KalamDataType;
    use kalamdb_commons::schemas::{
        ColumnDefinition, TableDefinition, TableOptions, TableType as KalamTableType,
    };
    use kalamdb_commons::{NamespaceId, TableId, TableName};
    use kalamdb_store::test_utils::InMemoryBackend;

    fn create_test_provider() -> TablesTableProvider {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        TablesTableProvider::new(backend)
    }

    fn create_test_table(namespace: &str, table_name: &str) -> (TableId, TableDefinition) {
        let namespace_id = NamespaceId::new(namespace);
        let table_name_id = TableName::new(table_name);
        let table_id = TableId::new(namespace_id.clone(), table_name_id.clone());

        let columns = vec![
            ColumnDefinition::new(
                "id",
                1,
                KalamDataType::Uuid,
                false,
                true,
                false,
                kalamdb_commons::schemas::ColumnDefault::None,
                None,
            ),
            ColumnDefinition::new(
                "name",
                2,
                KalamDataType::Text,
                false,
                false,
                false,
                kalamdb_commons::schemas::ColumnDefault::None,
                None,
            ),
        ];

        let table_def = TableDefinition::new(
            namespace_id,
            table_name_id,
            KalamTableType::User,
            columns,
            TableOptions::user(),
            None,
        )
        .expect("Failed to create table definition");

        (table_id, table_def)
    }

    #[test]
    fn test_create_and_get_table() {
        let provider = create_test_provider();
        let (table_id, table_def) = create_test_table("default", "conversations");

        provider.create_table(&table_id, &table_def).unwrap();

        let retrieved = provider.get_table_by_id(&table_id).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.namespace_id.as_str(), "default");
        assert_eq!(retrieved.table_name.as_str(), "conversations");
    }

    #[test]
    fn test_update_table() {
        let provider = create_test_provider();
        let (table_id, mut table_def) = create_test_table("default", "conversations");
        provider.create_table(&table_id, &table_def).unwrap();

        // Update
        table_def.schema_version = 2;
        provider.update_table(&table_id, &table_def).unwrap();

        // Verify
        let retrieved = provider.get_table_by_id(&table_id).unwrap().unwrap();
        assert_eq!(retrieved.schema_version, 2);
    }

    #[test]
    fn test_delete_table() {
        let provider = create_test_provider();
        let (table_id, table_def) = create_test_table("default", "conversations");

        provider.create_table(&table_id, &table_def).unwrap();
        provider.delete_table(&table_id).unwrap();

        let retrieved = provider.get_table_by_id(&table_id).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_scan_all_tables() {
        let provider = create_test_provider();

        // Insert multiple tables
        for i in 1..=3 {
            let (table_id, table_def) = create_test_table("default", &format!("table{}", i));
            provider.create_table(&table_id, &table_def).unwrap();
        }

        // Scan
        let batch = provider.scan_all_tables().unwrap();
        assert_eq!(batch.num_rows(), 3);
        assert_eq!(batch.num_columns(), 10); // Schema exposes 10 fields including access_level metadata
    }

    #[tokio::test]
    async fn test_datafusion_scan() {
        let provider = create_test_provider();

        // Insert test data
        let (table_id, table_def) = create_test_table("default", "conversations");
        provider.create_table(&table_id, &table_def).unwrap();

        // Create DataFusion session
        let ctx = datafusion::execution::context::SessionContext::new();
        let state = ctx.state();

        // Scan via DataFusion
        let plan = provider.scan(&state, None, &[], None).await.unwrap();
        assert!(plan.schema().fields().len() > 0);
    }
}
