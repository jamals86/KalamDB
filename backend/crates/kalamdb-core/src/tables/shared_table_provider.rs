//! Shared table provider for DataFusion integration
//!
//! This module provides a DataFusion TableProvider implementation for shared tables with:
//! - Global data accessible to all users in namespace
//! - System columns (_updated, _deleted)
//! - RocksDB buffer with Parquet persistence
//! - Flush policy support (row/time/combined)

use crate::catalog::{NamespaceId, TableMetadata, TableName};
use crate::error::KalamDbError;
use async_trait::async_trait;
use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use datafusion::datasource::TableProvider;
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::execution::context::SessionState;
use datafusion::logical_expr::{Expr, TableType as DataFusionTableType};
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_store::SharedTableStore;
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
        // Inject system columns
        let now_ms = chrono::Utc::now().timestamp_millis();
        if let Some(obj) = row_data.as_object_mut() {
            obj.insert("_updated".to_string(), serde_json::json!(now_ms));
            obj.insert("_deleted".to_string(), serde_json::json!(false));
        }

        // Store in SharedTableStore
        self.store
            .put(
                self.namespace_id().as_str(),
                self.table_name().as_str(),
                row_id,
                row_data,
            )
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
        // Get existing row
        let mut row_data = self
            .store
            .get(
                self.namespace_id().as_str(),
                self.table_name().as_str(),
                row_id,
            )
            .map_err(|e| KalamDbError::Other(e.to_string()))?
            .ok_or_else(|| KalamDbError::NotFound(format!("Row not found: {}", row_id)))?;

        // Apply updates
        if let (Some(existing), Some(new_fields)) = (row_data.as_object_mut(), updates.as_object())
        {
            for (key, value) in new_fields {
                existing.insert(key.clone(), value.clone());
            }
            // Update _updated timestamp
            existing.insert(
                "_updated".to_string(),
                serde_json::json!(chrono::Utc::now().timestamp_millis()),
            );
        }

        // Store updated row
        self.store
            .put(
                self.namespace_id().as_str(),
                self.table_name().as_str(),
                row_id,
                row_data,
            )
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        Ok(())
    }

    /// Validate that INSERT rows comply with schema constraints
    ///
    /// Checks:
    /// - NOT NULL constraints (non-nullable columns must have values)
    /// - Data type compatibility (basic type checking)
    fn validate_insert_rows(&self, rows: &[JsonValue]) -> Result<(), String> {
        for (row_idx, row) in rows.iter().enumerate() {
            let obj = row
                .as_object()
                .ok_or_else(|| format!("Row {} is not a JSON object", row_idx))?;

            // Check each field in the schema
            for field in self.schema.fields() {
                let field_name = field.name();
                
                // Skip system columns - they're auto-populated
                if field_name == "_updated" || field_name == "_deleted" {
                    continue;
                }

                // Skip auto-generated columns if any
                if field_name == "id" || field_name == "created_at" {
                    continue;
                }

                let value = obj.get(field_name);

                // Check NOT NULL constraint
                if !field.is_nullable() {
                    match value {
                        None | Some(JsonValue::Null) => {
                            return Err(format!(
                                "Row {}: Column '{}' is declared as non-nullable but received NULL value. Please provide a value for this column.",
                                row_idx, field_name
                            ));
                        }
                        _ => {} // Has a value, constraint satisfied
                    }
                }

                // Optional: Basic type validation
                if let Some(val) = value {
                    if !val.is_null() {
                        let type_valid = match field.data_type() {
                            DataType::Int32 | DataType::Int64 => val.is_i64() || val.is_u64(),
                            DataType::Float64 => val.is_f64() || val.is_i64() || val.is_u64(),
                            DataType::Utf8 => val.is_string(),
                            DataType::Boolean => val.is_boolean(),
                            _ => true, // Skip validation for other types
                        };

                        if !type_valid {
                            return Err(format!(
                                "Row {}: Column '{}' has incompatible type. Expected {:?}, got {:?}",
                                row_idx, field_name, field.data_type(), val
                            ));
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// DELETE operation (soft delete)
    ///
    /// Sets _deleted=true and updates _updated timestamp.
    /// Row remains in RocksDB until flush or cleanup job removes it.
    pub fn delete_soft(&self, row_id: &str) -> Result<(), KalamDbError> {
        // Get existing row
        let row_data = self
            .store
            .get(
                self.namespace_id().as_str(),
                self.table_name().as_str(),
                row_id,
            )
            .map_err(|e| KalamDbError::Other(e.to_string()))?
            .ok_or_else(|| KalamDbError::NotFound(format!("Row not found: {}", row_id)))?;

        // Create updated row with _deleted=true
        let updated_row = if let Some(obj) = row_data.as_object() {
            let mut new_obj = obj.clone();
            new_obj.insert("_deleted".to_string(), serde_json::json!(true));
            new_obj.insert(
                "_updated".to_string(),
                serde_json::json!(chrono::Utc::now().timestamp_millis()),
            );
            serde_json::json!(new_obj)
        } else {
            return Err(KalamDbError::InvalidOperation(
                "Row data is not a JSON object".to_string(),
            ));
        };

        // Store updated row
        self.store
            .put(
                self.namespace_id().as_str(),
                self.table_name().as_str(),
                row_id,
                updated_row,
            )
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        Ok(())
    }

    /// DELETE operation (hard delete)
    ///
    /// Permanently removes row from RocksDB.
    /// Used by cleanup jobs for expired soft-deleted rows.
    pub fn delete_hard(&self, row_id: &str) -> Result<(), KalamDbError> {
        self.store
            .delete(
                self.namespace_id().as_str(),
                self.table_name().as_str(),
                row_id,
                true, // hard delete
            )
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
        // Add system columns to the schema
        let mut fields = self.schema.fields().to_vec();

        // Add _updated (timestamp in milliseconds)
        fields.push(Arc::new(Field::new(
            "_updated",
            DataType::Timestamp(TimeUnit::Millisecond, None),
            false, // NOT NULL
        )));

        // Add _deleted (boolean)
        fields.push(Arc::new(Field::new(
            "_deleted",
            DataType::Boolean,
            false, // NOT NULL
        )));

        Arc::new(Schema::new(fields))
    }

    fn table_type(&self) -> DataFusionTableType {
        DataFusionTableType::Base
    }

    async fn scan(
        &self,
        _state: &SessionState,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        // Get the full schema with system columns
        let full_schema = self.schema();

        // Read all rows from the store
        let rows = self
            .store
            .scan(self.namespace_id().as_str(), self.table_name().as_str())
            .map_err(|e| DataFusionError::Execution(format!("Failed to scan table: {}", e)))?;

        // Convert JSON rows to Arrow RecordBatch (includes system columns now)
        let batch = json_rows_to_arrow_batch(&rows, &full_schema, limit).map_err(|e| {
            DataFusionError::Execution(format!("Failed to convert rows to Arrow: {}", e))
        })?;

        // Apply projection if specified
        let (final_batch, final_schema) = if let Some(proj_indices) = projection {
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
            .map_err(|e| DataFusionError::Execution(format!("Failed to project batch: {}", e)))?;

            (projected_batch, projected_schema)
        } else {
            // Use full_schema (with system columns) instead of self.schema
            (batch, full_schema.clone())
        };

        // Create a MemoryExec plan that returns our batch
        use datafusion::physical_plan::memory::MemoryExec;

        let partitions = vec![vec![final_batch]];
        let exec = MemoryExec::try_new(&partitions, final_schema, None).map_err(|e| {
            DataFusionError::Execution(format!("Failed to create MemoryExec: {}", e))
        })?;

        Ok(Arc::new(exec))
    }

    async fn insert_into(
        &self,
        _state: &SessionState,
        input: Arc<dyn ExecutionPlan>,
        _overwrite: bool,
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
            let json_rows = arrow_batch_to_json(&batch).map_err(|e| {
                DataFusionError::Execution(format!("Arrow to JSON conversion failed: {}", e))
            })?;

            // Validate schema constraints before insert
            self.validate_insert_rows(&json_rows)
                .map_err(|e| DataFusionError::Execution(format!("Schema validation failed: {}", e)))?;

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

/// Convert Arrow RecordBatch to Vec of JSON objects
///
/// This is a helper function for INSERT operations that converts Arrow columnar
/// data to row-oriented JSON objects suitable for storage.
fn arrow_batch_to_json(
    batch: &datafusion::arrow::record_batch::RecordBatch,
) -> Result<Vec<JsonValue>, String> {
    use datafusion::arrow::array::*;
    use datafusion::arrow::datatypes::DataType;

    let schema = batch.schema();
    let num_rows = batch.num_rows();
    let mut rows = Vec::with_capacity(num_rows);

    for row_idx in 0..num_rows {
        let mut row_map = serde_json::Map::new();

        for (col_idx, field) in schema.fields().iter().enumerate() {
            let column = batch.column(col_idx);

            // Skip system columns if present in input (we'll inject them)
            if field.name() == "_updated" || field.name() == "_deleted" {
                continue;
            }

            // Convert Arrow value to JSON
            let json_value = match field.data_type() {
                DataType::Utf8 => {
                    let array = column
                        .as_any()
                        .downcast_ref::<StringArray>()
                        .ok_or_else(|| {
                            format!("Failed to downcast {} to StringArray", field.name())
                        })?;
                    if array.is_null(row_idx) {
                        JsonValue::Null
                    } else {
                        JsonValue::String(array.value(row_idx).to_string())
                    }
                }
                DataType::Int32 => {
                    let array = column
                        .as_any()
                        .downcast_ref::<Int32Array>()
                        .ok_or_else(|| {
                            format!("Failed to downcast {} to Int32Array", field.name())
                        })?;
                    if array.is_null(row_idx) {
                        JsonValue::Null
                    } else {
                        JsonValue::Number(array.value(row_idx).into())
                    }
                }
                DataType::Int64 => {
                    let array = column
                        .as_any()
                        .downcast_ref::<Int64Array>()
                        .ok_or_else(|| {
                            format!("Failed to downcast {} to Int64Array", field.name())
                        })?;
                    if array.is_null(row_idx) {
                        JsonValue::Null
                    } else {
                        JsonValue::Number(array.value(row_idx).into())
                    }
                }
                DataType::Float64 => {
                    let array =
                        column
                            .as_any()
                            .downcast_ref::<Float64Array>()
                            .ok_or_else(|| {
                                format!("Failed to downcast {} to Float64Array", field.name())
                            })?;
                    if array.is_null(row_idx) {
                        JsonValue::Null
                    } else {
                        serde_json::Number::from_f64(array.value(row_idx))
                            .map(JsonValue::Number)
                            .unwrap_or(JsonValue::Null)
                    }
                }
                DataType::Boolean => {
                    let array =
                        column
                            .as_any()
                            .downcast_ref::<BooleanArray>()
                            .ok_or_else(|| {
                                format!("Failed to downcast {} to BooleanArray", field.name())
                            })?;
                    if array.is_null(row_idx) {
                        JsonValue::Null
                    } else {
                        JsonValue::Bool(array.value(row_idx))
                    }
                }
                _ => {
                    // For unsupported types, use string representation
                    JsonValue::String(format!("{:?}", column))
                }
            };

            row_map.insert(field.name().clone(), json_value);
        }

        rows.push(JsonValue::Object(row_map));
    }

    Ok(rows)
}

/// Convert JSON rows (from store) to Arrow RecordBatch
///
/// This is the inverse of arrow_batch_to_json, used for SELECT operations.
/// Converts row-oriented JSON data back to Arrow columnar format.
fn json_rows_to_arrow_batch(
    rows: &[(String, JsonValue)],
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
        // Return empty batch with correct schema
        let empty_arrays: Vec<Arc<dyn datafusion::arrow::array::Array>> = schema
            .fields()
            .iter()
            .map(|field| {
                let array: Arc<dyn datafusion::arrow::array::Array> = match field.data_type() {
                    DataType::Utf8 => Arc::new(StringArray::from(Vec::<Option<String>>::new())),
                    DataType::Int32 => Arc::new(Int32Array::from(Vec::<Option<i32>>::new())),
                    DataType::Int64 => Arc::new(Int64Array::from(Vec::<Option<i64>>::new())),
                    DataType::Float64 => Arc::new(Float64Array::from(Vec::<Option<f64>>::new())),
                    DataType::Boolean => Arc::new(BooleanArray::from(Vec::<Option<bool>>::new())),
                    DataType::Timestamp(TimeUnit::Millisecond, _) => {
                        Arc::new(TimestampMillisecondArray::from(Vec::<Option<i64>>::new()))
                    }
                    _ => Arc::new(StringArray::from(Vec::<Option<String>>::new())),
                };
                array
            })
            .collect();

        return datafusion::arrow::record_batch::RecordBatch::try_new(schema.clone(), empty_arrays)
            .map_err(|e| format!("Failed to create empty batch: {}", e));
    }

    // Build arrays for each column
    let mut arrays: Vec<Arc<dyn datafusion::arrow::array::Array>> = Vec::new();

    for field in schema.fields() {
        let array: Arc<dyn datafusion::arrow::array::Array> = match field.data_type() {
            DataType::Utf8 => {
                let values: Vec<Option<String>> = rows_to_process
                    .iter()
                    .map(|(_, row_data)| {
                        row_data
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
                            .get(field.name())
                            .and_then(|v| v.as_i64().map(|i| i as i32))
                    })
                    .collect();
                Arc::new(Int32Array::from(values))
            }
            DataType::Int64 => {
                let values: Vec<Option<i64>> = rows_to_process
                    .iter()
                    .map(|(_, row_data)| row_data.get(field.name()).and_then(|v| v.as_i64()))
                    .collect();
                Arc::new(Int64Array::from(values))
            }
            DataType::Float64 => {
                let values: Vec<Option<f64>> = rows_to_process
                    .iter()
                    .map(|(_, row_data)| row_data.get(field.name()).and_then(|v| v.as_f64()))
                    .collect();
                Arc::new(Float64Array::from(values))
            }
            DataType::Boolean => {
                let values: Vec<Option<bool>> = rows_to_process
                    .iter()
                    .map(|(_, row_data)| row_data.get(field.name()).and_then(|v| v.as_bool()))
                    .collect();
                Arc::new(BooleanArray::from(values))
            }
            DataType::Timestamp(TimeUnit::Millisecond, _) => {
                let values: Vec<Option<i64>> = rows_to_process
                    .iter()
                    .map(|(_, row_data)| {
                        row_data.get(field.name()).and_then(|v| {
                            // Try to parse as i64 timestamp or ISO string
                            v.as_i64().or_else(|| {
                                v.as_str().and_then(|s| {
                                    chrono::DateTime::parse_from_rfc3339(s)
                                        .ok()
                                        .map(|dt| dt.timestamp_millis())
                                })
                            })
                        })
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
    use kalamdb_store::test_utils::TestDb;

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
            storage_location: "/data/shared".to_string(),
            flush_policy: crate::flush::FlushPolicy::RowLimit { row_limit: 1000 },
            schema_version: 1,
            deleted_retention_hours: Some(24),
        };

        let store = Arc::new(SharedTableStore::new(test_db.db.clone()).unwrap());
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

        // Verify row was stored
        let stored = provider
            .store
            .get(
                provider.namespace_id().as_str(),
                provider.table_name().as_str(),
                "setting_1",
            )
            .unwrap();
        assert!(stored.is_some());

        let stored_data = stored.unwrap();
        assert_eq!(stored_data["setting_key"], "max_connections");
        assert_eq!(stored_data["_deleted"], serde_json::json!(false));
        assert!(stored_data.get("_updated").is_some());
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

        // Verify update
        let stored = provider
            .store
            .get(
                provider.namespace_id().as_str(),
                provider.table_name().as_str(),
                "setting_2",
            )
            .unwrap()
            .unwrap();
        assert_eq!(stored["setting_value"], "60");
        assert_eq!(stored["setting_key"], "timeout"); // Unchanged
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

        // Verify initial state
        let before_delete = provider
            .store
            .get(
                provider.namespace_id().as_str(),
                provider.table_name().as_str(),
                "setting_3",
            )
            .unwrap()
            .unwrap();
        assert_eq!(before_delete["_deleted"], serde_json::json!(false));

        // Soft delete
        let result = provider.delete_soft("setting_3");
        assert!(result.is_ok(), "delete_soft failed: {:?}", result.err());

        // Verify still exists but marked deleted
        let stored = provider
            .store
            .get(
                provider.namespace_id().as_str(),
                provider.table_name().as_str(),
                "setting_3",
            )
            .unwrap();

        assert!(stored.is_some(), "Row should still exist after soft delete");
        let stored_data = stored.unwrap();

        // Debug: print the actual value
        eprintln!("Stored _deleted value: {:?}", stored_data["_deleted"]);

        assert_eq!(
            stored_data["_deleted"],
            serde_json::json!(true),
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

        // Verify row is gone
        let stored = provider
            .store
            .get(
                provider.namespace_id().as_str(),
                provider.table_name().as_str(),
                "setting_4",
            )
            .unwrap();
        assert!(stored.is_none());
    }

    #[test]
    fn test_column_family_name() {
        let (provider, _test_db) = create_test_provider();
        assert_eq!(provider.column_family_name(), "shared_table:app:config");
    }
}
