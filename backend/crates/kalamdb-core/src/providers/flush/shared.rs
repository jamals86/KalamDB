//! Shared table flush implementation
//!
//! Flushes shared table data from RocksDB to a single Parquet file.
//! All rows are written to one file per flush operation.

use super::base::{FlushJobResult, FlushMetadata, TableFlush};
use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::error_extensions::KalamDbResultExt;
use crate::live_query::{ChangeNotification, LiveQueryManager};
use crate::manifest::{FlushManifestHelper, ManifestService};
use crate::providers::arrow_json_conversion::json_rows_to_arrow_batch;
use crate::schema_registry::SchemaRegistry;
use crate::storage::write_parquet_with_store_sync;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::scalar::ScalarValue;
use kalamdb_commons::constants::SystemColumnNames;
use kalamdb_commons::models::rows::Row;
use kalamdb_commons::models::{StorageId, TableId, UserId};
use kalamdb_tables::{SharedTableIndexedStore, SharedTableRow};
use std::sync::Arc;

/// Shared table flush job
///
/// Flushes shared table data to Parquet files. All rows are written to a single
/// Parquet file per flush operation (no user partitioning like user tables).
pub struct SharedTableFlushJob {
    store: Arc<SharedTableIndexedStore>,
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

impl SharedTableFlushJob {
    /// Create a new shared table flush job
    pub fn new(
        app_context: Arc<AppContext>,
        table_id: Arc<TableId>,
        store: Arc<SharedTableIndexedStore>,
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
                (cached.bloom_filter_columns().to_vec(), cached.indexed_columns().to_vec())
            })
            .unwrap_or_else(|| {
                log::warn!(
                    "⚠️  Table {} not in cache. Using default Bloom filter columns (_seq only)",
                    table_id
                );
                (vec![SystemColumnNames::SEQ.to_string()], vec![])
            });

        log::debug!(
            "🌸 [SharedTableFlushJob] Bloom filter columns: {:?}, indexed columns: {} entries",
            bloom_filter_columns,
            indexed_columns.len()
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

    /// Set the live query manager for this flush job (builder pattern)
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

    /// Generate batch filename using manifest max_batch (T115)
    /// Returns (batch_number, filename)
    fn generate_batch_filename(&self) -> Result<(u64, String), KalamDbError> {
        let batch_number = self.manifest_helper.get_next_batch_number(&self.table_id, None)?;
        let filename = FlushManifestHelper::generate_batch_filename(batch_number);
        log::debug!(
            "[MANIFEST] Generated batch filename: {} (batch_number={})",
            filename,
            batch_number
        );
        Ok((batch_number, filename))
    }

    /// Convert stored rows to Arrow RecordBatch without JSON round-trips
    fn rows_to_record_batch(&self, rows: &[(Vec<u8>, Row)]) -> Result<RecordBatch, KalamDbError> {
        let arrow_rows: Vec<Row> = rows.iter().map(|(_, row)| row.clone()).collect();
        json_rows_to_arrow_batch(&self.schema, arrow_rows)
            .into_kalamdb_error("Failed to build RecordBatch")
    }

    /// Delete flushed rows from RocksDB after successful Parquet write
    fn delete_flushed_rows(&self, keys: &[Vec<u8>]) -> Result<(), KalamDbError> {
        let mut parsed_keys = Vec::new();
        for key_bytes in keys {
            let key = kalamdb_commons::ids::SharedTableRowId::from_bytes(key_bytes)
                .into_invalid_operation("Invalid key bytes")?;
            parsed_keys.push(key);
        }

        if parsed_keys.is_empty() {
            return Ok(());
        }

        // Delete each key individually using IndexedEntityStore::delete
        // IMPORTANT: Must use self.store.delete() instead of EntityStore::delete()
        // to ensure both the entity AND its index entries are removed atomically.
        for key in &parsed_keys {
            self.store.delete(key).into_kalamdb_error("Failed to delete flushed row")?;
        }

        log::debug!("Deleted {} flushed rows from storage", parsed_keys.len());
        Ok(())
    }
}

impl TableFlush for SharedTableFlushJob {
    fn execute(&self) -> Result<FlushJobResult, KalamDbError> {
        log::debug!("🔄 Starting shared table flush: table={}", self.table_id);

        use super::base::{config, helpers, FlushDedupStats};
        use std::collections::HashMap;

        // Get primary key field name from schema
        let pk_field = helpers::extract_pk_field_name(&self.schema);
        log::debug!("📊 [FLUSH DEDUP] Using primary key field: {}", pk_field);

        // Map: pk_value -> (key_bytes, row, _seq)
        let mut latest_versions: HashMap<String, (Vec<u8>, SharedTableRow, i64)> = HashMap::new();
        // Track ALL keys to delete (including old versions)
        let mut all_keys_to_delete: Vec<Vec<u8>> = Vec::new();
        let mut stats = FlushDedupStats::default();

        // Batched scan with cursor
        let mut cursor: Option<Vec<u8>> = None;
        loop {
            let batch = self
                .store
                .scan_limited_with_prefix_and_start(None, cursor.as_deref(), config::BATCH_SIZE)
                .map_err(|e| {
                    log::error!("❌ Failed to scan rows for shared table={}: {}", self.table_id, e);
                    KalamDbError::Other(format!("Failed to scan rows: {}", e))
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

                // Extract PK value from fields
                let pk_value = match row.fields.get(&pk_field) {
                    Some(v) if !v.is_null() => v.to_string(),
                    _ => {
                        // No PK or null PK - use unique _seq as fallback
                        format!("_seq:{}", row._seq.as_i64())
                    },
                };

                let seq_val = row._seq.as_i64();

                // Track deleted rows
                if row._deleted {
                    stats.deleted_count += 1;
                }

                // Keep MAX(_seq) per pk_value
                match latest_versions.get(&pk_value) {
                    Some((_existing_key, _existing_row, existing_seq)) => {
                        if seq_val > *existing_seq {
                            latest_versions.insert(pk_value, (key_bytes, row, seq_val));
                        }
                    },
                    None => {
                        latest_versions.insert(pk_value, (key_bytes, row, seq_val));
                    },
                }
            }

            // Check if we got fewer rows than batch size (end of data)
            if batch_len < config::BATCH_SIZE {
                break;
            }
        }

        stats.rows_after_dedup = latest_versions.len();

        // STEP 2: Filter out deleted rows (tombstones) and convert to Rows
        let mut rows: Vec<(Vec<u8>, Row)> = Vec::new();

        for (_pk_value, (key_bytes, row, _seq)) in latest_versions {
            // Skip soft-deleted rows (tombstones)
            if row._deleted {
                stats.tombstones_filtered += 1;
                continue;
            }

            let mut row_data = row.fields.clone();
            row_data.values.insert(
                SystemColumnNames::SEQ.to_string(),
                ScalarValue::Int64(Some(row._seq.as_i64())),
            );
            row_data
                .values
                .insert(SystemColumnNames::DELETED.to_string(), ScalarValue::Boolean(Some(false)));

            rows.push((key_bytes, row_data));
        }

        // Log dedup statistics
        stats.log_summary(&self.table_id.to_string());

        // If no rows to flush, return early
        if rows.is_empty() {
            log::info!(
                "⚠️  No rows to flush for shared table={} (empty table or all deleted)",
                self.table_id
            );
            return Ok(FlushJobResult {
                rows_flushed: 0,
                parquet_files: vec![],
                metadata: FlushMetadata::shared_table(),
            });
        }

        let rows_count = rows.len();
        log::debug!(
            "💾 Flushing {} rows to Parquet for shared table={}",
            rows_count,
            self.table_id
        );

        // Convert rows to RecordBatch
        let batch = self.rows_to_record_batch(&rows)?;

        // T114-T115: Generate batch filename using manifest (sequential numbering)
        let (batch_number, batch_filename) = self.generate_batch_filename()?;
        let temp_filename = FlushManifestHelper::generate_temp_filename(batch_number);
        let full_dir = self.unified_cache.get_storage_path(
            self.app_context.as_ref(),
            &self.table_id,
            None,
            None,
        )?;

        // Use PathBuf for proper cross-platform path handling (avoids mixed slashes)
        let temp_path = std::path::Path::new(&full_dir)
            .join(&temp_filename)
            .to_string_lossy()
            .to_string();
        let destination_path = std::path::Path::new(&full_dir)
            .join(&batch_filename)
            .to_string_lossy()
            .to_string();

        let cached = self.unified_cache.get(&self.table_id).ok_or_else(|| {
            KalamDbError::TableNotFound(format!("Table not found: {}", self.table_id))
        })?;

        // Get storage from registry (cached lookup)
        let storage_id = cached.storage_id.clone().unwrap_or_else(StorageId::local);
        let storage =
            self.app_context.storage_registry().get_storage(&storage_id)?.ok_or_else(|| {
                KalamDbError::InvalidOperation(format!(
                    "Storage '{}' not found",
                    storage_id.as_str()
                ))
            })?;

        // Get cached ObjectStore instance (avoids rebuild overhead on each flush)
        let object_store = cached.object_store(self.app_context.as_ref())?;

        // Use cached bloom_filter_columns and indexed_columns (fetched once at job construction)
        // This avoids per-flush lookups and matches UserTableFlushJob optimization pattern
        let bloom_filter_columns = &self.bloom_filter_columns;
        let indexed_columns = &self.indexed_columns;

        log::debug!("🌸 Bloom filters enabled for columns: {:?}", bloom_filter_columns);

        // ===== ATOMIC FLUSH PATTERN =====
        // Step 1: Mark manifest as syncing (flush in progress)
        //         If crash occurs after this, we know a flush was in progress
        if let Err(e) = self.manifest_helper.mark_syncing(&self.table_id, None) {
            log::warn!("⚠️  Failed to mark manifest as syncing (continuing anyway): {}", e);
        }

        // Step 2: Write Parquet to TEMP location first
        log::debug!("📝 [ATOMIC] Writing Parquet to temp path: {}, rows={}", temp_path, rows_count);
        let result = write_parquet_with_store_sync(
            object_store.clone(),
            &storage,
            &temp_path,
            self.schema.clone(),
            vec![batch.clone()],
            Some(bloom_filter_columns.clone()),
        )
        .into_kalamdb_error("Filestore error")?;

        // Step 3: Rename temp file to final location (atomic operation)
        log::debug!("📝 [ATOMIC] Renaming {} -> {}", temp_path, destination_path);
        kalamdb_filestore::rename_file_sync(object_store, &storage, &temp_path, &destination_path)
            .into_kalamdb_error("Failed to rename Parquet file to final location")?;

        log::info!(
            "✅ Flushed {} rows for shared table={} to {}",
            rows_count,
            self.table_id,
            destination_path
        );

        let size_bytes = result.size_bytes;

        // Update manifest and cache using helper (with row-group stats)
        // Note: For remote storage, we don't have a local path; pass destination_path for stats
        // Phase 16: Include schema version to link Parquet file to specific schema
        let schema_version = self.get_schema_version();
        self.manifest_helper.update_manifest_after_flush(
            &self.table_id,
            kalamdb_commons::models::schemas::TableType::Shared,
            None,
            Some(&cached),
            batch_number,
            batch_filename.clone(),
            &std::path::PathBuf::from(&destination_path),
            &batch,
            size_bytes,
            &indexed_columns,
            schema_version,
        )?;

        // Delete ALL flushed rows from RocksDB (including old versions)
        log::info!(
            "📊 [FLUSH CLEANUP] Deleting {} rows from hot storage (including {} old versions)",
            all_keys_to_delete.len(),
            all_keys_to_delete.len() - rows_count
        );
        self.delete_flushed_rows(&all_keys_to_delete)?;

        // Compact RocksDB column family after flush to free space and optimize reads
        use kalamdb_store::entity_store::EntityStore;
        use kalamdb_store::Partition;
        let partition = Partition::new(self.store.partition());
        log::debug!("🔧 Compacting RocksDB column family after flush: {}", partition.name());
        if let Err(e) = self.store.backend().compact_partition(&partition) {
            log::warn!("⚠️  Failed to compact partition after flush: {}", e);
            // Non-fatal: flush succeeded, compaction is optimization
        }

        let parquet_path = destination_path;

        // Send flush notification if LiveQueryManager configured
        if let Some(manager) = &self.live_query_manager {
            let parquet_files = vec![parquet_path.clone()];
            let table_id = self.table_id.as_ref().clone();
            let notification =
                ChangeNotification::flush(table_id.clone(), rows_count, parquet_files.clone());
            let system_user = UserId::root();
            manager.notify_table_change_async(system_user, table_id, notification);
        }

        Ok(FlushJobResult {
            rows_flushed: rows_count,
            parquet_files: vec![parquet_path],
            metadata: FlushMetadata::shared_table(),
        })
    }

    fn table_identifier(&self) -> String {
        self.table_id.full_name()
    }

    fn live_query_manager(&self) -> Option<&Arc<LiveQueryManager>> {
        self.live_query_manager.as_ref()
    }
}
