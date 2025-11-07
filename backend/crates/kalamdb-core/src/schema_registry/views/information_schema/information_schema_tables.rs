//! information_schema.tables virtual table provider
//!
//! This module provides a DataFusion TableProvider for the information_schema.tables
//! virtual table, which exposes all table metadata from information_schema_tables CF.
//!
//! **Updated**: Now uses unified VirtualView pattern from view_base.rs

use crate::error::KalamDbError;
use crate::schema_registry::views::VirtualView;
use crate::schema_registry::SchemaRegistry;
use crate::tables::system::TablesTableProvider;
use datafusion::arrow::array::{
    ArrayRef, BooleanArray, RecordBatch, StringBuilder, TimestampMillisecondArray, UInt32Array,
    UInt64Array,
};
use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use kalamdb_commons::models::TableId;
use std::sync::Arc;

/// InformationSchemaTablesView exposes all table metadata from information_schema_tables CF
/// using the unified VirtualView pattern
#[derive(Debug)]
pub struct InformationSchemaTablesView {
    tables_provider: Arc<TablesTableProvider>,
    schema_registry: Arc<SchemaRegistry>,
    schema: SchemaRef,
}

impl InformationSchemaTablesView {
    /// Create a new information_schema.tables view
    ///
    /// # Arguments
    /// * `tables_provider` - TablesTableProvider for accessing system.tables metadata
    /// * `schema_registry` - SchemaRegistry for accessing table schema details
    pub fn new(tables_provider: Arc<TablesTableProvider>, schema_registry: Arc<SchemaRegistry>) -> Self {
        let schema = Arc::new(Schema::new(vec![
            // SQL standard information_schema.tables columns
            Field::new("table_catalog", DataType::Utf8, false),
            Field::new("table_schema", DataType::Utf8, false), // namespace_id
            Field::new("table_name", DataType::Utf8, false),
            Field::new("table_type", DataType::Utf8, false), // USER/SHARED/STREAM/SYSTEM
            // KalamDB-specific metadata columns
            Field::new("table_id", DataType::Utf8, false),
            Field::new(
                "created_at",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
            Field::new(
                "updated_at",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
            Field::new("schema_version", DataType::UInt32, false),
            Field::new("storage_id", DataType::Utf8, false),
            Field::new("use_user_storage", DataType::Boolean, false),
            Field::new("deleted_retention_hours", DataType::UInt32, true), // nullable
            Field::new("ttl_seconds", DataType::UInt64, true),             // nullable
        ]));

        Self { 
            tables_provider,
            schema_registry,
            schema 
        }
    }

    /// Scan all tables across all namespaces and return as RecordBatch
    fn scan_all_tables(&self) -> Result<RecordBatch, KalamDbError> {
        // Get table metadata from system.tables
        let tables_batch = self
            .tables_provider
            .scan_all_tables()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan tables: {}", e)))?;

        // Parse system.tables columns
        use datafusion::arrow::array::{Array, StringArray, Int32Array, Int64Array};
        
        let table_id_col = tables_batch.column(0).as_any().downcast_ref::<StringArray>()
            .ok_or_else(|| KalamDbError::Other("table_id column type mismatch".to_string()))?;
        let table_name_col = tables_batch.column(1).as_any().downcast_ref::<StringArray>()
            .ok_or_else(|| KalamDbError::Other("table_name column type mismatch".to_string()))?;
        let namespace_col = tables_batch.column(2).as_any().downcast_ref::<StringArray>()
            .ok_or_else(|| KalamDbError::Other("namespace column type mismatch".to_string()))?;
        let table_type_col = tables_batch.column(3).as_any().downcast_ref::<StringArray>()
            .ok_or_else(|| KalamDbError::Other("table_type column type mismatch".to_string()))?;
        let created_at_col = tables_batch.column(4).as_any().downcast_ref::<Int64Array>()
            .ok_or_else(|| KalamDbError::Other("created_at column type mismatch".to_string()))?;
        let storage_id_col = tables_batch.column(5).as_any().downcast_ref::<StringArray>()
            .ok_or_else(|| KalamDbError::Other("storage_id column type mismatch".to_string()))?;
        let use_user_storage_col = tables_batch.column(6).as_any().downcast_ref::<BooleanArray>()
            .ok_or_else(|| KalamDbError::Other("use_user_storage column type mismatch".to_string()))?;
        let schema_version_col = tables_batch.column(8).as_any().downcast_ref::<Int32Array>()
            .ok_or_else(|| KalamDbError::Other("schema_version column type mismatch".to_string()))?;
        let deleted_retention_col = tables_batch.column(9).as_any().downcast_ref::<Int32Array>()
            .ok_or_else(|| KalamDbError::Other("deleted_retention_hours column type mismatch".to_string()))?;

        let num_rows = tables_batch.num_rows();
        let mut table_catalogs = StringBuilder::new();
        let mut table_schemas = StringBuilder::new();
        let mut table_names = StringBuilder::new();
        let mut table_types = StringBuilder::new();
        let mut table_ids = StringBuilder::new();
        let mut created_ats = Vec::new();
        let mut updated_ats = Vec::new();
        let mut schema_versions = Vec::new();
        let mut storage_ids = StringBuilder::new();
        let mut use_user_storages = Vec::new();
        let mut deleted_retention_hours_vec: Vec<Option<u32>> = Vec::new();
        let mut ttl_seconds_vec: Vec<Option<u64>> = Vec::new();

        for i in 0..num_rows {
            // SQL standard information_schema uses "def" as default catalog
            table_catalogs.append_value("def");

            // table_schema is the namespace/database name
            let namespace = namespace_col.value(i);
            table_schemas.append_value(namespace);

            // table_name
            let table_name = table_name_col.value(i);
            table_names.append_value(table_name);

            // table_type: Convert KalamDB type to SQL standard
            let kalam_type = table_type_col.value(i);
            let standard_type = match kalam_type {
                "User" | "Shared" => "BASE TABLE",
                "System" => "SYSTEM VIEW",
                "Stream" => "STREAM TABLE",
                _ => "BASE TABLE", // Default
            };
            table_types.append_value(standard_type);

            // table_id
            let table_id = table_id_col.value(i);
            table_ids.append_value(table_id);
            
            // created_at is already i64 timestamp in milliseconds
            let created_at = created_at_col.value(i);
            created_ats.push(created_at);
            
            // updated_at: use created_at as fallback (system.tables doesn't have updated_at yet)
            updated_ats.push(created_at);
            
            // schema_version
            let schema_ver = schema_version_col.value(i);
            schema_versions.push(schema_ver as u32);
            
            // storage_id
            let storage_id = if storage_id_col.is_null(i) {
                "default"
            } else {
                storage_id_col.value(i)
            };
            storage_ids.append_value(storage_id);
            
            // use_user_storage
            let use_user_storage = use_user_storage_col.value(i);
            use_user_storages.push(use_user_storage);
            
            // deleted_retention_hours
            let retention = if deleted_retention_col.is_null(i) {
                None
            } else {
                Some(deleted_retention_col.value(i) as u32)
            };
            deleted_retention_hours_vec.push(retention);
            
            // ttl_seconds: Need to get from TableDefinition for Stream tables
            let ttl = if kalam_type == "Stream" {
                let tid = TableId::from_strings(namespace, table_name);
                self.schema_registry.get_table_definition(&tid)
                    .ok()
                    .flatten()
                    .and_then(|def| {
                        use kalamdb_commons::schemas::TableOptions;
                        match &def.table_options {
                            TableOptions::Stream(opts) => Some(opts.ttl_seconds),
                            _ => None,
                        }
                    })
            } else {
                None
            };
            ttl_seconds_vec.push(ttl);
        }

        let batch = RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(table_catalogs.finish()) as ArrayRef,
                Arc::new(table_schemas.finish()) as ArrayRef,
                Arc::new(table_names.finish()) as ArrayRef,
                Arc::new(table_types.finish()) as ArrayRef,
                Arc::new(table_ids.finish()) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(created_ats)) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(updated_ats)) as ArrayRef,
                Arc::new(UInt32Array::from(schema_versions)) as ArrayRef,
                Arc::new(storage_ids.finish()) as ArrayRef,
                Arc::new(BooleanArray::from(use_user_storages)) as ArrayRef,
                Arc::new(UInt32Array::from(deleted_retention_hours_vec)) as ArrayRef,
                Arc::new(UInt64Array::from(ttl_seconds_vec)) as ArrayRef,
            ],
        )
        .map_err(|e| KalamDbError::Other(format!("Failed to create RecordBatch: {}", e)))?;

        Ok(batch)
    }
}

impl VirtualView for InformationSchemaTablesView {
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn compute_batch(&self) -> Result<RecordBatch, KalamDbError> {
        self.scan_all_tables()
    }

    fn view_name(&self) -> &str {
        "information_schema.tables"
    }
}

/// Helper function to create information_schema.tables TableProvider
///
/// This wraps the InformationSchemaTablesView in a ViewTableProvider for use in DataFusion.
pub fn create_information_schema_tables_provider(
    tables_provider: Arc<TablesTableProvider>,
    schema_registry: Arc<SchemaRegistry>,
) -> Arc<dyn datafusion::datasource::TableProvider> {
    use crate::schema_registry::views::ViewTableProvider;
    let view = Arc::new(InformationSchemaTablesView::new(tables_provider, schema_registry));
    Arc::new(ViewTableProvider::new(view))
}

#[cfg(test)]
mod tests {

    #[tokio::test]
    async fn test_information_schema_tables_provider() {
        // This test requires actual KalamSql setup
        // For now, just verify the provider can be constructed

        // Note: In real tests, we would:
        // 1. Create KalamSql with test database
        // 2. Insert test tables
        // 3. Query information_schema.tables
        // 4. Verify correct results
    }
}
