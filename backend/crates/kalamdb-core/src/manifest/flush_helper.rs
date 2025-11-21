//! Flush Helper for Manifest Operations
//!
//! Centralizes manifest-related logic used during flush operations to eliminate
//! code duplication between user and shared table flush implementations.

use super::{ManifestCacheService, ManifestService};
use crate::error::KalamDbError;
use datafusion::arrow::array::*;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use kalamdb_commons::types::{BatchFileEntry, ManifestFile, RowGroupPruningStats};
use kalamdb_commons::{NamespaceId, TableId, TableName, UserId};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// Helper for manifest operations during flush
pub struct FlushManifestHelper {
    manifest_service: Arc<ManifestService>,
    manifest_cache: Arc<ManifestCacheService>,
}

impl FlushManifestHelper {
    /// Create a new FlushManifestHelper
    pub fn new(
        manifest_service: Arc<ManifestService>,
        manifest_cache: Arc<ManifestCacheService>,
    ) -> Self {
        Self {
            manifest_service,
            manifest_cache,
        }
    }

    /// Get next batch number for a table/scope by reading manifest
    ///
    /// Returns 0 if no manifest exists (first batch)
    pub fn get_next_batch_number(
        &self,
        namespace: &NamespaceId,
        table: &TableName,
        user_id: Option<&UserId>,
    ) -> Result<u64, KalamDbError> {
        let table_id = TableId::new(namespace.clone(), table.clone());
        match self.manifest_service.read_manifest(&table_id, user_id) {
            Ok(manifest) => Ok(manifest.max_batch + 1),
            Err(_) => Ok(0), // No manifest exists yet, start with batch 0
        }
    }

    /// Generate batch filename from batch number
    pub fn generate_batch_filename(batch_number: u64) -> String {
        format!("batch-{}.parquet", batch_number)
    }

    /// Extract min/max _seq values from RecordBatch
    ///
    /// Returns (min_seq, max_seq) tuple
    pub fn extract_seq_range(batch: &RecordBatch) -> (i64, i64) {
        let seq_column = batch
            .column_by_name("_seq")
            .and_then(|col| col.as_any().downcast_ref::<Int64Array>());

        if let Some(seq_arr) = seq_column {
            let min = seq_arr.iter().flatten().min().unwrap_or(0);
            let max = seq_arr.iter().flatten().max().unwrap_or(0);
            (min, max)
        } else {
            (0, 0)
        }
    }

    /// Extract row-group level statistics from a Parquet file
    ///
    /// Opens the Parquet file, iterates through row groups, and extracts:
    /// - Row counts per group
    /// - Min/max _seq per group
    /// - Column min/max for indexed columns per group
    /// - Byte ranges (if available from metadata)
    /// - Page index presence flag
    ///
    /// Returns (row_group_stats, has_page_index, footer_size)
    pub fn extract_row_group_stats(
        file_path: &Path,
        indexed_columns: &[String],
    ) -> Result<(Vec<RowGroupPruningStats>, bool, Option<u64>), KalamDbError> {
        use std::fs::File;

        let file = File::open(file_path).map_err(|e| KalamDbError::Io(e))?;

        let builder = ParquetRecordBatchReaderBuilder::try_new(file)
            .map_err(|e| KalamDbError::Other(format!("Failed to open Parquet file: {}", e)))?;

        let metadata = builder.metadata();
        let num_row_groups = metadata.num_row_groups();
        let footer_size = metadata.file_metadata().num_rows() as u64; // Approximation

        // Check if page index exists (check first row group)
        let has_page_index = if num_row_groups > 0 {
            metadata.row_group(0).columns().iter().any(|col| {
                col.column_index_offset().is_some() || col.offset_index_offset().is_some()
            })
        } else {
            false
        };

        let mut row_group_stats = Vec::with_capacity(num_row_groups);

        for rg_idx in 0..num_row_groups {
            let rg_metadata = metadata.row_group(rg_idx);
            let row_count = rg_metadata.num_rows() as u64;

            // Extract min/max _seq from row group statistics
            let mut min_seq = i64::MAX;
            let mut max_seq = i64::MIN;
            let mut column_min_max = HashMap::new();

            for col_metadata in rg_metadata.columns() {
                let col_name = col_metadata.column_path().string();

                if let Some(stats) = col_metadata.statistics() {
                    // Extract _seq bounds
                    if col_name == "_seq" {
                        if let (Some(min_bytes), Some(max_bytes)) =
                            (stats.min_bytes_opt(), stats.max_bytes_opt())
                        {
                            if min_bytes.len() == 8 {
                                let min_val = i64::from_le_bytes(min_bytes.try_into().unwrap());
                                min_seq = min_seq.min(min_val);
                            }
                            if max_bytes.len() == 8 {
                                let max_val = i64::from_le_bytes(max_bytes.try_into().unwrap());
                                max_seq = max_seq.max(max_val);
                            }
                        }
                    }

                    // Extract indexed column bounds
                    if indexed_columns.contains(&col_name.to_string()) {
                        if let (Some(min_bytes), Some(max_bytes)) =
                            (stats.min_bytes_opt(), stats.max_bytes_opt())
                        {
                            // For simplicity, store as JSON strings (could be type-specific)
                            let min_json = serde_json::json!(format!("{:?}", min_bytes));
                            let max_json = serde_json::json!(format!("{:?}", max_bytes));
                            column_min_max.insert(col_name.to_string(), (min_json, max_json));
                        }
                    }
                }
            }

            // Fallback if _seq stats missing
            if min_seq == i64::MAX || max_seq == i64::MIN {
                min_seq = 0;
                max_seq = 0;
            }

            // Byte range extraction (optional, depends on metadata availability)
            let byte_range = rg_metadata.columns().first().map(|col| {
                let offset = col.file_offset();
                (offset as u64, rg_metadata.compressed_size() as u64)
            });

            row_group_stats.push(RowGroupPruningStats::new(
                rg_idx as u32,
                row_count,
                min_seq,
                max_seq,
                column_min_max,
                byte_range,
            ));
        }

        Ok((row_group_stats, has_page_index, Some(footer_size)))
    }

    /// Update manifest and cache after successful flush
    ///
    /// This is the canonical flow for updating manifest during flush:
    /// 1. Create BatchFileEntry with metadata (including row-group stats)
    /// 2. Update manifest file on disk
    /// 3. Update cache with new manifest
    ///
    /// # Arguments
    /// * `namespace` - Namespace ID
    /// * `table` - Table name
    /// * `table_type` - Table type (Shared or User)
    /// * `user_id` - User ID for user tables, None for shared tables
    /// * `batch_number` - Batch number
    /// * `batch_filename` - Batch filename
    /// * `file_path` - Full path to the written Parquet file
    /// * `batch` - RecordBatch (for extracting stats)
    /// * `file_size_bytes` - Size of written Parquet file
    /// * `indexed_columns` - Columns with Bloom filters/page stats enabled
    ///
    /// # Returns
    /// Updated ManifestFile
    pub fn update_manifest_after_flush(
        &self,
        namespace: &NamespaceId,
        table: &TableName,
        table_type: kalamdb_commons::models::schemas::TableType,
        user_id: Option<&UserId>,
        batch_number: u64,
        batch_filename: String,
        file_path: &Path,
        batch: &RecordBatch,
        file_size_bytes: u64,
        indexed_columns: &[String],
    ) -> Result<ManifestFile, KalamDbError> {
        let table_id = TableId::new(namespace.clone(), table.clone());
        // Extract metadata from batch (batch-level)
        let (min_seq, max_seq) = Self::extract_seq_range(batch);

        let row_count = batch.num_rows() as u64;

        // Extract row-group level stats from the written Parquet file
        let (row_groups, has_page_index, footer_size) =
            Self::extract_row_group_stats(file_path, indexed_columns).unwrap_or_else(|e| {
                log::warn!(
                "⚠️  Failed to extract row-group stats for {}: {}. Skipping fine-grained metadata.",
                file_path.display(),
                e
            );
                (Vec::new(), false, None)
            });

        // Create batch entry with row-group metadata
        let mut batch_entry = BatchFileEntry::new(
            batch_number,
            batch_filename,
            min_seq,
            max_seq,
            row_count,
            file_size_bytes,
            1, // schema_version
        );
        batch_entry.row_groups = row_groups;
        batch_entry.has_page_index = has_page_index;
        batch_entry.footer_size = footer_size;

        // Update manifest on disk
        let updated_manifest = self
            .manifest_service
            .update_manifest(&table_id, table_type, user_id, batch_entry)
            .map_err(|e| {
                KalamDbError::Other(format!(
                    "Failed to update manifest for {}.{} (user_id={:?}): {}",
                    namespace.as_str(),
                    table.as_str(),
                    user_id.map(|u| u.as_str()),
                    e
                ))
            })?;

        // Update cache
        let scope_str = user_id
            .map(|u| u.as_str().to_string())
            .unwrap_or_else(|| "shared".to_string());
        let manifest_path = format!(
            "{}/{}/{}/manifest.json",
            namespace.as_str(),
            table.as_str(),
            scope_str
        );
        self.manifest_cache
            .update_after_flush(&table_id, user_id, &updated_manifest, None, manifest_path)
            .map_err(|e| {
                KalamDbError::Other(format!(
                    "Failed to update manifest cache for {}.{} (user_id={:?}): {}",
                    namespace.as_str(),
                    table.as_str(),
                    user_id.map(|u| u.as_str()),
                    e
                ))
            })?;

        log::info!(
            "[MANIFEST] ✅ Updated manifest and cache: {}.{} (user_id={:?}, batch={}, rows={}, size={} bytes)",
            namespace.as_str(),
            table.as_str(),
            user_id.map(|u| u.as_str()),
            batch_number,
            row_count,
            file_size_bytes
        );

        Ok(updated_manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::arrow::array::{Int64Array, StringArray};
    use datafusion::arrow::datatypes::{DataType, Field, Schema};
    use std::sync::Arc as StdArc;

    #[test]
    fn test_generate_batch_filename() {
        assert_eq!(
            FlushManifestHelper::generate_batch_filename(0),
            "batch-0.parquet"
        );
        assert_eq!(
            FlushManifestHelper::generate_batch_filename(42),
            "batch-42.parquet"
        );
        assert_eq!(
            FlushManifestHelper::generate_batch_filename(999),
            "batch-999.parquet"
        );
    }

    #[test]
    fn test_extract_seq_range() {
        let schema = StdArc::new(Schema::new(vec![
            Field::new("_seq", DataType::Int64, false),
            Field::new("data", DataType::Utf8, true),
        ]));

        let seq_array = Int64Array::from(vec![100, 200, 150, 175]);
        let data_array = StringArray::from(vec!["a", "b", "c", "d"]);

        let batch = RecordBatch::try_new(
            schema,
            vec![StdArc::new(seq_array), StdArc::new(data_array)],
        )
        .unwrap();

        let (min, max) = FlushManifestHelper::extract_seq_range(&batch);
        assert_eq!(min, 100);
        assert_eq!(max, 200);
    }
}
