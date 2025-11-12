//! Shared table flush implementation
//!
//! Flushes shared table data from RocksDB to a single Parquet file.
//! All rows are written to one file per flush operation.

use super::base::{FlushExecutor, FlushJobResult, FlushMetadata, TableFlush};
use crate::error::KalamDbError;
use crate::live_query::manager::{ChangeNotification, LiveQueryManager};
use crate::schema_registry::SchemaRegistry;
use crate::storage::ParquetWriter;
use crate::tables::shared_tables::shared_table_store::SharedTableRow;
use crate::tables::system::system_table_store::SharedTableStoreExt;
use crate::tables::SharedTableStore;
use chrono::Utc;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use kalamdb_commons::models::{TableId, UserId};
use kalamdb_commons::{NamespaceId, NodeId, TableName};
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

    /// Set the live query manager for this flush job (builder pattern)
    pub fn with_live_query_manager(mut self, manager: Arc<LiveQueryManager>) -> Self {
        self.live_query_manager = Some(manager);
        self
    }

    /// Execute flush job with tracking
    pub fn execute_tracked(&self) -> Result<FlushJobResult, KalamDbError> {
        FlushExecutor::execute_with_tracking(self, self.namespace_id.clone())
    }

    /// Generate batch filename with timestamp
    fn generate_batch_filename(&self) -> String {
        format!("batch-{}.parquet", Utc::now().timestamp_millis())
    }

    /// Convert JSON rows to Arrow RecordBatch
    fn rows_to_record_batch(&self, rows: &[(Vec<u8>, JsonValue)]) -> Result<RecordBatch, KalamDbError> {
        let mut builder = super::util::JsonBatchBuilder::new(self.schema.clone())?;
        for (_, row) in rows {
            builder.push_object_row(row)?;
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

        self.store.delete_batch_by_keys(&parsed_keys)
            .map_err(|e| KalamDbError::Other(format!("Failed to delete flushed rows: {}", e)))?;

        log::debug!("Deleted {} flushed rows from storage", parsed_keys.len());
        Ok(())
    }
}

impl TableFlush for SharedTableFlushJob {
    fn execute(&self) -> Result<FlushJobResult, KalamDbError> {
        log::debug!("🔄 Starting shared table flush: table={}.{}", 
                   self.namespace_id.as_str(), self.table_name.as_str());

        // Stream snapshot-backed scan and collect active rows
        let iter = self.store.scan_iter()
            .map_err(|e| {
                log::error!("❌ Failed to scan rows for shared table={}.{}: {}", 
                           self.namespace_id.as_str(), self.table_name.as_str(), e);
                KalamDbError::Other(format!("Failed to scan rows: {}", e))
            })?;
        
        let mut rows: Vec<(Vec<u8>, JsonValue)> = Vec::new();
        let mut scanned: usize = 0;

        for entry in iter {
            let (key_bytes, value_bytes) = match entry {
                Ok(pair) => pair,
                Err(e) => {
                    log::warn!("Skipping row due to iterator error (table={}.{}): {}", 
                              self.namespace_id.as_str(), self.table_name.as_str(), e);
                    continue;
                }
            };

            scanned += 1;

            let row: SharedTableRow = match serde_json::from_slice(&value_bytes) {
                Ok(v) => v,
                Err(e) => {
                    log::warn!("Skipping row due to deserialization error (table={}.{}): {}", 
                              self.namespace_id.as_str(), self.table_name.as_str(), e);
                    continue;
                }
            };

            // Skip soft-deleted rows
            if row._deleted {
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

        log::debug!("📊 Scanned {} rows from shared table={}.{} ({} active)",
                   scanned, self.namespace_id.as_str(), self.table_name.as_str(), rows.len());

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

        // Determine output path via unified SchemaCache
        let batch_filename = self.generate_batch_filename();
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
        writer.write(self.schema.clone(), vec![batch])?;

        log::info!("✅ Flushed {} rows for shared table={}.{} to {}", 
                  rows_count, self.namespace_id.as_str(), self.table_name.as_str(), output_path.display());

        // Delete flushed rows from RocksDB
        log::debug!("🗑️  Deleting {} flushed rows from RocksDB (table={}.{})",
                   rows_count, self.namespace_id.as_str(), self.table_name.as_str());
        self.delete_flushed_rows(&rows)?;

        let parquet_path = output_path.to_string_lossy().to_string();

        // Send flush notification if LiveQueryManager configured
        if let Some(manager) = &self.live_query_manager {
            let table_name = format!("{}.{}", self.namespace_id.as_str(), self.table_name.as_str());
            let parquet_files = vec![parquet_path.clone()];
            let notification = ChangeNotification::flush(table_name.clone(), rows_count, parquet_files.clone());
            let table_id = TableId::new(self.namespace_id.clone(), self.table_name.clone());
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
