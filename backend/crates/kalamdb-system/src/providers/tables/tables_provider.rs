//! System.tables table provider
//!
//! Phase 16: Consolidated provider using single store with TableVersionId keys.
//! Exposes all table versions with is_latest flag for schema history queries.

use super::{new_tables_store, TablesStore, TablesTableSchema};
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
use kalamdb_commons::models::TableId;
use kalamdb_commons::schemas::TableDefinition;
use kalamdb_store::StorageBackend;
use std::any::Any;
use std::sync::Arc;

/// System.tables table provider using consolidated store with versioning
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

    /// Create a new table entry (stores version 1)
    pub fn create_table(
        &self,
        table_id: &TableId,
        table_def: &TableDefinition,
    ) -> Result<(), SystemError> {
        Ok(self.store.put_version(table_id, table_def)?)
    }

    /// Async version of `create_table()`
    pub async fn create_table_async(
        &self,
        table_id: &TableId,
        table_def: &TableDefinition,
    ) -> Result<(), SystemError> {
        let table_id = table_id.clone();
        let table_def = table_def.clone();
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || store.put_version(&table_id, &table_def))
            .await
            .into_system_error("spawn_blocking error")?
            .map_err(SystemError::from)
    }

    /// Update a table (stores new version and updates latest pointer)
    pub fn update_table(
        &self,
        table_id: &TableId,
        table_def: &TableDefinition,
    ) -> Result<(), SystemError> {
        // Check if table exists
        if self.store.get_latest(table_id)?.is_none() {
            return Err(SystemError::NotFound(format!(
                "Table not found: {}",
                table_id
            )));
        }

        Ok(self.store.put_version(table_id, table_def)?)
    }

    /// Async version of `update_table()`
    pub async fn update_table_async(
        &self,
        table_id: &TableId,
        table_def: &TableDefinition,
    ) -> Result<(), SystemError> {
        let table_id = table_id.clone();
        let table_def = table_def.clone();
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || {
            // Check if table exists
            if store.get_latest(&table_id)?.is_none() {
                return Err(SystemError::NotFound(format!(
                    "Table not found: {}",
                    table_id
                )));
            }
            store.put_version(&table_id, &table_def)?;
            Ok(())
        })
        .await
        .into_system_error("spawn_blocking error")?
    }

    /// Delete a table entry (removes all versions)
    pub fn delete_table(&self, table_id: &TableId) -> Result<(), SystemError> {
        self.store.delete_all_versions(table_id)?;
        Ok(())
    }

    /// Async version of `delete_table()`
    pub async fn delete_table_async(&self, table_id: &TableId) -> Result<(), SystemError> {
        let table_id = table_id.clone();
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || store.delete_all_versions(&table_id))
            .await
            .into_system_error("spawn_blocking error")?
            .map_err(SystemError::from)?;
        Ok(())
    }

    /// Store a versioned schema entry (alias for put_version)
    pub fn put_versioned_schema(
        &self,
        table_id: &TableId,
        table_def: &TableDefinition,
    ) -> Result<(), SystemError> {
        Ok(self.store.put_version(table_id, table_def)?)
    }

    /// Get the latest version of a table by ID
    pub fn get_table_by_id(
        &self,
        table_id: &TableId,
    ) -> Result<Option<TableDefinition>, SystemError> {
        Ok(self.store.get_latest(table_id)?)
    }

    /// Get a specific version of a table definition
    ///
    /// Used for schema evolution when reading flushed Parquet files.
    pub fn get_version(
        &self,
        table_id: &TableId,
        version: u32,
    ) -> Result<Option<TableDefinition>, SystemError> {
        Ok(self.store.get_version(table_id, version)?)
    }

    /// Async version of `get_table_by_id()`
    pub async fn get_table_by_id_async(
        &self,
        table_id: &TableId,
    ) -> Result<Option<TableDefinition>, SystemError> {
        let table_id = table_id.clone();
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || store.get_latest(&table_id))
            .await
            .into_system_error("spawn_blocking error")?
            .map_err(SystemError::from)
    }

    /// List all latest table definitions
    pub fn list_tables(&self) -> Result<Vec<TableDefinition>, SystemError> {
        let tables = self.store.scan_all_latest()?;
        Ok(tables.into_iter().map(|(_, def)| def).collect())
    }

    /// List all tables in a specific namespace (latest versions only)
    pub fn list_tables_in_namespace(
        &self,
        namespace_id: &kalamdb_commons::models::NamespaceId,
    ) -> Result<Vec<TableDefinition>, SystemError> {
        let tables = self.store.scan_namespace(namespace_id)?;
        Ok(tables.into_iter().map(|(_, def)| def).collect())
    }

    /// Async version of `list_tables()`
    pub async fn list_tables_async(&self) -> Result<Vec<TableDefinition>, SystemError> {
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || {
            let tables = store.scan_all_latest()?;
            Ok(tables.into_iter().map(|(_, def)| def).collect())
        })
        .await
        .into_system_error("spawn_blocking error")?
    }

    /// Alias for list_tables (backward compatibility)
    pub fn scan_all(&self) -> Result<Vec<TableDefinition>, SystemError> {
        self.list_tables()
    }

    /// Scan all tables and return as RecordBatch (latest versions only)
    pub fn scan_all_tables(&self) -> Result<RecordBatch, SystemError> {
        // Only return latest versions to avoid duplicates
        // (the store has both <lat> pointer AND <ver>N entries)
        let entries = self.store.scan_all_latest()?;

        // Extract data into vectors
        let mut table_ids = Vec::with_capacity(entries.len());
        let mut table_names = Vec::with_capacity(entries.len());
        let mut namespaces = Vec::with_capacity(entries.len());
        let mut table_types = Vec::with_capacity(entries.len());
        let mut created_ats = Vec::with_capacity(entries.len());
        let mut schema_versions = Vec::with_capacity(entries.len());
        let mut columns_json = Vec::with_capacity(entries.len());
        let mut table_comments = Vec::with_capacity(entries.len());
        let mut updated_ats = Vec::with_capacity(entries.len());
        let mut options_json = Vec::with_capacity(entries.len());
        let mut access_levels = Vec::with_capacity(entries.len());
        let mut is_latest_flags = Vec::with_capacity(entries.len());
        let mut storage_ids = Vec::with_capacity(entries.len());
        let mut use_user_storage_flags = Vec::with_capacity(entries.len());

        for (_table_id, table_def) in entries {
            // All entries from scan_all_latest are latest versions
            let is_latest = true;
            // Convert TableId to string format
            let table_id_str = format!(
                "{}:{}",
                table_def.namespace_id.as_str(),
                table_def.table_name.as_str()
            );
            table_ids.push(Some(table_id_str));
            table_names.push(Some(table_def.table_name.as_str().to_string()));
            namespaces.push(Some(table_def.namespace_id.as_str().to_string()));
            table_types.push(Some(table_def.table_type.as_str().to_string()));
            created_ats.push(Some(table_def.created_at.timestamp_millis()));
            schema_versions.push(Some(table_def.schema_version as i32));
            
            // Serialize columns as JSON array
            let col_json = match serde_json::to_string(&table_def.columns) {
                Ok(json) => json,
                Err(e) => format!(
                    "{{\"error\":\"failed to serialize columns: {}\"}}",
                    e
                ),
            };
            columns_json.push(Some(col_json));
            
            table_comments.push(table_def.table_comment);
            updated_ats.push(Some(table_def.updated_at.timestamp_millis()));
            
            // Serialize TableOptions
            let opt_json = match serde_json::to_string(&table_def.table_options) {
                Ok(json) => json,
                Err(e) => format!(
                    "{{\"error\":\"failed to serialize options: {}\"}}",
                    e
                ),
            };
            options_json.push(Some(opt_json));
            
            // Access Level (only for Shared tables)
            use kalamdb_commons::schemas::TableOptions;
            let access_level = if let TableOptions::Shared(opts) = &table_def.table_options {
                opts.access_level.as_ref().map(|a| a.as_str().to_string())
            } else {
                None
            };
            access_levels.push(access_level);
            
            // is_latest flag
            is_latest_flags.push(Some(is_latest));

            // Extract storage_id from TableOptions
            let storage_id = match &table_def.table_options {
                TableOptions::User(opts) => Some(opts.storage_id.as_str().to_string()),
                TableOptions::Shared(opts) => Some(opts.storage_id.as_str().to_string()),
                TableOptions::Stream(_) => Some("local".to_string()),
                TableOptions::System(_) => Some("local".to_string()),
            };
            storage_ids.push(storage_id);

            // Extract use_user_storage from TableOptions (only for User tables)
            let use_user_storage = match &table_def.table_options {
                TableOptions::User(opts) => Some(opts.use_user_storage),
                _ => None,
            };
            use_user_storage_flags.push(use_user_storage);
        }

        // Build batch using RecordBatchBuilder
        let mut builder = RecordBatchBuilder::new(self.schema.clone());
        builder
            .add_string_column_owned(table_ids)
            .add_string_column_owned(table_names)
            .add_string_column_owned(namespaces)
            .add_string_column_owned(table_types)
            .add_timestamp_micros_column(created_ats)
            .add_int32_column(schema_versions)
            .add_string_column_owned(columns_json)
            .add_string_column_owned(table_comments)
            .add_timestamp_micros_column(updated_ats)
            .add_string_column_owned(options_json)
            .add_string_column_owned(access_levels)
            .add_boolean_column(is_latest_flags)
            .add_string_column_owned(storage_ids)
            .add_boolean_column(use_user_storage_flags);

        let batch = builder.build().into_arrow_error("Failed to create RecordBatch")?;

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
                1,
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
                2,
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

        // Verify latest version
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

        // Scan - should have 6 rows (3 latest + 3 versioned)
        let batch = provider.scan_all_tables().unwrap();
        assert_eq!(batch.num_rows(), 6);
        assert_eq!(batch.num_columns(), 12); // All 12 columns from system.tables definition
    }

    #[test]
    fn test_scan_all_tables_with_versions() {
        let provider = create_test_provider();

        // Create table with multiple versions
        let (table_id, mut table_def) = create_test_table("default", "users");
        provider.create_table(&table_id, &table_def).unwrap();
        
        table_def.schema_version = 2;
        provider.update_table(&table_id, &table_def).unwrap();

        // Scan - should have 3 rows (1 latest + 2 versioned)
        let batch = provider.scan_all_tables().unwrap();
        assert_eq!(batch.num_rows(), 3);
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
        assert!(!plan.schema().fields().is_empty());
    }
}
