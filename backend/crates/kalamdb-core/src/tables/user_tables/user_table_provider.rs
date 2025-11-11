//! User table access for DataFusion integration
//!
//! This module provides a lightweight per-request wrapper for user tables with:
//! - Data isolation via UserId key prefix filtering
//! - Integration with UserTableShared (singleton containing handlers and defaults)
//! - Hybrid RocksDB + Parquet querying
//!
//! **Phase 3C**: Refactored to eliminate redundant handler allocations
//! - Before: Every UserTableProvider instance allocated 3 Arc<Handler> + HashMap
//! - After: UserTableProvider wraps Arc<UserTableShared> (created once per table, cached)
//! - Memory savings: 6 fields → 3 fields (50% reduction per instance)

use super::UserTableInsertHandler;
use crate::schema_registry::{NamespaceId, TableName, UserId};
use crate::tables::base_table_provider::{BaseTableProvider, UserTableShared};
use crate::error::KalamDbError;
use crate::app_context::AppContext;
use kalamdb_commons::ids::SnowflakeGenerator;
use crate::tables::system::system_table_store::UserTableStoreExt;
use crate::tables::arrow_json_conversion::{
    arrow_batch_to_json, json_rows_to_arrow_batch, validate_insert_rows,
};
use crate::tables::user_tables::user_table_store::UserTableRow;
use async_trait::async_trait;
use chrono::Utc;
use datafusion::arrow::datatypes::SchemaRef;
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
// Bring EntityStoreV2 trait into scope to access default scan helpers
use kalamdb_store::EntityStoreV2 as EntityStore;

/// Shared Snowflake generator for auto-increment values
static AUTO_ID_GENERATOR: Lazy<SnowflakeGenerator> = Lazy::new(|| SnowflakeGenerator::new(0));

/// Stateless provider for USER tables with Row-Level Security
///
/// **Architecture** (Phase 10 Optimization):
/// - No per-request fields (current_user_id, access_role removed)
/// - Reads SessionUserContext from SessionState.extensions during scan()
/// - Registered ONCE in base_session_context (no clone overhead)
///
/// **RLS Enforcement**:
/// - scan() extracts user_id from SessionState → filters by key prefix
/// - insert_into() extracts user_id from SessionState → scopes data
/// - All DML operations enforce per-user isolation at storage layer
///
/// **Performance**: Zero SessionState clone overhead (vs 1-2μs per request)
///
/// **Usage**:
/// ```ignore
/// // Once at table registration:
/// let shared = UserTableShared::new(table_id, cache, schema, store);
/// let provider = UserTableProvider::new(shared);
/// base_session.register_table("my_table", Arc::new(provider))?;
///
/// // Per-request: SessionContext clones SessionState + injects user_id
/// let session = exec_ctx.create_session_with_user();
/// session.sql("SELECT * FROM my_table").await?;
/// ```
pub struct UserTableProvider {
    /// Shared table-level state (handlers, defaults, core fields)
    shared: Arc<UserTableShared>,
}

impl std::fmt::Debug for UserTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserTableProvider")
            .field("table_id", self.shared.core().table_id())
            .field("schema", &self.shared.core().schema_ref())
            .finish()
    }
}

impl UserTableProvider {
    /// Create a new stateless user table provider
    ///
    /// # Arguments
    /// * `shared` - Arc<UserTableShared> containing table-level singletons (cached in SchemaCache)
    ///
    /// User context (user_id, role) is read from SessionState.extensions during query execution.
    pub fn new(shared: Arc<UserTableShared>) -> Self {
        Self { shared }
    }

    /// Get access to the shared table state
    ///
    /// Useful for accessing handlers, store, or core fields
    pub fn shared(&self) -> &Arc<UserTableShared> {
        &self.shared
    }

    /// Extract user context from DataFusion SessionState
    ///
    /// **Purpose**: Read (user_id, role) from SessionState.config.options.extensions
    /// injected by ExecutionContext.create_session_with_user()
    ///
    /// **Returns**: (UserId, Role) tuple for RLS enforcement
    fn extract_user_context(state: &dyn datafusion::catalog::Session) -> Result<(UserId, Role), KalamDbError> {
        use crate::sql::executor::models::SessionUserContext;
        
        let session_state = state.as_any()
            .downcast_ref::<datafusion::execution::context::SessionState>()
            .ok_or_else(|| KalamDbError::InvalidOperation("Expected SessionState".to_string()))?;
        
        let user_ctx = session_state
            .config()
            .options()
            .extensions
            .get::<SessionUserContext>()
            .ok_or_else(|| KalamDbError::InvalidOperation("SessionUserContext not found in extensions".to_string()))?;
        
        Ok((user_ctx.user_id.clone(), user_ctx.role.clone()))
    }

    /// Get the column family name for this user table
    pub fn column_family_name(&self) -> String {
        format!(
            "user_table:{}:{}",
            self.shared.core().namespace().as_str(),
            self.shared.core().table_name().as_str()
        )
    }

    /// Get the namespace ID
    pub fn namespace_id(&self) -> &NamespaceId {
        self.shared.core().namespace()
    }

    /// Get the table name
    pub fn table_name(&self) -> &TableName {
        self.shared.core().table_name()
    }

    /// Get the namespace and table from cached metadata
    pub fn get_cached_metadata(&self) -> Option<(NamespaceId, TableName)> {
        if let Some(cached_data) = self.shared.core().cache().get(self.shared.core().table_id()) {
            Some((
                cached_data.table.namespace_id.clone(),
                cached_data.table.table_name.clone(),
            ))
        } else {
            None
        }
    }

    /// Insert a single row into this user table
    ///
    /// # Arguments
    /// * `user_id` - User ID for data isolation
    /// * `row_data` - Row data as JSON object
    ///
    /// # Returns
    /// The generated row ID
    pub fn insert_row(&self, user_id: &UserId, row_data: JsonValue) -> Result<String, KalamDbError> {
        // Apply generated columns FIRST (id, created_at), then defaults and validation.
        // This ensures NOT NULL id/created_at are present even if not specified by caller.
        let mut rows = vec![row_data];
        self.prepare_insert_rows(&mut rows)
            .map_err(KalamDbError::InvalidOperation)?;

        for row in rows.iter_mut() {
            UserTableInsertHandler::apply_defaults_and_validate(
                row,
                self.shared.core().schema_ref().as_ref(),
                self.shared.column_defaults().as_ref(),
                user_id,
            )?;
        }

        // At this point there is exactly one row
        let finalized = rows.into_iter().next().expect("row must exist");

        self.shared.insert_handler().insert_row(
            self.namespace_id(),
            self.table_name(),
            user_id,
            finalized,
        )
    }

    /// Insert multiple rows into this user table
    ///
    /// # Arguments
    /// * `user_id` - User ID for data isolation
    /// * `rows` - Vector of row data as JSON objects
    ///
    /// # Returns
    /// Vector of generated row IDs
    pub fn insert_batch(&self, user_id: &UserId, rows: Vec<JsonValue>) -> Result<Vec<String>, KalamDbError> {
        let mut rows = rows;
        // Populate generated columns FIRST
        self.prepare_insert_rows(&mut rows)
            .map_err(KalamDbError::InvalidOperation)?;

        // Then evaluate DEFAULTs and validate NOT NULL
        for row in rows.iter_mut() {
            UserTableInsertHandler::apply_defaults_and_validate(
                row,
                self.shared.core().schema_ref().as_ref(),
                self.shared.column_defaults().as_ref(),
                user_id,
            )?;
        }

        self.shared.insert_handler().insert_batch(
            self.namespace_id(),
            self.table_name(),
            user_id,
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
        validate_insert_rows(&self.shared.core().schema_ref(), rows)
    }

    /// Populate generated columns (id, created_at) when they are missing from the INSERT payload
    fn prepare_insert_rows(&self, rows: &mut [JsonValue]) -> Result<(), String> {
        let has_id = self.shared.core().schema_ref().field_with_name("id").is_ok();
        let has_created_at = self.shared.core().schema_ref().field_with_name("created_at").is_ok();

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
    /// * `user_id` - User ID for data isolation
    /// * `row_id` - Row ID to update
    /// * `updates` - Updated fields as JSON object
    ///
    /// # Returns
    /// The row ID of the updated row
    pub fn update_row(&self, user_id: &UserId, row_id: &str, updates: JsonValue) -> Result<String, KalamDbError> {
        self.shared.update_handler().update_row(
            self.namespace_id(),
            self.table_name(),
            user_id,
            row_id,
            updates,
        )
    }

    /// Update multiple rows in this user table
    ///
    /// # Arguments
    /// * `user_id` - User ID for data isolation
    /// * `updates` - Vector of (row_id, updates) tuples
    ///
    /// # Returns
    /// Vector of updated row IDs
    pub fn update_batch(
        &self,
        user_id: &UserId,
        updates: Vec<(String, JsonValue)>,
    ) -> Result<Vec<String>, KalamDbError> {
        self.shared.update_handler().update_batch(
            self.namespace_id(),
            self.table_name(),
            user_id,
            updates,
        )
    }

    /// Soft delete a single row in this user table
    ///
    /// # Arguments
    /// * `user_id` - User ID for data isolation
    /// * `row_id` - Row ID to delete
    ///
    /// # Returns
    /// The row ID of the deleted row
    pub fn delete_row(&self, user_id: &UserId, row_id: &str) -> Result<String, KalamDbError> {
        self.shared.delete_handler().delete_row(
            self.namespace_id(),
            self.table_name(),
            user_id,
            row_id,
        )
    }

    /// Soft delete multiple rows in this user table
    ///
    /// # Arguments
    /// * `user_id` - User ID for data isolation
    /// * `row_ids` - Vector of row IDs to delete
    ///
    /// # Returns
    /// Vector of deleted row IDs
    pub fn delete_batch(&self, user_id: &UserId, row_ids: Vec<String>) -> Result<Vec<String>, KalamDbError> {
        self.shared.delete_handler().delete_batch(
            self.namespace_id(),
            self.table_name(),
            user_id,
            row_ids,
        )
    }

    /// DELETE by logical `id` field (helper for SQL layer)
    ///
    /// Finds the row owned by current user whose JSON field `id` equals `id_value` 
    /// and performs a soft delete. This bridges the mismatch between external primary 
    /// key semantics (id column) and internal storage key (row_id).
    ///
    /// # Arguments
    /// * `user_id` - User ID for data isolation
    /// * `id_value` - Value of the logical id field to match
    pub fn delete_by_id_field(&self, user_id: &UserId, id_value: &str) -> Result<String, KalamDbError> {
        let rows = self.scan_current_user_rows(user_id)?;

        log::debug!("delete_by_id_field: Looking for id={} among {} user rows", id_value, rows.len());

        // Match either numeric or string representations
        let mut target_row_id: Option<String> = None;
        for (_row_id_key, row_data) in rows.iter() {
            if let Some(v) = row_data.fields.get("id") {
                log::debug!("delete_by_id_field: Found row with id field: {:?}", v);
                let is_match = match v {
                    serde_json::Value::Number(n) => {
                        let n_str = n.to_string();
                        log::debug!("delete_by_id_field: Comparing number {} with {}", n_str, id_value);
                        n_str == id_value
                    }
                    serde_json::Value::String(s) => {
                        log::debug!("delete_by_id_field: Comparing string {} with {}", s, id_value);
                        s == id_value
                    }
                    _ => {
                        log::debug!("delete_by_id_field: id field is neither number nor string: {:?}", v);
                        false
                    }
                };
                if is_match {
                    log::debug!("delete_by_id_field: Match found! row_id={}", row_data.row_id);
                    target_row_id = Some(row_data.row_id.clone());
                    break;
                }
            } else {
                log::debug!("delete_by_id_field: Row has no id field");
            }
        }

        let row_id = target_row_id
            .ok_or_else(|| {
                log::error!("delete_by_id_field: No row found with id={} for user {}", id_value, user_id.as_str());
                KalamDbError::NotFound(format!("Row with id={} not found for user", id_value))
            })?;
        self.delete_row(user_id, &row_id)
    }

    /// UPDATE by logical `id` field (helper for SQL layer)
    ///
    /// Finds the row owned by current user whose JSON field `id` equals `id_value`
    /// and performs an update. This bridges the mismatch between external primary
    /// key semantics (id column) and internal storage key (row_id).
    ///
    /// **Phase 3, US1, T061**: Now supports UPDATE on flushed records via version resolution
    ///
    /// # Arguments
    /// * `user_id` - User ID for data isolation
    /// * `id_value` - Value of the logical id field to match
    /// * `updates` - JSON object with field updates
    pub fn update_by_id_field(&self, user_id: &UserId, id_value: &str, updates: serde_json::Value) -> Result<(), KalamDbError> {
        // Use version resolution scan to find record in BOTH RocksDB and Parquet
        let rows = tokio::runtime::Handle::current().block_on(async {
            self.scan_current_user_rows_with_version_resolution(user_id).await
        })?;

        log::debug!("update_by_id_field: Looking for id={} among {} user rows (with version resolution)", id_value, rows.len());

        // Match either numeric or string representations
        let mut target_row_id: Option<String> = None;
        for (_row_id_key, row_data) in rows.iter() {
            if let Some(v) = row_data.fields.get("id") {
                let is_match = match v {
                    serde_json::Value::Number(n) => n.to_string() == id_value,
                    serde_json::Value::String(s) => s == id_value,
                    _ => false,
                };
                if is_match {
                    log::debug!("update_by_id_field: Match found! row_id={}", row_data.row_id);
                    target_row_id = Some(row_data.row_id.clone());
                    break;
                }
            }
        }

        let row_id = target_row_id
            .ok_or_else(|| {
                log::error!("update_by_id_field: No row found with id={} for user {}", id_value, user_id.as_str());
                KalamDbError::NotFound(format!("Row with id={} not found for user", id_value))
            })?;
        self.update_row(user_id, &row_id, updates).map(|_| ())
    }

    /// Get the user-specific key prefix for data isolation
    ///
    /// This implements T128 - data isolation enforcement
    ///
    /// All queries will be filtered to only access rows with this prefix,
    /// ensuring users can only see their own data.
    ///
    /// # Arguments
    /// * `user_id` - User ID for isolation
    ///
    /// # Returns
    /// Key prefix in format "{UserId}:"
    pub fn user_key_prefix(user_id: &UserId) -> Vec<u8> {
        format!("{}:", user_id.as_str())
            .as_bytes()
            .to_vec()
    }

    /// Scan all rows for the specified user.
    ///
    /// # Arguments
    /// * `user_id` - User ID for data isolation
    pub fn scan_current_user_rows(&self, user_id: &UserId) -> Result<Vec<(String, UserTableRow)>, KalamDbError> {
        self.shared.store().scan_user(
            self.namespace_id().as_str(),
            self.table_name().as_str(),
            user_id.as_str(),
        )
    }

    /// Scan all rows for the specified user WITH VERSION RESOLUTION (includes Parquet)
    ///
    /// This comprehensive scan:
    /// 1. Scans RocksDB (fast storage)
    /// 2. Scans Parquet files (flushed storage)
    /// 3. Applies version resolution (latest _updated wins)
    /// 4. Filters out deleted records (_deleted = true)
    /// 5. Converts back to UserTableRow structs
    ///
    /// **Phase 3, US1, T061-T065**: Support UPDATE/DELETE on flushed records
    ///
    /// # Arguments
    /// * `user_id` - User ID for data isolation
    ///
    /// # Returns
    /// Vector of (row_id, UserTableRow) tuples representing latest version of each record
    async fn scan_current_user_rows_with_version_resolution(
        &self,
        user_id: &UserId,
    ) -> Result<Vec<(String, UserTableRow)>, KalamDbError> {
        use crate::tables::base_table_provider::{scan_with_version_resolution_to_kvs, arrow_value_to_json};
        use datafusion::arrow::array::AsArray;
        use serde_json::{Map, Value as JsonValue};

        let schema = self.shared.core().arrow_schema();
        let user_id_clone = user_id.clone();

        // Define row converter closure
        let row_converter = move |batch: &datafusion::arrow::record_batch::RecordBatch, row_idx: usize| {
            // Extract _id (Snowflake ID)
            let id_col = batch
                .column_by_name("_id")
                .ok_or_else(|| datafusion::error::DataFusionError::Execution("Missing _id column".to_string()))?;
            let id_array = id_col.as_primitive::<datafusion::arrow::datatypes::Int64Type>();
            let snowflake_id = id_array.value(row_idx);
            let row_id = snowflake_id.to_string();

            // Extract _updated (RFC3339 timestamp string)
            let updated_col = batch
                .column_by_name("_updated")
                .ok_or_else(|| datafusion::error::DataFusionError::Execution("Missing _updated column".to_string()))?;
            let updated_array = updated_col.as_string::<i32>();
            let updated_str = updated_array.value(row_idx).to_string();

            // Extract _deleted (boolean)
            let deleted_col = batch
                .column_by_name("_deleted")
                .ok_or_else(|| datafusion::error::DataFusionError::Execution("Missing _deleted column".to_string()))?;
            let deleted_array = deleted_col.as_boolean();
            let deleted = deleted_array.value(row_idx);

            // Extract user-defined fields into JSON object
            let mut fields_map = Map::new();
            for (field_name, column) in batch.schema().fields().iter().zip(batch.columns().iter()) {
                // Skip system columns
                if field_name.name() == "_id" || field_name.name() == "_updated" || field_name.name() == "_deleted" {
                    continue;
                }

                // Convert Arrow value to JSON
                let json_value = arrow_value_to_json(column.as_ref(), row_idx)?;
                fields_map.insert(field_name.name().clone(), json_value);
            }

            let user_table_row = UserTableRow {
                row_id: row_id.clone(),
                user_id: user_id_clone.as_str().to_string(),
                fields: JsonValue::Object(fields_map),
                _updated: updated_str,
                _deleted: deleted,
            };

            Ok((row_id, user_table_row))
        };

        // Call the generic scan_with_version_resolution_to_kvs helper
        let rows = scan_with_version_resolution_to_kvs(
            schema.clone(),
            self.scan_rocksdb_as_batch(user_id, &schema, None),
            self.scan_parquet_as_batch(user_id, &schema),
            row_converter,
        )
        .await
        .map_err(|e| KalamDbError::Other(format!("Version resolution scan failed: {}", e)))?;

        Ok(rows)
    }

    /// Flatten stored `UserTableRow` into a JSON object matching the logical schema.
    fn flatten_row(&self, data: UserTableRow) -> JsonValue {
        let mut row = match data.fields {
            JsonValue::Object(map) => map,
            other => {
                log::warn!(
                    "Unexpected non-object payload in user table row {}.{}; defaulting to empty object",
                    self.table_id().namespace_id().as_str(),
                    self.table_id().table_name().as_str()
                );
                let mut map = serde_json::Map::new();
                if !other.is_null() {
                    map.insert("value".to_string(), other);
                }
                map
            }
        };

        // Add system columns to flattened row
        // _id: Parse row_id (Snowflake ID stored as String) to i64
        if let Ok(snowflake_id) = data.row_id.parse::<i64>() {
            row.insert("_id".to_string(), JsonValue::Number(snowflake_id.into()));
        } else {
            log::warn!("Failed to parse row_id as Snowflake ID: {}", data.row_id);
        }

        row.insert(
            "_updated".to_string(),
            JsonValue::String(data._updated.clone()),
        );
        row.insert("_deleted".to_string(), JsonValue::Bool(data._deleted));

        JsonValue::Object(row)
    }

    /// T055: Scan RocksDB and return Arrow RecordBatch (for version resolution)
    ///
    /// # Arguments
    /// * `user_id` - User ID for RLS isolation
    /// * `schema` - Full schema including system columns (_updated, _deleted)
    /// * `limit` - Optional limit for early termination
    ///
    /// # Returns
    /// RecordBatch from RocksDB with system columns
    async fn scan_rocksdb_as_batch(
        &self,
        user_id: &UserId,
        schema: &SchemaRef,
        limit: Option<usize>,
    ) -> DataFusionResult<datafusion::arrow::record_batch::RecordBatch> {
        let mut rocksdb_json: Vec<JsonValue> = Vec::new();
        let user_prefix = Self::user_key_prefix(user_id);

        if let Some(limit_value) = limit {
            log::debug!(
                "🔍 RLS SCAN (RocksDB): table={}.{}, user_id={}, limit={}",
                self.namespace_id().as_str(),
                self.table_name().as_str(),
                user_id.as_str(),
                limit_value
            );
            
            let raw = self
                .shared
                .store()
                .scan_prefix_limited_bytes(&user_prefix, limit_value * 2)
                .map_err(|e| DataFusionError::Execution(format!("Failed to scan user prefix: {}", e)))?;

            for (_key_bytes, row) in raw.into_iter() {
                // T055: Don't filter _deleted here - version resolution handles it
                rocksdb_json.push(self.flatten_row(row));
                if rocksdb_json.len() >= limit_value {
                    break;
                }
            }
        } else {
            log::debug!(
                "🔍 RLS SCAN (RocksDB, no limit): table={}.{}, user_id={}",
                self.namespace_id().as_str(),
                self.table_name().as_str(),
                user_id.as_str()
            );
            
            let raw_rows = self.shared.store()
                .scan_user(
                    self.namespace_id().as_str(),
                    self.table_name().as_str(),
                    user_id.as_str(),
                )
                .map_err(|e| DataFusionError::Execution(format!("Failed to scan user table: {}", e)))?;

            rocksdb_json = raw_rows
                .into_iter()
                .map(|(_id, data)| self.flatten_row(data))
                .collect();
        }

        log::debug!("RocksDB scan: {} rows", rocksdb_json.len());

        // Convert JSON to Arrow RecordBatch
        json_rows_to_arrow_batch(schema, rocksdb_json)
            .map_err(|e| DataFusionError::Execution(format!("JSON to Arrow conversion failed: {}", e)))
    }

    /// T055: Scan Parquet files and return Arrow RecordBatch (for version resolution)
    ///
    /// # Arguments
    /// * `user_id` - User ID for directory-level RLS isolation
    /// * `schema` - Full schema including system columns
    ///
    /// # Returns
    /// Concatenated RecordBatch from all Parquet files
    async fn scan_parquet_as_batch(
        &self,
        user_id: &UserId,
        schema: &SchemaRef,
    ) -> DataFusionResult<datafusion::arrow::record_batch::RecordBatch> {
        use std::path::Path;

        let resolve_template_fallback = || {
            self.shared
                .core()
                .cache()
                .get(self.shared.core().table_id())
                .map(|cached_data| {
                    cached_data
                        .storage_path_template
                        .replace("{userId}", user_id.as_str())
                        .replace("{shard}", "")
                })
                .unwrap_or_else(|| String::new())
        };

        let storage_path = self
            .shared
            .core()
            .cache()
            .get_storage_path(
                self.shared.core().table_id(),
                Some(user_id),
                None,
            )
            .unwrap_or_else(|err| {
                log::warn!(
                    "Failed to resolve storage path via cache for table {}.{}: {}",
                    self.namespace_id().as_str(),
                    self.table_name().as_str(),
                    err
                );
                resolve_template_fallback()
            });

        if storage_path.is_empty() {
            log::debug!(
                "Storage path resolution returned empty string for table {}.{}, skipping Parquet scan",
                self.namespace_id().as_str(),
                self.table_name().as_str()
            );
            // Return empty RecordBatch using shared helper
            return crate::tables::base_table_provider::create_empty_batch(schema);
        }

        let storage_dir = Path::new(&storage_path);
        log::debug!(
            "🔍 RLS (Parquet): Scanning for user={} in path={} (exists={})",
            user_id.as_str(),
            storage_path,
            storage_dir.exists()
        );

        // RLS assertion: Verify storage path contains user_id
        if !storage_path.contains(user_id.as_str()) {
            log::error!(
                "🚨 RLS VIOLATION: Storage path does NOT contain user_id! user={}, path={}",
                user_id.as_str(),
                storage_path
            );
            return Err(DataFusionError::Execution(format!(
                "RLS violation: storage path missing user_id isolation"
            )));
        }

        let table_identifier = format!(
            "{}.{} (user={})",
            self.namespace_id().as_str(),
            self.table_name().as_str(),
            user_id.as_str()
        );

        // Use shared Parquet scanning helper from base_table_provider
        crate::tables::base_table_provider::scan_parquet_files_as_batch(
            &storage_path,
            schema,
            &table_identifier,
        )
        .await
    }
}

impl BaseTableProvider for UserTableProvider {
    fn table_id(&self) -> &kalamdb_commons::models::TableId {
        self.shared.core().table_id()
    }

    fn schema_ref(&self) -> SchemaRef {
        self.shared.core().schema_ref()
    }

    fn table_type(&self) -> crate::schema_registry::TableType {
        self.shared.core().table_type()
    }
}

#[async_trait]
impl TableProvider for UserTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        // Phase 10, US1, FR-006: Use memoized Arrow schema (50-100× speedup)
        // Return the base schema without system columns
        // System columns (_updated, _deleted) are added dynamically during scan()
        self.shared.core().arrow_schema()
            .expect("Schema must be valid for user table")
    }

    fn table_type(&self) -> datafusion::datasource::TableType {
        datafusion::datasource::TableType::Base
    }

    async fn scan(
        &self,
        state: &dyn datafusion::catalog::Session,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        // Extract user_id and role from SessionState.extensions
        let (current_user_id, access_role) = Self::extract_user_context(state)
            .map_err(|e| DataFusionError::Execution(format!("Failed to extract user context: {}", e)))?;

        log::debug!(
            "UserTableProvider::scan() called for table {}.{}, user {}, role {:?}",
            self.namespace_id().as_str(),
            self.table_name().as_str(),
            current_user_id.as_str(),
            access_role
        );

        // Get the base schema and add system columns for the scan result
        let full_schema = crate::tables::base_table_provider::schema_with_system_columns(
            &self.shared.core().schema_ref()
        );

        // T055: STEP 1 - Scan RocksDB as Arrow RecordBatch (no _deleted filtering)
        log::info!(
            "🔍 RLS SCAN: table={}.{}, user_id={}, role={:?}",
            self.namespace_id().as_str(),
            self.table_name().as_str(),
            current_user_id.as_str(),
            access_role
        );
        
        let fast_batch = self.scan_rocksdb_as_batch(&current_user_id, &full_schema, limit).await?;
        log::debug!("RocksDB scan: {} rows", fast_batch.num_rows());

        // T055: STEP 2 - Scan Parquet as Arrow RecordBatch (no _deleted filtering)
        let long_batch = self.scan_parquet_as_batch(&current_user_id, &full_schema).await?;
        log::debug!("Parquet scan: {} rows", long_batch.num_rows());

        // T055: STEP 3-5 - Version Resolution + Deletion Filter + Limit (unified helper)
        let final_batch = crate::tables::base_table_provider::scan_with_version_resolution_and_filter(
            fast_batch,
            long_batch,
            full_schema.clone(),
            limit,
        )
        .await?;

        // Create an in-memory table over the full schema and let DataFusion handle projection
        use datafusion::datasource::MemTable;
        let partitions = vec![vec![final_batch]];
        let table = MemTable::try_new(full_schema, partitions)
            .map_err(|e| DataFusionError::Execution(format!("Failed to create MemTable: {}", e)))?;

        table.scan(state, projection, &[], None).await
    }

    async fn insert_into(
        &self,
        state: &dyn datafusion::catalog::Session,
        input: Arc<dyn ExecutionPlan>,
        _op: InsertOp,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        use datafusion::execution::TaskContext;
        use datafusion::physical_plan::collect;

        // Extract user_id from SessionState for RLS enforcement
        let (current_user_id, _role) = Self::extract_user_context(state)
            .map_err(|e| DataFusionError::Execution(format!("Failed to extract user context: {}", e)))?;

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
                    self.shared.core().schema_ref().as_ref(),
                    self.shared.column_defaults().as_ref(),
                    &current_user_id,
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
            self.insert_batch(&current_user_id, json_rows)
                .map_err(|e| DataFusionError::Execution(format!("Insert failed: {}", e)))?;
        }

        // Return empty execution plan (INSERT returns no rows)
        use datafusion::physical_plan::empty::EmptyExec;
        Ok(Arc::new(EmptyExec::new(self.shared.core().schema_ref())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tables::UserTableStore;
    use datafusion::arrow::array::{
        BooleanArray, Int64Array, StringArray, TimestampMillisecondArray,
    };
    use datafusion::arrow::datatypes::{DataType, Field, Schema};
    use datafusion::execution::context::SessionContext;
    use kalamdb_commons::{TableId, TableType};
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

    /// Phase 10: Create Arc<TableId> for test providers (avoids allocation on every cache lookup)
    fn create_test_table_id() -> Arc<TableId> {
        Arc::new(TableId::new(
            NamespaceId::new("chat"),
            TableName::new("messages"),
        ))
    }

    fn create_test_metadata() -> Arc<crate::schema_registry::SchemaRegistry> {
    use crate::schema_registry::{CachedTableData, SchemaRegistry};
    use kalamdb_commons::models::schemas::{TableDefinition, ColumnDefinition};
    use kalamdb_commons::datatypes::KalamDataType;

        let cache = Arc::new(SchemaRegistry::new(0, None));

        let table_id = TableId::new(NamespaceId::new("chat"), TableName::new("messages"));

        let columns = vec![
            ColumnDefinition::new(
                "id",
                1,
                KalamDataType::BigInt,  // Use BigInt for Int64
                false, // not nullable
                true,  // primary key
                false, // not unique
                kalamdb_commons::schemas::ColumnDefault::None,
                None,
            ),
            ColumnDefinition::new(
                "content",
                2,
                KalamDataType::Text,
                true,  // nullable
                false, // not primary key
                false, // not unique
                kalamdb_commons::schemas::ColumnDefault::None,
                None,
            ),
        ];

        let td: Arc<TableDefinition> = Arc::new(
            TableDefinition::new_with_defaults(
                NamespaceId::new("chat"),
                TableName::new("messages"),
                TableType::User,
                columns,
                None,
            ).unwrap()
        );

        let data = CachedTableData::new(td);

        cache.insert(table_id, Arc::new(data));
        cache
    }

    /// Phase 3C: Create UserTableShared for tests
    fn create_test_user_table_shared() -> Arc<UserTableShared> {
        let store = create_test_db();
        let schema = create_test_schema();
        let unified_cache = create_test_metadata();
        let table_id = create_test_table_id();
        let app_context = AppContext::get();
        
        UserTableShared::new(table_id, unified_cache, schema, store, app_context)
    }

    #[test]
    fn test_user_table_provider_creation() {
        let shared = create_test_user_table_shared();
        let schema = shared.core().schema_ref().clone();

        let provider = UserTableProvider::new(shared);

        assert_eq!(provider.schema(), schema);
        assert_eq!(provider.namespace_id(), &NamespaceId::new("chat"));
        assert_eq!(provider.table_name(), &TableName::new("messages"));
        // Use BaseTableProvider::table_type to disambiguate
        assert_eq!(
            crate::tables::base_table_provider::BaseTableProvider::table_type(&provider),
            TableType::User
        );
        assert_eq!(provider.column_family_name(), "user_table:chat:messages");
    }

    #[test]
    fn test_user_key_prefix() {
        let user_id = UserId::new("user123".to_string());

        let prefix = UserTableProvider::user_key_prefix(&user_id);
        let expected = b"user123:".to_vec();

        assert_eq!(prefix, expected);
    }

    #[tokio::test]
    async fn test_scan_flattens_user_rows() {
        let shared = create_test_user_table_shared();
        let user_id = UserId::new("user123".to_string());

        let provider = UserTableProvider::new(shared.clone());

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
            provider.shared.store().as_ref(),
            provider.namespace_id().as_str(),
            provider.table_name().as_str(),
            user_id.as_str(),
            &row.row_id,
            &row,
        )
        .expect("should persist user row");

        // Create a session with user context for the scan
        let ctx = SessionContext::new();
        let mut session_state = ctx.state();
        session_state.config_mut().options_mut().extensions.insert(
            crate::sql::executor::models::SessionUserContext {
                user_id: user_id.clone(),
                role: Role::User,
            }
        );
        let session_with_user = SessionContext::new_with_state(session_state);
        
        let exec_plan = provider
            .scan(&session_with_user.state(), None, &[], None)
            .await
            .expect("scan should succeed");

        let batches = datafusion::physical_plan::collect(exec_plan, session_with_user.task_ctx())
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
        let user_id = UserId::new("user123".to_string());
        let provider = UserTableProvider::new(create_test_user_table_shared());

        let row_data = json!({
            "content": "Hello, World!"
        });

        let result = provider.insert_row(&user_id, row_data);
        assert!(result.is_ok(), "Insert should succeed");

        let row_id = result.unwrap();
        assert!(!row_id.is_empty(), "Row ID should be generated");
    }

    #[test]
    fn test_insert_batch() {
        let user_id = UserId::new("user123".to_string());
        let provider = UserTableProvider::new(create_test_user_table_shared());

        let rows = vec![
            json!({"content": "Message 1"}),
            json!({"content": "Message 2"}),
            json!({"content": "Message 3"}),
        ];

        let result = provider.insert_batch(&user_id, rows);
        assert!(result.is_ok(), "Batch insert should succeed");

        let row_ids = result.unwrap();
        assert_eq!(row_ids.len(), 3, "Should generate 3 row IDs");
    }

    #[tokio::test]
    async fn test_delete_row_excludes_soft_deleted_from_scan() {
        let user_id = UserId::new("user123".to_string());
        let provider = UserTableProvider::new(create_test_user_table_shared());

        let row_a = json!({"content": "Keep me"});
        let row_b = json!({"content": "Delete me"});

        provider.insert_row(&user_id, row_a).expect("insert row_a");
        let id_b = provider.insert_row(&user_id, row_b).expect("insert row_b");

        provider.delete_row(&user_id, &id_b).expect("soft delete row_b");

        // Create a session with user context for the scan
        let ctx = SessionContext::new();
        let mut session_state = ctx.state();
        session_state.config_mut().options_mut().extensions.insert(
            crate::sql::executor::models::SessionUserContext {
                user_id: user_id.clone(),
                role: Role::User,
            }
        );
        let session_with_user = SessionContext::new_with_state(session_state);
        
        let exec_plan = provider
            .scan(&session_with_user.state(), None, &[], None)
            .await
            .expect("scan should succeed");

        let batches = datafusion::physical_plan::collect(exec_plan, session_with_user.task_ctx())
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
        let user_id = UserId::new("user123".to_string());
        let provider = UserTableProvider::new(create_test_user_table_shared());

        // Insert a row first
        let row_data = json!({"content": "Original"});
        let row_id = provider.insert_row(&user_id, row_data).unwrap();

        // Update the row
        let updates = json!({"content": "Updated"});
        let result = provider.update_row(&user_id, &row_id, updates);
        assert!(result.is_ok(), "Update should succeed");
        assert_eq!(result.unwrap(), row_id, "Should return the updated row ID");
    }

    #[test]
    fn test_delete_row() {
        let user_id = UserId::new("user123".to_string());
        let provider = UserTableProvider::new(create_test_user_table_shared());

        // Insert a row first
        let row_data = json!({"content": "To be deleted"});
        let row_id = provider.insert_row(&user_id, row_data).unwrap();

        // Delete the row (soft delete)
        let result = provider.delete_row(&user_id, &row_id);
        assert!(result.is_ok(), "Delete should succeed");
        assert_eq!(result.unwrap(), row_id, "Should return the deleted row ID");
    }

    #[test]
    fn test_data_isolation_different_users() {
        let shared = create_test_user_table_shared();

        let user1_id = UserId::new("user1".to_string());
        let user2_id = UserId::new("user2".to_string());

        let provider1 = UserTableProvider::new(shared.clone());
        let provider2 = UserTableProvider::new(shared);

        // Insert data for user1
        let row_data_1 = json!({"content": "User1 message"});
        let row_id_1 = provider1.insert_row(&user1_id, row_data_1).unwrap();

        // Insert data for user2
        let row_data_2 = json!({"content": "User2 message"});
        let row_id_2 = provider2.insert_row(&user2_id, row_data_2).unwrap();

        // Verify different key prefixes
        let prefix1 = UserTableProvider::user_key_prefix(&user1_id);
        let prefix2 = UserTableProvider::user_key_prefix(&user2_id);
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




