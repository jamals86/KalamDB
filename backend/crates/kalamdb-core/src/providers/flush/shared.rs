//! Shared table flush implementation
//!
//! Flushes shared table data from RocksDB to a single Parquet file.
//! All rows are written to one file per flush operation.

use super::base::{FlushExecutor, FlushJobResult, FlushMetadata, TableFlush};
use crate::error::KalamDbError;
use crate::live_query::manager::{ChangeNotification, LiveQueryManager};
use crate::manifest::{FlushManifestHelper, ManifestCacheService, ManifestService};
use crate::schema_registry::SchemaRegistry;
use crate::storage::ParquetWriter;
use kalamdb_tables::{SharedTableRow, SharedTableStore};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use kalamdb_commons::models::{TableId, UserId};
use kalamdb_commons::{NamespaceId, NodeId, TableName};
use kalamdb_store::entity_store::EntityStore;
use serde_json::Value as JsonValue;
use std::path::PathBuf;
use std::sync::Arc;

/// Shared table flush job
pub struct SharedTableFlushJob {
    store: Arc<SharedTableStore>,
    table_id: Arc<TableId>,
    namespace_id: NamespaceId,
    table_name: TableName,
    schema: SchemaRef, //TODO: needed?
    unified_cache: Arc<SchemaRegistry>, //TODO: We have AppContext now
    node_id: NodeId, //TODO: We can pass AppContext and has node_id there
    live_query_manager: Option<Arc<LiveQueryManager>>, //TODO: We can pass AppContext and has live_query_manager there
    manifest_helper: FlushManifestHelper,
}

impl SharedTableFlushJob {
    /// Create a new shared table flush job
    pub fn new(
        table_id: Arc<TableId>,
        store: Arc<SharedTableStore>,
        namespace_id: NamespaceId,
        table_name: TableName,
        schema: SchemaRef,
        unified_cache: Arc<SchemaRegistry>,
        manifest_service: Arc<ManifestService>,
        manifest_cache: Arc<ManifestCacheService>,
    ) -> Self {
        let node_id = NodeId::from(format!("node-{}", std::process::id()));
        let manifest_helper = FlushManifestHelper::new(manifest_service, manifest_cache);
        Self {
            store,
            table_id,
            namespace_id,
            table_name,
            schema,
            unified_cache,
            node_id,
            live_query_manager: None,
            manifest_helper,
        }
    }

    /// Set the live query manager for this flush job (builder pattern)
    pub fn with_live_query_manager(mut self, manager: Arc<LiveQueryManager>) -> Self {
        self.live_query_manager = Some(manager);
        self
    }

    /// Execute flush job with tracking
    pub fn execute_tracked(&self) -> Result<FlushJobResult, KalamDbError> {
        FlushExecutor::execute_with_tracking(self, self.namespace_id.clone())
    }

    /// Generate batch filename using manifest max_batch (T115)
    /// Returns (batch_number, filename)
    fn generate_batch_filename(&self) -> Result<(u64, String), KalamDbError> {
        let batch_number = self.manifest_helper.get_next_batch_number(&self.namespace_id, &self.table_name, None)?;
        let filename = FlushManifestHelper::generate_batch_filename(batch_number);
        log::debug!("[MANIFEST] Generated batch filename: {} (batch_number={})", filename, batch_number);
        Ok((batch_number, filename))
    }

    /// Convert JSON rows to Arrow RecordBatch
    fn rows_to_record_batch(&self, rows: &[(Vec<u8>, JsonValue)]) -> Result<RecordBatch, KalamDbError> {
        let mut builder = super::util::JsonBatchBuilder::new(self.schema.clone())?;
        for (key_bytes, row) in rows {
            // Ensure system columns are present in the row per schema expectations
            // Parse SeqId from key bytes for _seq if missing
            let mut patched = row.clone();
            if let Some(obj) = patched.as_object_mut() {
                if !obj.contains_key("_seq") || obj.get("_seq").map(|v| v.is_null()).unwrap_or(true) {
                    if let Ok(seq_id) = kalamdb_commons::ids::SeqId::from_bytes(key_bytes) {
                        obj.insert("_seq".to_string(), serde_json::json!(seq_id.as_i64()));
                    }
                }
                if !obj.contains_key("_deleted") || obj.get("_deleted").map(|v| v.is_null()).unwrap_or(true) {
                    obj.insert("_deleted".to_string(), serde_json::json!(false));
                }
            }
            builder.push_object_row(&patched)?;
        }
        builder.finish()
    }

    /// Delete flushed rows from RocksDB after successful Parquet write
    fn delete_flushed_rows(&self, rows: &[(Vec<u8>, JsonValue)]) -> Result<(), KalamDbError> {
        let mut parsed_keys = Vec::new();
        for (key_bytes, _) in rows {
            let key = kalamdb_commons::ids::SharedTableRowId::from_bytes(key_bytes)
                .map_err(|e| KalamDbError::InvalidOperation(format!("Invalid key bytes: {}", e)))?;
            parsed_keys.push(key);
        }

        if parsed_keys.is_empty() {
            return Ok(());
        }

        // Delete each key individually (no batch_delete in EntityStore trait)
        for key in &parsed_keys {
            EntityStore::delete(self.store.as_ref(), key)
                .map_err(|e| KalamDbError::Other(format!("Failed to delete flushed row: {}", e)))?;
        }

        log::debug!("Deleted {} flushed rows from storage", parsed_keys.len());
        Ok(())
    }
}

impl TableFlush for SharedTableFlushJob {
    fn execute(&self) -> Result<FlushJobResult, KalamDbError> {
        log::debug!("🔄 Starting shared table flush: table={}.{}", 
                   self.namespace_id.as_str(), self.table_name.as_str());

        // Scan all rows (EntityStore::scan_all returns Vec<(Vec<u8>, V)>)
        let entries = EntityStore::scan_all(self.store.as_ref())
            .map_err(|e| {
                log::error!("❌ Failed to scan rows for shared table={}.{}: {}", 
                           self.namespace_id.as_str(), self.table_name.as_str(), e);
                KalamDbError::Other(format!("Failed to scan rows: {}", e))
            })?;
        
        let rows_before_dedup = entries.len();
        log::info!("📊 [FLUSH DEDUP] Scanned {} total rows from hot storage (table={}.{})",
                   rows_before_dedup, self.namespace_id.as_str(), self.table_name.as_str());

        // STEP 1: Deduplicate using MAX(_seq) per PK (version resolution)
        use std::collections::HashMap;
        
        // Get primary key field name from schema
        let pk_field = self.schema.fields()
            .iter()
            .find(|f| !f.name().starts_with('_'))
            .map(|f| f.name().clone())
            .unwrap_or_else(|| "id".to_string());
        
        log::debug!("📊 [FLUSH DEDUP] Using primary key field: {}", pk_field);
        
        // Map: pk_value -> (key_bytes, row, _seq)
        let mut latest_versions: HashMap<String, (Vec<u8>, SharedTableRow, i64)> = HashMap::new();
        let mut deleted_count = 0;
        
        for (key_bytes, row) in entries {
            // Extract PK value from fields
            let pk_value = match row.fields.get(&pk_field) {
                Some(v) if !v.is_null() => v.to_string(),
                _ => {
                    // No PK or null PK - use unique _seq as fallback
                    format!("_seq:{}", row._seq.as_i64())
                }
            };
            
            let seq_val = row._seq.as_i64();
            
            // Track deleted rows
            if row._deleted {
                deleted_count += 1;
            }
            
            // Keep MAX(_seq) per pk_value
            match latest_versions.get(&pk_value) {
                Some((_existing_key, _existing_row, existing_seq)) => {
                    if seq_val > *existing_seq {
                        log::trace!("[FLUSH DEDUP] Replacing pk={}: old_seq={}, new_seq={}, deleted={}",
                                   pk_value, existing_seq, seq_val, row._deleted);
                        latest_versions.insert(pk_value, (key_bytes, row, seq_val));
                    } else {
                        log::trace!("[FLUSH DEDUP] Keeping existing pk={}: existing_seq={} >= new_seq={}",
                                   pk_value, existing_seq, seq_val);
                    }
                }
                None => {
                    log::trace!("[FLUSH DEDUP] First version pk={}: _seq={}, deleted={}",
                               pk_value, seq_val, row._deleted);
                    latest_versions.insert(pk_value, (key_bytes, row, seq_val));
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
        
        // STEP 2: Filter out deleted rows (tombstones) and convert to JSON
        let mut rows: Vec<(Vec<u8>, JsonValue)> = Vec::new();
        let mut tombstones_filtered = 0;

        for (_pk_value, (key_bytes, row, _seq)) in latest_versions {
            // Skip soft-deleted rows (tombstones)
            if row._deleted {
                tombstones_filtered += 1;
                continue;
            }

            // Build JSON object with metadata fields
            let mut json_obj = row.fields.clone();
            if let Some(obj) = json_obj.as_object_mut() {
                obj.insert("_seq".to_string(), JsonValue::Number(row._seq.as_i64().into()));
                obj.insert("_deleted".to_string(), JsonValue::Bool(false));
            }

            rows.push((key_bytes, json_obj));
        }

        log::info!("📊 [FLUSH DEDUP] Final: {} rows to flush ({} tombstones filtered)",
                   rows.len(), tombstones_filtered);

        // If no rows to flush, return early
        if rows.is_empty() {
            log::info!("⚠️  No rows to flush for shared table={}.{} (empty table or all deleted)",
                      self.namespace_id.as_str(), self.table_name.as_str());
            return Ok(FlushJobResult {
                job_record: self.create_job_record(&self.generate_job_id(), self.namespace_id.clone()),
                rows_flushed: 0,
                parquet_files: vec![],
                metadata: FlushMetadata::shared_table(),
            });
        }

        let rows_count = rows.len();
        log::debug!("💾 Flushing {} rows to Parquet for shared table={}.{}",
                   rows_count, self.namespace_id.as_str(), self.table_name.as_str());

        // Convert rows to RecordBatch
        let batch = self.rows_to_record_batch(&rows)?;

        // T114-T115: Generate batch filename using manifest (sequential numbering)
        let (batch_number, batch_filename) = self.generate_batch_filename()?;
        let full_path = self.unified_cache.get_storage_path(&*self.table_id, None, None)?;
        let table_dir = PathBuf::from(full_path);
        let output_path = table_dir.join(&batch_filename);

        // Ensure directory exists
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| KalamDbError::Other(format!("Failed to create directory: {}", e)))?;
        }

        // Write to Parquet
        log::debug!("📝 Writing Parquet file: path={}, rows={}", output_path.display(), rows_count);
        let writer = ParquetWriter::new(output_path.to_str().unwrap());
        writer.write(self.schema.clone(), vec![batch.clone()])?;

        log::info!("✅ Flushed {} rows for shared table={}.{} to {}", 
                  rows_count, self.namespace_id.as_str(), self.table_name.as_str(), output_path.display());

        // Get file size
        let size_bytes = std::fs::metadata(&output_path)
            .map(|m| m.len())
            .unwrap_or(0);

        // Update manifest and cache using helper
        self.manifest_helper.update_manifest_after_flush(
            &self.namespace_id,
            &self.table_name,
            None,
            batch_number,
            batch_filename.clone(),
            &batch,
            size_bytes,
        )?;

        // Delete flushed rows from RocksDB
        log::debug!("🗑️  Deleting {} flushed rows from RocksDB (table={}.{})",
                   rows_count, self.namespace_id.as_str(), self.table_name.as_str());
        self.delete_flushed_rows(&rows)?;

        let parquet_path = output_path.to_string_lossy().to_string();

        // Send flush notification if LiveQueryManager configured
        if let Some(manager) = &self.live_query_manager {
            let table_name = format!("{}.{}", self.namespace_id.as_str(), self.table_name.as_str());
            let parquet_files = vec![parquet_path.clone()];
            let table_id = TableId::new(self.namespace_id.clone(), self.table_name.clone());
            let notification = ChangeNotification::flush(table_id.clone(), rows_count, parquet_files.clone());
            let system_user = UserId::system();
            manager.notify_table_change_async(system_user, table_id, notification);
        }

        Ok(FlushJobResult {
            job_record: self.create_job_record(&self.generate_job_id(), self.namespace_id.clone()),
            rows_flushed: rows_count,
            parquet_files: vec![parquet_path],
            metadata: FlushMetadata::shared_table(),
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
