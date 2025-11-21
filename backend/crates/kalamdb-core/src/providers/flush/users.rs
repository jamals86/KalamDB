//! User table flush implementation
//!
//! Flushes user table data from RocksDB to Parquet files, grouping by UserId.
//! Each user's data is written to a separate Parquet file for RLS isolation.

use super::base::{FlushJobResult, FlushMetadata, TableFlush};
use crate::error::KalamDbError;
use crate::live_query::{ChangeNotification, LiveQueryManager};
use crate::manifest::{FlushManifestHelper, ManifestCacheService, ManifestService};
use crate::providers::arrow_json_conversion::json_rows_to_arrow_batch;
use crate::schema_registry::SchemaRegistry;
use crate::storage::ParquetWriter;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::scalar::ScalarValue;
use kalamdb_commons::models::{Row, TableId, UserId};
use kalamdb_store::entity_store::EntityStore;
use kalamdb_tables::UserTableStore;
use std::path::PathBuf;
use std::sync::Arc;

/// User table flush job
pub struct UserTableFlushJob {
    store: Arc<UserTableStore>,
    table_id: Arc<TableId>,
    schema: SchemaRef,                                 //TODO: needed?
    unified_cache: Arc<SchemaRegistry>,                //TODO: wE HAVE APPCONTEXT NOW
    live_query_manager: Option<Arc<LiveQueryManager>>, //TODO: We can pass AppContext and has live_query_manager there
    manifest_helper: FlushManifestHelper,
    /// Bloom filter columns (PRIMARY KEY + _seq) - fetched once per job for efficiency
    bloom_filter_columns: Vec<String>,
}

impl UserTableFlushJob {
    /// Create a new user table flush job
    pub fn new(
        table_id: Arc<TableId>,
        store: Arc<UserTableStore>,
        schema: SchemaRef,
        unified_cache: Arc<SchemaRegistry>,
        manifest_service: Arc<ManifestService>,
        manifest_cache: Arc<ManifestCacheService>,
    ) -> Self {
        let manifest_helper = FlushManifestHelper::new(manifest_service, manifest_cache);

        // Fetch Bloom filter columns once per job (PRIMARY KEY + _seq)
        // This avoids fetching TableDefinition for each user during flush
        let bloom_filter_columns = unified_cache
            .get_bloom_filter_columns(&table_id)
            .unwrap_or_else(|e| {
                log::warn!(
                    "⚠️  Failed to get Bloom filter columns for {}: {}. Using default (_seq only)",
                    table_id,
                    e
                );
                vec!["_seq".to_string()]
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
            live_query_manager: None,
            manifest_helper,
            bloom_filter_columns,
        }
    }

    /// Set the LiveQueryManager for flush notifications (builder pattern)
    pub fn with_live_query_manager(mut self, manager: Arc<LiveQueryManager>) -> Self {
        self.live_query_manager = Some(manager);
        self
    }

    fn namespace_id(&self) -> &kalamdb_commons::NamespaceId {
        self.table_id.namespace_id()
    }

    fn table_name(&self) -> &kalamdb_commons::TableName {
        self.table_id.table_name()
    }

    /// Resolve storage path for a specific user
    fn resolve_storage_path_for_user(&self, user_id: &UserId) -> Result<String, KalamDbError> {
        Ok(self
            .unified_cache
            .get_storage_path(&*self.table_id, Some(user_id), None)?)
    }

    /// Convert JSON rows to Arrow RecordBatch
    fn rows_to_record_batch(&self, rows: &[(Vec<u8>, Row)]) -> Result<RecordBatch, KalamDbError> {
        let arrow_rows: Vec<Row> = rows.iter().map(|(_, row)| row.clone()).collect();
        json_rows_to_arrow_batch(&self.schema, arrow_rows)
            .map_err(|e| KalamDbError::Other(format!("Failed to build RecordBatch: {}", e)))
    }

    /// Flush accumulated rows for a single user to Parquet
    fn flush_user_data(
        &self,
        user_id: &str,
        rows: &[(Vec<u8>, Row)],
        parquet_files: &mut Vec<String>,
        bloom_filter_columns: &[String],
    ) -> Result<usize, KalamDbError> {
        if rows.is_empty() {
            return Ok(0);
        }

        let rows_count = rows.len();
        log::debug!(
            "💾 Flushing {} rows for user {} (table={}.{})",
            rows_count,
            user_id,
            self.namespace_id().as_str(),
            self.table_name().as_str()
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
        let batch_number = self.manifest_helper.get_next_batch_number(
            self.namespace_id(),
            self.table_name(),
            Some(&user_id_typed),
        )?;
        let batch_filename = FlushManifestHelper::generate_batch_filename(batch_number);
        let output_path = PathBuf::from(&storage_path).join(&batch_filename);

        // Ensure directory exists
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| KalamDbError::Other(format!("Failed to create directory: {}", e)))?;
        }

        // Write to Parquet with Bloom filters on PRIMARY KEY + _seq (FR-054, FR-055)
        log::debug!(
            "📝 Writing Parquet file: path={}, rows={}",
            output_path.display(),
            rows_count
        );
        let writer = ParquetWriter::new(output_path.to_str().unwrap());
        writer.write_with_bloom_filter(
            self.schema.clone(),
            vec![batch.clone()],
            Some(bloom_filter_columns.to_vec()),
        )?;

        // Calculate file size
        let size_bytes = std::fs::metadata(&output_path)
            .map(|m| m.len())
            .unwrap_or(0);

        // Update manifest and cache using helper (with row-group stats)
        self.manifest_helper.update_manifest_after_flush(
            self.namespace_id(),
            self.table_name(),
            kalamdb_commons::models::schemas::TableType::User,
            Some(&user_id_typed),
            batch_number,
            batch_filename.clone(),
            &output_path,
            &batch,
            size_bytes,
            bloom_filter_columns,
        )?;

        log::info!(
            "✅ Flushed {} rows for user {} to {} (batch={})",
            rows_count,
            user_id,
            output_path.display(),
            batch_number
        );

        parquet_files.push(output_path.to_string_lossy().to_string());
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
                .map_err(|e| KalamDbError::InvalidOperation(format!("Invalid key bytes: {}", e)))?;
            parsed_keys.push(key);
        }

        // Delete each key individually (no batch_delete in EntityStore trait)
        for key in &parsed_keys {
            EntityStore::delete(self.store.as_ref(), key)
                .map_err(|e| KalamDbError::Other(format!("Failed to delete flushed row: {}", e)))?;
        }

        log::debug!("Deleted {} flushed rows from storage", keys.len());
        Ok(())
    }
}

impl TableFlush for UserTableFlushJob {
    fn execute(&self) -> Result<FlushJobResult, KalamDbError> {
        log::debug!(
            "🔄 Starting user table flush: table={}.{}, partition={}",
            self.namespace_id().as_str(),
            self.table_name().as_str(),
            self.store.partition()
        );

        // Scan all rows (EntityStore::scan_all returns Vec<(Vec<u8>, V)>)
        let entries =
            EntityStore::scan_all(self.store.as_ref(), None, None, None).map_err(|e| {
                log::error!(
                    "❌ Failed to scan table={}.{}: {}",
                    self.namespace_id().as_str(),
                    self.table_name().as_str(),
                    e
                );
                KalamDbError::Other(format!("Failed to scan table: {}", e))
            })?;

        let rows_before_dedup = entries.len();
        log::info!(
            "📊 [FLUSH DEDUP] Scanned {} total rows from hot storage (table={}.{})",
            rows_before_dedup,
            self.namespace_id().as_str(),
            self.table_name().as_str()
        );

        // STEP 1: Deduplicate using MAX(_seq) per PK (version resolution)
        // Group by (user_id, primary_key) and keep only the latest version
        use std::collections::HashMap;

        // Get primary key field name from schema
        let pk_field = self
            .schema
            .fields()
            .iter()
            .find(|f| !f.name().starts_with('_'))
            .map(|f| f.name().clone())
            .unwrap_or_else(|| "id".to_string());

        log::debug!("📊 [FLUSH DEDUP] Using primary key field: {}", pk_field);

        // Map: (user_id, pk_value) -> (key_bytes, row, _seq)
        let mut latest_versions: HashMap<
            (String, String),
            (Vec<u8>, kalamdb_tables::UserTableRow, i64),
        > = HashMap::new();
        let mut deleted_count = 0;

        for (key_bytes, row) in entries {
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
                deleted_count += 1;
            }

            // Keep MAX(_seq) per (user_id, pk_value)
            match latest_versions.get(&group_key) {
                Some((_existing_key, _existing_row, existing_seq)) => {
                    if seq_val > *existing_seq {
                        log::trace!("[FLUSH DEDUP] Replacing user={}, pk={}: old_seq={}, new_seq={}, deleted={}",
                                   user_id, pk_value, existing_seq, seq_val, row._deleted);
                        latest_versions.insert(group_key, (key_bytes, row, seq_val));
                    } else {
                        log::trace!("[FLUSH DEDUP] Keeping existing user={}, pk={}: existing_seq={} >= new_seq={}",
                                   user_id, pk_value, existing_seq, seq_val);
                    }
                }
                None => {
                    log::trace!(
                        "[FLUSH DEDUP] First version user={}, pk={}: _seq={}, deleted={}",
                        user_id,
                        pk_value,
                        seq_val,
                        row._deleted
                    );
                    latest_versions.insert(group_key, (key_bytes, row, seq_val));
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

        // STEP 2: Filter out deleted rows (tombstones)
        let mut rows_by_user: HashMap<String, Vec<(Vec<u8>, Row)>> = HashMap::new();
        let mut tombstones_filtered = 0;

        for ((user_id, _pk_value), (key_bytes, row, _seq)) in latest_versions {
            // Skip soft-deleted rows (tombstones)
            if row._deleted {
                tombstones_filtered += 1;
                continue;
            }

            // Convert to JSON and inject system columns
            let mut row_data = row.fields.clone();
            row_data.values.insert(
                "_seq".to_string(),
                ScalarValue::Int64(Some(row._seq.as_i64())),
            );
            row_data.values.insert(
                "_deleted".to_string(),
                ScalarValue::Boolean(Some(false)),
            );

            rows_by_user
                .entry(user_id)
                .or_default()
                .push((key_bytes, row_data));
        }

        let rows_to_flush = rows_by_user.values().map(|v| v.len()).sum::<usize>();
        log::info!(
            "📊 [FLUSH DEDUP] Final: {} rows to flush ({} users, {} tombstones filtered)",
            rows_to_flush,
            rows_by_user.len(),
            tombstones_filtered
        );

        // If no rows to flush, return early
        if rows_by_user.is_empty() {
            log::info!(
                "⚠️  No rows to flush for user table={}.{} (empty table or all deleted)",
                self.namespace_id().as_str(),
                self.table_name().as_str()
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

        for (user_id, rows) in &rows_by_user {
            match self.flush_user_data(
                user_id,
                rows,
                &mut parquet_files,
                &self.bloom_filter_columns,
            ) {
                Ok(rows_count) => {
                    let keys: Vec<Vec<u8>> = rows.iter().map(|(key, _)| key.clone()).collect();
                    if let Err(e) = self.delete_flushed_keys(&keys) {
                        log::error!("Failed to delete flushed rows for user {}: {}", user_id, e);
                        error_messages
                            .push(format!("Failed to delete rows for user {}: {}", user_id, e));
                    } else {
                        total_rows_flushed += rows_count;
                    }
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
                }
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
                "❌ User table flush failed: table={}.{} — {}",
                self.namespace_id().as_str(),
                self.table_name().as_str(),
                summary
            );
            return Err(KalamDbError::Other(summary));
        }

        log::info!("✅ User table flush completed: table={}.{}, rows_flushed={}, users_count={}, parquet_files={}",
                  self.namespace_id().as_str(), self.table_name().as_str(),
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
        format!(
            "{}.{}",
            self.namespace_id().as_str(),
            self.table_name().as_str()
        )
    }

    fn live_query_manager(&self) -> Option<&Arc<LiveQueryManager>> {
        self.live_query_manager.as_ref()
    }
}
