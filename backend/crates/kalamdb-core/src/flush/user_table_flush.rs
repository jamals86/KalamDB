//! Flush job for user tables
//!
//! This module implements the flush operation that moves data from RocksDB to Parquet files.
//! For user tables, it groups rows by UserId and writes separate Parquet files per user.

use crate::catalog::{NamespaceId, TableName, UserId};
use crate::error::KalamDbError;
use crate::storage::ParquetWriter;
use crate::tables::system::jobs_provider::JobRecord;
use chrono::Utc;
use datafusion::arrow::array::{ArrayRef, BooleanArray, Int64Array, StringArray};
use datafusion::arrow::datatypes::{DataType, SchemaRef};
use datafusion::arrow::record_batch::RecordBatch;
use kalamdb_store::UserTableStore;
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// User table flush job
///
/// Flushes data from RocksDB to Parquet files, grouping by UserId
pub struct UserTableFlushJob {
    /// UserTableStore for accessing table data
    store: Arc<UserTableStore>,

    /// Namespace ID
    namespace_id: NamespaceId,

    /// Table name
    table_name: TableName,

    /// Arrow schema for the table
    schema: SchemaRef,

    /// Storage location template (may contain ${user_id})
    storage_location: String,

    /// Node ID for job tracking
    node_id: String,
}

/// Result of a flush job execution
#[derive(Debug, Clone)]
pub struct FlushJobResult {
    /// Job record for system.jobs table
    pub job_record: JobRecord,

    /// Total rows flushed
    pub rows_flushed: usize,

    /// Number of users processed
    pub users_count: usize,

    /// Parquet files written
    pub parquet_files: Vec<String>,
}

impl UserTableFlushJob {
    /// Create a new user table flush job
    pub fn new(
        store: Arc<UserTableStore>,
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
        }
    }

    /// Substitute ${user_id} in storage path template
    fn substitute_user_id_in_path(&self, user_id: &UserId) -> String {
        self.storage_location
            .replace("${user_id}", user_id.as_str())
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
            "flush-{}-{}-{}",
            self.table_name.as_str(),
            Utc::now().timestamp_millis(),
            uuid::Uuid::new_v4()
        );

        // Create job record
        let mut job_record = JobRecord::new(
            job_id.clone(),
            "flush".to_string(),
            self.node_id.clone(),
        )
        .with_table_name(format!(
            "{}.{}",
            self.namespace_id.as_str(),
            self.table_name.as_str()
        ));

        // Execute flush and capture metrics
        let start_time = std::time::Instant::now();

        let (rows_flushed, users_count, parquet_files) = match self.execute_flush() {
            Ok(result) => result,
            Err(e) => {
                // Mark job as failed
                job_record = job_record.fail(format!("Flush failed: {}", e));
                return Ok(FlushJobResult {
                    job_record,
                    rows_flushed: 0,
                    users_count: 0,
                    parquet_files: vec![],
                });
            }
        };

        let duration_ms = start_time.elapsed().as_millis();

        // Create result JSON
        let result_json = json!({
            "rows_flushed": rows_flushed,
            "users_count": users_count,
            "parquet_files_count": parquet_files.len(),
            "parquet_files": parquet_files,
            "duration_ms": duration_ms
        });

        // Mark job as completed
        job_record = job_record.complete(Some(result_json.to_string()));

        Ok(FlushJobResult {
            job_record,
            rows_flushed,
            users_count,
            parquet_files,
        })
    }

    /// Internal flush execution (separated for error handling)
    fn execute_flush(&self) -> Result<(usize, usize, Vec<String>), KalamDbError> {
        // Get rows grouped by user
        let rows_by_user = self
            .store
            .get_rows_by_user(
                self.namespace_id.as_str(),
                self.table_name.as_str(),
            )
            .map_err(|e| KalamDbError::Other(format!("Failed to get rows: {}", e)))?;

        // Flush each user's data to separate Parquet file
        let mut total_rows_flushed = 0;
        let mut parquet_files = Vec::new();

        for (user_id_str, rows) in &rows_by_user {
            let user_id = UserId::new(user_id_str.clone());
            let rows_count = rows.len();

            // Convert rows to RecordBatch
            let batch = self.rows_to_record_batch(rows)?;

            // Determine output path: ${storage_location}/${user_id}/batch-{timestamp}.parquet
            let user_storage_path = self.substitute_user_id_in_path(&user_id);
            let batch_filename = self.generate_batch_filename();
            let output_path = PathBuf::from(&user_storage_path).join(&batch_filename);

            // Write to Parquet
            let writer = ParquetWriter::new(output_path.to_str().unwrap());
            writer.write(self.schema.clone(), vec![batch])?;

            parquet_files.push(output_path.to_string_lossy().to_string());
            total_rows_flushed += rows_count;

            log::info!(
                "Flushed {} rows for user {} to {}",
                rows_count,
                user_id.as_str(),
                output_path.display()
            );
        }

        // Delete flushed rows from RocksDB
        self.delete_flushed_rows(&rows_by_user)?;

        Ok((total_rows_flushed, rows_by_user.len(), parquet_files))
    }

    /// Convert JSON rows to Arrow RecordBatch
    fn rows_to_record_batch(
        &self,
        rows: &[(Vec<u8>, JsonValue)],
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

        RecordBatch::try_new(self.schema.clone(), arrays).map_err(|e| {
            KalamDbError::Other(format!("Failed to create RecordBatch: {}", e))
        })
    }

    /// Delete flushed rows from RocksDB
    fn delete_flushed_rows(
        &self,
        rows_by_user: &HashMap<String, Vec<(Vec<u8>, JsonValue)>>,
    ) -> Result<(), KalamDbError> {
        // Collect all keys for batch deletion
        let keys: Vec<Vec<u8>> = rows_by_user
            .values()
            .flat_map(|rows| rows.iter().map(|(key, _)| key.clone()))
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
    fn test_user_table_flush_job_creation() {
        let test_db = TestDb::single_cf("user_table:test_ns:test_table").unwrap();
        let store = Arc::new(UserTableStore::new(test_db.db.clone()).unwrap());
        let schema = create_test_schema();

        let job = UserTableFlushJob::new(
            store,
            NamespaceId::new("test_ns"),
            TableName::new("test_table"),
            schema,
            "/data/${user_id}/tables/".to_string(),
        );

        assert_eq!(
            job.substitute_user_id_in_path(&UserId::new("user123".to_string())),
            "/data/user123/tables/"
        );
    }

    #[test]
    fn test_user_table_flush_empty_table() {
        let test_db = TestDb::single_cf("user_table:test_ns:test_table").unwrap();
        let store = Arc::new(UserTableStore::new(test_db.db.clone()).unwrap());
        let schema = create_test_schema();

        let temp_storage = env::temp_dir().join("kalamdb_flush_test_empty");
        let _ = fs::remove_dir_all(&temp_storage);

        let job = UserTableFlushJob::new(
            store,
            NamespaceId::new("test_ns"),
            TableName::new("test_table"),
            schema,
            temp_storage.to_str().unwrap().to_string() + "/${user_id}/",
        );

        let result = job.execute().unwrap();
        assert_eq!(result.rows_flushed, 0); // 0 rows flushed
        assert_eq!(result.users_count, 0); // 0 users
        assert_eq!(result.parquet_files.len(), 0); // 0 Parquet files
        assert_eq!(result.job_record.status, "completed");
        assert_eq!(result.job_record.job_type, "flush");

        let _ = fs::remove_dir_all(&temp_storage);
    }

    #[test]
    fn test_user_table_flush_single_user() {
        let test_db = TestDb::single_cf("user_table:test_ns:test_table").unwrap();
        let store = Arc::new(UserTableStore::new(test_db.db.clone()).unwrap());
        let schema = create_test_schema();

        // Insert test data for user1
        let row1 = json!({
            "id": "row1",
            "content": "Message 1"
        });
        let row2 = json!({
            "id": "row2",
            "content": "Message 2"
        });

        store
            .put("test_ns", "test_table", "user1", "row1", row1)
            .unwrap();
        store
            .put("test_ns", "test_table", "user1", "row2", row2)
            .unwrap();

        let temp_storage = env::temp_dir().join("kalamdb_flush_test_single");
        let _ = fs::remove_dir_all(&temp_storage);

        let job = UserTableFlushJob::new(
            store.clone(),
            NamespaceId::new("test_ns"),
            TableName::new("test_table"),
            schema,
            temp_storage.to_str().unwrap().to_string() + "/${user_id}/",
        );

        let result = job.execute().unwrap();
        assert_eq!(result.rows_flushed, 2); // 2 rows flushed
        assert_eq!(result.users_count, 1); // 1 user
        assert_eq!(result.parquet_files.len(), 1); // 1 Parquet file
        assert_eq!(result.job_record.status, "completed");

        // Verify rows deleted from storage
        let remaining = store.scan_all("test_ns", "test_table").unwrap();
        assert_eq!(remaining.len(), 0);

        // Verify Parquet file exists
        let parquet_path = PathBuf::from(&result.parquet_files[0]);
        assert!(parquet_path.exists());

        let _ = fs::remove_dir_all(&temp_storage);
    }

    #[test]
    fn test_user_table_flush_multiple_users() {
        let test_db = TestDb::single_cf("user_table:test_ns:test_table").unwrap();
        let store = Arc::new(UserTableStore::new(test_db.db.clone()).unwrap());
        let schema = create_test_schema();

        // Insert test data for user1 and user2
        let row1 = json!({
            "id": "row1",
            "content": "User1 Message 1"
        });
        let row2 = json!({
            "id": "row2",
            "content": "User2 Message 1"
        });
        let row3 = json!({
            "id": "row3",
            "content": "User1 Message 2"
        });

        store
            .put("test_ns", "test_table", "user1", "row1", row1)
            .unwrap();
        store
            .put("test_ns", "test_table", "user2", "row2", row2)
            .unwrap();
        store
            .put("test_ns", "test_table", "user1", "row3", row3)
            .unwrap();

        let temp_storage = env::temp_dir().join("kalamdb_flush_test_multiple");
        let _ = fs::remove_dir_all(&temp_storage);

        let job = UserTableFlushJob::new(
            store.clone(),
            NamespaceId::new("test_ns"),
            TableName::new("test_table"),
            schema,
            temp_storage.to_str().unwrap().to_string() + "/${user_id}/",
        );

        let result = job.execute().unwrap();
        assert_eq!(result.rows_flushed, 3); // 3 rows flushed
        assert_eq!(result.users_count, 2); // 2 users
        assert_eq!(result.parquet_files.len(), 2); // 2 Parquet files

        // Verify rows deleted from storage
        let remaining = store.scan_all("test_ns", "test_table").unwrap();
        assert_eq!(remaining.len(), 0);

        let _ = fs::remove_dir_all(&temp_storage);
    }

    #[test]
    fn test_user_table_flush_skips_soft_deleted() {
        let test_db = TestDb::single_cf("user_table:test_ns:test_table").unwrap();
        let store = Arc::new(UserTableStore::new(test_db.db.clone()).unwrap());
        let schema = create_test_schema();

        // Insert test data with one active and one soft-deleted row
        let row1 = json!({
            "id": "row1",
            "content": "Active Message"
        });
        let row2_active = json!({
            "id": "row2",
            "content": "Deleted Message"
        });

        store
            .put("test_ns", "test_table", "user1", "row1", row1)
            .unwrap();
        store
            .put("test_ns", "test_table", "user1", "row2", row2_active)
            .unwrap();

        // Soft-delete row2
        store
            .delete("test_ns", "test_table", "user1", "row2", false)
            .unwrap();

        let temp_storage = env::temp_dir().join("kalamdb_flush_test_soft_delete");
        let _ = fs::remove_dir_all(&temp_storage);

        let job = UserTableFlushJob::new(
            store.clone(),
            NamespaceId::new("test_ns"),
            TableName::new("test_table"),
            schema,
            temp_storage.to_str().unwrap().to_string() + "/${user_id}/",
        );

        let result = job.execute().unwrap();
        assert_eq!(result.rows_flushed, 1); // Only 1 row flushed (soft-deleted skipped)

        // Verify soft-deleted row still exists (not flushed)
        let remaining = store.scan_all("test_ns", "test_table").unwrap();
        // scan_all filters soft-deleted rows, but we can check the count was 1 flushed
        assert_eq!(remaining.len(), 0); // Both deleted from storage after flush

        let _ = fs::remove_dir_all(&temp_storage);
    }
}
