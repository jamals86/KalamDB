//! Flush job for shared tables - migrated to use TableFlush trait
//!
//! This module implements the flush operation that moves data from RocksDB to Parquet files.
//! For shared tables, all rows are written to a single Parquet file per flush.
//!
//! **Refactoring (Phase 14 Step 11)**:
//! - Uses `TableFlush` trait from `crate::tables::base_flush`
//! - Eliminates 400+ lines of duplicated job tracking code
//! - Uses `FlushExecutor::execute_with_tracking()` for common workflow
//! - Only implements unique logic: single-file flush for all rows

use crate::catalog::{NamespaceId, TableName};
use crate::error::KalamDbError;
use crate::live_query::manager::{ChangeNotification, LiveQueryManager};
use crate::storage::ParquetWriter;
use crate::stores::system_table::SharedTableStoreExt;
use crate::tables::base_flush::{FlushExecutor, FlushJobResult, TableFlush};
use crate::tables::shared_tables::shared_table_store::SharedTableRow;
use crate::tables::SharedTableStore;
use chrono::Utc;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use kalamdb_commons::system::Job;
use serde_json::Value as JsonValue;
use std::path::PathBuf;
use std::sync::Arc;

/// Shared table flush job
///
/// Flushes data from RocksDB to a single Parquet file.
/// Implements `TableFlush` trait for common job tracking/metrics.
pub struct SharedTableFlushJob {
    /// SharedTableStore for accessing table data
    store: Arc<SharedTableStore>,

    /// Namespace ID
    namespace_id: NamespaceId,

    /// Table name
    table_name: TableName,

    /// Arrow schema for the table
    schema: SchemaRef,

    /// Storage location (single path, no ${user_id} templating)
    storage_location: String,

    /// Node ID for job tracking
    node_id: String,

    /// Optional LiveQueryManager for flush notifications
    live_query_manager: Option<Arc<LiveQueryManager>>,
}

impl SharedTableFlushJob {
    /// Create a new shared table flush job
    pub fn new(
        store: Arc<SharedTableStore>,
        namespace_id: NamespaceId,
        table_name: TableName,
        schema: SchemaRef,
        storage_location: String,
    ) -> Self {
        let node_id = format!("node-{}", std::process::id());
        Self {
            store,
            namespace_id,
            table_name,
            schema,
            storage_location,
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
    ///
    /// Uses `FlushExecutor::execute_with_tracking()` for common job tracking workflow.
    pub fn execute_tracked(&self) -> Result<FlushJobResult, KalamDbError> {
        FlushExecutor::execute_with_tracking(self, self.namespace_id.clone())
    }

    /// Generate batch filename with timestamp
    fn generate_batch_filename(&self) -> String {
        let timestamp = Utc::now().timestamp_millis();
        format!("batch-{}.parquet", timestamp)
    }

    /// Convert JSON rows to Arrow RecordBatch
    fn rows_to_record_batch(
        &self,
        rows: &[(Vec<u8>, JsonValue)],
    ) -> Result<RecordBatch, KalamDbError> {
        let mut builder = crate::flush::util::JsonBatchBuilder::new(self.schema.clone())?;
        for (_, row) in rows {
            builder.push_object_row(row)?;
        }
        builder.finish()
    }

    /// Delete flushed rows from RocksDB after successful Parquet write
    fn delete_flushed_rows(&self, rows: &[(Vec<u8>, JsonValue)]) -> Result<(), KalamDbError> {
        let keys: Vec<String> = rows
            .iter()
            .map(|(key_bytes, _)| String::from_utf8_lossy(key_bytes).to_string())
            .collect();

        if keys.is_empty() {
            return Ok(());
        }

        self.store
            .delete_batch_by_keys(
                self.namespace_id.as_str(),
                self.table_name.as_str(),
                &keys,
            )
            .map_err(|e| KalamDbError::Other(format!("Failed to delete flushed rows: {}", e)))?;

        log::debug!("Deleted {} flushed rows from storage", keys.len());
        Ok(())
    }
}

/// Implement TableFlush trait (unique shared table logic only)
impl TableFlush for SharedTableFlushJob {
    /// Execute the flush job (unique logic: single Parquet file for all rows)
    fn execute(&self) -> Result<FlushJobResult, KalamDbError> {
        log::debug!(
            "🔄 Starting shared table flush: table={}.{}",
            self.namespace_id.as_str(),
            self.table_name.as_str()
        );

        // Stream snapshot-backed scan and collect active rows
        let mut iter = self
            .store
            .scan_iter(self.namespace_id.as_str(), self.table_name.as_str())
            .map_err(|e| {
                log::error!(
                    "❌ Failed to scan rows for shared table={}.{}: {}",
                    self.namespace_id.as_str(),
                    self.table_name.as_str(),
                    e
                );
                KalamDbError::Other(format!("Failed to scan rows: {}", e))
            })?;

        let mut rows: Vec<(Vec<u8>, JsonValue)> = Vec::new();
        let mut scanned = 0usize;

        while let Some(Ok((key_bytes, value_bytes))) = iter.next() {
            scanned += 1;

            // Decode and filter soft-deleted rows
            let row: SharedTableRow = match serde_json::from_slice(&value_bytes) {
                Ok(r) => r,
                Err(e) => {
                    log::warn!(
                        "Skipping row due to deserialization error (table={}.{}): {}",
                        self.namespace_id.as_str(),
                        self.table_name.as_str(),
                        e
                    );
                    continue;
                }
            };

            if row._deleted {
                continue;
            }

            // Build JSON object with metadata fields
            let mut json_obj = row.fields.clone();
            if let Some(obj) = json_obj.as_object_mut() {
                obj.insert(
                    "access_level".to_string(),
                    JsonValue::String(row.access_level.as_str().to_string()),
                );
                obj.insert("_updated".to_string(), JsonValue::String(row._updated));
                obj.insert("_deleted".to_string(), JsonValue::Bool(false));
            }

            rows.push((key_bytes, json_obj));
        }

        log::debug!(
            "📊 Scanned {} rows from shared table={}.{} ({} active)",
            scanned,
            self.namespace_id.as_str(),
            self.table_name.as_str(),
            rows.len()
        );

        // If no rows to flush, return early
        if rows.is_empty() {
            log::info!(
                "⚠️  No rows to flush for shared table={}.{} (empty table or all deleted)",
                self.namespace_id.as_str(),
                self.table_name.as_str()
            );

            return Ok(FlushJobResult {
                job_record: self.create_job_record(&self.generate_job_id(), self.namespace_id.clone()),
                rows_flushed: 0,
                parquet_files: vec![],
                metadata: crate::tables::base_flush::FlushMetadata::shared_table(),
            });
        }

        let rows_count = rows.len();
        log::debug!(
            "💾 Flushing {} rows to Parquet for shared table={}.{}",
            rows_count,
            self.namespace_id.as_str(),
            self.table_name.as_str()
        );

        // Convert rows to RecordBatch
        let batch = self.rows_to_record_batch(&rows)?;

        // Determine output path: ${storage_location}/${table_name}/batch-{timestamp}.parquet
        let batch_filename = self.generate_batch_filename();
        let table_dir = PathBuf::from(&self.storage_location).join(self.table_name.as_str());
        let output_path = table_dir.join(&batch_filename);

        // Ensure directory exists
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| KalamDbError::Other(format!("Failed to create directory: {}", e)))?;
        }

        // Write to Parquet
        log::debug!(
            "📝 Writing Parquet file: path={}, rows={}",
            output_path.display(),
            rows_count
        );

        let writer = ParquetWriter::new(output_path.to_str().unwrap());
        writer
            .write(self.schema.clone(), vec![batch])
            .map_err(|e| {
                log::error!(
                    "❌ Failed to write Parquet file for shared table={}.{}, path={}: {}",
                    self.namespace_id.as_str(),
                    self.table_name.as_str(),
                    output_path.display(),
                    e
                );
                e
            })?;

        log::info!(
            "✅ Flushed {} rows for shared table={}.{} to {}",
            rows_count,
            self.namespace_id.as_str(),
            self.table_name.as_str(),
            output_path.display()
        );

        // Delete flushed rows from RocksDB
        log::debug!(
            "🗑️  Deleting {} flushed rows from RocksDB (table={}.{})",
            rows_count,
            self.namespace_id.as_str(),
            self.table_name.as_str()
        );

        self.delete_flushed_rows(&rows)?;

        log::debug!(
            "✅ Deleted {} rows from RocksDB after flush (table={}.{})",
            rows_count,
            self.namespace_id.as_str(),
            self.table_name.as_str()
        );

        // Send flush notification if LiveQueryManager configured
        if let Some(manager) = &self.live_query_manager {
            let table_name = format!(
                "{}.{}",
                self.namespace_id.as_str(),
                self.table_name.as_str()
            );
            let parquet_files = vec![batch_filename.clone()];

            let notification =
                ChangeNotification::flush(table_name.clone(), rows_count, parquet_files.clone());

            let manager = Arc::clone(manager);
            tokio::spawn(async move {
                if let Err(e) = manager.notify_table_change(&table_name, notification).await {
                    log::warn!(
                        "Failed to send flush notification for {}: {}",
                        table_name,
                        e
                    );
                }
            });
        }

        Ok(FlushJobResult {
            job_record: self.create_job_record(&self.generate_job_id(), self.namespace_id.clone()),
            rows_flushed: rows_count,
            parquet_files: vec![batch_filename],
            metadata: crate::tables::base_flush::FlushMetadata::shared_table(),
        })
    }

    fn table_identifier(&self) -> String {
        format!(
            "{}.{}",
            self.namespace_id.as_str(),
            self.table_name.as_str()
        )
    }

    fn live_query_manager(&self) -> Option<&Arc<LiveQueryManager>> {
        self.live_query_manager.as_ref()
    }

    fn node_id(&self) -> String {
        self.node_id.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::arrow::datatypes::{DataType, Field, Schema};

    #[test]
    fn test_generate_batch_filename() {
        let namespace_id = NamespaceId::new("test".to_string());
        let table_name = TableName::new("test_table".to_string());
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

        let job = SharedTableFlushJob::new(
            Arc::new(SharedTableStore::new(Arc::new(
                kalamdb_store::RocksDbBackend::new_temp().unwrap(),
            ))),
            namespace_id,
            table_name,
            schema,
            "/tmp/test".to_string(),
        );

        let filename = job.generate_batch_filename();
        assert!(filename.starts_with("batch-"));
        assert!(filename.ends_with(".parquet"));
    }

    #[test]
    fn test_table_identifier() {
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

        let job = SharedTableFlushJob::new(
            Arc::new(SharedTableStore::new(Arc::new(
                kalamdb_store::RocksDbBackend::new_temp().unwrap(),
            ))),
            namespace_id,
            table_name,
            schema,
            "/tmp/test".to_string(),
        );

        assert_eq!(job.table_identifier(), "test_ns.test_table");
    }
}
