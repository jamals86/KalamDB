//! Flush Helper for Manifest Operations
//!
//! Centralizes manifest-related logic used during flush operations to eliminate
//! code duplication between user and shared table flush implementations.

use super::{ManifestCacheService, ManifestService};
use crate::error::KalamDbError;
use datafusion::arrow::array::*;
use datafusion::arrow::compute;
use datafusion::arrow::compute::kernels::aggregate::{max_string, min_string};
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
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<u64, KalamDbError> {
        match self.manifest_service.read_manifest(table_id, user_id) {
            Ok(manifest) => {
                // Use last_sequence_number which tracks the last batch index
                // Next batch should be last_sequence_number + 1
                let next_batch = if manifest.segments.is_empty() {
                    0 // First batch
                } else {
                    manifest.last_sequence_number + 1
                };
                Ok(next_batch)
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
    pub fn extract_column_stats(
        batch: &RecordBatch,
        indexed_columns: &[String],
    ) -> HashMap<String, ColumnStats> {
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

                stats.insert(
                    name.clone(),
                    ColumnStats {
                        min,
                        max,
                        null_count: Some(null_count),
                    },
                );
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
    /// * `schema_version` - Schema version (Phase 16) to link Parquet file to specific schema
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
        schema_version: u32,
    ) -> Result<Manifest, KalamDbError> {
        let table_id = TableId::new(namespace.clone(), table.clone());
        // Extract metadata from batch (batch-level)
        let (min_seq, max_seq) = Self::extract_seq_range(batch);
        let column_stats = Self::extract_column_stats(batch, indexed_columns);

        let row_count = batch.num_rows() as u64;

        // Create segment metadata
        // Use filename as ID for now, or generate UUID
        let segment_id = batch_filename.clone();

        // Store just the filename in manifest - path resolution happens at read time
        // The storage_path_template + user_id substitution is used during reads,
        // so we only need to store the batch filename here
        let relative_path = batch_filename.clone();

        // Phase 16: Use with_schema_version to record schema version in manifest
        // IMPORTANT: Use relative_path instead of batch_filename for the path field
        let segment = SegmentMetadata::with_schema_version(
            segment_id,
            relative_path,
            column_stats,
            min_seq,
            max_seq,
            row_count,
            file_size_bytes,
            schema_version,
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
        self.manifest_service
            .flush_manifest(&table_id, user_id)
            .map_err(|e| {
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
            Field::new(SystemColumnNames::SEQ, DataType::Int64, false),
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

    #[test]
    fn test_extract_seq_range_empty() {
        let schema = StdArc::new(Schema::new(vec![Field::new(
            SystemColumnNames::SEQ,
            DataType::Int64,
            false,
        )]));
        let seq_array = Int64Array::from(vec![] as Vec<i64>);
        let batch = RecordBatch::try_new(schema, vec![StdArc::new(seq_array)]).unwrap();

        let (min_seq, max_seq) = FlushManifestHelper::extract_seq_range(&batch);
        assert_eq!(min_seq, 0);
        assert_eq!(max_seq, 0);
    }

    #[test]
    fn test_extract_seq_range_missing_column() {
        let schema = StdArc::new(Schema::new(vec![Field::new(
            "other_column",
            DataType::Int64,
            false,
        )]));
        let col_array = Int64Array::from(vec![1, 2, 3]);
        let batch = RecordBatch::try_new(schema, vec![StdArc::new(col_array)]).unwrap();

        let (min_seq, max_seq) = FlushManifestHelper::extract_seq_range(&batch);
        assert_eq!(min_seq, 0);
        assert_eq!(max_seq, 0);
    }

    #[test]
    fn test_extract_column_stats_int_types() {
        use datafusion::arrow::array::{Int32Array, Int64Array};

        let schema = StdArc::new(Schema::new(vec![
            Field::new("id", DataType::Int32, true),
            Field::new("timestamp", DataType::Int64, false),
            Field::new(SystemColumnNames::SEQ, DataType::Int64, false),
        ]));
        let id_array = Int32Array::from(vec![Some(5), Some(10), None, Some(1), Some(8)]);
        let ts_array = Int64Array::from(vec![1000000, 2000000, 1500000, 1800000, 1200000]);
        let seq_array = Int64Array::from(vec![1, 2, 3, 4, 5]);
        let batch = RecordBatch::try_new(
            schema,
            vec![
                StdArc::new(id_array),
                StdArc::new(ts_array),
                StdArc::new(seq_array),
            ],
        )
        .unwrap();

        let indexed_columns = vec!["id".to_string(), "timestamp".to_string()];
        let stats = FlushManifestHelper::extract_column_stats(&batch, &indexed_columns);

        assert_eq!(stats.len(), 2);
        let id_stats = stats.get("id").unwrap();
        assert_eq!(id_stats.min, Some(serde_json::json!(1)));
        assert_eq!(id_stats.max, Some(serde_json::json!(10)));
        assert_eq!(id_stats.null_count, Some(1));

        let ts_stats = stats.get("timestamp").unwrap();
        assert_eq!(ts_stats.min, Some(serde_json::json!(1000000)));
        assert_eq!(ts_stats.max, Some(serde_json::json!(2000000)));
        assert_eq!(ts_stats.null_count, Some(0));
    }

    #[test]
    fn test_extract_column_stats_string() {
        let schema = StdArc::new(Schema::new(vec![Field::new("name", DataType::Utf8, true)]));
        let array = StringArray::from(vec![Some("alice"), Some("bob"), None, Some("charlie")]);
        let batch = RecordBatch::try_new(schema, vec![StdArc::new(array)]).unwrap();

        let indexed_columns = vec!["name".to_string()];
        let stats = FlushManifestHelper::extract_column_stats(&batch, &indexed_columns);

        let name_stats = stats.get("name").unwrap();
        assert_eq!(name_stats.min, Some(serde_json::json!("alice")));
        assert_eq!(name_stats.max, Some(serde_json::json!("charlie")));
        assert_eq!(name_stats.null_count, Some(1));
    }

    #[test]
    fn test_extract_column_stats_skips_seq_column() {
        use datafusion::arrow::array::Int32Array;

        let schema = StdArc::new(Schema::new(vec![
            Field::new("id", DataType::Int32, false),
            Field::new(SystemColumnNames::SEQ, DataType::Int64, false),
        ]));
        let id_array = Int32Array::from(vec![1, 2, 3]);
        let seq_array = Int64Array::from(vec![10, 20, 30]);
        let batch = RecordBatch::try_new(
            schema,
            vec![StdArc::new(id_array), StdArc::new(seq_array)],
        )
        .unwrap();

        let indexed_columns = vec!["id".to_string(), SystemColumnNames::SEQ.to_string()];
        let stats = FlushManifestHelper::extract_column_stats(&batch, &indexed_columns);

        // Should only have stats for "id", not "_seq"
        assert_eq!(stats.len(), 1);
        assert!(stats.contains_key("id"));
        assert!(!stats.contains_key(SystemColumnNames::SEQ));
    }

    #[test]
    fn test_extract_column_stats_only_indexed_columns() {
        use datafusion::arrow::array::Int32Array;

        let schema = StdArc::new(Schema::new(vec![
            Field::new("id", DataType::Int32, false),
            Field::new("name", DataType::Utf8, true),
            Field::new("age", DataType::Int32, true),
        ]));
        let id_array = Int32Array::from(vec![1, 2, 3]);
        let name_array = StringArray::from(vec![Some("alice"), Some("bob"), Some("charlie")]);
        let age_array = Int32Array::from(vec![Some(30), Some(25), Some(35)]);
        let batch = RecordBatch::try_new(
            schema,
            vec![
                StdArc::new(id_array),
                StdArc::new(name_array),
                StdArc::new(age_array),
            ],
        )
        .unwrap();

        // Only index "id" and "age", not "name"
        let indexed_columns = vec!["id".to_string(), "age".to_string()];
        let stats = FlushManifestHelper::extract_column_stats(&batch, &indexed_columns);

        assert_eq!(stats.len(), 2);
        assert!(stats.contains_key("id"));
        assert!(stats.contains_key("age"));
        assert!(!stats.contains_key("name"));
    }

    #[test]
    fn test_extract_column_stats_empty_batch() {
        use datafusion::arrow::array::Int32Array;

        let schema = StdArc::new(Schema::new(vec![Field::new(
            "id",
            DataType::Int32,
            false,
        )]));
        let id_array = Int32Array::from(vec![] as Vec<i32>);
        let batch = RecordBatch::try_new(schema, vec![StdArc::new(id_array)]).unwrap();

        let indexed_columns = vec!["id".to_string()];
        let stats = FlushManifestHelper::extract_column_stats(&batch, &indexed_columns);

        // Empty batch should still produce stats entry but with None min/max
        assert_eq!(stats.len(), 1);
        let id_stats = stats.get("id").unwrap();
        assert_eq!(id_stats.min, None);
        assert_eq!(id_stats.max, None);
        assert_eq!(id_stats.null_count, Some(0));
    }

    #[test]
    fn test_extract_column_stats_all_nulls() {
        use datafusion::arrow::array::Int32Array;

        let schema = StdArc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int32,
            true,
        )]));
        let array = Int32Array::from(vec![None, None, None]);
        let batch = RecordBatch::try_new(schema, vec![StdArc::new(array)]).unwrap();

        let indexed_columns = vec!["value".to_string()];
        let stats = FlushManifestHelper::extract_column_stats(&batch, &indexed_columns);

        let value_stats = stats.get("value").unwrap();
        assert_eq!(value_stats.min, None);
        assert_eq!(value_stats.max, None);
        assert_eq!(value_stats.null_count, Some(3));
    }

    #[test]
    fn test_extract_column_stats_float_types() {
        use datafusion::arrow::array::{Float32Array, Float64Array};

        let schema = StdArc::new(Schema::new(vec![
            Field::new("f32_col", DataType::Float32, true),
            Field::new("f64_col", DataType::Float64, true),
        ]));
        let f32_array = Float32Array::from(vec![Some(1.5), Some(2.5), Some(0.5)]);
        let f64_array = Float64Array::from(vec![Some(10.5), Some(20.5), Some(5.5)]);
        let batch = RecordBatch::try_new(
            schema,
            vec![StdArc::new(f32_array), StdArc::new(f64_array)],
        )
        .unwrap();

        let indexed_columns = vec!["f32_col".to_string(), "f64_col".to_string()];
        let stats = FlushManifestHelper::extract_column_stats(&batch, &indexed_columns);

        assert_eq!(stats.len(), 2);
        assert!(stats.get("f32_col").unwrap().min.is_some());
        assert!(stats.get("f32_col").unwrap().max.is_some());
        assert!(stats.get("f64_col").unwrap().min.is_some());
        assert!(stats.get("f64_col").unwrap().max.is_some());
    }
}
