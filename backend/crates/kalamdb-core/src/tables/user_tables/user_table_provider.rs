//! User table provider for DataFusion integration
//!
//! This module provides a DataFusion TableProvider implementation for user tables with:
//! - Data isolation via UserId key prefix filtering
//! - Integration with UserTableInsertHandler, UserTableUpdateHandler, UserTableDeleteHandler
//! - Schema management and version tracking
//! - Storage path templating with ${user_id} substitution
//! - Hybrid RocksDB + Parquet querying

use super::{UserTableDeleteHandler, UserTableInsertHandler, UserTableUpdateHandler};
use crate::catalog::{NamespaceId, TableMetadata, TableName, TableType, UserId};
use crate::error::KalamDbError;
use crate::ids::SnowflakeGenerator;
use crate::live_query::manager::LiveQueryManager;
use crate::stores::system_table::UserTableStoreExt;
use crate::tables::arrow_json_conversion::{
    arrow_batch_to_json, json_rows_to_arrow_batch, validate_insert_rows,
};
use crate::tables::user_tables::user_table_store::UserTableRow;
use crate::tables::UserTableStore;
use async_trait::async_trait;
use chrono::Utc;
use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use datafusion::datasource::TableProvider;
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::dml::InsertOp;
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::schemas::ColumnDefault;
use kalamdb_commons::Role;
use once_cell::sync::Lazy;
use serde_json::Value as JsonValue;
use std::any::Any;
use std::collections::HashMap;
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

    /// Column default definitions for INSERT operations
    column_defaults: HashMap<String, ColumnDefault>,

    /// Storage registry for resolving full storage paths (including base_directory)
    storage_registry: Option<Arc<crate::storage::StorageRegistry>>,
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
        let column_defaults = Self::derive_column_defaults(&schema);

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
            column_defaults,
            storage_registry: None,
        }
    }

    /// Build default column map for INSERT operations.
    ///
    /// Currently injects SNOWFLAKE_ID default for auto-generated `id` columns.
    fn derive_column_defaults(schema: &SchemaRef) -> HashMap<String, ColumnDefault> {
        let mut defaults = HashMap::new();
        if schema.field_with_name("id").is_ok() {
            defaults.insert(
                "id".to_string(),
                ColumnDefault::function("SNOWFLAKE_ID", vec![]),
            );
        }
        defaults
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

    /// Set the StorageRegistry for resolving full storage paths (builder pattern)
    ///
    /// # Arguments
    /// * `registry` - StorageRegistry instance for path resolution
    pub fn with_storage_registry(mut self, registry: Arc<crate::storage::StorageRegistry>) -> Self {
        self.storage_registry = Some(registry);
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
        // Apply generated columns FIRST (id, created_at), then defaults and validation.
        // This ensures NOT NULL id/created_at are present even if not specified by caller.
        let mut rows = vec![row_data];
        self.prepare_insert_rows(&mut rows)
            .map_err(KalamDbError::InvalidOperation)?;

        for row in rows.iter_mut() {
            UserTableInsertHandler::apply_defaults_and_validate(
                row,
                self.schema.as_ref(),
                &self.column_defaults,
                &self.current_user_id,
            )?;
        }

        // At this point there is exactly one row
        let finalized = rows.into_iter().next().expect("row must exist");

        self.insert_handler.insert_row(
            self.namespace_id(),
            self.table_name(),
            &self.current_user_id,
            finalized,
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
        let mut rows = rows;
        // Populate generated columns FIRST
        self.prepare_insert_rows(&mut rows)
            .map_err(KalamDbError::InvalidOperation)?;

        // Then evaluate DEFAULTs and validate NOT NULL
        for row in rows.iter_mut() {
            UserTableInsertHandler::apply_defaults_and_validate(
                row,
                self.schema.as_ref(),
                &self.column_defaults,
                &self.current_user_id,
            )?;
        }

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
        validate_insert_rows(&self.schema, rows)
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

    /// Scan all rows for the current user.
    pub fn scan_current_user_rows(&self) -> Result<Vec<(String, UserTableRow)>, KalamDbError> {
        self.store.scan_user(
            self.namespace_id().as_str(),
            self.table_name().as_str(),
            self.current_user_id.as_str(),
        )
    }

    /// Flatten stored `UserTableRow` into a JSON object matching the logical schema.
    fn flatten_row(metadata: &TableMetadata, data: UserTableRow) -> JsonValue {
        let mut row = match data.fields {
            JsonValue::Object(map) => map,
            other => {
                log::warn!(
                    "Unexpected non-object payload in user table row {}.{}; defaulting to empty object",
                    metadata.namespace.as_str(),
                    metadata.table_name.as_str()
                );
                let mut map = serde_json::Map::new();
                if !other.is_null() {
                    map.insert("value".to_string(), other);
                }
                map
            }
        };

        row.insert(
            "_updated".to_string(),
            JsonValue::String(data._updated.clone()),
        );
        row.insert("_deleted".to_string(), JsonValue::Bool(data._deleted));

        JsonValue::Object(row)
    }

    /// Scan Parquet files and return JSON rows
    ///
    /// Reads all Parquet files for the current user from the storage directory
    /// and converts them to JSON for merging with RocksDB data.
    async fn scan_parquet_files(&self, _schema: &SchemaRef) -> DataFusionResult<Vec<JsonValue>> {
        use datafusion::parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
        use std::fs;
        use std::path::Path;

        // Resolve full storage path using StorageRegistry (same logic as flush job)
        let storage_path = if let Some(ref registry) = self.storage_registry {
            // Use StorageRegistry to get full path (includes base_directory)
            match registry.get_storage_config("local") {
                Ok(Some(storage)) => {
                    let path = storage
                        .user_tables_template()
                        .replace("{namespace}", self.namespace_id().as_str())
                        .replace("{tableName}", self.table_name().as_str())
                        .replace("{userId}", self.current_user_id.as_str())
                        .replace("{shard}", "");

                    if storage.base_directory().is_empty() {
                        path
                    } else {
                        format!(
                            "{}/{}",
                            storage.base_directory().trim_end_matches('/'),
                            path.trim_start_matches('/')
                        )
                    }
                }
                _ => {
                    // Fallback to template substitution if registry lookup fails
                    self.table_metadata
                        .storage_location
                        .replace("${user_id}", self.current_user_id.as_str())
                        .replace("{userId}", self.current_user_id.as_str())
                        .replace("{namespace}", self.namespace_id().as_str())
                        .replace("{tableName}", self.table_name().as_str())
                        .replace("{shard}", "")
                }
            }
        } else {
            // No registry available - use template substitution (legacy)
            self.table_metadata
                .storage_location
                .replace("${user_id}", self.current_user_id.as_str())
                .replace("{userId}", self.current_user_id.as_str())
                .replace("{namespace}", self.namespace_id().as_str())
                .replace("{tableName}", self.table_name().as_str())
                .replace("{shard}", "")
        };

        let storage_dir = Path::new(&storage_path);

        log::debug!(
            "Scanning Parquet files in: {} (exists: {})",
            storage_path,
            storage_dir.exists()
        );

        // If directory doesn't exist, no Parquet files to scan
        if !storage_dir.exists() {
            log::debug!("Storage directory does not exist, returning empty result");
            return Ok(Vec::new());
        }

        // List all .parquet files in the directory
        let parquet_files: Vec<_> = fs::read_dir(storage_dir)
            .map_err(|e| {
                DataFusionError::Execution(format!("Failed to read storage directory: {}", e))
            })?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().and_then(|s| s.to_str()) == Some("parquet"))
            .map(|entry| entry.path())
            .collect();

        log::debug!("Found {} Parquet files", parquet_files.len());

        if parquet_files.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_json_rows = Vec::new();

        // Read each Parquet file
        for parquet_file in parquet_files {
            log::debug!("Reading Parquet file: {:?}", parquet_file);

            let file = fs::File::open(&parquet_file).map_err(|e| {
                DataFusionError::Execution(format!(
                    "Failed to open Parquet file {:?}: {}",
                    parquet_file, e
                ))
            })?;

            let builder = ParquetRecordBatchReaderBuilder::try_new(file).map_err(|e| {
                DataFusionError::Execution(format!(
                    "Failed to create Parquet reader for {:?}: {}",
                    parquet_file, e
                ))
            })?;

            let reader = builder.build().map_err(|e| {
                DataFusionError::Execution(format!(
                    "Failed to build Parquet reader for {:?}: {}",
                    parquet_file, e
                ))
            })?;

            // Read all batches from this file
            for batch_result in reader {
                let batch = batch_result.map_err(|e| {
                    DataFusionError::Execution(format!(
                        "Failed to read batch from {:?}: {}",
                        parquet_file, e
                    ))
                })?;

                // Convert Arrow batch to JSON rows
                let json_rows = arrow_batch_to_json(&batch, true).map_err(|e| {
                    DataFusionError::Execution(format!("Arrow to JSON conversion failed: {}", e))
                })?;

                // Filter out soft-deleted rows and add system columns if missing
                for mut row in json_rows {
                    // Ensure _deleted field exists (default to false for Parquet rows)
                    if let Some(obj) = row.as_object_mut() {
                        if !obj.contains_key("_deleted") {
                            obj.insert("_deleted".to_string(), JsonValue::Bool(false));
                        }

                        // Ensure _updated field exists (use current time as fallback)
                        if !obj.contains_key("_updated") {
                            let now = chrono::Utc::now().to_rfc3339();
                            obj.insert("_updated".to_string(), JsonValue::String(now));
                        }
                    }

                    // Check if row is deleted
                    if let Some(deleted) = row.get("_deleted").and_then(|v| v.as_bool()) {
                        if !deleted {
                            all_json_rows.push(row);
                        }
                    } else {
                        // If _deleted field is missing or not a bool, include the row
                        all_json_rows.push(row);
                    }
                }
            }
        }

        log::debug!("Total rows from Parquet files: {}", all_json_rows.len());
        Ok(all_json_rows)
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
        log::debug!(
            "UserTableProvider::scan() called for table {}.{}, user {}",
            self.namespace_id().as_str(),
            self.table_name().as_str(),
            self.current_user_id.as_str()
        );

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

        // **STEP 1: Scan RocksDB buffered data**
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
            .filter(|(_row_id, row_data)| !row_data._deleted)
            .collect();

        // Convert RocksDB rows to JSON for merging
        let rocksdb_json: Vec<JsonValue> = filtered_rows
            .into_iter()
            .map(|(_id, data)| Self::flatten_row(&self.table_metadata, data))
            .collect();

        log::debug!("RocksDB rows: {}", rocksdb_json.len());

        // **STEP 2: Scan Parquet files**
        let parquet_json = self.scan_parquet_files(&full_schema).await?;

        log::debug!("Parquet rows: {}", parquet_json.len());

        // **STEP 3: Merge RocksDB + Parquet data**
        let mut all_rows = parquet_json;
        all_rows.extend(rocksdb_json);

        // Apply limit if specified
        let limited_rows = if let Some(limit_value) = limit {
            all_rows.into_iter().take(limit_value).collect()
        } else {
            all_rows
        };

        // Convert JSON rows to Arrow RecordBatch (includes system columns)
        let batch = json_rows_to_arrow_batch(&full_schema, limited_rows).map_err(|e| {
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
            let mut json_rows = arrow_batch_to_json(&batch, true).map_err(|e| {
                DataFusionError::Execution(format!("Arrow to JSON conversion failed: {}", e))
            })?;

            // Validate schema constraints (NOT NULL, etc.) before insert
            self.validate_insert_rows(&json_rows).map_err(|e| {
                DataFusionError::Execution(format!("Schema validation failed: {}", e))
            })?;

            // Evaluate DEFAULT expressions (Snowflake IDs, timestamps, etc.)
            for row in json_rows.iter_mut() {
                UserTableInsertHandler::apply_defaults_and_validate(
                    row,
                    self.schema.as_ref(),
                    &self.column_defaults,
                    &self.current_user_id,
                )
                .map_err(|e| {
                    DataFusionError::Execution(format!("DEFAULT evaluation failed: {}", e))
                })?;
            }

            // Populate auto-increment IDs when missing
            self.prepare_insert_rows(&mut json_rows)
                .map_err(DataFusionError::Execution)?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flush::FlushPolicy;
    use crate::tables::UserTableStore;
    use chrono::Utc;
    use datafusion::arrow::array::{
        BooleanArray, Int64Array, StringArray, TimestampMillisecondArray,
    };
    use datafusion::arrow::datatypes::{DataType, Field, Schema};
    use datafusion::execution::context::SessionContext;
    use kalamdb_store::test_utils::TestDb;
    use serde_json::json;

    fn create_test_db() -> Arc<UserTableStore> {
        let test_db = TestDb::single_cf("user_table:chat:messages").unwrap();
        let backend: Arc<dyn kalamdb_store::StorageBackend> =
            Arc::new(kalamdb_store::RocksDBBackend::new(test_db.db));
        Arc::new(UserTableStore::new(backend, "user_table:chat:messages"))
    }

    fn create_test_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("content", DataType::Utf8, true),
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

    #[tokio::test]
    async fn test_scan_flattens_user_rows() {
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

        let row = UserTableRow {
            row_id: "row1".to_string(),
            user_id: user_id.as_str().to_string(),
            fields: json!({
                "id": 123_i64,
                "content": "Hello, KalamDB!"
            }),
            _updated: "2025-01-01T00:00:00Z".to_string(),
            _deleted: false,
        };

        UserTableStoreExt::put(
            provider.store.as_ref(),
            provider.namespace_id().as_str(),
            provider.table_name().as_str(),
            provider.current_user_id().as_str(),
            &row.row_id,
            &row,
        )
        .expect("should persist user row");

        let ctx = SessionContext::new();
        let exec_plan = provider
            .scan(&ctx.state(), None, &[], None)
            .await
            .expect("scan should succeed");

        let batches = datafusion::physical_plan::collect(exec_plan, ctx.task_ctx())
            .await
            .expect("collect should succeed");

        assert_eq!(batches.len(), 1, "should produce a single batch");
        let batch = &batches[0];
        assert_eq!(batch.num_rows(), 1, "should have one row");

        let schema = batch.schema();

        let id_idx = schema.index_of("id").expect("id column missing");
        let id_array = batch
            .column(id_idx)
            .as_any()
            .downcast_ref::<Int64Array>()
            .expect("id column should be Int64");
        assert_eq!(id_array.value(0), 123_i64);

        let content_idx = schema.index_of("content").expect("content column missing");
        let content_array = batch
            .column(content_idx)
            .as_any()
            .downcast_ref::<StringArray>()
            .expect("content column should be Utf8");
        assert_eq!(content_array.value(0), "Hello, KalamDB!");

        let updated_idx = schema
            .index_of("_updated")
            .expect("_updated column missing");
        let updated_array = batch
            .column(updated_idx)
            .as_any()
            .downcast_ref::<TimestampMillisecondArray>()
            .expect("_updated column should be timestamp");
        assert!(updated_array.value(0) > 0, "_updated should be populated");

        let deleted_idx = schema
            .index_of("_deleted")
            .expect("_deleted column missing");
        let deleted_array = batch
            .column(deleted_idx)
            .as_any()
            .downcast_ref::<BooleanArray>()
            .expect("_deleted column should be boolean");
        assert!(!deleted_array.value(0), "_deleted should be false");
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

    #[tokio::test]
    async fn test_delete_row_excludes_soft_deleted_from_scan() {
        let store = create_test_db();
        let schema = create_test_schema();
        let metadata = create_test_metadata();
        let user_id = UserId::new("user123".to_string());

        let provider =
            UserTableProvider::new(metadata, schema, store, user_id.clone(), Role::User, vec![]);

        let row_a = json!({"content": "Keep me"});
        let row_b = json!({"content": "Delete me"});

        provider.insert_row(row_a).expect("insert row_a");
        let id_b = provider.insert_row(row_b).expect("insert row_b");

        provider.delete_row(&id_b).expect("soft delete row_b");

        let ctx = SessionContext::new();
        let exec_plan = provider
            .scan(&ctx.state(), None, &[], None)
            .await
            .expect("scan should succeed");

        let batches = datafusion::physical_plan::collect(exec_plan, ctx.task_ctx())
            .await
            .expect("collect should succeed");

        assert_eq!(batches.len(), 1);
        let batch = &batches[0];
        assert_eq!(
            batch.num_rows(),
            1,
            "soft deleted row should be filtered out"
        );

        let schema = batch.schema();
        let content_idx = schema.index_of("content").expect("content column missing");
        let content_array = batch
            .column(content_idx)
            .as_any()
            .downcast_ref::<StringArray>()
            .expect("content column should be Utf8");
        assert_eq!(content_array.value(0), "Keep me");

        // ensure remaining row is id_a
        let id_idx = schema.index_of("id").expect("id column missing");
        let id_array = batch
            .column(id_idx)
            .as_any()
            .downcast_ref::<Int64Array>()
            .expect("id column should be Int64");
        assert!(
            id_array.value(0) > 0,
            "remaining row should have generated id"
        );
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
