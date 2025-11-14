//! User table provider implementation with RLS
//!
//! This module provides UserTableProvider implementing BaseTableProvider<UserTableRowId, UserTableRow>
//! with Row-Level Security (RLS) enforced via user_id parameter.
//!
//! **Key Features**:
//! - Direct fields (no UserTableShared wrapper)
//! - Shared core via Arc<TableProviderCore>
//! - No handlers - all DML logic inline
//! - RLS via user_id parameter in DML methods
//! - SessionState extraction for scan_rows()

use super::base::{BaseTableProvider, TableProviderCore};
use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::schema_registry::TableType;
use kalamdb_tables::{UserTableRow, UserTableStore};
use async_trait::async_trait;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::catalog::Session;
use datafusion::datasource::TableProvider;
use datafusion::datasource::memory::MemTable;
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use datafusion::parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use kalamdb_commons::ids::UserTableRowId;
use kalamdb_commons::models::UserId;
use kalamdb_commons::{Role, TableId};
use kalamdb_store::entity_store::EntityStore;
use serde_json::Value as JsonValue;
use std::any::Any;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

// Arrow <-> JSON helpers
use crate::providers::arrow_json_conversion::json_rows_to_arrow_batch;
use serde_json::json;
use crate::live_query::manager::ChangeNotification;

/// User table provider with RLS
///
/// **Architecture**:
/// - Stateless provider (user context passed per-operation)
/// - Direct fields (no wrapper layer)
/// - Shared core via Arc<TableProviderCore>
/// - RLS enforced via user_id parameter
pub struct UserTableProvider {
    /// Shared core (app_context, live_query_manager, storage_registry)
    core: Arc<TableProviderCore>,
    
    /// Table identifier
    table_id: Arc<TableId>,
    
    /// Logical table type
    table_type: TableType,
    
    /// UserTableStore for DML operations (public for flush jobs)
    pub(crate) store: Arc<UserTableStore>,
    
    /// Cached primary key field name
    primary_key_field_name: String,
}

impl UserTableProvider {
    /// Create a new user table provider
    ///
    /// # Arguments
    /// * `core` - Shared core with app_context and optional services
    /// * `table_id` - Table identifier
    /// * `store` - UserTableStore for this table
    /// * `primary_key_field_name` - Primary key field name from schema
    pub fn new(
        core: Arc<TableProviderCore>,
        table_id: TableId,
        store: Arc<UserTableStore>,
        primary_key_field_name: String,
    ) -> Self {
        Self {
            core,
            table_id: Arc::new(table_id),
            table_type: TableType::User,
            store,
            primary_key_field_name,
        }
    }
    
    /// Get the primary key field name
    pub fn primary_key_field_name(&self) -> &str {
        &self.primary_key_field_name
    }
    
    /// Check if a primary key value already exists (non-deleted version)
    ///
    /// **Performance**: O(log n) lookup by PK value using version resolution.
    /// This is MUCH faster than O(n) full table scan.
    ///
    /// # Arguments
    /// * `user_id` - User ID to scope the search (user tables are user-scoped)
    /// * `pk_value` - String representation of PK value to check
    ///
    /// # Returns
    /// * `Ok(true)` - PK value exists with non-deleted version
    /// * `Ok(false)` - PK value does not exist or only deleted versions exist
    /// * `Err` - Storage error
    fn pk_value_exists(&self, user_id: &UserId, pk_value: &str) -> Result<bool, KalamDbError> {
        let pk_name = self.primary_key_field_name();
        
        // Scan with version resolution (returns only latest non-deleted versions)
        let resolved = self.scan_with_version_resolution_to_kvs(user_id, None)?;
        
        // Check if any resolved row has this PK value
        for (_key, row) in resolved.into_iter() {
            if let Some(val) = row.fields.get(pk_name) {
                // Compare as strings to handle different JSON types
                let row_pk_str = match val {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    _ => continue,
                };
                
                if row_pk_str == pk_value {
                    return Ok(true); // PK value already exists
                }
            }
        }
        
        Ok(false) // PK value does not exist
    }
    
    /// Extract user context from DataFusion SessionState
    ///
    /// **Purpose**: Read (user_id, role) from SessionState.config.options.extensions
    /// injected by ExecutionContext.create_session_with_user()
    ///
    /// **Returns**: (UserId, Role) tuple for RLS enforcement
    fn extract_user_context(state: &dyn Session) -> Result<(UserId, Role), KalamDbError> {
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
    
    /// Scan Parquet files from cold storage for a specific user
    ///
    /// Lists all *.parquet files in the user's storage directory and merges them into a single RecordBatch.
    /// Returns an empty batch if no Parquet files exist.
    fn scan_parquet_files_as_batch(&self, user_id: &UserId) -> Result<RecordBatch, KalamDbError> {
        // Resolve storage path for user
        let storage_path = self.core.app_context.schema_registry()
            .get_storage_path(&*self.table_id, Some(user_id), None)?;
        
        let storage_dir = PathBuf::from(&storage_path);
        
        // If directory doesn't exist, return empty batch
        if !storage_dir.exists() {
            log::debug!("No Parquet directory found for user {}: {}", user_id.as_str(), storage_path);
            let schema = self.schema_ref();
            return Ok(RecordBatch::new_empty(schema));
        }
        
        // List all .parquet files in directory
        let entries = fs::read_dir(&storage_dir)
            .map_err(|e| KalamDbError::Io(e))?;
        
        let mut parquet_files = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| KalamDbError::Io(e))?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("parquet") {
                parquet_files.push(path);
            }
        }
        
        if parquet_files.is_empty() {
            log::debug!("No Parquet files found for user {} in {}", user_id.as_str(), storage_path);
            let schema = self.schema_ref();
            return Ok(RecordBatch::new_empty(schema));
        }
        
        let parquet_count = parquet_files.len();
        log::debug!("Found {} Parquet files for user {} in {}", parquet_count, user_id.as_str(), storage_path);
        
        // Read all Parquet files and merge batches
        let mut all_batches = Vec::new();
        let schema = self.schema_ref();
        
        for parquet_file in parquet_files {
            log::debug!("Reading Parquet file: {}", parquet_file.display());
            
            let file = fs::File::open(&parquet_file)
                .map_err(|e| KalamDbError::Io(e))?;
            
            let builder = ParquetRecordBatchReaderBuilder::try_new(file)
                .map_err(|e| KalamDbError::Other(format!("Failed to create Parquet reader: {}", e)))?;
            
            let mut reader = builder.build()
                .map_err(|e| KalamDbError::Other(format!("Failed to build Parquet reader: {}", e)))?;
            
            // Read all batches from this file
            while let Some(batch_result) = reader.next() {
                let batch = batch_result
                    .map_err(|e| KalamDbError::Other(format!("Failed to read Parquet batch: {}", e)))?;
                all_batches.push(batch);
            }
        }
        
        if all_batches.is_empty() {
            log::debug!("All Parquet files were empty for user {}", user_id.as_str());
            return Ok(RecordBatch::new_empty(schema));
        }
        
        // Concatenate all batches
        let combined = datafusion::arrow::compute::concat_batches(&schema, &all_batches)
            .map_err(|e| KalamDbError::Other(format!("Failed to concatenate Parquet batches: {}", e)))?;
        
        log::debug!("Loaded {} total rows from {} Parquet files for user {}", 
                   combined.num_rows(), parquet_count, user_id.as_str());
        
        Ok(combined)
    }
}

impl BaseTableProvider<UserTableRowId, UserTableRow> for UserTableProvider {
    fn table_id(&self) -> &TableId {
        &self.table_id
    }
    
    fn schema_ref(&self) -> SchemaRef {
        // Get memoized Arrow schema from SchemaRegistry via AppContext
        self.core.app_context
            .schema_registry()
            .get_arrow_schema(&self.table_id)
            .expect("Failed to get Arrow schema from registry")
    }
    
    fn provider_table_type(&self) -> TableType { self.table_type }
    
    fn app_context(&self) -> &Arc<AppContext> {
        &self.core.app_context
    }
    
    fn primary_key_field_name(&self) -> &str {
        &self.primary_key_field_name
    }
    
    fn insert(&self, user_id: &UserId, row_data: JsonValue) -> Result<UserTableRowId, KalamDbError> {
        // T060: Validate PRIMARY KEY uniqueness if user provided PK value
        // This is O(log n) lookup by PK value, not O(n) table scan!
        let pk_name = self.primary_key_field_name();
        
        if let Some(pk_value) = row_data.get(pk_name) {
            // User provided PK value - check if it already exists (non-deleted version)
            if !pk_value.is_null() {
                let pk_str = crate::providers::unified_dml::extract_user_pk_value(&row_data, pk_name)?;
                
                // Check if this PK value already exists with a non-deleted version
                if self.pk_value_exists(user_id, &pk_str)? {
                    return Err(KalamDbError::AlreadyExists(format!(
                        "Primary key violation: value '{}' already exists in column '{}'",
                        pk_str, pk_name
                    )));
                }
            }
        }
        // If PK not provided, it must have a DEFAULT value (validated at CREATE TABLE time)
        
        // Generate new SeqId via SystemColumnsService
        let sys_cols = self.core.system_columns.clone();
        let seq_id = sys_cols.generate_seq_id()?;
        
        // Create UserTableRow directly
        let entity = UserTableRow {
            user_id: user_id.clone(),
            _seq: seq_id,
            _deleted: false,
            fields: row_data,
        };
        
        // Create composite key
        let row_key = UserTableRowId::new(user_id.clone(), seq_id);
        
        // Store the entity in RocksDB (hot storage)
        self.store.put(&row_key, &entity).map_err(|e| {
            KalamDbError::InvalidOperation(format!(
                "Failed to insert user table row: {}",
                e
            ))
        })?;
        
        log::debug!(
            "Inserted user table row for user {} with _seq {}",
            user_id.as_str(),
            seq_id
        );

        // Fire live query notification (INSERT)
        if let Some(manager) = &self.core.live_query_manager {
            let table_id = (*self.table_id).clone();
            let table_name = format!(
                "{}.{}",
                table_id.namespace_id().as_str(),
                table_id.table_name().as_str()
            );

            //TODO: Should we keep the user_id in the row JSON?
            // Flatten row fields and include user_id for filter matching
            let mut obj = entity
                .fields
                .as_object()
                .cloned()
                .unwrap_or_default();
            obj.insert("user_id".to_string(), json!(user_id.as_str()));
            let row_json = JsonValue::Object(obj);

            let notification = ChangeNotification::insert(table_name, row_json);
            manager.notify_table_change_async(user_id.clone(), table_id, notification);
        }
        
        Ok(row_key)
    }
    
    fn update(&self, user_id: &UserId, key: &UserTableRowId, updates: JsonValue) -> Result<UserTableRowId, KalamDbError> {
        let update_obj = updates.as_object().cloned().ok_or_else(|| {
            KalamDbError::InvalidSql("UPDATE requires object of column assignments".to_string())
        })?;

        // Load referenced version to extract PK
        let prior = EntityStore::get(&*self.store, key)
            .map_err(|e| KalamDbError::Other(format!("Failed to load prior version: {}", e)))?
            .ok_or_else(|| KalamDbError::NotFound("Row not found for update".to_string()))?;

        let pk_name = self.primary_key_field_name().to_string();
        let pk_value = crate::providers::unified_dml::extract_user_pk_value(&prior.fields, &pk_name)?;

        // Find latest resolved row for this PK under same user
        let resolved = self.scan_with_version_resolution_to_kvs(user_id, None)?;
        let mut latest_opt: Option<(UserTableRowId, UserTableRow)> = None;
        for (k, r) in resolved.into_iter() {
            if let Some(val) = r.fields.get(&pk_name) {
                if val.to_string() == pk_value {
                    latest_opt = Some((k, r));
                    break;
                }
            }
        }
        let (_latest_key, latest_row) = latest_opt
            .ok_or_else(|| KalamDbError::NotFound(format!("Row with {}={} not found", pk_name, pk_value)))?;

        // Merge updates onto latest
        let mut merged = latest_row
            .fields
            .as_object()
            .cloned()
            .unwrap_or_default();
        for (k, v) in update_obj.into_iter() { merged.insert(k, v); }

        let new_fields = JsonValue::Object(merged);
        let sys_cols = self.core.system_columns.clone();
        let seq_id = sys_cols.generate_seq_id()?;
        let entity = UserTableRow { user_id: user_id.clone(), _seq: seq_id, _deleted: false, fields: new_fields };
        let row_key = UserTableRowId::new(user_id.clone(), seq_id);
        self.store.put(&row_key, &entity).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to update user table row: {}", e))
        })?;

        // Fire live query notification (UPDATE)
        if let Some(manager) = &self.core.live_query_manager {
            let table_id = (*self.table_id).clone();
            let table_name = format!(
                "{}.{}",
                table_id.namespace_id().as_str(),
                table_id.table_name().as_str()
            );

            //TODO: Should we keep the user_id in the row JSON?
            // Old data: latest prior resolved row
            let mut old_obj = latest_row
                .fields
                .as_object()
                .cloned()
                .unwrap_or_default();
            old_obj.insert("user_id".to_string(), json!(user_id.as_str()));
            let old_json = JsonValue::Object(old_obj);

            // New data: merged entity
            let mut new_obj = entity
                .fields
                .as_object()
                .cloned()
                .unwrap_or_default();
            new_obj.insert("user_id".to_string(), json!(user_id.as_str()));
            let new_json = JsonValue::Object(new_obj);

            let notification = ChangeNotification::update(table_name, old_json, new_json);
            manager.notify_table_change_async(user_id.clone(), table_id, notification);
        }
        Ok(row_key)
    }
    
    fn delete(&self, user_id: &UserId, key: &UserTableRowId) -> Result<(), KalamDbError> {
        // Load referenced version to extract PK (for validation; we append tombstone regardless)
        let prior = EntityStore::get(&*self.store, key)
            .map_err(|e| KalamDbError::Other(format!("Failed to load prior version: {}", e)))?
            .ok_or_else(|| KalamDbError::NotFound("Row not found for delete".to_string()))?;
        let pk_name = self.primary_key_field_name().to_string();
        let _pk_value = crate::providers::unified_dml::extract_user_pk_value(&prior.fields, &pk_name)?;

        let sys_cols = self.core.system_columns.clone();
        let seq_id = sys_cols.generate_seq_id()?;
        // Preserve the primary key value in the tombstone so version resolution groups
        // the tombstone with the same logical row and suppresses older versions.
        let pk_json_val = prior
            .fields
            .get(&pk_name)
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let entity = UserTableRow {
            user_id: user_id.clone(),
            _seq: seq_id,
            _deleted: true,
            fields: serde_json::json!({ pk_name.clone(): pk_json_val.clone() }),
        };
        let row_key = UserTableRowId::new(user_id.clone(), seq_id);
        log::info!(
            "[UserProvider DELETE] Writing tombstone: user={}, _seq={}, PK={}:{}",
            user_id.as_str(),
            seq_id.as_i64(),
            pk_name,
            pk_json_val
        );
        self.store.put(&row_key, &entity).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to delete user table row: {}", e))
        })?;

        // Fire live query notification (DELETE soft)
        if let Some(manager) = &self.core.live_query_manager {
            let table_id = (*self.table_id).clone();
            let table_name = format!(
                "{}.{}",
                table_id.namespace_id().as_str(),
                table_id.table_name().as_str()
            );

                        //TODO: Should we keep the user_id in the row JSON?
            // Provide prior fields for filter matching, include user_id
            let mut obj = prior
                .fields
                .as_object()
                .cloned()
                .unwrap_or_default();
            obj.insert("user_id".to_string(), json!(user_id.as_str()));
            let row_json = JsonValue::Object(obj);

            let notification = ChangeNotification::delete_soft(table_name, row_json);
            manager.notify_table_change_async(user_id.clone(), table_id, notification);
        }
        Ok(())
    }
    
    fn scan_rows(&self, state: &dyn Session, filter: Option<&Expr>) -> Result<RecordBatch, KalamDbError> {
        // Extract user_id from SessionState for RLS
        let (user_id, _role) = Self::extract_user_context(state)?;
        
        // Perform KV scan with version resolution
        let kvs = self.scan_with_version_resolution_to_kvs(&user_id, filter)?;

        // Convert rows to JSON values aligned with current schema
        let schema = self.schema_ref();
        let mut rows: Vec<JsonValue> = Vec::with_capacity(kvs.len());
        let has_seq = schema.field_with_name("_seq").is_ok();
        let has_deleted = schema.field_with_name("_deleted").is_ok();

        for (_key, row) in kvs.into_iter() {
            // Base fields from stored row
            let mut obj = row
                .fields
                .as_object()
                .cloned()
                .unwrap_or_default();

            if has_seq {
                obj.insert("_seq".to_string(), json!(row._seq.as_i64()));
            }
            if has_deleted {
                obj.insert("_deleted".to_string(), json!(row._deleted));
            }

            rows.push(JsonValue::Object(obj));
        }

        // Build record batch from JSON rows
        let batch = json_rows_to_arrow_batch(&schema, rows)
            .map_err(|e| KalamDbError::InvalidOperation(format!("Failed to build Arrow batch: {}", e)))?;

        Ok(batch)
    }
    
    fn scan_with_version_resolution_to_kvs(
        &self,
    user_id: &UserId,
    _filter: Option<&Expr>,
    ) -> Result<Vec<(UserTableRowId, UserTableRow)>, KalamDbError> {
        // 1) Scan hot storage (RocksDB) with user_id prefix filter
        // Build prefix bytes: [len:1][user_id]
        let user_bytes = user_id.as_str().as_bytes();
        let len = (user_bytes.len().min(255)) as u8;
        let mut prefix = Vec::with_capacity(1 + len as usize);
        prefix.push(len);
        prefix.extend_from_slice(&user_bytes[..len as usize]);

        // Prefer full partition scan then filter by user_id to avoid prefix-scan edge cases
        let raw_all = self
            .store
            .scan_all()
            .map_err(|e| KalamDbError::InvalidOperation(format!(
                "Failed to scan user table hot storage: {}",
                e
            )))?;
        let raw: Vec<(Vec<u8>, UserTableRow)> = raw_all
            .into_iter()
            .filter(|(_k, r)| &r.user_id == user_id)
            .collect();
        log::info!(
            "[UserProvider] Hot scan (filtered): {} rows for user {} (table={}.{})",
            raw.len(),
            user_id.as_str(),
            self.table_id.namespace_id().as_str(),
            self.table_id.table_name().as_str()
        );

        // 2) Scan cold storage (Parquet files)
        let parquet_batch = self.scan_parquet_files_as_batch(user_id)?;

        // 3) Version resolution: keep MAX(_seq) per primary key; honor tombstones
        use std::collections::HashMap;
        let pk_name = self.primary_key_field_name().to_string();
        let mut best: HashMap<String, (UserTableRowId, UserTableRow)> = HashMap::new();

        // Process RocksDB rows
        for (key_bytes, row) in raw.into_iter() {
            // Parse typed key from bytes
            let maybe_key = kalamdb_commons::ids::UserTableRowId::from_bytes(&key_bytes);
            let key = match maybe_key {
                Ok(k) => k,
                Err(err) => {
                    log::warn!("Skipping invalid UserTableRowId key bytes: {}", err);
                    continue;
                }
            };

            // Determine grouping key: prefer declared PK when present and non-null; else use unique _seq
            let pk_key = match row.fields.get(&pk_name).filter(|v| !v.is_null()) {
                Some(v) => {
                    let key_str = v.to_string();
                    log::debug!(
                        "[UserProvider] Hot row _seq={}: PK({})={}, _deleted={}",
                        row._seq.as_i64(),
                        pk_name,
                        key_str,
                        row._deleted
                    );
                    key_str
                }
                None => {
                    // Missing or null PK in row: group by unique _seq to avoid collapsing
                    let fallback = format!("_seq:{}", row._seq.as_i64());
                    log::debug!(
                        "[UserProvider] Hot row _seq={}: PK({}) missing/null, using fallback={}, _deleted={}",
                        row._seq.as_i64(),
                        pk_name,
                        fallback,
                        row._deleted
                    );
                    fallback
                }
            };

            match best.get(&pk_key) {
                Some((_existing_key, existing_row)) => {
                    if row._seq > existing_row._seq {
                        log::debug!(
                            "[UserProvider] Replacing winner for PK '{}': old_seq={}, new_seq={}, new_deleted={}",
                            pk_key,
                            existing_row._seq.as_i64(),
                            row._seq.as_i64(),
                            row._deleted
                        );
                        best.insert(pk_key, (key, row));
                    } else {
                        log::debug!(
                            "[UserProvider] Keeping existing winner for PK '{}': existing_seq={} >= new_seq={}",
                            pk_key,
                            existing_row._seq.as_i64(),
                            row._seq.as_i64()
                        );
                    }
                }
                None => {
                    log::debug!(
                        "[UserProvider] First entry for PK '{}': _seq={}, _deleted={}",
                        pk_key,
                        row._seq.as_i64(),
                        row._deleted
                    );
                    best.insert(pk_key, (key, row));
                }
            }
        }

        log::info!(
            "[UserProvider] After hot version-resolution: {} rows (table={}.{})",
            best.len(),
            self.table_id.namespace_id().as_str(),
            self.table_id.table_name().as_str()
        );

        // Process Parquet rows (cold storage)
        log::info!(
            "[UserProvider] Cold scan: {} Parquet rows (table={}.{}; user={})",
            parquet_batch.num_rows(),
            self.table_id.namespace_id().as_str(),
            self.table_id.table_name().as_str(),
            user_id.as_str()
        );
        
        // Convert RecordBatch to rows and merge with RocksDB data
        if parquet_batch.num_rows() > 0 {
            use crate::providers::arrow_json_conversion::arrow_value_to_json;
            use datafusion::arrow::array::{Array, BooleanArray, Int64Array};
            use serde_json::Map;
            
            // Extract column arrays
            let schema = parquet_batch.schema();
            
            // Find _seq column index
            let seq_idx = schema.fields().iter().position(|f| f.name() == "_seq")
                .ok_or_else(|| KalamDbError::Other("Missing _seq column in Parquet batch".to_string()))?;
            
            // Find _deleted column index (optional)
            let deleted_idx = schema.fields().iter().position(|f| f.name() == "_deleted");
            
            // Get _seq array
            let seq_array = parquet_batch.column(seq_idx)
                .as_any()
                .downcast_ref::<Int64Array>()
                .ok_or_else(|| KalamDbError::Other("_seq column is not Int64Array".to_string()))?;
            
            // Get _deleted array if exists
            let deleted_array = deleted_idx.and_then(|idx| {
                parquet_batch.column(idx)
                    .as_any()
                    .downcast_ref::<BooleanArray>()
            });
            
            // Convert each row to UserTableRow
            for row_idx in 0..parquet_batch.num_rows() {
                let seq_val = seq_array.value(row_idx);
                let seq_id = kalamdb_commons::ids::SeqId::from_i64(seq_val);
                
                let deleted = deleted_array
                    .and_then(|arr| if !arr.is_null(row_idx) { Some(arr.value(row_idx)) } else { None })
                    .unwrap_or(false);
                
                // Build JSON object from all columns
                let mut json_row = Map::new();
                for (col_idx, field) in schema.fields().iter().enumerate() {
                    let col_name = field.name();
                    // Skip system columns (_seq, _deleted) - they're stored separately
                    if col_name == "_seq" || col_name == "_deleted" {
                        continue;
                    }
                    
                    let array = parquet_batch.column(col_idx);
                    match arrow_value_to_json(array.as_ref(), row_idx) {
                        Ok(val) => {
                            json_row.insert(col_name.clone(), val);
                        }
                        Err(e) => {
                            log::warn!("Failed to convert column {} for row {}: {}", col_name, row_idx, e);
                        }
                    }
                }
                
                // Determine grouping key: declared PK value (non-null) or fallback to unique _seq
                let pk_key = match json_row.get(&pk_name).filter(|v| !v.is_null()) {
                    Some(v) => v.to_string(),
                    None => format!("_seq:{}", seq_val),
                };
                
                // Create UserTableRow
                let row = UserTableRow {
                    user_id: user_id.clone(),
                    _seq: seq_id,
                    _deleted: deleted,
                    fields: JsonValue::Object(json_row),
                };
                
                let key = UserTableRowId::new(user_id.clone(), seq_id);
                
                // Merge with existing data using MAX(_seq)
                match best.get(&pk_key) {
                    Some((_existing_key, existing_row)) => {
                        if row._seq > existing_row._seq {
                            best.insert(pk_key, (key, row));
                        }
                    }
                    None => {
                        best.insert(pk_key, (key, row));
                    }
                }
            }
        }

        // Filter winners where latest version is not deleted
        let pre_filter_count = best.len();
        let deleted_count = best.values().filter(|(_k, r)| r._deleted).count();
        log::info!(
            "[UserProvider] Pre-tombstone filter: {} total winners, {} deleted",
            pre_filter_count,
            deleted_count
        );
        
        let result: Vec<(UserTableRowId, UserTableRow)> = best
            .into_values()
            .filter(|(_k, r)| !r._deleted)
            .collect();
        log::info!(
            "[UserProvider] Final version-resolved (post-tombstone): {} rows (table={}.{}; user={})",
            result.len(),
            self.table_id.namespace_id().as_str(),
            self.table_id.table_name().as_str(),
            user_id.as_str()
        );
        Ok(result)
    }
    
    fn extract_fields(row: &UserTableRow) -> Option<&JsonValue> {
        Some(&row.fields)
    }
}

// Manual Debug to satisfy DataFusion's TableProvider: Debug bound
impl std::fmt::Debug for UserTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserTableProvider")
            .field("table_id", &self.table_id)
            .field("table_type", &self.table_type)
            .field("primary_key_field_name", &self.primary_key_field_name)
            .finish()
    }
}

// Implement DataFusion TableProvider trait
#[async_trait]
impl TableProvider for UserTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }
    
    fn schema(&self) -> SchemaRef {
        self.schema_ref()
    }
    
    fn table_type(&self) -> datafusion::logical_expr::TableType {
        datafusion::logical_expr::TableType::Base
    }
    
    async fn scan(
        &self,
        state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        log::info!(
            "[UserTableProvider::scan] INVOKED for table {}.{}",
            self.table_id.namespace_id().as_str(),
            self.table_id.table_name().as_str()
        );
        // Build a single RecordBatch using our scan_rows(), then wrap in MemTable
        let batch = self
            .scan_rows(state, None)
            .map_err(|e| DataFusionError::Execution(format!("scan_rows failed: {}", e)))?;

        let mem = MemTable::try_new(self.schema_ref(), vec![vec![batch]])?;
        mem.scan(state, projection, filters, limit).await
    }
}
