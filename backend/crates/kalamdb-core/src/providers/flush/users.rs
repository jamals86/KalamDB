//! User table flush implementation
//!
//! Flushes user table data from RocksDB to Parquet files, grouping by UserId.
//! Each user's data is written to a separate Parquet file for RLS isolation.

use super::base::{FlushExecutor, FlushJobResult, FlushMetadata, TableFlush};
use crate::error::KalamDbError;
use crate::live_query::manager::{ChangeNotification, LiveQueryManager};
use crate::schema_registry::SchemaRegistry;
use crate::storage::ParquetWriter;
use kalamdb_tables::UserTableStoreExt;
use kalamdb_tables::UserTableStore;
use chrono::Utc;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use kalamdb_commons::models::{TableId, UserId};
use kalamdb_commons::{NamespaceId, NodeId, TableName};
use kalamdb_store::entity_store::EntityStore;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// User table flush job
pub struct UserTableFlushJob {
    store: Arc<UserTableStore>,
    table_id: Arc<TableId>,
    namespace_id: NamespaceId,
    table_name: TableName,
    schema: SchemaRef, //TODO: needed?
    unified_cache: Arc<SchemaRegistry>, //TODO: wE HAVE APPCONTEXT NOW
    node_id: NodeId, //TODO: We can pass AppContext and has node_id there
    live_query_manager: Option<Arc<LiveQueryManager>>, //TODO: We can pass AppContext and has live_query_manager there
}

impl UserTableFlushJob {
    /// Create a new user table flush job
    pub fn new(
        table_id: Arc<TableId>,
        store: Arc<UserTableStore>,
        namespace_id: NamespaceId,
        table_name: TableName,
        schema: SchemaRef,
        unified_cache: Arc<SchemaRegistry>,
    ) -> Self {
        let node_id = NodeId::from(format!("node-{}", std::process::id()));
        Self {
            store,
            table_id,
            namespace_id,
            table_name,
            schema,
            unified_cache,
            node_id,
            live_query_manager: None,
        }
    }

    /// Set the LiveQueryManager for flush notifications (builder pattern)
    pub fn with_live_query_manager(mut self, manager: Arc<LiveQueryManager>) -> Self {
        self.live_query_manager = Some(manager);
        self
    }

    /// Execute flush job with tracking
    pub fn execute_tracked(&self) -> Result<FlushJobResult, KalamDbError> {
        FlushExecutor::execute_with_tracking(self, self.namespace_id.clone())
    }

    /// Resolve storage path for a specific user
    fn resolve_storage_path_for_user(
        &self,
        user_id: &UserId,
    ) -> Result<String, KalamDbError> {
        Ok(self.unified_cache.get_storage_path(
            &*self.table_id,
            Some(user_id),
            None,
        )?)
    }

    /// Generate batch filename with ISO 8601 timestamp
    fn generate_batch_filename(&self) -> String {
        Utc::now().format("%Y-%m-%dT%H-%M-%S.parquet").to_string()
    }

    /// Parse user key to extract user_id and row_id
    fn parse_user_key(&self, key_str: &str) -> Result<(String, String), KalamDbError> {
        let delimiter = if key_str.contains('#') { '#' } else { ':' };
        
        let mut parts = key_str.splitn(2, delimiter);
        let user_id = parts.next()
            .ok_or_else(|| KalamDbError::Other(format!("Invalid user table key format: {}", key_str)))?
            .to_string();
        let row_id = parts.next()
            .ok_or_else(|| KalamDbError::Other(format!("Invalid user table key format: {}", key_str)))?
            .to_string();

        Ok((user_id, row_id))
    }

    /// Convert JSON rows to Arrow RecordBatch
    fn rows_to_record_batch(&self, rows: &[(Vec<u8>, JsonValue)]) -> Result<RecordBatch, KalamDbError> {
        let mut builder = super::util::JsonBatchBuilder::new(self.schema.clone())?;

        for (key_bytes, row) in rows.iter() {
            let key_str = String::from_utf8_lossy(key_bytes).to_string();
            let (_user_from_key, row_id_from_key) = self.parse_user_key(&key_str)
                .unwrap_or_else(|_| (String::new(), String::new()));
            
            let mut patched = row.clone();
            if let Some(obj) = patched.as_object_mut() {
                // Backfill missing required fields
                self.backfill_required_fields(obj, &row_id_from_key);
            }
            builder.push_object_row(&patched)?;
        }
        builder.finish()
    }

    /// Backfill required fields (id, created_at) if missing
    fn backfill_required_fields(&self, obj: &mut serde_json::Map<String, JsonValue>, row_id_from_key: &str) {
        use datafusion::arrow::datatypes::DataType;
        
        for field in self.schema.fields() {
            let name = field.name();
            let missing = !obj.contains_key(name) || obj.get(name).map(|v| v.is_null()).unwrap_or(true);
            
            if !missing || field.is_nullable() {
                continue;
            }

            match name.as_str() {
                "id" => {
                    match field.data_type() {
                        DataType::Int64 => {
                            let value = row_id_from_key.parse::<i64>()
                                .unwrap_or_else(|_| Self::generate_numeric_id());
                            obj.insert(name.to_string(), serde_json::json!(value));
                        }
                        DataType::Utf8 => {
                            let value = if !row_id_from_key.is_empty() {
                                row_id_from_key.to_string()
                            } else {
                                Self::generate_numeric_id().to_string()
                            };
                            obj.insert(name.to_string(), serde_json::json!(value));
                        }
                        _ => {}
                    }
                }
                "created_at" => {
                    let now_ms = chrono::Utc::now().timestamp_millis();
                    match field.data_type() {
                        DataType::Int64 => {
                            obj.insert(name.to_string(), serde_json::json!(now_ms));
                        }
                        DataType::Utf8 => {
                            obj.insert(name.to_string(), serde_json::json!(chrono::Utc::now().to_rfc3339()));
                        }
                        DataType::Timestamp(_, _) => {
                            obj.insert(name.to_string(), serde_json::json!(now_ms));
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }

    /// Generate numeric ID (timestamp-based)
    fn generate_numeric_id() -> i64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::{SystemTime, UNIX_EPOCH};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let c = COUNTER.fetch_add(1, Ordering::SeqCst) % 4096;
        now_ms.saturating_mul(1000).saturating_add(c) as i64
    }

    /// Flush accumulated rows for a single user to Parquet
    fn flush_user_data(
        &self,
        user_id: &str,
        rows: &[(Vec<u8>, JsonValue)],
        parquet_files: &mut Vec<String>,
    ) -> Result<usize, KalamDbError> {
        if rows.is_empty() {
            return Ok(0);
        }

        let rows_count = rows.len();
        log::debug!("💾 Flushing {} rows for user {} (table={}.{})", 
                   rows_count, user_id, self.namespace_id.as_str(), self.table_name.as_str());

        // Convert rows to RecordBatch
        let batch = self.rows_to_record_batch(rows)?;

        // Resolve storage path for this user
        let user_id_typed = UserId::new(user_id.to_string());
        let storage_path = self.resolve_storage_path_for_user(&user_id_typed)?;

        // RLS ASSERTION: Verify storage path contains user_id
        if !storage_path.contains(user_id) {
            log::error!("🚨 RLS VIOLATION: Flush storage path does NOT contain user_id! user={}, path={}", 
                       user_id, storage_path);
            return Err(KalamDbError::Other(format!(
                "RLS violation: flush path missing user_id isolation for user {}", user_id
            )));
        }

        // Generate output path
        let batch_filename = self.generate_batch_filename();
        let output_path = PathBuf::from(&storage_path).join(&batch_filename);

        // Ensure directory exists
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| KalamDbError::Other(format!("Failed to create directory: {}", e)))?;
        }

        // Write to Parquet
        log::debug!("📝 Writing Parquet file: path={}, rows={}", output_path.display(), rows_count);
        let writer = ParquetWriter::new(output_path.to_str().unwrap());
        writer.write(self.schema.clone(), vec![batch])?;

        log::info!("✅ Flushed {} rows for user {} to {}", rows_count, user_id, output_path.display());

        parquet_files.push(output_path.to_string_lossy().to_string());
        Ok(rows_count)
    }

    /// Delete flushed rows from RocksDB
    fn delete_flushed_keys(&self, keys: &[Vec<u8>]) -> Result<(), KalamDbError> {
        if keys.is_empty() {
            return Ok(());
        }

        let mut parsed_keys = Vec::new();
        for key_bytes in keys {
            let key = kalamdb_commons::ids::UserTableRowId::from_bytes(key_bytes)
                .map_err(|e| KalamDbError::InvalidOperation(format!("Invalid key bytes: {}", e)))?;
            parsed_keys.push(key);
        }

        // Delete each key individually (no batch_delete in EntityStore trait)
        for key in &parsed_keys {
            EntityStore::delete(self.store.as_ref(), key)
                .map_err(|e| KalamDbError::Other(format!("Failed to delete flushed row: {}", e)))?;
        }

        log::debug!("Deleted {} flushed rows from storage", keys.len());
        Ok(())
    }
}

impl TableFlush for UserTableFlushJob {
    fn execute(&self) -> Result<FlushJobResult, KalamDbError> {
        log::debug!("🔄 Starting user table flush: table={}.{}, partition={}", 
                   self.namespace_id.as_str(), self.table_name.as_str(), self.store.partition());

        // Scan all rows (EntityStore::scan_all returns Vec<(Vec<u8>, V)>)
        let entries = EntityStore::scan_all(self.store.as_ref())
            .map_err(|e| {
                log::error!("❌ Failed to scan table={}.{}: {}", 
                           self.namespace_id.as_str(), self.table_name.as_str(), e);
                KalamDbError::Other(format!("Failed to scan table: {}", e))
            })?;

        let rows_before_dedup = entries.len();
        log::info!("📊 [FLUSH DEDUP] Scanned {} total rows from hot storage (table={}.{})",
                   rows_before_dedup, self.namespace_id.as_str(), self.table_name.as_str());

        // STEP 1: Deduplicate using MAX(_seq) per PK (version resolution)
        // Group by (user_id, primary_key) and keep only the latest version
        use std::collections::HashMap;
        
        // Get primary key field name from schema
        let pk_field = self.schema.fields()
            .iter()
            .find(|f| !f.name().starts_with('_'))
            .map(|f| f.name().clone())
            .unwrap_or_else(|| "id".to_string());
        
        log::debug!("📊 [FLUSH DEDUP] Using primary key field: {}", pk_field);
        
        // Map: (user_id, pk_value) -> (key_bytes, row, _seq)
        let mut latest_versions: HashMap<(String, String), (Vec<u8>, kalamdb_tables::UserTableRow, i64)> = HashMap::new();
        let mut deleted_count = 0;
        
        for (key_bytes, row) in entries {
            // Parse user_id from key
            let row_id = match kalamdb_commons::ids::UserTableRowId::from_bytes(&key_bytes) {
                Ok(id) => id,
                Err(e) => {
                    log::warn!("Skipping row due to invalid key format: {}", e);
                    continue;
                }
            };
            let user_id = row_id.user_id().as_str().to_string();
            
            // Extract PK value from fields
            let pk_value = match row.fields.get(&pk_field) {
                Some(v) if !v.is_null() => v.to_string(),
                _ => {
                    // No PK or null PK - use unique _seq as fallback to avoid collapsing
                    format!("_seq:{}", row._seq.as_i64())
                }
            };
            
            let group_key = (user_id.clone(), pk_value.clone());
            let seq_val = row._seq.as_i64();
            
            // Track deleted rows
            if row._deleted {
                deleted_count += 1;
            }
            
            // Keep MAX(_seq) per (user_id, pk_value)
            match latest_versions.get(&group_key) {
                Some((_existing_key, _existing_row, existing_seq)) => {
                    if seq_val > *existing_seq {
                        log::trace!("[FLUSH DEDUP] Replacing user={}, pk={}: old_seq={}, new_seq={}, deleted={}",
                                   user_id, pk_value, existing_seq, seq_val, row._deleted);
                        latest_versions.insert(group_key, (key_bytes, row, seq_val));
                    } else {
                        log::trace!("[FLUSH DEDUP] Keeping existing user={}, pk={}: existing_seq={} >= new_seq={}",
                                   user_id, pk_value, existing_seq, seq_val);
                    }
                }
                None => {
                    log::trace!("[FLUSH DEDUP] First version user={}, pk={}: _seq={}, deleted={}",
                               user_id, pk_value, seq_val, row._deleted);
                    latest_versions.insert(group_key, (key_bytes, row, seq_val));
                }
            }
        }
        
        let rows_after_dedup = latest_versions.len();
        let dedup_ratio = if rows_before_dedup > 0 {
            (rows_before_dedup - rows_after_dedup) as f64 / rows_before_dedup as f64 * 100.0
        } else {
            0.0
        };
        
        log::info!("📊 [FLUSH DEDUP] Version resolution complete: {} rows → {} unique (dedup: {:.1}%, deleted: {})",
                   rows_before_dedup, rows_after_dedup, dedup_ratio, deleted_count);
        
        // STEP 2: Filter out deleted rows (tombstones)
        let mut rows_by_user: HashMap<String, Vec<(Vec<u8>, JsonValue)>> = HashMap::new();
        let mut tombstones_filtered = 0;
        
        for ((user_id, _pk_value), (key_bytes, row, _seq)) in latest_versions {
            // Skip soft-deleted rows (tombstones)
            if row._deleted {
                tombstones_filtered += 1;
                continue;
            }

            // Convert to JSON and inject system columns
            let mut row_data = match row.fields {
                serde_json::Value::Object(map) => JsonValue::Object(map),
                other => other,
            };
            if let Some(obj) = row_data.as_object_mut() {
                obj.insert("_seq".to_string(), JsonValue::Number(row._seq.as_i64().into()));
                obj.insert("_deleted".to_string(), JsonValue::Bool(false));
            }

            rows_by_user.entry(user_id).or_default().push((key_bytes, row_data));
        }

        let rows_to_flush = rows_by_user.values().map(|v| v.len()).sum::<usize>();
        log::info!("📊 [FLUSH DEDUP] Final: {} rows to flush ({} users, {} tombstones filtered)",
                   rows_to_flush, rows_by_user.len(), tombstones_filtered);

        // If no rows to flush, return early
        if rows_by_user.is_empty() {
            log::info!("⚠️  No rows to flush for user table={}.{} (empty table or all deleted)",
                      self.namespace_id.as_str(), self.table_name.as_str());
            return Ok(FlushJobResult {
                job_record: self.create_job_record(&self.generate_job_id(), self.namespace_id.clone()),
                rows_flushed: 0,
                parquet_files: vec![],
                metadata: FlushMetadata::user_table(0, vec![]),
            });
        }

        // Flush each user's data to separate Parquet file
        let mut parquet_files: Vec<String> = Vec::new();
        let mut total_rows_flushed = 0;
        let mut error_messages: Vec<String> = Vec::new();

        for (user_id, rows) in &rows_by_user {
            match self.flush_user_data(user_id, rows, &mut parquet_files) {
                Ok(rows_count) => {
                    let keys: Vec<Vec<u8>> = rows.iter().map(|(key, _)| key.clone()).collect();
                    if let Err(e) = self.delete_flushed_keys(&keys) {
                        log::error!("Failed to delete flushed rows for user {}: {}", user_id, e);
                        error_messages.push(format!("Failed to delete rows for user {}: {}", user_id, e));
                    } else {
                        total_rows_flushed += rows_count;
                    }
                }
                Err(e) => {
                    let error_msg = format!("Failed to flush {} rows for user {}: {}", rows.len(), user_id, e);
                    log::error!("{}. Rows kept in buffer.", error_msg);
                    error_messages.push(error_msg);
                }
            }
        }

        // If any user flush failed, treat entire job as failed
        if !error_messages.is_empty() {
            let summary = format!(
                "One or more user partitions failed to flush ({} errors). Rows flushed before failure: {}. First error: {}",
                error_messages.len(), total_rows_flushed,
                error_messages.first().cloned().unwrap_or_else(|| "unknown error".to_string())
            );
            log::error!("❌ User table flush failed: table={}.{} — {}", 
                       self.namespace_id.as_str(), self.table_name.as_str(), summary);
            return Err(KalamDbError::Other(summary));
        }

        log::info!("✅ User table flush completed: table={}.{}, rows_flushed={}, users_count={}, parquet_files={}",
                  self.namespace_id.as_str(), self.table_name.as_str(),
                  total_rows_flushed, rows_by_user.len(), parquet_files.len());

        // Send flush notification if LiveQueryManager configured
        if let Some(manager) = &self.live_query_manager {
            let table_name = format!("{}.{}", self.namespace_id.as_str(), self.table_name.as_str());
            let notification = ChangeNotification::flush(table_name.clone(), total_rows_flushed, parquet_files.clone());
            let table_id = TableId::new(self.namespace_id.clone(), self.table_name.clone());
            let system_user = UserId::system();
            manager.notify_table_change_async(system_user, table_id, notification);
        }

        Ok(FlushJobResult {
            job_record: self.create_job_record(&self.generate_job_id(), self.namespace_id.clone()),
            rows_flushed: total_rows_flushed,
            parquet_files,
            metadata: FlushMetadata::user_table(rows_by_user.len(), error_messages),
        })
    }

    fn table_identifier(&self) -> String {
        format!("{}.{}", self.namespace_id.as_str(), self.table_name.as_str())
    }

    fn live_query_manager(&self) -> Option<&Arc<LiveQueryManager>> {
        self.live_query_manager.as_ref()
    }

    fn node_id(&self) -> NodeId {
        self.node_id.clone()
    }
}
