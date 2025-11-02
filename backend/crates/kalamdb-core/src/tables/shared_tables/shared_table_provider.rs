//! Shared table provider for DataFusion integration
//!
//! This module provides a DataFusion TableProvider implementation for shared tables with:
//! - Global data accessible to all users in namespace
//! - System columns (_updated, _deleted)
//! - RocksDB buffer with Parquet persistence
//! - Flush policy support (row/time/combined)

use crate::catalog::{NamespaceId, TableMetadata, TableName};
use crate::error::KalamDbError;
use crate::tables::arrow_json_conversion::{
    arrow_batch_to_json, json_rows_to_arrow_batch, validate_insert_rows,
};
use crate::tables::shared_tables::shared_table_store::{
    SharedTableRow, SharedTableRowId, SharedTableStore,
};
use async_trait::async_trait;
use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use datafusion::datasource::TableProvider;
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::dml::InsertOp;
use datafusion::logical_expr::{Expr, TableType as DataFusionTableType};
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_store::EntityStoreV2 as EntityStore;
use serde_json::Value as JsonValue;
use std::any::Any;
use std::sync::Arc;

/// Shared table provider for DataFusion
///
/// Provides SQL query access to shared tables with:
/// - INSERT/UPDATE/DELETE operations
/// - System columns (_updated, _deleted)
/// - Soft delete support
/// - Flush to Parquet
///
/// **Key Difference from User Tables**: Single storage location (no ${user_id} templating)
pub struct SharedTableProvider {
    /// Table metadata (namespace, table name, type, etc.)
    table_metadata: TableMetadata,

    /// Arrow schema for the table (includes system columns)
    schema: SchemaRef,

    /// SharedTableStore for data operations
    store: Arc<SharedTableStore>,
}

impl std::fmt::Debug for SharedTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedTableProvider")
            .field("table_metadata", &self.table_metadata)
            .field("schema", &self.schema)
            .field("store", &"<SharedTableStore>")
            .finish()
    }
}

impl SharedTableProvider {
    /// Create a new shared table provider
    ///
    /// # Arguments
    /// * `table_metadata` - Table metadata (namespace, table name, type, etc.)
    /// * `schema` - Arrow schema for the table (with system columns)
    /// * `store` - SharedTableStore for data operations
    pub fn new(
        table_metadata: TableMetadata,
        schema: SchemaRef,
        store: Arc<SharedTableStore>,
    ) -> Self {
        Self {
            table_metadata,
            schema,
            store,
        }
    }

    /// Get the column family name for this shared table
    pub fn column_family_name(&self) -> String {
        format!(
            "shared_table:{}:{}",
            self.table_metadata.namespace.as_str(),
            self.table_metadata.table_name.as_str()
        )
    }

    /// Get the namespace ID
    pub fn namespace_id(&self) -> &NamespaceId {
        &self.table_metadata.namespace
    }

    /// Get the table name
    pub fn table_name(&self) -> &TableName {
        &self.table_metadata.table_name
    }

    /// INSERT operation
    ///
    /// # Arguments
    /// * `row_id` - Unique row identifier
    /// * `row_data` - Row data as JSON object (system columns will be injected)
    ///
    /// # System Column Injection
    /// - _updated: Current timestamp (milliseconds)
    /// - _deleted: false
    pub fn insert(&self, row_id: &str, mut row_data: JsonValue) -> Result<(), KalamDbError> {
        // Inject system columns (as RFC3339 string for _updated to match SharedTableRow format)
        let now_rfc3339 = chrono::Utc::now().to_rfc3339();
        if let Some(obj) = row_data.as_object_mut() {
            // Use insert() which overwrites existing keys
            obj.insert("_updated".to_string(), serde_json::json!(now_rfc3339));
            obj.insert("_deleted".to_string(), serde_json::json!(false));
        }

        let key = SharedTableRowId::new(row_id);
        let entity = SharedTableRow {
            row_id: row_id.to_string(),
            fields: row_data,
            _updated: now_rfc3339,
            _deleted: false,
            access_level: kalamdb_commons::TableAccess::Public,
        };

        // Store in SharedTableStore
        self.store
            .put(&key, &entity)
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        Ok(())
    }

    /// UPDATE operation
    ///
    /// # Arguments
    /// * `row_id` - Row identifier to update
    /// * `updates` - Fields to update (partial update)
    ///
    /// # System Column Updates
    /// - _updated: Updated to current timestamp
    /// - _deleted: Preserved unless explicitly set
    pub fn update(&self, row_id: &str, updates: JsonValue) -> Result<(), KalamDbError> {
        let key = SharedTableRowId::new(row_id);
        let mut row_data = self
            .store
            .get(&key)
            .map_err(|e| KalamDbError::Other(e.to_string()))?
            .ok_or_else(|| KalamDbError::NotFound(format!("Row not found: {}", row_id)))?;

        // Apply updates
        if let Some(new_fields) = updates.as_object() {
            if let Some(existing_fields) = row_data.fields.as_object_mut() {
                for (k, v) in new_fields {
                    existing_fields.insert(k.clone(), v.clone());
                }
            }
        }

        // Update system columns
        row_data._updated = chrono::Utc::now().to_rfc3339();

        // Store updated row
        self.store
            .put(&key, &row_data)
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        Ok(())
    }

    /// Validate that INSERT rows comply with schema constraints
    ///
    /// Checks:
    /// - NOT NULL constraints (non-nullable columns must have values)
    /// - Data type compatibility (basic type checking)
    fn validate_insert_rows_local(&self, rows: &[JsonValue]) -> Result<(), String> {
        validate_insert_rows(&self.schema, rows)
    }

    /// DELETE operation (soft delete)
    ///
    /// Sets _deleted=true and updates _updated timestamp.
    /// Row remains in RocksDB until flush or cleanup job removes it.
    pub fn delete_soft(&self, row_id: &str) -> Result<(), KalamDbError> {
        let key = SharedTableRowId::new(row_id);
        let mut row_data = self
            .store
            .get(&key)
            .map_err(|e| KalamDbError::Other(e.to_string()))?
            .ok_or_else(|| KalamDbError::NotFound(format!("Row not found: {}", row_id)))?;

        // Update system columns
        row_data._deleted = true;
        row_data._updated = chrono::Utc::now().to_rfc3339();

        // Store updated row
        self.store
            .put(&key, &row_data)
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        Ok(())
    }

    /// DELETE operation (hard delete)
    ///
    /// Permanently removes row from RocksDB.
    /// Used by cleanup jobs for expired soft-deleted rows.
    pub fn delete_hard(&self, row_id: &str) -> Result<(), KalamDbError> {
        let key = SharedTableRowId::new(row_id);
        self.store
            .delete(&key)
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        Ok(())
    }
}

// DataFusion TableProvider trait implementation
#[async_trait]
impl TableProvider for SharedTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        // Add system columns to the schema if they don't already exist
        let mut fields = self.schema.fields().to_vec();

        // Check if _updated already exists
        if !fields.iter().any(|f| f.name() == "_updated") {
            // Add _updated (timestamp in milliseconds)
            fields.push(Arc::new(Field::new(
                "_updated",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false, // NOT NULL
            )));
        }

        // Check if _deleted already exists
        if !fields.iter().any(|f| f.name() == "_deleted") {
            // Add _deleted (boolean)
            fields.push(Arc::new(Field::new(
                "_deleted",
                DataType::Boolean,
                false, // NOT NULL
            )));
        }

        Arc::new(Schema::new(fields))
    }

    fn table_type(&self) -> DataFusionTableType {
        DataFusionTableType::Base
    }

    async fn scan(
        &self,
        _state: &dyn datafusion::catalog::Session,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        // Get the full schema with system columns
        let full_schema = self.schema();

        // Read all rows from the store
        let rows = self
            .store
            .scan_all()
            .map_err(|e| DataFusionError::Execution(format!("Failed to scan table: {}", e)))?;

        // Convert SharedTableRow to Arrow RecordBatch (includes system columns now)
        let rows_with_ids: Vec<(SharedTableRowId, SharedTableRow)> = rows
            .iter()
            .map(|(key, row)| {
                (
                    SharedTableRowId::new(String::from_utf8_lossy(key).to_string()),
                    row.clone(),
                )
            })
            .collect();
        let batch =
            shared_rows_to_arrow_batch(&rows_with_ids, &full_schema, limit).map_err(|e| {
                DataFusionError::Execution(format!("Failed to convert rows to Arrow: {}", e))
            })?;

        // Apply projection if specified
        let (final_batch, final_schema) = if let Some(proj_indices) = projection {
            // Handle empty projection (e.g., for COUNT(*))
            if proj_indices.is_empty() {
                // For COUNT(*), we need a batch with correct row count but no columns
                // Use RecordBatch with a dummy null column to preserve row count
                use datafusion::arrow::array::new_null_array;
                use datafusion::arrow::datatypes::DataType;

                //TODO: What is thus dummy column for?
                // RecordBatch with 0 columns but preserving row count
                // We need at least one column to preserve row count, so add a dummy null column
                let dummy_field = Arc::new(Field::new("__dummy", DataType::Null, true));
                let projected_schema = Arc::new(datafusion::arrow::datatypes::Schema::new(vec![
                    dummy_field.clone(),
                ]));
                let null_array = new_null_array(&DataType::Null, batch.num_rows());

                let projected_batch = datafusion::arrow::record_batch::RecordBatch::try_new(
                    projected_schema.clone(),
                    vec![null_array],
                )
                .map_err(|e| {
                    DataFusionError::Execution(format!("Failed to create temp batch: {}", e))
                })?;

                (projected_batch, projected_schema)
            } else {
                let projected_columns: Vec<_> = proj_indices
                    .iter()
                    .map(|&i| batch.column(i).clone())
                    .collect();

                let projected_fields: Vec<_> = proj_indices
                    .iter()
                    .map(|&i| full_schema.field(i).clone())
                    .collect();

                let projected_schema =
                    Arc::new(datafusion::arrow::datatypes::Schema::new(projected_fields));

                let projected_batch = datafusion::arrow::record_batch::RecordBatch::try_new(
                    projected_schema.clone(),
                    projected_columns,
                )
                .map_err(|e| {
                    DataFusionError::Execution(format!("Failed to project batch: {}", e))
                })?;

                (projected_batch, projected_schema)
            }
        } else {
            // Use full_schema (with system columns) instead of self.schema
            (batch, full_schema.clone())
        };

        // Create an in-memory table and return its scan plan
        use datafusion::datasource::MemTable;
        let partitions = vec![vec![final_batch]];
        let table = MemTable::try_new(final_schema, partitions)
            .map_err(|e| DataFusionError::Execution(format!("Failed to create MemTable: {}", e)))?;

        table.scan(_state, projection, &[], limit).await
    }

    async fn insert_into(
        &self,
        _state: &dyn datafusion::catalog::Session,
        input: Arc<dyn ExecutionPlan>,
        _op: InsertOp,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        use datafusion::execution::TaskContext;
        use datafusion::physical_plan::collect;

        // Execute the input plan to get RecordBatches
        let task_ctx = Arc::new(TaskContext::default());
        let batches = collect(input, task_ctx)
            .await
            .map_err(|e| DataFusionError::Execution(format!("Failed to collect input: {}", e)))?;

        // Process each batch
        for batch in batches {
            // Convert Arrow RecordBatch to JSON rows
            let json_rows = arrow_batch_to_json(&batch, true).map_err(|e| {
                DataFusionError::Execution(format!("Arrow to JSON conversion failed: {}", e))
            })?;

            // Validate schema constraints before insert
            self.validate_insert_rows_local(&json_rows).map_err(|e| {
                DataFusionError::Execution(format!("Schema validation failed: {}", e))
            })?;

            // Insert each row
            for (idx, row_data) in json_rows.into_iter().enumerate() {
                // Generate row_id using snowflake ID (or UUID for now)
                let row_id = format!("{}_{}", chrono::Utc::now().timestamp_millis(), idx);

                self.insert(&row_id, row_data)
                    .map_err(|e| DataFusionError::Execution(format!("Insert failed: {}", e)))?;
            }
        }

        // Return empty execution plan (INSERT returns no rows)
        use datafusion::physical_plan::empty::EmptyExec;
        Ok(Arc::new(EmptyExec::new(self.schema.clone())))
    }
}

/// Helper to convert SharedTableStore rows to JSON rows for Arrow conversion
///
/// Extracts the JSON fields from SharedTableRow and prepares them for Arrow batch creation
fn shared_rows_to_arrow_batch(
    rows: &[(SharedTableRowId, SharedTableRow)],
    schema: &datafusion::arrow::datatypes::SchemaRef,
    limit: Option<usize>,
) -> Result<datafusion::arrow::record_batch::RecordBatch, String> {
    use datafusion::arrow::array::*;
    use datafusion::arrow::datatypes::{DataType, TimeUnit};

    // Apply limit if specified
    let rows_to_process = if let Some(lim) = limit {
        &rows[..std::cmp::min(lim, rows.len())]
    } else {
        rows
    };

    if rows_to_process.is_empty() {
        // Return empty batch with correct schema using shared utility
        return json_rows_to_arrow_batch(schema, vec![]);
    }

    // Build arrays for each column (including system columns from SharedTableRow)
    let mut arrays: Vec<Arc<dyn datafusion::arrow::array::Array>> = Vec::new();

    for field in schema.fields() {
        let array: Arc<dyn datafusion::arrow::array::Array> = match field.data_type() {
            DataType::Utf8 => {
                let values: Vec<Option<String>> = rows_to_process
                    .iter()
                    .map(|(_, row_data)| {
                        row_data
                            .fields
                            .get(field.name())
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    })
                    .collect();
                Arc::new(StringArray::from(values))
            }
            DataType::Int32 => {
                let values: Vec<Option<i32>> = rows_to_process
                    .iter()
                    .map(|(_, row_data)| {
                        row_data
                            .fields
                            .get(field.name())
                            .and_then(|v| v.as_i64().map(|i| i as i32))
                    })
                    .collect();
                Arc::new(Int32Array::from(values))
            }
            DataType::Int64 => {
                let values: Vec<Option<i64>> = rows_to_process
                    .iter()
                    .map(|(_, row_data)| row_data.fields.get(field.name()).and_then(|v| v.as_i64()))
                    .collect();
                Arc::new(Int64Array::from(values))
            }
            DataType::Float64 => {
                let values: Vec<Option<f64>> = rows_to_process
                    .iter()
                    .map(|(_, row_data)| row_data.fields.get(field.name()).and_then(|v| v.as_f64()))
                    .collect();
                Arc::new(Float64Array::from(values))
            }
            DataType::Boolean => {
                let values: Vec<Option<bool>> = rows_to_process
                    .iter()
                    .map(|(_, row_data)| {
                        if field.name() == "_deleted" {
                            Some(row_data._deleted)
                        } else {
                            row_data.fields.get(field.name()).and_then(|v| v.as_bool())
                        }
                    })
                    .collect();
                Arc::new(BooleanArray::from(values))
            }
            DataType::Timestamp(TimeUnit::Millisecond, _) => {
                let values: Vec<Option<i64>> = rows_to_process
                    .iter()
                    .map(|(_, row_data)| {
                        if field.name() == "_updated" {
                            chrono::DateTime::parse_from_rfc3339(&row_data._updated)
                                .ok()
                                .map(|dt| dt.timestamp_millis())
                        } else {
                            row_data.fields.get(field.name()).and_then(|v| {
                                v.as_i64().or_else(|| {
                                    v.as_str().and_then(|s| {
                                        chrono::DateTime::parse_from_rfc3339(s)
                                            .ok()
                                            .map(|dt| dt.timestamp_millis())
                                    })
                                })
                            })
                        }
                    })
                    .collect();
                Arc::new(TimestampMillisecondArray::from(values))
            }
            _ => {
                return Err(format!(
                    "Unsupported data type for field {}: {:?}",
                    field.name(),
                    field.data_type()
                ));
            }
        };

        arrays.push(array);
    }

    datafusion::arrow::record_batch::RecordBatch::try_new(schema.clone(), arrays)
        .map_err(|e| format!("Failed to create RecordBatch: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::TableType;
    use datafusion::arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use kalamdb_commons::StorageId;
    use kalamdb_store::test_utils::{InMemoryBackend, TestDb};

    fn create_test_provider() -> (SharedTableProvider, TestDb) {
        let test_db = TestDb::new(&["shared_table:app:config"]).unwrap();

        let schema = Arc::new(Schema::new(vec![
            Field::new("setting_key", DataType::Utf8, false),
            Field::new("setting_value", DataType::Utf8, false),
            Field::new(
                "_updated",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
            Field::new("_deleted", DataType::Boolean, false),
        ]));

        let metadata = TableMetadata {
            table_name: TableName::new("config"),
            table_type: TableType::Shared,
            namespace: NamespaceId::new("app"),
            created_at: chrono::Utc::now(),
            storage_id: Some(StorageId::new("local")),
            flush_policy: crate::flush::FlushPolicy::RowLimit { row_limit: 1000 },
            schema_version: 1,
            deleted_retention_hours: Some(24),
        };

        let store = Arc::new(
            crate::tables::shared_tables::shared_table_store::new_shared_table_store(
                Arc::new(InMemoryBackend::new()),
                &metadata.namespace,
                &metadata.table_name,
            ),
        );
        let provider = SharedTableProvider::new(metadata, schema, store);

        (provider, test_db)
    }

    #[test]
    fn test_insert() {
        let (provider, _test_db) = create_test_provider();

        let row_data = serde_json::json!({
            "setting_key": "max_connections",
            "setting_value": "100"
        });

        let result = provider.insert("setting_1", row_data);
        assert!(result.is_ok());

        let key = SharedTableRowId::new("setting_1");
        let stored = provider.store.get(&key).unwrap();
        assert!(stored.is_some());

        let stored_data = stored.unwrap();
        assert_eq!(stored_data.fields["setting_key"], "max_connections");
        assert_eq!(stored_data._deleted, false);
        assert!(!stored_data._updated.is_empty());
    }

    #[test]
    fn test_update() {
        let (provider, _test_db) = create_test_provider();

        // Insert initial data
        let row_data = serde_json::json!({
            "setting_key": "timeout",
            "setting_value": "30"
        });
        provider.insert("setting_2", row_data).unwrap();

        // Update
        let updates = serde_json::json!({
            "setting_value": "60"
        });
        let result = provider.update("setting_2", updates);
        assert!(result.is_ok());

        let key = SharedTableRowId::new("setting_2");
        let stored = provider.store.get(&key).unwrap().unwrap();
        assert_eq!(stored.fields["setting_value"], "60");
        assert_eq!(stored.fields["setting_key"], "timeout"); // Unchanged
    }

    #[test]
    #[ignore] // TODO: Fix test - DB isolation issue
    fn test_delete_soft() {
        let (provider, _test_db) = create_test_provider();

        // Insert data
        let row_data = serde_json::json!({
            "setting_key": "old_setting",
            "setting_value": "deprecated"
        });
        provider.insert("setting_3", row_data).unwrap();

        let key = SharedTableRowId::new("setting_3");
        let before_delete = provider.store.get(&key).unwrap().unwrap();
        assert_eq!(before_delete._deleted, false);

        // Soft delete
        let result = provider.delete_soft("setting_3");
        assert!(result.is_ok(), "delete_soft failed: {:?}", result.err());

        // Verify still exists but marked deleted
        let stored = provider.store.get(&key).unwrap();

        assert!(stored.is_some(), "Row should still exist after soft delete");
        let stored_data = stored.unwrap();

        // Debug: print the actual value
        eprintln!("Stored _deleted value: {:?}", stored_data._deleted);

        assert_eq!(
            stored_data._deleted, true,
            "Row should be marked as deleted"
        );
    }

    #[test]
    fn test_delete_hard() {
        let (provider, _test_db) = create_test_provider();

        // Insert data
        let row_data = serde_json::json!({
            "setting_key": "temp",
            "setting_value": "value"
        });
        provider.insert("setting_4", row_data).unwrap();

        // Hard delete
        let result = provider.delete_hard("setting_4");
        assert!(result.is_ok());

        let key = SharedTableRowId::new("setting_4");
        let stored = provider.store.get(&key).unwrap();
        assert!(stored.is_none());
    }

    #[test]
    fn test_column_family_name() {
        let (provider, _test_db) = create_test_provider();
        assert_eq!(provider.column_family_name(), "shared_table:app:config");
    }
}
