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
use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use datafusion::arrow::record_batch::RecordBatch;
use rocksdb::{IteratorMode, WriteBatch, DB};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// User table flush job
///
/// Flushes data from RocksDB to Parquet files, grouping by UserId
pub struct UserTableFlushJob {
    /// RocksDB instance
    db: Arc<DB>,

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
        db: Arc<DB>,
        namespace_id: NamespaceId,
        table_name: TableName,
        schema: SchemaRef,
        storage_location: String,
    ) -> Self {
        let node_id = format!("node-{}", std::process::id());
        Self {
            db,
            namespace_id,
            table_name,
            schema,
            storage_location,
            node_id,
        }
    }

    /// Get the column family name for this table
    fn column_family_name(&self) -> String {
        format!(
            "user_table:{}:{}",
            self.namespace_id.as_str(),
            self.table_name.as_str()
        )
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
        let cf_name = self.column_family_name();
        let cf = self
            .db
            .cf_handle(&cf_name)
            .ok_or_else(|| KalamDbError::NotFound(format!("Column family not found: {}", cf_name)))?;

        // Group rows by UserId
        let mut rows_by_user: HashMap<UserId, Vec<(Vec<u8>, JsonValue)>> = HashMap::new();

        // Iterate over all rows in the column family
        for item in self.db.iterator_cf(cf, IteratorMode::Start) {
            let (key_bytes, value_bytes) = item
                .map_err(|e| KalamDbError::Other(format!("Iterator error: {}", e)))?;

            // Parse key: {UserId}:{row_id}
            let key_str = String::from_utf8_lossy(&key_bytes);
            let parts: Vec<&str> = key_str.splitn(2, ':').collect();

            if parts.len() != 2 {
                log::warn!("Invalid key format (expected UserId:row_id): {}", key_str);
                continue;
            }

            let user_id = UserId::new(parts[0].to_string());

            // Parse JSON value
            let row_data: JsonValue = serde_json::from_slice(&value_bytes).map_err(|e| {
                KalamDbError::SerializationError(format!("Failed to parse row data: {}", e))
            })?;

            // Skip soft-deleted rows (don't flush them)
            if let Some(deleted) = row_data.get("_deleted") {
                if deleted.as_bool() == Some(true) {
                    continue;
                }
            }

            rows_by_user
                .entry(user_id)
                .or_insert_with(Vec::new)
                .push((key_bytes.to_vec(), row_data));
        }

        // Flush each user's data to separate Parquet file
        let mut total_rows_flushed = 0;
        let mut parquet_files = Vec::new();

        for (user_id, rows) in &rows_by_user {
            let rows_count = rows.len();

            // Convert rows to RecordBatch
            let batch = self.rows_to_record_batch(rows)?;

            // Determine output path: ${storage_location}/${user_id}/batch-{timestamp}.parquet
            let user_storage_path = self.substitute_user_id_in_path(user_id);
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
        rows_by_user: &HashMap<UserId, Vec<(Vec<u8>, JsonValue)>>,
    ) -> Result<(), KalamDbError> {
        let cf_name = self.column_family_name();
        let cf = self
            .db
            .cf_handle(&cf_name)
            .ok_or_else(|| KalamDbError::NotFound(format!("Column family not found: {}", cf_name)))?;

        let mut batch = WriteBatch::default();

        for (_, rows) in rows_by_user {
            for (key_bytes, _) in rows {
                batch.delete_cf(cf, key_bytes);
            }
        }

        self.db
            .write(batch)
            .map_err(|e| KalamDbError::Other(format!("Failed to delete flushed rows: {}", e)))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::arrow::datatypes::{DataType, Field, Schema};
    use rocksdb::Options;
    use serde_json::json;
    use std::env;
    use std::fs;

    fn create_test_db() -> (Arc<DB>, tempfile::TempDir) {
        let temp_dir = tempfile::tempdir().unwrap();

        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let cf_names = vec!["user_table:test_ns:test_table"];

        let db = DB::open_cf(&opts, temp_dir.path(), &cf_names).unwrap();
        (Arc::new(db), temp_dir)
    }

    fn create_test_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("content", DataType::Utf8, true),
            Field::new("_updated", DataType::Int64, false),
            Field::new("_deleted", DataType::Boolean, false),
        ]))
    }

    #[test]
    fn test_user_table_flush_job_creation() {
        let (db, _temp_dir) = create_test_db();
        let schema = create_test_schema();

        let job = UserTableFlushJob::new(
            db,
            NamespaceId::new("test_ns"),
            TableName::new("test_table"),
            schema,
            "/data/${user_id}/tables/".to_string(),
        );

        assert_eq!(job.column_family_name(), "user_table:test_ns:test_table");
        assert_eq!(
            job.substitute_user_id_in_path(&UserId::new("user123".to_string())),
            "/data/user123/tables/"
        );
    }

    #[test]
    fn test_user_table_flush_empty_table() {
        let (db, _temp_dir) = create_test_db();
        let schema = create_test_schema();

        let temp_storage = env::temp_dir().join("kalamdb_flush_test_empty");
        let _ = fs::remove_dir_all(&temp_storage);

        let job = UserTableFlushJob::new(
            db,
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
        let (db, _temp_dir) = create_test_db();
        let schema = create_test_schema();

        let cf = db.cf_handle("user_table:test_ns:test_table").unwrap();

        // Insert test data for user1
        let row1 = json!({
            "id": "row1",
            "content": "Message 1",
            "_updated": 1234567890000i64,
            "_deleted": false
        });
        let row2 = json!({
            "id": "row2",
            "content": "Message 2",
            "_updated": 1234567891000i64,
            "_deleted": false
        });

        db.put_cf(cf, b"user1:row1", serde_json::to_vec(&row1).unwrap())
            .unwrap();
        db.put_cf(cf, b"user1:row2", serde_json::to_vec(&row2).unwrap())
            .unwrap();

        let temp_storage = env::temp_dir().join("kalamdb_flush_test_single");
        let _ = fs::remove_dir_all(&temp_storage);

        let job = UserTableFlushJob::new(
            db.clone(),
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

        // Verify rows deleted from RocksDB
        assert!(db.get_cf(cf, b"user1:row1").unwrap().is_none());
        assert!(db.get_cf(cf, b"user1:row2").unwrap().is_none());

        // Verify Parquet file exists
        let parquet_path = PathBuf::from(&result.parquet_files[0]);
        assert!(parquet_path.exists());

        let _ = fs::remove_dir_all(&temp_storage);
    }

    #[test]
    fn test_user_table_flush_multiple_users() {
        let (db, _temp_dir) = create_test_db();
        let schema = create_test_schema();

        let cf = db.cf_handle("user_table:test_ns:test_table").unwrap();

        // Insert test data for user1 and user2
        let row1 = json!({
            "id": "row1",
            "content": "User1 Message 1",
            "_updated": 1234567890000i64,
            "_deleted": false
        });
        let row2 = json!({
            "id": "row2",
            "content": "User2 Message 1",
            "_updated": 1234567891000i64,
            "_deleted": false
        });
        let row3 = json!({
            "id": "row3",
            "content": "User1 Message 2",
            "_updated": 1234567892000i64,
            "_deleted": false
        });

        db.put_cf(cf, b"user1:row1", serde_json::to_vec(&row1).unwrap())
            .unwrap();
        db.put_cf(cf, b"user2:row2", serde_json::to_vec(&row2).unwrap())
            .unwrap();
        db.put_cf(cf, b"user1:row3", serde_json::to_vec(&row3).unwrap())
            .unwrap();

        let temp_storage = env::temp_dir().join("kalamdb_flush_test_multiple");
        let _ = fs::remove_dir_all(&temp_storage);

        let job = UserTableFlushJob::new(
            db.clone(),
            NamespaceId::new("test_ns"),
            TableName::new("test_table"),
            schema,
            temp_storage.to_str().unwrap().to_string() + "/${user_id}/",
        );

        let result = job.execute().unwrap();
        assert_eq!(result.rows_flushed, 3); // 3 rows flushed
        assert_eq!(result.users_count, 2); // 2 users
        assert_eq!(result.parquet_files.len(), 2); // 2 Parquet files

        // Verify rows deleted from RocksDB
        assert!(db.get_cf(cf, b"user1:row1").unwrap().is_none());
        assert!(db.get_cf(cf, b"user2:row2").unwrap().is_none());
        assert!(db.get_cf(cf, b"user1:row3").unwrap().is_none());

        let _ = fs::remove_dir_all(&temp_storage);
    }

    #[test]
    fn test_user_table_flush_skips_soft_deleted() {
        let (db, _temp_dir) = create_test_db();
        let schema = create_test_schema();

        let cf = db.cf_handle("user_table:test_ns:test_table").unwrap();

        // Insert test data with soft-deleted row
        let row1 = json!({
            "id": "row1",
            "content": "Active Message",
            "_updated": 1234567890000i64,
            "_deleted": false
        });
        let row2 = json!({
            "id": "row2",
            "content": "Deleted Message",
            "_updated": 1234567891000i64,
            "_deleted": true  // Soft deleted
        });

        db.put_cf(cf, b"user1:row1", serde_json::to_vec(&row1).unwrap())
            .unwrap();
        db.put_cf(cf, b"user1:row2", serde_json::to_vec(&row2).unwrap())
            .unwrap();

        let temp_storage = env::temp_dir().join("kalamdb_flush_test_soft_delete");
        let _ = fs::remove_dir_all(&temp_storage);

        let job = UserTableFlushJob::new(
            db.clone(),
            NamespaceId::new("test_ns"),
            TableName::new("test_table"),
            schema,
            temp_storage.to_str().unwrap().to_string() + "/${user_id}/",
        );

        let result = job.execute().unwrap();
        assert_eq!(result.rows_flushed, 1); // Only 1 row flushed (soft-deleted skipped)

        // Soft-deleted row should remain in RocksDB
        assert!(db.get_cf(cf, b"user1:row2").unwrap().is_some());

        let _ = fs::remove_dir_all(&temp_storage);
    }
}
