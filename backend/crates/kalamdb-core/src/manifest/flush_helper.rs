//! Flush Helper for Manifest Operations
//!
//! Centralizes manifest-related logic used during flush operations to eliminate
//! code duplication between user and shared table flush implementations.

use super::{ManifestCacheService, ManifestService};
use crate::error::KalamDbError;
use datafusion::arrow::array::*;
use datafusion::arrow::compute;
use datafusion::arrow::compute::kernels::aggregate::{min_string, max_string};
use datafusion::arrow::datatypes::DataType;
use datafusion::arrow::record_batch::RecordBatch;
use kalamdb_commons::constants::SystemColumnNames;
use kalamdb_commons::types::{ColumnStats, Manifest, SegmentMetadata};
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
            Ok(manifest) => {
                // Find max batch number from segments
                let max_batch = manifest
                    .segments
                    .iter()
                    .filter_map(|s| {
                        // Assuming path format "batch-{N}.parquet"
                        let filename = Path::new(&s.path).file_name()?.to_str()?;
                        if filename.starts_with("batch-") && filename.ends_with(".parquet") {
                            filename
                                .strip_prefix("batch-")?
                                .strip_suffix(".parquet")?
                                .parse::<u64>()
                                .ok()
                        } else {
                            None
                        }
                    })
                    .max()
                    .unwrap_or(0);
                Ok(max_batch + 1)
            }
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
            .column_by_name(SystemColumnNames::SEQ)
            .and_then(|col| col.as_any().downcast_ref::<Int64Array>());

        if let Some(seq_arr) = seq_column {
            let min = compute::min(seq_arr).unwrap_or(0);
            let max = compute::max(seq_arr).unwrap_or(0);
            (min, max)
        } else {
            (0, 0)
        }
    }

    /// Extract column statistics (min/max/nulls) from RecordBatch
    pub fn extract_column_stats(batch: &RecordBatch, indexed_columns: &[String]) -> HashMap<String, ColumnStats> {
        let mut stats = HashMap::new();
        for field in batch.schema().fields() {
            let name = field.name();
            if name == SystemColumnNames::SEQ {
                continue; // Handled separately
            }
            
            // Only compute stats for indexed columns
            if !indexed_columns.contains(name) {
                continue;
            }

            if let Some(col) = batch.column_by_name(name) {
                let null_count = col.null_count() as i64;
                let (min, max) = Self::compute_min_max(col);
                
                stats.insert(name.clone(), ColumnStats {
                    min,
                    max,
                    null_count: Some(null_count),
                });
            }
        }
        stats
    }

    fn compute_min_max(array: &ArrayRef) -> (Option<serde_json::Value>, Option<serde_json::Value>) {
        match array.data_type() {
            DataType::Int8 => {
                let arr = array.as_any().downcast_ref::<Int8Array>().unwrap();
                (
                    compute::min(arr).map(|v| serde_json::json!(v)),
                    compute::max(arr).map(|v| serde_json::json!(v)),
                )
            }
            DataType::Int16 => {
                let arr = array.as_any().downcast_ref::<Int16Array>().unwrap();
                (
                    compute::min(arr).map(|v| serde_json::json!(v)),
                    compute::max(arr).map(|v| serde_json::json!(v)),
                )
            }
            DataType::Int32 => {
                let arr = array.as_any().downcast_ref::<Int32Array>().unwrap();
                (
                    compute::min(arr).map(|v| serde_json::json!(v)),
                    compute::max(arr).map(|v| serde_json::json!(v)),
                )
            }
            DataType::Int64 => {
                let arr = array.as_any().downcast_ref::<Int64Array>().unwrap();
                (
                    compute::min(arr).map(|v| serde_json::json!(v)),
                    compute::max(arr).map(|v| serde_json::json!(v)),
                )
            }
            DataType::UInt8 => {
                let arr = array.as_any().downcast_ref::<UInt8Array>().unwrap();
                (
                    compute::min(arr).map(|v| serde_json::json!(v)),
                    compute::max(arr).map(|v| serde_json::json!(v)),
                )
            }
            DataType::UInt16 => {
                let arr = array.as_any().downcast_ref::<UInt16Array>().unwrap();
                (
                    compute::min(arr).map(|v| serde_json::json!(v)),
                    compute::max(arr).map(|v| serde_json::json!(v)),
                )
            }
            DataType::UInt32 => {
                let arr = array.as_any().downcast_ref::<UInt32Array>().unwrap();
                (
                    compute::min(arr).map(|v| serde_json::json!(v)),
                    compute::max(arr).map(|v| serde_json::json!(v)),
                )
            }
            DataType::UInt64 => {
                let arr = array.as_any().downcast_ref::<UInt64Array>().unwrap();
                (
                    compute::min(arr).map(|v| serde_json::json!(v)),
                    compute::max(arr).map(|v| serde_json::json!(v)),
                )
            }
            DataType::Float32 => {
                let arr = array.as_any().downcast_ref::<Float32Array>().unwrap();
                (
                    compute::min(arr).map(|v| serde_json::json!(v)),
                    compute::max(arr).map(|v| serde_json::json!(v)),
                )
            }
            DataType::Float64 => {
                let arr = array.as_any().downcast_ref::<Float64Array>().unwrap();
                (
                    compute::min(arr).map(|v| serde_json::json!(v)),
                    compute::max(arr).map(|v| serde_json::json!(v)),
                )
            }
            DataType::Utf8 => {
                let arr = array.as_any().downcast_ref::<StringArray>().unwrap();
                (
                    min_string(arr).map(|v| serde_json::json!(v)),
                    max_string(arr).map(|v| serde_json::json!(v)),
                )
            }
            DataType::LargeUtf8 => {
                let arr = array.as_any().downcast_ref::<LargeStringArray>().unwrap();
                (
                    min_string(arr).map(|v| serde_json::json!(v)),
                    max_string(arr).map(|v| serde_json::json!(v)),
                )
            }
            DataType::Boolean => {
                let _arr = array.as_any().downcast_ref::<BooleanArray>().unwrap();
                // Boolean min/max is not directly supported by compute::min/max in older arrow versions?
                // But let's try. If not, we can implement manually.
                // Actually compute::min_boolean exists or generic min works.
                // Let's assume generic min works or use a workaround.
                // For boolean, min is false (0), max is true (1).
                // If all true, min=true. If all false, max=false.
                // Let's skip boolean stats for now or implement simple check.
                (None, None) 
            }
            _ => (None, None), // Unsupported types for stats
        }
    }

    /// Update manifest and cache after successful flush
    ///
    /// This is the canonical flow for updating manifest during flush:
    /// 1. Create SegmentMetadata with metadata
    /// 2. Update manifest file on disk (via service)
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
    /// Updated Manifest
    pub fn update_manifest_after_flush(
        &self,
        namespace: &NamespaceId,
        table: &TableName,
        table_type: kalamdb_commons::models::schemas::TableType,
        user_id: Option<&UserId>,
        _batch_number: u64,
        batch_filename: String,
        file_path: &Path,
        batch: &RecordBatch,
        file_size_bytes: u64,
        indexed_columns: &[String],
    ) -> Result<Manifest, KalamDbError> {
        let table_id = TableId::new(namespace.clone(), table.clone());
        // Extract metadata from batch (batch-level)
        let (min_seq, max_seq) = Self::extract_seq_range(batch);
        let column_stats = Self::extract_column_stats(batch, indexed_columns);

        let row_count = batch.num_rows() as u64;

        // Create segment metadata
        // Use filename as ID for now, or generate UUID
        let segment_id = batch_filename.clone(); 
        
        let segment = SegmentMetadata::new(
            segment_id,
            batch_filename,
            column_stats,
            min_seq,
            max_seq,
            row_count,
            file_size_bytes,
        );

        // Update manifest (Hot Store update)
        let updated_manifest = self
            .manifest_service
            .update_manifest(&table_id, table_type, user_id, segment)
            .map_err(|e| {
                KalamDbError::Other(format!(
                    "Failed to update manifest for {}.{} (user_id={:?}): {}",
                    namespace.as_str(),
                    table.as_str(),
                    user_id.map(|u| u.as_str()),
                    e
                ))
            })?;
            
        // Flush manifest to disk (Cold Store persistence)
        // In the new architecture, we might want to flush periodically or immediately depending on policy.
        // For now, let's flush immediately to maintain durability guarantees similar to before.
        self.manifest_service.flush_manifest(&table_id, user_id).map_err(|e| {
             KalamDbError::Other(format!(
                "Failed to flush manifest for {}.{} (user_id={:?}): {}",
                namespace.as_str(),
                table.as_str(),
                user_id.map(|u| u.as_str()),
                e
            ))
        })?;

        // Update cache service (if it's separate from ManifestService's internal cache)
        // ManifestService now has its own cache, but ManifestCacheService might be a higher level or legacy service?
        // The prompt says "Modify ManifestService... to implement Hot/Cold split".
        // ManifestCacheService seems to be doing similar things (Hot cache + RocksDB).
        // If ManifestService now handles caching, maybe ManifestCacheService is redundant or needs to be integrated.
        // For now, I'll keep updating ManifestCacheService to avoid breaking other things, 
        // but I should probably rely on ManifestService.
        
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
            "[MANIFEST] ✅ Updated manifest and cache: {}.{} (user_id={:?}, file={}, rows={}, size={} bytes)",
            namespace.as_str(),
            table.as_str(),
            user_id.map(|u| u.as_str()),
            file_path.display(),
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
