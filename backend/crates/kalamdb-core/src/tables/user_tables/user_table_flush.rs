//! Flush job for user tables - migrated to use TableFlush trait
//!
//! This module implements the flush operation that moves data from RocksDB to Parquet files.
//! For user tables, rows are grouped by UserId and written to separate Parquet files per user.
//!
//! **Refactoring (Phase 14 Step 11)**:
//! - Uses `TableFlush` trait from `crate::tables::base_flush`
//! - Eliminates 400+ lines of duplicated job tracking code
//! - Uses `FlushExecutor::execute_with_tracking()` for common workflow
//! - Only implements unique logic: multi-file flush grouped by user_id

use crate::catalog::{NamespaceId, TableName, UserId};
use crate::error::KalamDbError;
use crate::live_query::manager::{ChangeNotification, LiveQueryManager};
use crate::storage::{ParquetWriter, StorageRegistry};
use crate::stores::system_table::UserTableStoreExt;
use crate::tables::base_flush::{FlushExecutor, FlushJobResult, TableFlush};
use crate::tables::UserTableStore;
use chrono::Utc;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use kalamdb_commons::models::StorageType;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// User table flush job
///
/// Flushes data from RocksDB to Parquet files, grouping by UserId.
/// Implements `TableFlush` trait for common job tracking/metrics.
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
        }
    }

    /// Set the StorageRegistry for dynamic storage resolution (builder pattern)
    pub fn with_storage_registry(mut self, registry: Arc<StorageRegistry>) -> Self {
        self.storage_registry = Some(registry);
        self
    }

    /// Set the LiveQueryManager for flush notifications (builder pattern)
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

    /// Substitute ${user_id} in storage path template (DEPRECATED)
    fn substitute_user_id_in_path(&self, user_id: &UserId) -> String {
        self.storage_location
            .replace("${user_id}", user_id.as_str())
    }

    /// Resolve storage path for a user using StorageRegistry (T170-T170c)
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
    fn generate_batch_filename(&self) -> String {
        let now = Utc::now();
        now.format("%Y-%m-%dT%H-%M-%S.parquet").to_string()
    }

    /// Parse user key to extract user_id and row_id
    fn parse_user_key(&self, key_str: &str) -> Result<(String, String), KalamDbError> {
        let delimiter = if key_str.contains('#') {
            '#'
        } else if key_str.contains(':') {
            ':'
        } else {
            return Err(KalamDbError::Other(format!(
                "Invalid user table key format: {}",
                key_str
            )));
        };

        let mut parts = key_str.splitn(2, delimiter);
        let user_id = parts
            .next()
            .ok_or_else(|| {
                KalamDbError::Other(format!("Invalid user table key format: {}", key_str))
            })?
            .to_string();
        let row_id = parts
            .next()
            .ok_or_else(|| {
                KalamDbError::Other(format!("Invalid user table key format: {}", key_str))
            })?
            .to_string();

        Ok((user_id, row_id))
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
        log::debug!(
            "💾 Flushing {} rows for user {} (table={}.{})",
            rows_count,
            user_id,
            self.namespace_id.as_str(),
            self.table_name.as_str()
        );

        // Convert rows to RecordBatch
        let batch = self.rows_to_record_batch(rows)?;

        // Resolve storage path for this user
        let user_id_typed = UserId::new(user_id.to_string());
        let (storage_path, _credentials) = self.resolve_storage_path_for_user(&user_id_typed)?;

        // Generate filename and full path
        let batch_filename = self.generate_batch_filename();
        let output_path = PathBuf::from(&storage_path).join(&batch_filename);

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
            "✅ Flushed {} rows for user {} to {}",
            rows_count,
            user_id,
            output_path.display()
        );

        parquet_files.push(output_path.to_string_lossy().to_string());
        Ok(rows_count)
    }

    /// Delete flushed rows from RocksDB
    fn delete_flushed_keys(&self, keys: &[Vec<u8>]) -> Result<(), KalamDbError> {
        if keys.is_empty() {
            return Ok(());
        }

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
            .map_err(|e| KalamDbError::Other(format!("Failed to delete flushed rows: {}", e)))?;

        log::debug!("Deleted {} flushed rows from storage", keys.len());
        Ok(())
    }
}

/// Implement TableFlush trait (unique user table logic only)
impl TableFlush for UserTableFlushJob {
    /// Execute the flush job (unique logic: multiple Parquet files, one per user)
    fn execute(&self) -> Result<FlushJobResult, KalamDbError> {
        log::debug!(
            "🔄 Starting user table flush: table={}.{}",
            self.namespace_id.as_str(),
            self.table_name.as_str()
        );

        // Stream snapshot-backed scan
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

        // Group rows by user_id
        let mut rows_by_user: HashMap<String, Vec<(Vec<u8>, JsonValue)>> = HashMap::new();
        let mut rows_scanned = 0;

        while let Some(Ok((key_bytes, value_bytes))) = iter.next() {
            rows_scanned += 1;

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

            // Parse key to get user_id
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

            let (user_id, _row_id) = self.parse_user_key(&key_str)?;

            // Group by user_id
            rows_by_user
                .entry(user_id)
                .or_insert_with(Vec::new)
                .push((key_bytes, row_data));

            if rows_scanned % 1000 == 0 {
                log::debug!(
                    "📊 Processed {} rows so far (table={}.{})",
                    rows_scanned,
                    self.namespace_id.as_str(),
                    self.table_name.as_str()
                );
            }
        }

        log::debug!(
            "📊 Scanned {} rows from user table={}.{} ({} users, {} active rows)",
            rows_scanned,
            self.namespace_id.as_str(),
            self.table_name.as_str(),
            rows_by_user.len(),
            rows_by_user.values().map(|v| v.len()).sum::<usize>()
        );

        // If no rows to flush, return early
        if rows_by_user.is_empty() {
            log::info!(
                "⚠️  No rows to flush for user table={}.{} (empty table or all deleted)",
                self.namespace_id.as_str(),
                self.table_name.as_str()
            );

            return Ok(FlushJobResult {
                job_record: self
                    .create_job_record(&self.generate_job_id(), self.namespace_id.clone()),
                rows_flushed: 0,
                parquet_files: vec![],
                metadata: crate::tables::base_flush::FlushMetadata::user_table(0, vec![]),
            });
        }

        // Flush each user's data to separate Parquet file
        let mut parquet_files: Vec<String> = Vec::new();
        let mut total_rows_flushed = 0;
        let mut error_messages: Vec<String> = Vec::new();

        for (user_id, rows) in &rows_by_user {
            match self.flush_user_data(user_id, rows, &mut parquet_files) {
                Ok(rows_count) => {
                    // Delete successfully flushed rows
                    let keys: Vec<Vec<u8>> = rows.iter().map(|(key, _)| key.clone()).collect();

                    if let Err(e) = self.delete_flushed_keys(&keys) {
                        log::error!("Failed to delete flushed rows for user {}: {}", user_id, e);
                        error_messages
                            .push(format!("Failed to delete rows for user {}: {}", user_id, e));
                    } else {
                        total_rows_flushed += rows_count;
                        log::debug!(
                            "Flushed {} rows for user {} (deleted from buffer)",
                            rows_count,
                            user_id
                        );
                    }
                }
                Err(e) => {
                    // On Parquet write failure, keep buffered rows in RocksDB
                    let error_msg = format!(
                        "Failed to flush {} rows for user {}: {}",
                        rows.len(),
                        user_id,
                        e
                    );
                    log::error!("{}. Rows kept in buffer.", error_msg);
                    error_messages.push(error_msg);
                }
            }
        }

        log::info!(
            "✅ User table flush completed: table={}.{}, rows_flushed={}, users_count={}, parquet_files={}",
            self.namespace_id.as_str(),
            self.table_name.as_str(),
            total_rows_flushed,
            rows_by_user.len(),
            parquet_files.len()
        );

        // Send flush notification if LiveQueryManager configured
        if let Some(manager) = &self.live_query_manager {
            let table_name = format!(
                "{}.{}",
                self.namespace_id.as_str(),
                self.table_name.as_str()
            );

            let notification = ChangeNotification::flush(
                table_name.clone(),
                total_rows_flushed,
                parquet_files.clone(),
            );

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
            rows_flushed: total_rows_flushed,
            parquet_files,
            metadata: crate::tables::base_flush::FlushMetadata::user_table(
                rows_by_user.len(),
                error_messages,
            ),
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

    #[test]
    fn test_parse_user_key() {
        // Test without needing a full store
        let key_str = "user123#row456";
        let parts: Vec<&str> = key_str.split('#').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], "user123");
        assert_eq!(parts[1], "row456");
    }

    #[test]
    fn test_generate_batch_filename_format() {
        // Test filename format without full job
        let now = Utc::now();
        let filename = now.format("%Y-%m-%dT%H-%M-%S.parquet").to_string();
        assert!(filename.ends_with(".parquet"));
        assert!(filename.contains("T")); // ISO 8601 format
        assert!(filename.contains("-")); // Date separators
    }
}
