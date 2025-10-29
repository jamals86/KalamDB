//! User table provider for DataFusion integration
//!
//! This module provides a DataFusion TableProvider implementation for user tables with:
//! - Data isolation via UserId key prefix filtering
//! - Integration with UserTableInsertHandler, UserTableUpdateHandler, UserTableDeleteHandler
//! - Schema management and version tracking
//! - Storage path templating with ${user_id} substitution
//! - Hybrid RocksDB + Parquet querying

use crate::catalog::{NamespaceId, TableMetadata, TableName, TableType, UserId};
use crate::error::KalamDbError;
use crate::ids::SnowflakeGenerator;
use crate::live_query::manager::LiveQueryManager;
use crate::stores::UserTableStore;
use crate::tables::user_table_delete::UserTableDeleteHandler;
use crate::tables::user_table_insert::UserTableInsertHandler;
use crate::tables::user_table_update::UserTableUpdateHandler;
use async_trait::async_trait;
use chrono::Utc;
use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use datafusion::datasource::TableProvider;
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::dml::InsertOp;
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::Role;
use once_cell::sync::Lazy;
use serde_json::Value as JsonValue;
use std::any::Any;
use std::sync::Arc;

/// Shared Snowflake generator for auto-increment values
static AUTO_ID_GENERATOR: Lazy<SnowflakeGenerator> = Lazy::new(|| SnowflakeGenerator::new(0));

/// User table provider for DataFusion
///
/// Provides SQL query access to user tables with:
/// - Automatic data isolation by UserId
/// - DML operations (INSERT, UPDATE, DELETE)
/// - Hybrid RocksDB + Parquet scanning
/// - Schema evolution support
pub struct UserTableProvider {
    /// Table metadata (namespace, table name, type, storage location, etc.)
    table_metadata: TableMetadata,

    /// Arrow schema for the table
    schema: SchemaRef,

    /// UserTableStore for DML operations
    store: Arc<UserTableStore>,

    /// Current user ID for data isolation
    current_user_id: UserId,

    /// Role associated with the current session (determines access scope)
    access_role: Role,

    /// INSERT handler
    insert_handler: Arc<UserTableInsertHandler>,

    /// UPDATE handler
    update_handler: Arc<UserTableUpdateHandler>,

    /// DELETE handler
    delete_handler: Arc<UserTableDeleteHandler>,

    /// Parquet file paths for cold data (optional)
    parquet_paths: Vec<String>,

    /// LiveQueryManager for WebSocket notifications
    live_query_manager: Option<Arc<LiveQueryManager>>,
}

impl std::fmt::Debug for UserTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserTableProvider")
            .field("table_metadata", &self.table_metadata)
            .field("schema", &self.schema)
            .field("current_user_id", &self.current_user_id)
            .field("access_role", &self.access_role)
            .field("parquet_paths", &self.parquet_paths)
            .finish()
    }
}

impl UserTableProvider {
    /// Create a new user table provider
    ///
    /// # Arguments
    /// * `table_metadata` - Table metadata (namespace, table name, type, etc.)
    /// * `schema` - Arrow schema for the table
    /// * `store` - UserTableStore for DML operations
    /// * `current_user_id` - Current user ID for data isolation
    /// * `access_role` - Role of the caller (determines access scope)
    /// * `parquet_paths` - Optional list of Parquet file paths for cold data
    pub fn new(
        table_metadata: TableMetadata,
        schema: SchemaRef,
        store: Arc<UserTableStore>,
        current_user_id: UserId,
        access_role: Role,
        parquet_paths: Vec<String>,
    ) -> Self {
        let insert_handler = Arc::new(UserTableInsertHandler::new(store.clone()));
        let update_handler = Arc::new(UserTableUpdateHandler::new(store.clone()));
        let delete_handler = Arc::new(UserTableDeleteHandler::new(store.clone()));

        Self {
            table_metadata,
            schema,
            store,
            current_user_id,
            access_role,
            insert_handler,
            update_handler,
            delete_handler,
            parquet_paths,
            live_query_manager: None,
        }
    }

    /// Configure LiveQueryManager for WebSocket notifications
    ///
    /// # Arguments
    /// * `manager` - LiveQueryManager instance for notifications
    pub fn with_live_query_manager(mut self, manager: Arc<LiveQueryManager>) -> Self {
        // Wire through to all handlers
        self.insert_handler = Arc::new(
            UserTableInsertHandler::new(self.store.clone())
                .with_live_query_manager(Arc::clone(&manager)),
        );
        self.update_handler = Arc::new(
            UserTableUpdateHandler::new(self.store.clone())
                .with_live_query_manager(Arc::clone(&manager)),
        );
        self.delete_handler = Arc::new(
            UserTableDeleteHandler::new(self.store.clone())
                .with_live_query_manager(Arc::clone(&manager)),
        );

        self.live_query_manager = Some(manager);
        self
    }

    /// Get the column family name for this table
    pub fn column_family_name(&self) -> String {
        self.table_metadata.column_family_name()
    }

    /// Get the namespace ID
    pub fn namespace_id(&self) -> &NamespaceId {
        &self.table_metadata.namespace
    }

    /// Get the table name
    pub fn table_name(&self) -> &TableName {
        &self.table_metadata.table_name
    }

    /// Get the table type
    pub fn table_type(&self) -> &TableType {
        &self.table_metadata.table_type
    }

    /// Get the current user ID
    pub fn current_user_id(&self) -> &UserId {
        &self.current_user_id
    }

    /// Substitute ${user_id} in storage paths with actual user ID
    ///
    /// This implements T127 - user ID path substitution
    ///
    /// # Arguments
    /// * `template` - Storage path template (e.g., "s3://bucket/users/${user_id}/data/")
    ///
    /// # Returns
    /// Storage path with ${user_id} replaced (e.g., "s3://bucket/users/user123/data/")
    pub fn substitute_user_id_in_path(&self, template: &str) -> String {
        template.replace("${user_id}", self.current_user_id.as_str())
    }

    /// Get the storage location for this user
    ///
    /// Applies ${user_id} substitution to the table's storage location
    pub fn user_storage_location(&self) -> String {
        self.substitute_user_id_in_path(&self.table_metadata.storage_location)
    }

    /// Insert a single row into this user table
    ///
    /// # Arguments
    /// * `row_data` - Row data as JSON object
    ///
    /// # Returns
    /// The generated row ID
    pub fn insert_row(&self, row_data: JsonValue) -> Result<String, KalamDbError> {
        self.insert_handler.insert_row(
            self.namespace_id(),
            self.table_name(),
            &self.current_user_id,
            row_data,
        )
    }

    /// Insert multiple rows into this user table
    ///
    /// # Arguments
    /// * `rows` - Vector of row data as JSON objects
    ///
    /// # Returns
    /// Vector of generated row IDs
    pub fn insert_batch(&self, rows: Vec<JsonValue>) -> Result<Vec<String>, KalamDbError> {
        self.insert_handler.insert_batch(
            self.namespace_id(),
            self.table_name(),
            &self.current_user_id,
            rows,
        )
    }

    /// Validate that INSERT rows comply with schema constraints
    ///
    /// Checks:
    /// - NOT NULL constraints (non-nullable columns must have values)
    /// - Data type compatibility (basic type checking)
    ///
    /// This validation happens BEFORE the actual insert to ensure data consistency.
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

                // Skip auto-generated columns - they're handled by prepare_insert_rows
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

    /// Populate generated columns (id, created_at) when they are missing from the INSERT payload
    fn prepare_insert_rows(&self, rows: &mut [JsonValue]) -> Result<(), String> {
        let has_id = self.schema.field_with_name("id").is_ok();
        let has_created_at = self.schema.field_with_name("created_at").is_ok();

        if !has_id && !has_created_at {
            return Ok(());
        }

        for row in rows.iter_mut() {
            let obj = row
                .as_object_mut()
                .ok_or_else(|| "Row data must be a JSON object".to_string())?;

            if has_id {
                let needs_id = match obj.get("id") {
                    None | Some(JsonValue::Null) => true,
                    _ => false,
                };

                if needs_id {
                    let id = AUTO_ID_GENERATOR
                        .next_id()
                        .map_err(|e| format!("Failed to generate auto-increment id: {}", e))?;
                    obj.insert("id".to_string(), JsonValue::from(id));
                }
            }

            if has_created_at {
                let needs_ts = match obj.get("created_at") {
                    None | Some(JsonValue::Null) => true,
                    _ => false,
                };

                if needs_ts {
                    let now_ms = Utc::now().timestamp_millis();
                    obj.insert("created_at".to_string(), JsonValue::from(now_ms));
                }
            }
        }

        Ok(())
    }

    /// Update a single row in this user table
    ///
    /// # Arguments
    /// * `row_id` - Row ID to update
    /// * `updates` - Updated fields as JSON object
    ///
    /// # Returns
    /// The row ID of the updated row
    pub fn update_row(&self, row_id: &str, updates: JsonValue) -> Result<String, KalamDbError> {
        self.update_handler.update_row(
            self.namespace_id(),
            self.table_name(),
            &self.current_user_id,
            row_id,
            updates,
        )
    }

    /// Update multiple rows in this user table
    ///
    /// # Arguments
    /// * `updates` - Vector of (row_id, updates) tuples
    ///
    /// # Returns
    /// Vector of updated row IDs
    pub fn update_batch(
        &self,
        updates: Vec<(String, JsonValue)>,
    ) -> Result<Vec<String>, KalamDbError> {
        self.update_handler.update_batch(
            self.namespace_id(),
            self.table_name(),
            &self.current_user_id,
            updates,
        )
    }

    /// Soft delete a single row in this user table
    ///
    /// # Arguments
    /// * `row_id` - Row ID to delete
    ///
    /// # Returns
    /// The row ID of the deleted row
    pub fn delete_row(&self, row_id: &str) -> Result<String, KalamDbError> {
        self.delete_handler.delete_row(
            self.namespace_id(),
            self.table_name(),
            &self.current_user_id,
            row_id,
        )
    }

    /// Soft delete multiple rows in this user table
    ///
    /// # Arguments
    /// * `row_ids` - Vector of row IDs to delete
    ///
    /// # Returns
    /// Vector of deleted row IDs
    pub fn delete_batch(&self, row_ids: Vec<String>) -> Result<Vec<String>, KalamDbError> {
        self.delete_handler.delete_batch(
            self.namespace_id(),
            self.table_name(),
            &self.current_user_id,
            row_ids,
        )
    }

    /// Get the user-specific key prefix for data isolation
    ///
    /// This implements T128 - data isolation enforcement
    ///
    /// All queries will be filtered to only access rows with this prefix,
    /// ensuring users can only see their own data.
    ///
    /// # Returns
    /// Key prefix in format "{UserId}:"
    pub fn user_key_prefix(&self) -> Vec<u8> {
        format!("{}:", self.current_user_id.as_str())
            .as_bytes()
            .to_vec()
    }
}

#[async_trait]
impl TableProvider for UserTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        // Return the base schema without system columns
        // System columns (_updated, _deleted) are added dynamically during scan()
        self.schema.clone()
    }

    fn table_type(&self) -> datafusion::datasource::TableType {
        datafusion::datasource::TableType::Base
    }

    async fn scan(
        &self,
        _state: &dyn datafusion::catalog::Session,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        // Get the base schema and add system columns for the scan result
        let mut fields = self.schema.fields().to_vec();

        // Add system columns: _updated (timestamp) and _deleted (boolean)
        fields.push(Arc::new(Field::new(
            "_updated",
            DataType::Timestamp(TimeUnit::Millisecond, None),
            false, // NOT NULL
        )));
        fields.push(Arc::new(Field::new(
            "_deleted",
            DataType::Boolean,
            false, // NOT NULL
        )));

        let full_schema = Arc::new(Schema::new(fields));

        // Determine scan scope based on role (service/dba/system can see all users)
        let raw_rows = if matches!(self.access_role, Role::Service | Role::Dba | Role::System) {
            self.store
                .scan_all(self.namespace_id().as_str(), self.table_name().as_str())
                .map_err(|e| {
                    DataFusionError::Execution(format!("Failed to scan user table: {}", e))
                })?
        } else {
            self.store
                .scan_user(
                    self.namespace_id().as_str(),
                    self.table_name().as_str(),
                    self.current_user_id.as_str(),
                )
                .map_err(|e| {
                    DataFusionError::Execution(format!("Failed to scan user table: {}", e))
                })?
        };

        // Filter out soft-deleted rows (_deleted=true)
        let filtered_rows: Vec<_> = raw_rows
            .into_iter()
            .filter(|(_row_id, row_data)| {
                row_data
                    .get("_deleted")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                    == false
            })
            .collect();

        // Apply limit if specified
        let limited_rows = if let Some(limit_value) = limit {
            filtered_rows.into_iter().take(limit_value).collect()
        } else {
            filtered_rows
        };

        // Convert JSON rows to Arrow RecordBatch (includes system columns)
        let row_values: Vec<JsonValue> = limited_rows.into_iter().map(|(_id, data)| data).collect();

        let batch = json_rows_to_arrow_batch(&full_schema, row_values).map_err(|e| {
            DataFusionError::Execution(format!("JSON to Arrow conversion failed: {}", e))
        })?;

        // Create an in-memory table over the full schema and let DataFusion handle projection
        use datafusion::datasource::MemTable;
        let partitions = vec![vec![batch]];
        let table = MemTable::try_new(full_schema, partitions)
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
            let mut json_rows = arrow_batch_to_json(&batch).map_err(|e| {
                DataFusionError::Execution(format!("Arrow to JSON conversion failed: {}", e))
            })?;

            // Validate schema constraints (NOT NULL, etc.) before insert
            self.validate_insert_rows(&json_rows).map_err(|e| {
                DataFusionError::Execution(format!("Schema validation failed: {}", e))
            })?;

            // Populate auto-increment IDs when missing
            self.prepare_insert_rows(&mut json_rows)
                .map_err(|e| DataFusionError::Execution(e))?;

            // Insert each row using the insert_batch method
            // This automatically handles user_id scoping
            self.insert_batch(json_rows)
                .map_err(|e| DataFusionError::Execution(format!("Insert failed: {}", e)))?;
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
///
/// Supports common Arrow data types:
/// - Utf8 (String)
/// - Int32, Int64 (Integers)
/// - Float64 (Floating point)
/// - Boolean
/// - Timestamp (milliseconds)
///
/// Handles null values correctly for all types.
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
            let field_name = field.name().clone();

            // Skip system columns - they'll be added automatically by the insert handler
            if field_name == "_updated" || field_name == "_deleted" {
                continue;
            }

            // Extract value based on data type
            let value = match field.data_type() {
                DataType::Utf8 => {
                    let array = column
                        .as_any()
                        .downcast_ref::<StringArray>()
                        .ok_or_else(|| {
                            format!("Failed to downcast column {} to StringArray", field_name)
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
                            format!("Failed to downcast column {} to Int32Array", field_name)
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
                            format!("Failed to downcast column {} to Int64Array", field_name)
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
                                format!("Failed to downcast column {} to Float64Array", field_name)
                            })?;

                    if array.is_null(row_idx) {
                        JsonValue::Null
                    } else {
                        let f = array.value(row_idx);
                        JsonValue::Number(
                            serde_json::Number::from_f64(f)
                                .ok_or_else(|| format!("Invalid f64 value: {}", f))?,
                        )
                    }
                }
                DataType::Boolean => {
                    let array =
                        column
                            .as_any()
                            .downcast_ref::<BooleanArray>()
                            .ok_or_else(|| {
                                format!("Failed to downcast column {} to BooleanArray", field_name)
                            })?;

                    if array.is_null(row_idx) {
                        JsonValue::Null
                    } else {
                        JsonValue::Bool(array.value(row_idx))
                    }
                }
                DataType::Timestamp(_, _) => {
                    let array = column
                        .as_any()
                        .downcast_ref::<TimestampMillisecondArray>()
                        .ok_or_else(|| {
                            format!(
                                "Failed to downcast column {} to TimestampMillisecondArray",
                                field_name
                            )
                        })?;

                    if array.is_null(row_idx) {
                        JsonValue::Null
                    } else {
                        JsonValue::Number(array.value(row_idx).into())
                    }
                }
                _ => {
                    return Err(format!(
                        "Unsupported data type for column {}: {:?}",
                        field_name,
                        field.data_type()
                    ));
                }
            };

            row_map.insert(field_name, value);
        }

        rows.push(JsonValue::Object(row_map));
    }

    Ok(rows)
}

/// Convert JSON rows to Arrow RecordBatch
///
/// This is a helper function for SELECT operations that converts row-oriented
/// JSON objects to Arrow columnar format.
///
/// Supports common Arrow data types:
/// - Utf8 (String)
/// - Int32, Int64 (Integers)
/// - Float64 (Floating point)
/// - Boolean
/// - Timestamp (milliseconds)
///
/// Handles null values correctly for all types.
fn json_rows_to_arrow_batch(
    schema: &SchemaRef,
    rows: Vec<JsonValue>,
) -> Result<datafusion::arrow::record_batch::RecordBatch, String> {
    use datafusion::arrow::array::*;
    use datafusion::arrow::datatypes::DataType;

    if rows.is_empty() {
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
                let values: Vec<Option<String>> = rows
                    .iter()
                    .map(|row_data| {
                        row_data
                            .get(field.name())
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    })
                    .collect();
                Arc::new(StringArray::from(values))
            }
            DataType::Int32 => {
                let values: Vec<Option<i32>> = rows
                    .iter()
                    .map(|row_data| {
                        row_data
                            .get(field.name())
                            .and_then(|v| v.as_i64().map(|i| i as i32))
                    })
                    .collect();
                Arc::new(Int32Array::from(values))
            }
            DataType::Int64 => {
                let values: Vec<Option<i64>> = rows
                    .iter()
                    .map(|row_data| row_data.get(field.name()).and_then(|v| v.as_i64()))
                    .collect();
                Arc::new(Int64Array::from(values))
            }
            DataType::Float64 => {
                let values: Vec<Option<f64>> = rows
                    .iter()
                    .map(|row_data| row_data.get(field.name()).and_then(|v| v.as_f64()))
                    .collect();
                Arc::new(Float64Array::from(values))
            }
            DataType::Boolean => {
                let values: Vec<Option<bool>> = rows
                    .iter()
                    .map(|row_data| row_data.get(field.name()).and_then(|v| v.as_bool()))
                    .collect();
                Arc::new(BooleanArray::from(values))
            }
            DataType::Timestamp(TimeUnit::Millisecond, _) => {
                let values: Vec<Option<i64>> = rows
                    .iter()
                    .map(|row_data| {
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
    use crate::flush::FlushPolicy;
    use crate::stores::UserTableStore;
    use chrono::Utc;
    use datafusion::arrow::datatypes::{DataType, Field, Schema};
    use kalamdb_store::test_utils::TestDb;
    use serde_json::json;

    fn create_test_db() -> Arc<UserTableStore> {
        let test_db = TestDb::single_cf("user_table:chat:messages").unwrap();
        Arc::new(UserTableStore::new(test_db.db).unwrap())
    }

    fn create_test_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("content", DataType::Utf8, true),
            Field::new("_updated", DataType::Int64, false),
            Field::new("_deleted", DataType::Boolean, false),
        ]))
    }

    fn create_test_metadata() -> TableMetadata {
        TableMetadata {
            table_name: TableName::new("messages"),
            table_type: TableType::User,
            namespace: NamespaceId::new("chat"),
            created_at: Utc::now(),
            storage_location: "s3://bucket/users/${user_id}/messages/".to_string(),
            flush_policy: FlushPolicy::row_limit(1000).unwrap(),
            schema_version: 1,
            deleted_retention_hours: Some(720),
        }
    }

    #[test]
    fn test_user_table_provider_creation() {
        let store = create_test_db();
        let schema = create_test_schema();
        let metadata = create_test_metadata();
        let user_id = UserId::new("user123".to_string());

        let provider = UserTableProvider::new(
            metadata.clone(),
            schema.clone(),
            store.clone(),
            user_id.clone(),
            Role::User,
            vec![],
        );

        assert_eq!(provider.schema(), schema);
        assert_eq!(provider.namespace_id(), &NamespaceId::new("chat"));
        assert_eq!(provider.table_name(), &TableName::new("messages"));
        assert_eq!(provider.table_type(), &TableType::User);
        assert_eq!(provider.current_user_id(), &user_id);
        assert_eq!(provider.column_family_name(), "user_table:chat:messages");
    }

    #[test]
    fn test_substitute_user_id_in_path() {
        let store = create_test_db();
        let schema = create_test_schema();
        let metadata = create_test_metadata();
        let user_id = UserId::new("user123".to_string());

        let provider = UserTableProvider::new(metadata, schema, store, user_id, Role::User, vec![]);

        // Test ${user_id} substitution
        assert_eq!(
            provider.substitute_user_id_in_path("s3://bucket/users/${user_id}/messages/"),
            "s3://bucket/users/user123/messages/"
        );

        // Test user_storage_location()
        assert_eq!(
            provider.user_storage_location(),
            "s3://bucket/users/user123/messages/"
        );

        // Test path without template variable
        assert_eq!(
            provider.substitute_user_id_in_path("/data/messages/"),
            "/data/messages/"
        );
    }

    #[test]
    fn test_user_key_prefix() {
        let store = create_test_db();
        let schema = create_test_schema();
        let metadata = create_test_metadata();
        let user_id = UserId::new("user123".to_string());

        let provider = UserTableProvider::new(metadata, schema, store, user_id, Role::User, vec![]);

        let prefix = provider.user_key_prefix();
        let expected = b"user123:".to_vec();

        assert_eq!(prefix, expected);
    }

    #[test]
    fn test_insert_row() {
        let store = create_test_db();
        let schema = create_test_schema();
        let metadata = create_test_metadata();
        let user_id = UserId::new("user123".to_string());

        let provider = UserTableProvider::new(metadata, schema, store, user_id, Role::User, vec![]);

        let row_data = json!({
            "content": "Hello, World!"
        });

        let result = provider.insert_row(row_data);
        assert!(result.is_ok(), "Insert should succeed");

        let row_id = result.unwrap();
        assert!(!row_id.is_empty(), "Row ID should be generated");
    }

    #[test]
    fn test_insert_batch() {
        let store = create_test_db();
        let schema = create_test_schema();
        let metadata = create_test_metadata();
        let user_id = UserId::new("user123".to_string());

        let provider = UserTableProvider::new(metadata, schema, store, user_id, Role::User, vec![]);

        let rows = vec![
            json!({"content": "Message 1"}),
            json!({"content": "Message 2"}),
            json!({"content": "Message 3"}),
        ];

        let result = provider.insert_batch(rows);
        assert!(result.is_ok(), "Batch insert should succeed");

        let row_ids = result.unwrap();
        assert_eq!(row_ids.len(), 3, "Should generate 3 row IDs");
    }

    #[test]
    fn test_update_row() {
        let store = create_test_db();
        let schema = create_test_schema();
        let metadata = create_test_metadata();
        let user_id = UserId::new("user123".to_string());

        let provider = UserTableProvider::new(metadata, schema, store, user_id, Role::User, vec![]);

        // Insert a row first
        let row_data = json!({"content": "Original"});
        let row_id = provider.insert_row(row_data).unwrap();

        // Update the row
        let updates = json!({"content": "Updated"});
        let result = provider.update_row(&row_id, updates);
        assert!(result.is_ok(), "Update should succeed");
        assert_eq!(result.unwrap(), row_id, "Should return the updated row ID");
    }

    #[test]
    fn test_delete_row() {
        let store = create_test_db();
        let schema = create_test_schema();
        let metadata = create_test_metadata();
        let user_id = UserId::new("user123".to_string());

        let provider = UserTableProvider::new(metadata, schema, store, user_id, Role::User, vec![]);

        // Insert a row first
        let row_data = json!({"content": "To be deleted"});
        let row_id = provider.insert_row(row_data).unwrap();

        // Delete the row (soft delete)
        let result = provider.delete_row(&row_id);
        assert!(result.is_ok(), "Delete should succeed");
        assert_eq!(result.unwrap(), row_id, "Should return the deleted row ID");
    }

    #[test]
    fn test_data_isolation_different_users() {
        let store = create_test_db();
        let schema = create_test_schema();
        let metadata = create_test_metadata();

        let user1_id = UserId::new("user1".to_string());
        let user2_id = UserId::new("user2".to_string());

        let provider1 = UserTableProvider::new(
            metadata.clone(),
            schema.clone(),
            store.clone(),
            user1_id.clone(),
            Role::User,
            vec![],
        );

        let provider2 = UserTableProvider::new(
            metadata,
            schema,
            store,
            user2_id.clone(),
            Role::User,
            vec![],
        );

        // Insert data for user1
        let row_data_1 = json!({"content": "User1 message"});
        let row_id_1 = provider1.insert_row(row_data_1).unwrap();

        // Insert data for user2
        let row_data_2 = json!({"content": "User2 message"});
        let row_id_2 = provider2.insert_row(row_data_2).unwrap();

        // Verify different key prefixes
        let prefix1 = provider1.user_key_prefix();
        let prefix2 = provider2.user_key_prefix();
        assert_ne!(
            prefix1, prefix2,
            "Different users should have different key prefixes"
        );

        assert_eq!(prefix1, b"user1:".to_vec());
        assert_eq!(prefix2, b"user2:".to_vec());

        // Row IDs should be unique
        assert_ne!(row_id_1, row_id_2);
    }
}
