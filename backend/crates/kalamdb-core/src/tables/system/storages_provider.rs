//! DataFusion TableProvider for system.storages

use crate::error::KalamDbError;
use crate::tables::system::storages::SystemStorages;
use crate::tables::system::SystemTableProviderExt;
use datafusion::arrow::array::{ArrayRef, RecordBatch, StringArray, TimestampMillisecondArray};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::Result as DataFusionResult;
use datafusion::execution::context::SessionState;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_sql::KalamSql;
use std::any::Any;
use std::sync::Arc;

/// system.storages provider backed by kalamdb-sql metadata
pub struct SystemStoragesProvider {
    kalam_sql: Arc<KalamSql>,
}

impl SystemStoragesProvider {
    pub fn new(kalam_sql: Arc<KalamSql>) -> Self {
        Self { kalam_sql }
    }

    /// Convert system.storages records into Arrow RecordBatch
    fn create_batch(&self) -> Result<RecordBatch, KalamDbError> {
        // Query system_storages CF for all storage configurations
        let storages = self
            .kalam_sql
            .scan_all_storages()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan system.storages: {}", e)))?;

        let mut storage_ids = Vec::new();
        let mut storage_names = Vec::new();
        let mut descriptions = Vec::new();
        let mut storage_types = Vec::new();
        let mut base_directories = Vec::new();
        let mut shared_tables_templates = Vec::new();
        let mut user_tables_templates = Vec::new();
        let mut created_ats = Vec::new();
        let mut updated_ats = Vec::new();

        for storage in storages {
            storage_ids.push(storage.storage_id);
            storage_names.push(storage.storage_name);
            descriptions.push(storage.description);
            storage_types.push(storage.storage_type);
            base_directories.push(storage.base_directory);
            shared_tables_templates.push(storage.shared_tables_template);
            user_tables_templates.push(storage.user_tables_template);
            created_ats.push(storage.created_at);
            updated_ats.push(storage.updated_at);
        }

        let batch = RecordBatch::try_new(
            SystemStorages::schema(),
            vec![
                Arc::new(StringArray::from(storage_ids)) as ArrayRef,
                Arc::new(StringArray::from(storage_names)) as ArrayRef,
                Arc::new(StringArray::from(descriptions)) as ArrayRef,
                Arc::new(StringArray::from(storage_types)) as ArrayRef,
                Arc::new(StringArray::from(base_directories)) as ArrayRef,
                Arc::new(StringArray::from(shared_tables_templates)) as ArrayRef,
                Arc::new(StringArray::from(user_tables_templates)) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(created_ats)) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(updated_ats)) as ArrayRef,
            ],
        )
        .map_err(|e| {
            KalamDbError::Other(format!("Failed to create system.storages batch: {}", e))
        })?;

        Ok(batch)
    }
}

impl SystemTableProviderExt for SystemStoragesProvider {
    fn table_name(&self) -> &'static str {
        kalamdb_commons::constants::SystemTableNames::STORAGES
    }

    fn schema_ref(&self) -> SchemaRef {
        SystemStorages::schema()
    }

    fn load_batch(&self) -> Result<RecordBatch, KalamDbError> {
        self.create_batch()
    }
}

#[async_trait::async_trait]
impl TableProvider for SystemStoragesProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        SystemStorages::schema()
    }

    async fn scan(
        &self,
        _state: &SessionState,
        projection: Option<&Vec<usize>>,
        _filters: &[datafusion::prelude::Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        self.into_memory_exec(projection)
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }
}
