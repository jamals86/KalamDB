//! Flush job for shared tables
//!
//! This module implements the flush operation that moves data from RocksDB to Parquet files.
//! For shared tables, all rows are written to a single Parquet file per flush.

use crate::catalog::{NamespaceId, TableName};
use crate::error::KalamDbError;
use crate::live_query::manager::{ChangeNotification, LiveQueryManager};
use crate::storage::ParquetWriter;
use kalamdb_commons::system::Job;
use kalamdb_commons::{JobStatus, JobType};
use chrono::Utc;
use datafusion::arrow::array::{ArrayRef, BooleanArray, Int64Array, StringArray};
use datafusion::arrow::datatypes::{DataType, SchemaRef};
use datafusion::arrow::record_batch::RecordBatch;
use kalamdb_store::SharedTableStore;
use serde_json::{json, Value as JsonValue};
use std::path::PathBuf;
use std::sync::Arc;

/// Shared table flush job
///
/// Flushes data from RocksDB to a single Parquet file
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

/// Result of a flush job execution
#[derive(Debug, Clone)]
pub struct FlushJobResult {
    /// Job record for system.jobs table
    pub job_record: Job,

    /// Total rows flushed
    pub rows_flushed: usize,

    /// Parquet file written
    pub parquet_file: Option<String>,
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

    /// Set the live query manager for this flush job
    pub fn with_live_query_manager(mut self, manager: Arc<LiveQueryManager>) -> Self {
        self.live_query_manager = Some(manager);
        self
    }

    /// Generate batch filename with timestamp
    fn generate_batch_filename(&self) -> String {
        let timestamp = Utc::now().timestamp_millis();
        format!("batch-{}.parquet", timestamp)
    }

    /// Execute the flush job
    ///
    /// # Returns
    /// FlushJobResult with job record for system.jobs table
    pub fn execute(&self) -> Result<FlushJobResult, KalamDbError> {
        // Generate job ID
        let job_id = format!(
            "flush-shared-{}-{}-{}",
            self.table_name.as_str(),
            Utc::now().timestamp_millis(),
            uuid::Uuid::new_v4()
        );

        // Create job record
        let params = vec![
            format!("namespace={}", self.namespace_id.as_str()),
            format!("table={}", self.table_name.as_str()),
        ];
        let params_json = serde_json::to_string(&params)
            .unwrap_or_else(|_| "[]".to_string());
        
        let mut job_record =
            Job::new(job_id.clone(), JobType::Flush, self.namespace_id.clone(), self.node_id.clone())
                .with_table_name(self.table_name.clone())
                .with_parameters(params_json);

        log::info!(
            "🚀 Shared table flush job started: job_id={}, table={}.{}, timestamp={}",
            job_id,
            self.namespace_id.as_str(),
            self.table_name.as_str(),
            Utc::now().to_rfc3339()
        );

        // Execute flush and capture metrics
        let start_time = std::time::Instant::now();

        let (rows_flushed, parquet_file) = match self.execute_flush() {
            Ok(result) => result,
            Err(e) => {
                log::error!(
                    "❌ Shared table flush failed: job_id={}, table={}.{}, error={}",
                    job_id,
                    self.namespace_id.as_str(),
                    self.table_name.as_str(),
                    e
                );
                // Mark job as failed
                job_record = job_record.fail(format!("Flush failed: {}", e));
                return Ok(FlushJobResult {
                    job_record,
                    rows_flushed: 0,
                    parquet_file: None,
                });
            }
        };

        let duration_ms = start_time.elapsed().as_millis();

        // Create result JSON
        let result_json = json!({
            "rows_flushed": rows_flushed,
            "parquet_file": parquet_file,
            "duration_ms": duration_ms
        });

        // Mark job as completed
        job_record = job_record.complete(Some(result_json.to_string()));

        log::info!(
            "✅ Shared table flush completed: job_id={}, table={}.{}, rows_flushed={}, duration_ms={}, parquet_file={}",
            job_id,
            self.namespace_id.as_str(),
            self.table_name.as_str(),
            rows_flushed,
            duration_ms,
            parquet_file.as_deref().unwrap_or("none")
        );

        // Send flush notification to live query subscribers
        if let Some(live_query_manager) = &self.live_query_manager {
            let table_name = format!(
                "{}.{}",
                self.namespace_id.as_str(),
                self.table_name.as_str()
            );
            let parquet_files = parquet_file
                .as_ref()
                .map(|f| vec![f.clone()])
                .unwrap_or_default();

            let notification =
                ChangeNotification::flush(table_name.clone(), rows_flushed, parquet_files);

            let manager = Arc::clone(live_query_manager);
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
            job_record,
            rows_flushed,
            parquet_file,
        })
    }

    /// Internal flush execution (separated for error handling)
    fn execute_flush(&self) -> Result<(usize, Option<String>), KalamDbError> {
        log::debug!(
            "🔄 Starting shared table flush: table={}.{}",
            self.namespace_id.as_str(),
            self.table_name.as_str()
        );

        // Scan all rows (scan already filters out soft-deleted rows)
        let rows = self
            .store
            .scan(self.namespace_id.as_str(), self.table_name.as_str())
            .map_err(|e| {
                log::error!(
                    "❌ Failed to scan rows for shared table={}.{}: {}",
                    self.namespace_id.as_str(),
                    self.table_name.as_str(),
                    e
                );
                KalamDbError::Other(format!("Failed to scan rows: {}", e))
            })?;

        log::debug!(
            "📊 Scanned {} rows from shared table={}.{}",
            rows.len(),
            self.namespace_id.as_str(),
            self.table_name.as_str()
        );

        // If no rows to flush, return early
        if rows.is_empty() {
            log::info!(
                "⚠️  No rows to flush for shared table={}.{} (empty table or all deleted)",
                self.namespace_id.as_str(),
                self.table_name.as_str()
            );
            return Ok((0, None));
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

        let output_path_str = output_path.to_string_lossy().to_string();

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

        Ok((rows_count, Some(output_path_str)))
    }

    /// Convert JSON rows to Arrow RecordBatch
    fn rows_to_record_batch(
        &self,
        rows: &[(String, JsonValue)],
    ) -> Result<RecordBatch, KalamDbError> {
        // Build arrays for each column based on schema
        let mut arrays: Vec<ArrayRef> = Vec::new();

        for field in self.schema.fields() {
            let field_name = field.name();
            let array: ArrayRef = match field.data_type() {
                DataType::Utf8 => {
                    let values: Vec<Option<String>> = rows
                        .iter()
                        .map(|(_, row)| {
                            row.get(field_name)
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                        })
                        .collect();
                    Arc::new(StringArray::from(values))
                }
                DataType::Int64 => {
                    let values: Vec<Option<i64>> = rows
                        .iter()
                        .map(|(_, row)| row.get(field_name).and_then(|v| v.as_i64()))
                        .collect();
                    Arc::new(Int64Array::from(values))
                }
                DataType::Boolean => {
                    let values: Vec<Option<bool>> = rows
                        .iter()
                        .map(|(_, row)| row.get(field_name).and_then(|v| v.as_bool()))
                        .collect();
                    Arc::new(BooleanArray::from(values))
                }
                _ => {
                    return Err(KalamDbError::Other(format!(
                        "Unsupported data type for flush: {:?}",
                        field.data_type()
                    )))
                }
            };

            arrays.push(array);
        }

        RecordBatch::try_new(self.schema.clone(), arrays)
            .map_err(|e| KalamDbError::Other(format!("Failed to create RecordBatch: {}", e)))
    }

    /// Delete flushed rows from RocksDB
    fn delete_flushed_rows(&self, rows: &[(String, JsonValue)]) -> Result<(), KalamDbError> {
        // Collect all row IDs for batch deletion
        let row_ids: Vec<String> = rows.iter().map(|(row_id, _)| row_id.clone()).collect();

        if row_ids.is_empty() {
            return Ok(());
        }

        self.store
            .delete_batch_by_row_ids(
                self.namespace_id.as_str(),
                self.table_name.as_str(),
                &row_ids,
            )
            .map_err(|e| KalamDbError::Other(format!("Failed to delete flushed rows: {}", e)))?;

        log::debug!("Deleted {} flushed rows from storage", row_ids.len());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::arrow::datatypes::{DataType, Field, Schema};
    use kalamdb_store::test_utils::TestDb;
    use serde_json::json;
    use std::env;
    use std::fs;

    fn create_test_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("content", DataType::Utf8, true),
            Field::new("_updated", DataType::Utf8, false), // Stored as RFC3339 string
            Field::new("_deleted", DataType::Boolean, false),
        ]))
    }

    #[test]
    fn test_shared_table_flush_job_creation() {
        let test_db = TestDb::single_cf("shared_table:test_ns:test_table").unwrap();
        let store = Arc::new(SharedTableStore::new(test_db.db.clone()).unwrap());
        let schema = create_test_schema();

        let job = SharedTableFlushJob::new(
            store,
            NamespaceId::new("test_ns"),
            TableName::new("test_table"),
            schema,
            "/data/shared".to_string(),
        );

        assert_eq!(job.storage_location, "/data/shared");
        assert_eq!(job.namespace_id.as_str(), "test_ns");
        assert_eq!(job.table_name.as_str(), "test_table");
    }

    #[test]
    fn test_shared_table_flush_empty_table() {
        let test_db = TestDb::single_cf("shared_table:test_ns:test_table").unwrap();
        let store = Arc::new(SharedTableStore::new(test_db.db.clone()).unwrap());
        let schema = create_test_schema();

        let temp_storage = env::temp_dir().join("kalamdb_shared_flush_test_empty");
        let _ = fs::remove_dir_all(&temp_storage);

        let job = SharedTableFlushJob::new(
            store,
            NamespaceId::new("test_ns"),
            TableName::new("test_table"),
            schema,
            temp_storage.to_str().unwrap().to_string(),
        );

        let result = job.execute().unwrap();
        assert_eq!(result.rows_flushed, 0); // 0 rows flushed
        assert!(result.parquet_file.is_none()); // No Parquet file
        assert_eq!(result.job_record.status, JobStatus::Completed);
        assert_eq!(result.job_record.job_type, JobType::Flush);

        let _ = fs::remove_dir_all(&temp_storage);
    }

    #[test]
    fn test_shared_table_flush_with_rows() {
        let test_db = TestDb::single_cf("shared_table:test_ns:test_table").unwrap();
        let store = Arc::new(SharedTableStore::new(test_db.db.clone()).unwrap());
        let schema = create_test_schema();

        // Insert test data
        let row1 = json!({
            "id": "row1",
            "content": "Shared Message 1"
        });
        let row2 = json!({
            "id": "row2",
            "content": "Shared Message 2"
        });
        let row3 = json!({
            "id": "row3",
            "content": "Shared Message 3"
        });

        store.put("test_ns", "test_table", "row1", row1).unwrap();
        store.put("test_ns", "test_table", "row2", row2).unwrap();
        store.put("test_ns", "test_table", "row3", row3).unwrap();

        let temp_storage = env::temp_dir().join("kalamdb_shared_flush_test_with_rows");
        let _ = fs::remove_dir_all(&temp_storage);

        let job = SharedTableFlushJob::new(
            store.clone(),
            NamespaceId::new("test_ns"),
            TableName::new("test_table"),
            schema,
            temp_storage.to_str().unwrap().to_string(),
        );

        let result = job.execute().unwrap();
        assert_eq!(result.rows_flushed, 3); // 3 rows flushed
        assert!(result.parquet_file.is_some()); // Parquet file created
        assert_eq!(result.job_record.status, JobStatus::Completed);

        // Verify rows deleted from storage
        let remaining = store.scan("test_ns", "test_table").unwrap();
        assert_eq!(remaining.len(), 0);

        // Verify Parquet file exists
        let parquet_path = PathBuf::from(result.parquet_file.unwrap());
        assert!(parquet_path.exists());
        assert!(parquet_path.to_string_lossy().contains("test_table"));

        let _ = fs::remove_dir_all(&temp_storage);
    }

    #[test]
    fn test_shared_table_flush_filters_soft_deleted() {
        let test_db = TestDb::single_cf("shared_table:test_ns:test_table").unwrap();
        let store = Arc::new(SharedTableStore::new(test_db.db.clone()).unwrap());
        let schema = create_test_schema();

        // Insert test data with one active and one soft-deleted row
        let row1 = json!({
            "id": "row1",
            "content": "Active Message"
        });
        let row2 = json!({
            "id": "row2",
            "content": "Deleted Message"
        });

        store.put("test_ns", "test_table", "row1", row1).unwrap();
        store.put("test_ns", "test_table", "row2", row2).unwrap();

        // Soft-delete row2
        store
            .delete("test_ns", "test_table", "row2", false)
            .unwrap();

        let temp_storage = env::temp_dir().join("kalamdb_shared_flush_test_soft_delete");
        let _ = fs::remove_dir_all(&temp_storage);

        let job = SharedTableFlushJob::new(
            store.clone(),
            NamespaceId::new("test_ns"),
            TableName::new("test_table"),
            schema,
            temp_storage.to_str().unwrap().to_string(),
        );

        let result = job.execute().unwrap();
        assert_eq!(result.rows_flushed, 1); // Only 1 row flushed (soft-deleted filtered out by scan)
        assert_eq!(result.job_record.status, JobStatus::Completed);

        // Verify only active row was flushed and deleted
        let remaining = store.scan("test_ns", "test_table").unwrap();
        assert_eq!(remaining.len(), 0); // All active rows flushed

        let _ = fs::remove_dir_all(&temp_storage);
    }

    #[test]
    fn test_shared_table_flush_job_record() {
        let test_db = TestDb::single_cf("shared_table:test_ns:test_table").unwrap();
        let store = Arc::new(SharedTableStore::new(test_db.db.clone()).unwrap());
        let schema = create_test_schema();

        // Insert test data
        let row1 = json!({
            "id": "row1",
            "content": "Test Message"
        });
        store.put("test_ns", "test_table", "row1", row1).unwrap();

        let temp_storage = env::temp_dir().join("kalamdb_shared_flush_test_job_record");
        let _ = fs::remove_dir_all(&temp_storage);

        let job = SharedTableFlushJob::new(
            store.clone(),
            NamespaceId::new("test_ns"),
            TableName::new("test_table"),
            schema,
            temp_storage.to_str().unwrap().to_string(),
        );

        let result = job.execute().unwrap();

        // Verify job record fields
        assert_eq!(result.job_record.job_type, JobType::Flush);
        assert_eq!(result.job_record.status, JobStatus::Completed);
        assert!(result
            .job_record
            .job_id
            .starts_with("flush-shared-test_table-"));
        assert_eq!(
            result
                .job_record
                .table_name
                .as_ref()
                .map(|name| name.as_str()),
            Some("test_table")
        );
        assert_eq!(result.job_record.namespace_id.as_str(), "test_ns");
        assert!(result.job_record.result.is_some());

        // Verify result JSON contains expected fields
        let result_json = result.job_record.result.unwrap();
        assert!(result_json.contains("rows_flushed"));
        assert!(result_json.contains("parquet_file"));
        assert!(result_json.contains("duration_ms"));

        let _ = fs::remove_dir_all(&temp_storage);
    }
}
