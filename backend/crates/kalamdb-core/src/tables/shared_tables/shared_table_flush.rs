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

use crate::schema_registry::{NamespaceId, TableName};
use crate::schema_registry::SchemaRegistry; // Phase 10: Use unified cache instead of old TableCache
use crate::error::KalamDbError;
use crate::live_query::manager::{ChangeNotification, LiveQueryManager};
use crate::live_query::NodeId;
use crate::storage::ParquetWriter;
use crate::tables::system::system_table_store::SharedTableStoreExt;
use crate::tables::base_flush::{FlushExecutor, FlushJobResult, TableFlush};
use crate::tables::shared_tables::shared_table_store::SharedTableRow;
use crate::tables::SharedTableStore;
use chrono::Utc;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use kalamdb_commons::models::TableId; // Phase 10: Arc<TableId> optimization
use serde_json::Value as JsonValue;
use std::path::PathBuf;
use std::sync::Arc;

/// Shared table flush job
///
/// Flushes data from RocksDB to a single Parquet file.
/// Implements `TableFlush` trait for common job tracking/metrics.
///
/// **Phase 10 Optimization (T319)**: Stores Arc<TableId> to avoid repeated allocations
/// during cache lookups. Uses unified SchemaCache for storage path resolution.
pub struct SharedTableFlushJob {
    /// SharedTableStore for accessing table data
    store: Arc<SharedTableStore>,

    /// Composite table identifier (Phase 10: Arc for zero-allocation cache lookups)
    table_id: Arc<TableId>,

    /// Namespace ID (kept for backward compatibility with existing code)
    namespace_id: NamespaceId,

    /// Table name (kept for backward compatibility with existing code)
    table_name: TableName,

    /// Arrow schema for the table
    schema: SchemaRef,

    /// Unified SchemaCache for dynamic storage path resolution (Phase 10 - replaces TableCache)
    unified_cache: Arc<SchemaRegistry>,

    /// Node ID for job tracking
    node_id: NodeId,

    /// Optional LiveQueryManager for flush notifications
    live_query_manager: Option<Arc<LiveQueryManager>>,
}

impl SharedTableFlushJob {
    /// Create a new shared table flush job
    ///
    /// # Arguments
    /// * `table_id` - Arc<TableId> created once at registration (Phase 10: zero-allocation cache lookups)
    /// * `store` - SharedTableStore for accessing table data
    /// * `namespace_id` - Namespace ID (kept for backward compatibility)
    /// * `table_name` - Table name (kept for backward compatibility)
    /// * `schema` - Arrow schema for the table
    /// * `unified_cache` - Unified SchemaCache for storage path resolution (Phase 10)
    pub fn new(
        table_id: Arc<TableId>,
        store: Arc<SharedTableStore>,
        namespace_id: NamespaceId,
        table_name: TableName,
        schema: SchemaRef,
        unified_cache: Arc<SchemaRegistry>,
    ) -> Self {
        //TODO: Use the nodeId from global config or context
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
            .delete_batch_by_keys(self.namespace_id.as_str(), self.table_name.as_str(), &keys)
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
        let mut scanned: usize = 0;

        for entry in iter {
            let (key_bytes, value_bytes) = match entry {
                Ok(pair) => pair,
                Err(e) => {
                    log::warn!(
                        "Skipping row due to iterator error (table={}.{}): {}",
                        self.namespace_id.as_str(),
                        self.table_name.as_str(),
                        e
                    );
                    continue;
                }
            };

            scanned += 1;

            let row: SharedTableRow = match serde_json::from_slice(&value_bytes) {
                Ok(v) => v,
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

            // Skip soft-deleted rows
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
                job_record: self
                    .create_job_record(&self.generate_job_id(), self.namespace_id.clone()),
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

        // Determine output path via unified SchemaCache (Phase 10 - T319)
        let batch_filename = self.generate_batch_filename();
        let full_path = self.unified_cache.get_storage_path(
            &*self.table_id,  // Phase 10: Zero-allocation cache lookup
            None,             // No user_id for shared tables
            None,             // No sharding for now
        )?;
        let table_dir = PathBuf::from(full_path);
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
        writer.write(self.schema.clone(), vec![batch])?;

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

        let parquet_path = output_path.to_string_lossy().to_string();

        // Send flush notification if LiveQueryManager configured
        if let Some(manager) = &self.live_query_manager {
            let table_name = format!(
                "{}.{}",
                self.namespace_id.as_str(),
                self.table_name.as_str()
            );
            let parquet_files = vec![parquet_path.clone()];

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
            parquet_files: vec![parquet_path],
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

    fn node_id(&self) -> NodeId {
        self.node_id.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::arrow::datatypes::{DataType, Field, Schema};
    use kalamdb_store::{test_utils::TestDb, RocksDBBackend};

    /// Phase 10: Create Arc<TableId> for test jobs (avoids allocation on every cache lookup)
    fn create_test_table_id() -> Arc<TableId> {
        Arc::new(TableId::new(
            NamespaceId::new("test"),
            TableName::new("test_table"),
        ))
    }

    /// Phase 10: Create mock unified cache for tests
    fn create_test_cache() -> Arc<SchemaRegistry> {
        // For now, create empty cache - in real scenarios this would be populated
        Arc::new(SchemaRegistry::new(100, None))
    }

    #[test]
    fn test_generate_batch_filename() {
        let test_db = TestDb::new(&["shared_table:test:test_table"]).unwrap();
        let backend = Arc::new(RocksDBBackend::new(test_db.db.clone()));
        let namespace_id = NamespaceId::new("test".to_string());
        let table_name = TableName::new("test_table".to_string());
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

        let job = SharedTableFlushJob::new(
            create_test_table_id(),
            Arc::new(SharedTableStore::new(
                backend,
                "shared_table:test:test_table",
            )),
            namespace_id,
            table_name,
            schema,
            create_test_cache(),
        );

        let filename = job.generate_batch_filename();
        assert!(filename.starts_with("batch-"));
        assert!(filename.ends_with(".parquet"));
    }

    #[test]
    fn test_table_identifier() {
        let test_db = TestDb::new(&["shared_table:test_ns:test_table"]).unwrap();
        let backend = Arc::new(RocksDBBackend::new(test_db.db.clone()));
        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

        let job = SharedTableFlushJob::new(
            create_test_table_id(),
            Arc::new(SharedTableStore::new(
                backend,
                "shared_table:test_ns:test_table",
            )),
            namespace_id,
            table_name,
            schema,
            create_test_cache(),
        );

        assert_eq!(job.table_identifier(), "test_ns.test_table");
    }
}
