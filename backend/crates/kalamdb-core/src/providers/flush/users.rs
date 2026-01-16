//! User table flush implementation
//!
//! Flushes user table data from RocksDB to Parquet files, grouping by UserId.
//! Each user's data is written to a separate Parquet file for RLS isolation.

use super::base::{FlushJobResult, FlushMetadata, TableFlush};
use crate::error::KalamDbError;
use crate::error_extensions::KalamDbResultExt;
use crate::live_query::{ChangeNotification, LiveQueryManager};
use crate::manifest::{FlushManifestHelper, ManifestService};
use crate::providers::arrow_json_conversion::json_rows_to_arrow_batch;
use crate::schema_registry::SchemaRegistry;
use crate::app_context::AppContext;
use crate::storage::write_parquet_with_store_sync;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::scalar::ScalarValue;
use kalamdb_commons::constants::SystemColumnNames;
use kalamdb_commons::models::rows::Row;
use kalamdb_commons::models::{StorageId, TableId, UserId};
use kalamdb_store::entity_store::EntityStore;
use kalamdb_tables::UserTableIndexedStore;
use std::sync::Arc;

/// User table flush job
///
/// Flushes user table data to Parquet files. Each user's data is written to a
/// separate Parquet file for RLS isolation. Uses Bloom filters on PRIMARY KEY + _seq
/// columns for efficient query pruning.
pub struct UserTableFlushJob {
    store: Arc<UserTableIndexedStore>,
    table_id: Arc<TableId>,
    schema: SchemaRef,
    unified_cache: Arc<SchemaRegistry>,
    app_context: Arc<AppContext>,
    live_query_manager: Option<Arc<LiveQueryManager>>,
    manifest_helper: FlushManifestHelper,
    /// Bloom filter columns (PRIMARY KEY + _seq) - fetched once per job for efficiency
    bloom_filter_columns: Vec<String>,
    /// Indexed columns with column_id for stats extraction (column_id, column_name)
    indexed_columns: Vec<(u64, String)>,
}

impl UserTableFlushJob {
    /// Create a new user table flush job
    pub fn new(
        app_context: Arc<AppContext>,
        table_id: Arc<TableId>,
        store: Arc<UserTableIndexedStore>,
        schema: SchemaRef,
        unified_cache: Arc<SchemaRegistry>,
        manifest_service: Arc<ManifestService>,
    ) -> Self {
        let manifest_helper = FlushManifestHelper::new(manifest_service);

        // Get cached values from CachedTableData (computed once at cache entry creation)
        // This avoids any recomputation - values are already cached in the schema registry
        let (bloom_filter_columns, indexed_columns) = unified_cache
            .get(&table_id)
            .map(|cached| {
                (
                    cached.bloom_filter_columns().to_vec(),
                    cached.indexed_columns().to_vec(),
                )
            })
            .unwrap_or_else(|| {
                log::warn!(
                    "⚠️  Table {} not in cache. Using default Bloom filter columns (_seq only)",
                    table_id
                );
                (vec![SystemColumnNames::SEQ.to_string()], vec![])
            });

        log::debug!(
            "🌸 Bloom filters enabled for columns: {:?}",
            bloom_filter_columns
        );

        Self {
            store,
            table_id,
            schema,
            unified_cache,
            app_context,
            live_query_manager: None,
            manifest_helper,
            bloom_filter_columns,
            indexed_columns,
        }
    }

    /// Set the LiveQueryManager for flush notifications (builder pattern)
    pub fn with_live_query_manager(mut self, manager: Arc<LiveQueryManager>) -> Self {
        self.live_query_manager = Some(manager);
        self
    }

    /// Get current schema version for the table
    fn get_schema_version(&self) -> u32 {
        self.unified_cache
            .get(&self.table_id)
            .map(|cached| cached.table.schema_version)
            .unwrap_or(1)
    }

    /// Resolve storage path for a specific user
    fn resolve_storage_path_for_user(&self, user_id: &UserId) -> Result<String, KalamDbError> {
        self.unified_cache.get_storage_path(
            self.app_context.as_ref(),
            &self.table_id,
            Some(user_id),
            None,
        )
    }

    /// Convert JSON rows to Arrow RecordBatch
    fn rows_to_record_batch(&self, rows: &[(Vec<u8>, Row)]) -> Result<RecordBatch, KalamDbError> {
        let arrow_rows: Vec<Row> = rows.iter().map(|(_, row)| row.clone()).collect();
        json_rows_to_arrow_batch(&self.schema, arrow_rows)
            .into_kalamdb_error("Failed to build RecordBatch")
    }

    /// Flush accumulated rows for a single user to Parquet
    fn flush_user_data(
        &self,
        user_id: &str,
        rows: &[(Vec<u8>, Row)],
        parquet_files: &mut Vec<String>,
        bloom_filter_columns: &[String],
        indexed_columns: &[(u64, String)],
    ) -> Result<usize, KalamDbError> {
        if rows.is_empty() {
            return Ok(0);
        }

        let rows_count = rows.len();
        log::debug!(
            "💾 Flushing {} rows for user {} (table={})",
            rows_count,
            user_id,
            self.table_id
        );

        // Convert rows to RecordBatch
        let batch = self.rows_to_record_batch(rows)?;

        // Resolve storage path for this user
        let user_id_typed = UserId::new(user_id.to_string());
        let storage_path = self.resolve_storage_path_for_user(&user_id_typed)?;

        // RLS ASSERTION: Verify storage path contains user_id
        if !storage_path.contains(user_id) {
            log::error!(
                "🚨 RLS VIOLATION: Flush storage path does NOT contain user_id! user={}, path={}",
                user_id,
                storage_path
            );
            return Err(KalamDbError::Other(format!(
                "RLS violation: flush path missing user_id isolation for user {}",
                user_id
            )));
        }

        // Generate batch filename using manifest
        let user_id_typed = kalamdb_commons::models::UserId::from(user_id);
        let batch_number = self.manifest_helper
            .get_next_batch_number(&self.table_id, Some(&user_id_typed))?;
        let batch_filename = FlushManifestHelper::generate_batch_filename(batch_number);
        let temp_filename = FlushManifestHelper::generate_temp_filename(batch_number);
        
        // Use PathBuf for proper cross-platform path handling (avoids mixed slashes)
        let temp_path = std::path::Path::new(&storage_path)
            .join(&temp_filename)
            .to_string_lossy()
            .to_string();
        let destination_path = std::path::Path::new(&storage_path)
            .join(&batch_filename)
            .to_string_lossy()
            .to_string();

        let cached = self
            .unified_cache
            .get(&self.table_id)
            .ok_or_else(|| KalamDbError::TableNotFound(format!("Table not found: {}", self.table_id)))?;

        // Get storage from registry (cached lookup)
        let storage_id = cached.storage_id.clone().unwrap_or_else(StorageId::local);
        let storage = self
            .app_context
            .storage_registry()
            .get_storage(&storage_id)?
            .ok_or_else(|| {
                KalamDbError::InvalidOperation(format!(
                    "Storage '{}' not found",
                    storage_id.as_str()
                ))
            })?;

        // Get cached ObjectStore instance (avoids rebuild overhead on each flush)
        let object_store = cached.object_store(self.app_context.as_ref())?;

        // ===== ATOMIC FLUSH PATTERN =====
        // Step 1: Mark manifest as syncing (flush in progress)
        if let Err(e) = self.manifest_helper.mark_syncing(&self.table_id, Some(&user_id_typed)) {
            log::warn!("⚠️  Failed to mark manifest as syncing for user {}: {} (continuing)", user_id, e);
        }

        // Step 2: Write Parquet to TEMP location first
        log::debug!(
            "📝 [ATOMIC] Writing Parquet to temp path: {}, rows={}",
            temp_path,
            rows_count
        );
        let result = write_parquet_with_store_sync(
            object_store.clone(),
            &storage,
            &temp_path,
            self.schema.clone(),
            vec![batch.clone()],
            Some(bloom_filter_columns.to_vec()),
        )
        .into_kalamdb_error("Filestore error")?;

        // Step 3: Rename temp file to final location (atomic operation)
        log::debug!(
            "📝 [ATOMIC] Renaming {} -> {}",
            temp_path,
            destination_path
        );
        kalamdb_filestore::rename_file_sync(
            object_store,
            &storage,
            &temp_path,
            &destination_path,
        )
        .into_kalamdb_error("Failed to rename Parquet file to final location")?;

        let size_bytes = result.size_bytes;

        // Update manifest and cache using helper (with row-group stats)
        // Note: For remote storage, we don't have a local path; pass destination_path for stats
        // Phase 16: Include schema version to link Parquet file to specific schema
        let schema_version = self.get_schema_version();
        self.manifest_helper.update_manifest_after_flush(
            &self.table_id,
            kalamdb_commons::models::schemas::TableType::User,
            Some(&user_id_typed),
            Some(&cached),
            batch_number,
            batch_filename.clone(),
            &std::path::PathBuf::from(&destination_path),
            &batch,
            size_bytes,
            indexed_columns,
            schema_version,
        )?;

        log::debug!(
            "✅ Flushed {} rows for user {} to {} (batch={})",
            rows_count,
            user_id,
            destination_path,
            batch_number
        );

        parquet_files.push(destination_path);
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
                .into_invalid_operation("Invalid key bytes")?;
            parsed_keys.push(key);
        }

        // Delete each key individually using IndexedEntityStore::delete
        // IMPORTANT: Must use self.store.delete() instead of EntityStore::delete()
        // to ensure both the entity AND its index entries are removed atomically.
        for key in &parsed_keys {
            self.store.delete(key)
                .into_kalamdb_error("Failed to delete flushed row")?;
        }

        log::debug!("Deleted {} flushed rows from storage", keys.len());
        Ok(())
    }
}

impl TableFlush for UserTableFlushJob {
    fn execute(&self) -> Result<FlushJobResult, KalamDbError> {
        log::debug!(
            "🔄 Starting user table flush: table={}, partition={}",
            self.table_id,
            self.store.partition()
        );

        use super::base::{config, helpers, FlushDedupStats};
        use std::collections::HashMap;

        // Get primary key field name from schema
        let pk_field = helpers::extract_pk_field_name(&self.schema);
        log::debug!("📊 [FLUSH DEDUP] Using primary key field: {}", pk_field);

        // Map: (user_id, pk_value) -> (key_bytes, row, _seq)
        let mut latest_versions: HashMap<
            (String, String),
            (Vec<u8>, kalamdb_tables::UserTableRow, i64),
        > = HashMap::new();
        // Track ALL keys to delete (including old versions)
        // Pre-allocate with reasonable capacity to reduce reallocations during scan
        let mut all_keys_to_delete: Vec<Vec<u8>> = Vec::with_capacity(1024);
        let mut stats = FlushDedupStats::default();

        // Batched scan with cursor
        let mut cursor: Option<Vec<u8>> = None;
        loop {
            let batch = self
                .store
                .scan_limited_with_prefix_and_start(None, cursor.as_deref(), config::BATCH_SIZE)
                .map_err(|e| {
                    log::error!(
                        "❌ Failed to scan table={}: {}",
                        self.table_id,
                        e
                    );
                    KalamDbError::Other(format!("Failed to scan table: {}", e))
                })?;

            if batch.is_empty() {
                break;
            }

            log::trace!(
                "[FLUSH] Processing batch of {} rows (cursor={:?})",
                batch.len(),
                cursor.as_ref().map(|c| c.len())
            );

            // Update cursor for next batch
            cursor = batch.last().map(|(key, _)| helpers::advance_cursor(key));

            let batch_len = batch.len();
            stats.rows_before_dedup += batch_len;

            for (key_bytes, row) in batch {
                // Track ALL keys for deletion (before dedup)
                all_keys_to_delete.push(key_bytes.clone());

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
                    stats.deleted_count += 1;
                }

                // Keep MAX(_seq) per (user_id, pk_value)
                match latest_versions.get(&group_key) {
                    Some((_existing_key, _existing_row, existing_seq)) => {
                        if seq_val > *existing_seq {
                            log::trace!("[FLUSH DEDUP] Replacing user={}, pk={}: old_seq={}, new_seq={}, deleted={}",
                                       user_id, pk_value, existing_seq, seq_val, row._deleted);
                            latest_versions.insert(group_key, (key_bytes, row, seq_val));
                        }
                    }
                    None => {
                        latest_versions.insert(group_key, (key_bytes, row, seq_val));
                    }
                }
            }

            // Check if we got fewer rows than batch size (end of data)
            if batch_len < config::BATCH_SIZE {
                break;
            }
        }

        stats.rows_after_dedup = latest_versions.len();

        // STEP 2: Filter out deleted rows (tombstones)
        let mut rows_by_user: HashMap<String, Vec<(Vec<u8>, Row)>> = HashMap::new();

        for ((user_id, _pk_value), (key_bytes, row, _seq)) in latest_versions {
            // Skip soft-deleted rows (tombstones)
            if row._deleted {
                stats.tombstones_filtered += 1;
                continue;
            }

            // Convert to JSON and inject system columns
            let mut row_data = row.fields.clone();
            row_data.values.insert(
                SystemColumnNames::SEQ.to_string(),
                ScalarValue::Int64(Some(row._seq.as_i64())),
            );
            row_data
                .values
                .insert(SystemColumnNames::DELETED.to_string(), ScalarValue::Boolean(Some(false)));

            rows_by_user
                .entry(user_id)
                .or_default()
                .push((key_bytes, row_data));
        }

        // Log dedup statistics
        stats.log_summary(&self.table_id.to_string());
        let rows_to_flush = rows_by_user.values().map(|v| v.len()).sum::<usize>();
        log::debug!(
            "📊 [FLUSH USER] Partitioned into {} users, {} rows to flush",
            rows_by_user.len(),
            rows_to_flush
        );

        // If no rows to flush, return early
        if rows_by_user.is_empty() {
            log::debug!(
                "⚠️  No rows to flush for user table={} (empty table or all deleted)",
                self.table_id
            );
            return Ok(FlushJobResult {
                rows_flushed: 0,
                parquet_files: vec![],
                metadata: FlushMetadata::user_table(0, vec![]),
            });
        }

        // Flush each user's data to separate Parquet file
        let mut parquet_files: Vec<String> = Vec::new();
        let mut total_rows_flushed = 0;
        let mut error_messages: Vec<String> = Vec::new();
        let mut flush_succeeded = true;

        for (user_id, rows) in &rows_by_user {
            match self.flush_user_data(
                user_id,
                rows,
                &mut parquet_files,
                &self.bloom_filter_columns,
                &self.indexed_columns,
            ) {
                Ok(rows_count) => {
                    total_rows_flushed += rows_count;
                }
                Err(e) => {
                    let error_msg = format!(
                        "Failed to flush {} rows for user {}: {}",
                        rows.len(),
                        user_id,
                        e
                    );
                    log::error!("{}. Rows kept in buffer.", error_msg);
                    error_messages.push(error_msg);
                    flush_succeeded = false;
                }
            }
        }

        // Only delete ALL rows (including old versions) if ALL users flushed successfully
        if flush_succeeded {
            log::debug!(
                "📊 [FLUSH CLEANUP] Deleting {} rows from hot storage (including {} old versions)",
                all_keys_to_delete.len(),
                all_keys_to_delete.len() - rows_by_user.values().map(|v| v.len()).sum::<usize>()
            );
            if let Err(e) = self.delete_flushed_keys(&all_keys_to_delete) {
                log::error!("Failed to delete flushed rows: {}", e);
                error_messages.push(format!("Failed to delete flushed rows: {}", e));
            }
        }

        // If any user flush failed, treat entire job as failed
        if !error_messages.is_empty() {
            let summary = format!(
                "One or more user partitions failed to flush ({} errors). Rows flushed before failure: {}. First error: {}",
                error_messages.len(), total_rows_flushed,
                error_messages.first().cloned().unwrap_or_else(|| "unknown error".to_string())
            );
            log::error!(
                "❌ User table flush failed: table={} — {}",
                self.table_id,
                summary
            );
            return Err(KalamDbError::Other(summary));
        }

        log::info!("✅ User table flush completed: table={}, rows_flushed={}, users_count={}, parquet_files={}",
                  self.table_id,
                  total_rows_flushed, rows_by_user.len(), parquet_files.len());

        // Send flush notification if LiveQueryManager configured
        if let Some(manager) = &self.live_query_manager {
            let table_id = self.table_id.as_ref().clone();
            let notification = ChangeNotification::flush(
                table_id.clone(),
                total_rows_flushed,
                parquet_files.clone(),
            );
            let root_user = UserId::root();
            manager.notify_table_change_async(root_user, table_id, notification);
        }

        Ok(FlushJobResult {
            rows_flushed: total_rows_flushed,
            parquet_files,
            metadata: FlushMetadata::user_table(rows_by_user.len(), error_messages),
        })
    }

    fn table_identifier(&self) -> String {
        self.table_id.full_name()
    }

    fn live_query_manager(&self) -> Option<&Arc<LiveQueryManager>> {
        self.live_query_manager.as_ref()
    }
}
