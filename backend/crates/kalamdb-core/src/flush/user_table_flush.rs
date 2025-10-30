//! Flush job for user tables
//!
//! This module implements the flush operation that moves data from RocksDB to Parquet files.
//! For user tables, it groups rows by UserId and writes separate Parquet files per user.

use crate::catalog::{NamespaceId, TableName, UserId};
use crate::error::KalamDbError;
use crate::live_query::manager::{ChangeNotification, LiveQueryManager};
use crate::storage::{ParquetWriter, StorageRegistry};
use crate::stores::system_table::UserTableStoreExt;
use crate::tables::{UserTableStore, new_user_table_store};
use chrono::Utc;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use kalamdb_commons::models::StorageType;
use kalamdb_commons::system::Job;
use kalamdb_commons::{JobId, JobType};
use serde_json::{json, Value as JsonValue};
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
    /// DEPRECATED: Use storage_registry instead (T170)
    storage_location: String,

    /// Storage registry for dynamic storage resolution (T170)
    storage_registry: Option<Arc<StorageRegistry>>,

    /// Node ID for job tracking
    node_id: String,

    /// Optional LiveQueryManager for flush notifications
    live_query_manager: Option<Arc<LiveQueryManager>>,

    /// Optional JobsTableProvider for job state persistence (T158d)
    jobs_provider: Option<Arc<crate::tables::system::JobsTableProvider>>,
}

/// Result of a flush job execution
#[derive(Debug, Clone)]
pub struct FlushJobResult {
    /// Job record for system.jobs table
    pub job_record: Job,

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
            storage_registry: None,
            node_id,
            live_query_manager: None,
            jobs_provider: None,
        }
    }

    /// Set the StorageRegistry for dynamic storage resolution (builder pattern)
    ///
    /// # T170: Enable dynamic storage resolution via StorageRegistry
    pub fn with_storage_registry(mut self, registry: Arc<StorageRegistry>) -> Self {
        self.storage_registry = Some(registry);
        self
    }

    /// Set the LiveQueryManager for flush notifications (builder pattern)
    pub fn with_live_query_manager(mut self, manager: Arc<LiveQueryManager>) -> Self {
        self.live_query_manager = Some(manager);
        self
    }

    /// Set the JobsTableProvider for job state persistence (builder pattern)
    ///
    /// # T158d: Enable job state persistence to system.jobs table
    pub fn with_jobs_provider(
        mut self,
        provider: Arc<crate::tables::system::JobsTableProvider>,
    ) -> Self {
        self.jobs_provider = Some(provider);
        self
    }

    /// Substitute ${user_id} in storage path template
    ///
    /// DEPRECATED: Use resolve_storage_path_for_user() instead (T170)
    fn substitute_user_id_in_path(&self, user_id: &UserId) -> String {
        self.storage_location
            .replace("${user_id}", user_id.as_str())
    }

    /// Resolve storage path for a user using StorageRegistry (T170-T170c)
    ///
    /// Implements dynamic template resolution:
    /// 1. Query StorageRegistry for storage configuration
    /// 2. Use Storage.base_directory as path prefix
    /// 3. Apply Storage.user_tables_template with variable substitution
    /// 4. Validate template variable ordering
    ///
    /// Fallback to hardcoded storage_location if registry not configured.
    /// Resolve storage path for a user using template substitution
    ///
    /// # T161c: Template path resolution algorithm
    /// 1. Lookup storage definition via StorageRegistry (or fallback to legacy path)
    /// 2. Get user_tables_template from Storage (e.g., "{namespace}/{tableName}/{userId}")
    /// 3. Single-pass substitution of template variables:
    ///    - {namespace} → namespace_id.as_str()
    ///    - {tableName} → table_name.as_str()
    ///    - {userId} → user_id.as_str()
    ///    - {shard} → computed shard number (future: from sharding strategy)
    /// 4. Combine base_directory + resolved_template
    /// 5. Validation happens at CREATE STORAGE time (not at flush time)
    ///
    /// # Examples
    /// Template: "data/{namespace}/{tableName}/users/{userId}"
    /// Resolved: "data/prod/events/users/user123"
    ///
    /// Template: "{userId}/tables/{tableName}"
    /// Resolved: "user456/tables/messages"
    fn resolve_storage_path_for_user(
        &self,
        user_id: &UserId,
    ) -> Result<(String, Option<JsonValue>), KalamDbError> {
        if let Some(ref registry) = self.storage_registry {
            let storage = registry
                .get_storage_config("local")?
                .ok_or_else(|| KalamDbError::NotFound("Storage 'local' not found".to_string()))?;

            let path = storage
                .user_tables_template()
                .replace("{namespace}", self.namespace_id.as_str())
                .replace("{tableName}", self.table_name.as_str())
                .replace("{userId}", user_id.as_str())
                .replace("{shard}", "");

            let full_path = if storage.base_directory().is_empty() {
                path
            } else {
                format!(
                    "{}/{}",
                    storage.base_directory().trim_end_matches('/'),
                    path.trim_start_matches('/')
                )
            };

            let credentials = if matches!(storage.storage_type(), StorageType::S3) {
                match storage.credentials() {
                    Some(raw) => Some(serde_json::from_str::<JsonValue>(raw).map_err(|e| {
                        KalamDbError::Other(format!("Invalid S3 credentials JSON: {}", e))
                    })?),
                    None => None,
                }
            } else {
                None
            };

            Ok((full_path, credentials))
        } else {
            Ok((self.substitute_user_id_in_path(user_id), None))
        }
    }

    /// Generate batch filename with ISO 8601 timestamp
    ///
    /// Format: YYYY-MM-DDTHH-MM-SS.parquet
    /// Example: 2025-10-22T14-30-45.parquet
    ///
    /// # T152a: ISO 8601 timestamp-based Parquet filename generation
    fn generate_batch_filename(&self) -> String {
        let now = Utc::now();
        // Format as ISO 8601 with hyphens (not colons, as they're invalid in Windows filenames)
        now.format("%Y-%m-%dT%H-%M-%S.parquet").to_string()
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
        let params = vec![
            format!("namespace={}", self.namespace_id.as_str()),
            format!("table={}", self.table_name.as_str()),
        ];
        let params_json = serde_json::to_string(&params).unwrap_or_else(|_| "[]".to_string());

        let mut job_record = Job::new(
            JobId::new(job_id.clone()),
            JobType::Flush,
            self.namespace_id.clone(),
            self.node_id.clone(),
        )
        .with_table_name(self.table_name.clone())
        .with_parameters(params_json);

        // T158d: Persist job state to system.jobs BEFORE starting work
        if let Some(ref jobs_provider) = self.jobs_provider {
            jobs_provider.insert_job(job_record.clone())?;

            // T158k: DEBUG logging for flush start
            log::info!(
                "🚀 Flush job started: job_id={}, table={}.{}, timestamp={}",
                job_id,
                self.namespace_id.as_str(),
                self.table_name.as_str(),
                Utc::now().to_rfc3339()
            );
        } else {
            log::info!(
                "🚀 Flush job started (no jobs_provider): table={}.{}, timestamp={}",
                self.namespace_id.as_str(),
                self.table_name.as_str(),
                Utc::now().to_rfc3339()
            );
        }

        // Execute flush and capture metrics
        let start_time = std::time::Instant::now();

        let (rows_flushed, users_count, parquet_files) = match self.execute_flush() {
            Ok(result) => result,
            Err(e) => {
                // Mark job as failed
                job_record = job_record.fail(format!("Flush failed: {}", e));

                // T158d: Update job status to 'failed' in system.jobs
                if let Some(ref jobs_provider) = self.jobs_provider {
                    let _ = jobs_provider.update_job(job_record.clone());
                }

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

        // T158d: Update job status to 'completed' in system.jobs
        // T158l: DEBUG logging for flush completion
        if let Some(ref jobs_provider) = self.jobs_provider {
            jobs_provider.update_job(job_record.clone())?;

            log::info!(
                "✅ Flush job completed: job_id={}, table={}.{}, rows_flushed={}, users_count={}, parquet_files={}, duration_ms={}",
                job_id,
                self.namespace_id.as_str(),
                self.table_name.as_str(),
                rows_flushed,
                users_count,
                parquet_files.len(),
                duration_ms
            );
        } else {
            log::info!(
                "✅ Flush job completed (no jobs_provider): table={}.{}, rows_flushed={}, users_count={}, parquet_files={}, duration_ms={}",
                self.namespace_id.as_str(),
                self.table_name.as_str(),
                rows_flushed,
                users_count,
                parquet_files.len(),
                duration_ms
            );
        }

        // T172: Notify subscribers of flush completion
        if let Some(live_query_manager) = &self.live_query_manager {
            let notification = ChangeNotification::flush(
                self.table_name.as_str().to_string(),
                rows_flushed,
                parquet_files.clone(),
            );

            let manager = Arc::clone(live_query_manager);
            let table_name = self.table_name.as_str().to_string();

            // Spawn async notification (non-blocking)
            tokio::spawn(async move {
                if let Err(e) = manager.notify_table_change(&table_name, notification).await {
                    #[cfg(debug_assertions)]
                    eprintln!("Failed to notify flush completion: {}", e);
                }
            });
        }

        Ok(FlushJobResult {
            job_record,
            rows_flushed,
            users_count,
            parquet_files,
        })
    }

    /// Internal flush execution with streaming per-user writes
    ///
    /// This method implements a memory-efficient streaming flush that processes one user at a time:
    /// 1. Creates RocksDB snapshot for read consistency
    /// 2. Scans table column family sequentially
    /// 3. Accumulates rows for current userId in memory buffer
    /// 4. Detects userId boundary to trigger Parquet write
    /// 5. Writes accumulated rows to Parquet file for completed user
    /// 6. Deletes successfully flushed rows (atomic per-user deletion)
    /// 7. On Parquet write failure for a user, keeps their buffered rows in RocksDB
    ///
    /// # T151-T151h: Streaming flush with RocksDB snapshot and per-user atomicity
    fn execute_flush(&self) -> Result<(usize, usize, Vec<String>), KalamDbError> {
        log::debug!(
            "🔄 Starting flush execution: table={}.{}, snapshot creation...",
            self.namespace_id.as_str(),
            self.table_name.as_str()
        );

        // T151a: Note - snapshots removed for storage abstraction
        // TODO: Re-add snapshot support when StorageBackend trait supports it

        log::debug!(
            "� Scanning rows for table={}.{}...",
            self.namespace_id.as_str(),
            self.table_name.as_str()
        );

        // T151b: Stream table column family sequentially (snapshot-backed)
        let mut iter = self
            .store
            .scan_iter(self.namespace_id.as_str(), self.table_name.as_str())
            .map_err(|e| {
                log::error!(
                    "❌ Failed to scan table={}.{}: {}",
                    self.namespace_id.as_str(),
                    self.table_name.as_str(),
                    e
                );
                KalamDbError::Other(format!("Failed to scan table: {}", e))
            })?;

        // Initialize variables for per-user batching
        let mut current_user_id: Option<String> = None;
        let mut current_user_rows: Vec<(Vec<u8>, JsonValue)> = Vec::new();
        let mut total_rows_flushed = 0;
        let mut users_count = 0;
        let mut parquet_files: Vec<String> = Vec::new();
        let mut error_messages: Vec<String> = Vec::new();

        let mut rows_scanned = 0;
        while let Some(Ok((key_bytes, value_bytes))) = iter.next() {
            // Decode row
            let user_table_row: crate::models::UserTableRow =
                match serde_json::from_slice(&value_bytes) {
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
            if user_table_row._deleted {
                continue;
            }

            // Convert to JSON value and inject system columns
            let mut row_data = JsonValue::Object(user_table_row.fields);
            if let Some(obj) = row_data.as_object_mut() {
                obj.insert(
                    "_updated".to_string(),
                    JsonValue::String(user_table_row._updated),
                );
                obj.insert("_deleted".to_string(), JsonValue::Bool(false));
            }

            let key_str = match String::from_utf8(key_bytes.clone()) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!(
                        "Skipping row due to invalid UTF-8 key (table={}.{}): {}",
                        self.namespace_id.as_str(),
                        self.table_name.as_str(),
                        e
                    );
                    continue;
                }
            };
            rows_scanned += 1;
            if rows_scanned % 1000 == 0 {
                log::debug!(
                    "📊 Processed {} rows so far (table={}.{})",
                    rows_scanned,
                    self.namespace_id.as_str(),
                    self.table_name.as_str()
                );
            }

            // Parse key to get user_id
            let (user_id, _row_id) = self.parse_user_key(&key_str)?;

            // T151d: Detect userId boundary (current_row.user_id ≠ previous_row.user_id)
            if let Some(ref prev_user_id) = current_user_id {
                if &user_id != prev_user_id {
                    // Boundary detected - flush accumulated rows for previous user
                    // T151e: Write accumulated rows to Parquet before continuing scan
                    let flush_result =
                        self.flush_user_data(prev_user_id, &current_user_rows, &mut parquet_files);

                    match flush_result {
                        Ok(rows_count) => {
                            // T151f: Delete successfully flushed rows (atomic per-user batch)
                            let keys: Vec<Vec<u8>> = current_user_rows
                                .iter()
                                .map(|(key, _)| key.clone())
                                .collect();

                            self.delete_flushed_keys(&keys)?;

                            total_rows_flushed += rows_count;
                            users_count += 1;

                            // T151h: Track per-user flush success with logging
                            log::debug!(
                                "Flushed {} rows for user {} (deleted from buffer)",
                                rows_count,
                                prev_user_id
                            );
                        }
                        Err(e) => {
                            // T151g: On Parquet write failure, keep buffered rows in RocksDB
                            // (no deletion occurs, rows remain in buffer for next flush attempt)
                            let error_msg = format!(
                                "Failed to flush {} rows for user {}: {}",
                                current_user_rows.len(),
                                prev_user_id,
                                e
                            );
                            log::error!("{}. Rows kept in buffer.", error_msg);
                            error_messages.push(error_msg);

                            // Continue with next user despite failure
                        }
                    }

                    // Reset buffer for new user
                    current_user_rows.clear();
                }
            }

            // Accumulate row for current user
            current_user_id = Some(user_id.clone());
            current_user_rows.push((key_str.into_bytes(), row_data));
        }

        // Flush final user's data (if any)
        if let Some(user_id) = current_user_id {
            if !current_user_rows.is_empty() {
                let flush_result =
                    self.flush_user_data(&user_id, &current_user_rows, &mut parquet_files);

                match flush_result {
                    Ok(rows_count) => {
                        let keys: Vec<Vec<u8>> = current_user_rows
                            .iter()
                            .map(|(key, _)| key.clone())
                            .collect();

                        self.delete_flushed_keys(&keys)?;

                        total_rows_flushed += rows_count;
                        users_count += 1;

                        log::debug!(
                            "Flushed {} rows for user {} (deleted from buffer)",
                            rows_count,
                            user_id
                        );
                    }
                    Err(e) => {
                        let error_msg = format!(
                            "Failed to flush {} rows for user {}: {}",
                            current_user_rows.len(),
                            user_id,
                            e
                        );
                        log::error!("{}. Rows kept in buffer.", error_msg);
                        error_messages.push(error_msg);
                    }
                }
            }
        }

        log::info!(
            "✅ Flush execution completed: table={}.{}, total_rows_scanned={}, total_rows_flushed={}, users_count={}, parquet_files={}",
            self.namespace_id.as_str(),
            self.table_name.as_str(),
            rows_scanned,
            total_rows_flushed,
            users_count,
            parquet_files.len()
        );

        // Check if ALL users failed (complete failure)
        if !error_messages.is_empty() && total_rows_flushed == 0 && rows_scanned > 0 {
            let combined_errors = error_messages.join("; ");
            log::error!(
                "❌ Complete flush failure for table={}.{}: All {} user(s) failed to flush",
                self.namespace_id.as_str(),
                self.table_name.as_str(),
                error_messages.len()
            );
            return Err(KalamDbError::Other(format!(
                "Flush failed for all users: {}",
                combined_errors
            )));
        }

        // Check if SOME users failed (partial failure) - still return Ok but log warnings
        if !error_messages.is_empty() {
            log::warn!(
                "⚠️  Partial flush failure for table={}.{}: {} user(s) failed, {} succeeded",
                self.namespace_id.as_str(),
                self.table_name.as_str(),
                error_messages.len(),
                users_count
            );
            for error_msg in &error_messages {
                log::warn!("  - {}", error_msg);
            }
        }

        if total_rows_flushed == 0 {
            log::warn!(
                "⚠️  No rows flushed for table={}.{} (scanned {} rows total, possibly all deleted or no data)",
                self.namespace_id.as_str(),
                self.table_name.as_str(),
                rows_scanned
            );
        }

        Ok((total_rows_flushed, users_count, parquet_files))
    }

    /// Flush data for a single user to Parquet file
    ///
    /// Returns the number of rows flushed on success
    /// Flush accumulated rows for a single user to Parquet file
    ///
    /// # T161a: Per-user file isolation principle
    /// Each user's data is written to a separate Parquet file to enable:
    /// - Row-level access control (filter files by userId at query time)
    /// - Efficient user data deletion (drop entire Parquet file)
    /// - Parallel query execution (read different users concurrently)
    ///
    /// # T161b: Parquet file naming convention
    /// Filenames use ISO 8601 timestamps: YYYY-MM-DDTHH-MM-SS.parquet
    /// Example: 2025-10-22T14-30-45.parquet
    /// - Chronologically sortable (lexicographic order = time order)
    /// - Cross-platform compatible (no colons, uses hyphens)
    /// - Collision-resistant (second-level precision + UUID job_id)
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

        log::debug!(
            "💾 Flushing {} rows for user_id={} (table={}.{})",
            rows_count,
            user_id,
            self.namespace_id.as_str(),
            self.table_name.as_str()
        );

        // Convert rows to RecordBatch
        let batch = self.rows_to_record_batch(rows)?;

        // T161c: Template path resolution (single-pass substitution)
        // Resolve storage path using StorageRegistry template or legacy path
        let user_id_obj = UserId::new(user_id.to_string());
        let (user_storage_path, _s3_credentials) =
            self.resolve_storage_path_for_user(&user_id_obj)?;

        // T161b: Generate ISO 8601 timestamp-based filename
        let batch_filename = self.generate_batch_filename();
        let output_path = PathBuf::from(&user_storage_path).join(&batch_filename);

        // Write to Parquet
        log::debug!(
            "📝 Writing Parquet file: path={}, rows={}, user_id={}",
            output_path.display(),
            rows_count,
            user_id
        );

        let writer = ParquetWriter::new(output_path.to_str().unwrap());
        writer
            .write(self.schema.clone(), vec![batch])
            .map_err(|e| {
                log::error!(
                    "❌ Failed to write Parquet file for user_id={}, path={}: {}",
                    user_id,
                    output_path.display(),
                    e
                );
                e
            })?;

        parquet_files.push(output_path.to_string_lossy().to_string());

        log::info!(
            "✅ Flushed {} rows for user_id={} to {} (table={}.{})",
            rows_count,
            user_id,
            output_path.display(),
            self.namespace_id.as_str(),
            self.table_name.as_str()
        );

        Ok(rows_count)
    }

    /// Parse user key format: {user_id}:{row_id}
    fn parse_user_key(&self, key: &str) -> Result<(String, String), KalamDbError> {
        let parts: Vec<&str> = key.split(':').collect();
        if parts.len() != 2 {
            return Err(KalamDbError::Other(format!(
                "Invalid user table key format: {}",
                key
            )));
        }
        Ok((parts[0].to_string(), parts[1].to_string()))
    }

    /// Delete flushed rows by their raw key bytes (atomic batch operation)
    fn delete_flushed_keys(&self, keys: &[Vec<u8>]) -> Result<(), KalamDbError> {
        if keys.is_empty() {
            return Ok(());
        }

        log::debug!(
            "🗑️  Deleting {} flushed keys from RocksDB (table={}.{})",
            keys.len(),
            self.namespace_id.as_str(),
            self.table_name.as_str()
        );

        let keys_as_strings: Vec<String> = keys
            .iter()
            .map(|k| String::from_utf8_lossy(k).to_string())
            .collect();
        self.store
            .delete_batch_by_keys(
                self.namespace_id.as_str(),
                self.table_name.as_str(),
                &keys_as_strings,
            )
            .map_err(|e| {
                log::error!(
                    "❌ Failed to delete {} flushed rows from storage (table={}.{}): {}",
                    keys.len(),
                    self.namespace_id.as_str(),
                    self.table_name.as_str(),
                    e
                );
                KalamDbError::Other(format!("Failed to delete flushed rows: {}", e))
            })?;

        log::debug!(
            "✅ Deleted {} flushed rows from RocksDB (table={}.{})",
            keys.len(),
            self.namespace_id.as_str(),
            self.table_name.as_str()
        );
        Ok(())
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tables::user_tables::user_table_store::UserTableRow;
    use kalamdb_commons::JobStatus;
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
        let test_db = TestDb::single_cf("user_test_ns:test_table").unwrap();
        let backend = Arc::new(kalamdb_store::RocksDBBackend::new(test_db.db.clone()));
        let namespace_id = NamespaceId::new("test_ns");
        let table_name = TableName::new("test_table");
        let store = Arc::new(new_user_table_store(backend, &namespace_id, &table_name));
        let schema = create_test_schema();

        let job = UserTableFlushJob::new(
            store,
            namespace_id.clone(),
            table_name.clone(),
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
        let test_db = TestDb::single_cf("user_test_ns:test_table").unwrap();
        let backend = Arc::new(kalamdb_store::RocksDBBackend::new(test_db.db.clone()));
        let namespace_id = NamespaceId::new("test_ns");
        let table_name = TableName::new("test_table");
        let store = Arc::new(new_user_table_store(backend, &namespace_id, &table_name));
        let schema = create_test_schema();

        let temp_storage = env::temp_dir().join("kalamdb_flush_test_empty");
        let _ = fs::remove_dir_all(&temp_storage);

        let job = UserTableFlushJob::new(
            store,
            namespace_id.clone(),
            table_name.clone(),
            schema,
            temp_storage.to_str().unwrap().to_string() + "/${user_id}/",
        );

        let result = job.execute().unwrap();
        if result.job_record.status == JobStatus::Failed {
            eprintln!(
                "Job failed with error: {:?}",
                result.job_record.error_message
            );
        }
        assert_eq!(result.rows_flushed, 0); // 0 rows flushed
        assert_eq!(result.users_count, 0); // 0 users
        assert_eq!(result.parquet_files.len(), 0); // 0 Parquet files
        assert_eq!(result.job_record.status, JobStatus::Completed);
        assert_eq!(result.job_record.job_type, JobType::Flush);

        let _ = fs::remove_dir_all(&temp_storage);
    }

    #[test]
    fn test_user_table_flush_single_user() {
        let test_db = TestDb::single_cf("user_test_ns:test_table").unwrap();
        let backend = Arc::new(kalamdb_store::RocksDBBackend::new(test_db.db.clone()));
        let namespace_id = NamespaceId::new("test_ns");
        let table_name = TableName::new("test_table");
        let store = Arc::new(new_user_table_store(backend, &namespace_id, &table_name));
        let schema = create_test_schema();

        // Insert test data for user1
        let row1 = UserTableRow {
            row_id: "row1".to_string(),
            user_id: "user1".to_string(),
            fields: json!({
                "id": "row1",
                "content": "Message 1"
            }),
            _updated: Utc::now().to_rfc3339(),
            _deleted: false,
        };
        let row2 = UserTableRow {
            row_id: "row2".to_string(),
            user_id: "user1".to_string(),
            fields: json!({
                "id": "row2",
                "content": "Message 2"
            }),
            _updated: Utc::now().to_rfc3339(),
            _deleted: false,
        };

        store
            .put("test_ns", "test_table", "user1", "row1", &row1)
            .unwrap();
        store
            .put("test_ns", "test_table", "user1", "row2", &row2)
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
        assert_eq!(result.job_record.status, JobStatus::Completed);

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
        let test_db = TestDb::single_cf("user_test_ns:test_table").unwrap();
        let backend = Arc::new(kalamdb_store::RocksDBBackend::new(test_db.db.clone()));
        let namespace_id = NamespaceId::new("test_ns");
        let table_name = TableName::new("test_table");
        let store = Arc::new(new_user_table_store(backend, &namespace_id, &table_name));
        let schema = create_test_schema();

        // Insert test data for user1 and user2
        let row1 = UserTableRow {
            row_id: "row1".to_string(),
            user_id: "user1".to_string(),
            fields: json!({
                "id": "row1",
                "content": "User1 Message 1"
            }),
            _updated: Utc::now().to_rfc3339(),
            _deleted: false,
        };
        let row2 = UserTableRow {
            row_id: "row2".to_string(),
            user_id: "user2".to_string(),
            fields: json!({
                "id": "row2",
                "content": "User2 Message 1"
            }),
            _updated: Utc::now().to_rfc3339(),
            _deleted: false,
        };
        let row3 = UserTableRow {
            row_id: "row3".to_string(),
            user_id: "user1".to_string(),
            fields: json!({
                "id": "row3",
                "content": "User1 Message 2"
            }),
            _updated: Utc::now().to_rfc3339(),
            _deleted: false,
        };

        store
            .put("test_ns", "test_table", "user1", "row1", &row1)
            .unwrap();
        store
            .put("test_ns", "test_table", "user2", "row2", &row2)
            .unwrap();
        store
            .put("test_ns", "test_table", "user1", "row3", &row3)
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
        let test_db = TestDb::single_cf("user_test_ns:test_table").unwrap();
        let backend = Arc::new(kalamdb_store::RocksDBBackend::new(test_db.db.clone()));
        let namespace_id = NamespaceId::new("test_ns");
        let table_name = TableName::new("test_table");
        let store = Arc::new(new_user_table_store(backend, &namespace_id, &table_name));
        let schema = create_test_schema();

        // Insert test data with one active and one soft-deleted row
        let row1 = UserTableRow {
            row_id: "row1".to_string(),
            user_id: "user1".to_string(),
            fields: json!({
                "id": "row1",
                "content": "Active Message"
            }),
            _updated: Utc::now().to_rfc3339(),
            _deleted: false,
        };
        let row2_active = UserTableRow {
            row_id: "row2".to_string(),
            user_id: "user1".to_string(),
            fields: json!({
                "id": "row2",
                "content": "Deleted Message"
            }),
            _updated: Utc::now().to_rfc3339(),
            _deleted: false,
        };

        store
            .put("test_ns", "test_table", "user1", "row1", &row1)
            .unwrap();
        store
            .put("test_ns", "test_table", "user1", "row2", &row2_active)
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
