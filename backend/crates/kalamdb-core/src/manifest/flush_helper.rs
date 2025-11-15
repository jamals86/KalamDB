//! Flush Helper for Manifest Operations
//!
//! Centralizes manifest-related logic used during flush operations to eliminate
//! code duplication between user and shared table flush implementations.

use super::{ManifestCacheService, ManifestService};
use crate::error::KalamDbError;
use datafusion::arrow::array::*;
use datafusion::arrow::datatypes::DataType;
use datafusion::arrow::record_batch::RecordBatch;
use kalamdb_commons::types::{BatchFileEntry, ManifestFile};
use kalamdb_commons::{NamespaceId, TableName, UserId};
use std::collections::HashMap;
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
        match self.manifest_service.read_manifest(namespace, table, user_id) {
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

    /// Extract column statistics (min/max) from RecordBatch
    ///
    /// Returns HashMap of column_name -> (min_value, max_value)
    pub fn extract_column_stats(
        batch: &RecordBatch,
    ) -> Result<HashMap<String, (serde_json::Value, serde_json::Value)>, KalamDbError> {
        let mut column_min_max = HashMap::new();

        for (idx, field) in batch.schema().fields().iter().enumerate() {
            let column = batch.column(idx);
            let column_name = field.name();

            // Skip if all nulls
            if column.null_count() == column.len() {
                continue;
            }

            // Extract min/max based on data type
            let (min_val, max_val) = match field.data_type() {
                DataType::Int64 => {
                    if let Some(arr) = column.as_any().downcast_ref::<Int64Array>() {
                        let values: Vec<i64> = (0..arr.len())
                            .filter_map(|i| if arr.is_null(i) { None } else { Some(arr.value(i)) })
                            .collect();
                        if let (Some(min), Some(max)) = (values.iter().min(), values.iter().max()) {
                            (serde_json::json!(min), serde_json::json!(max))
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }
                DataType::Int32 => {
                    if let Some(arr) = column.as_any().downcast_ref::<Int32Array>() {
                        let values: Vec<i32> = (0..arr.len())
                            .filter_map(|i| if arr.is_null(i) { None } else { Some(arr.value(i)) })
                            .collect();
                        if let (Some(min), Some(max)) = (values.iter().min(), values.iter().max()) {
                            (serde_json::json!(min), serde_json::json!(max))
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }
                DataType::Float64 => {
                    if let Some(arr) = column.as_any().downcast_ref::<Float64Array>() {
                        let values: Vec<f64> = (0..arr.len())
                            .filter_map(|i| if arr.is_null(i) { None } else { Some(arr.value(i)) })
                            .collect();
                        if let (Some(min), Some(max)) = (
                            values.iter().min_by(|a, b| a.partial_cmp(b).unwrap()),
                            values.iter().max_by(|a, b| a.partial_cmp(b).unwrap()),
                        ) {
                            (serde_json::json!(min), serde_json::json!(max))
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }
                DataType::Utf8 => {
                    if let Some(arr) = column.as_any().downcast_ref::<StringArray>() {
                        let values: Vec<&str> = (0..arr.len())
                            .filter_map(|i| if arr.is_null(i) { None } else { Some(arr.value(i)) })
                            .collect();
                        if let (Some(min), Some(max)) = (values.iter().min(), values.iter().max()) {
                            (serde_json::json!(min), serde_json::json!(max))
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }
                DataType::Timestamp(_, _) => {
                    if let Some(arr) = column.as_any().downcast_ref::<TimestampMillisecondArray>()
                    {
                        let values: Vec<i64> = (0..arr.len())
                            .filter_map(|i| if arr.is_null(i) { None } else { Some(arr.value(i)) })
                            .collect();
                        if let (Some(min), Some(max)) = (values.iter().min(), values.iter().max()) {
                            (serde_json::json!(min), serde_json::json!(max))
                        } else {
                            continue;
                        }
                    } else if let Some(arr) =
                        column.as_any().downcast_ref::<TimestampMicrosecondArray>()
                    {
                        let values: Vec<i64> = (0..arr.len())
                            .filter_map(|i| if arr.is_null(i) { None } else { Some(arr.value(i)) })
                            .collect();
                        if let (Some(min), Some(max)) = (values.iter().min(), values.iter().max()) {
                            (serde_json::json!(min), serde_json::json!(max))
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }
                DataType::Boolean => {
                    if let Some(arr) = column.as_any().downcast_ref::<BooleanArray>() {
                        let has_true = (0..arr.len()).any(|i| !arr.is_null(i) && arr.value(i));
                        let has_false = (0..arr.len()).any(|i| !arr.is_null(i) && !arr.value(i));
                        if has_true && has_false {
                            (serde_json::json!(false), serde_json::json!(true))
                        } else if has_true {
                            (serde_json::json!(true), serde_json::json!(true))
                        } else if has_false {
                            (serde_json::json!(false), serde_json::json!(false))
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }
                _ => continue, // Skip other types for now
            };

            column_min_max.insert(column_name.clone(), (min_val, max_val));
        }

        Ok(column_min_max)
    }

    /// Update manifest and cache after successful flush
    ///
    /// This is the canonical flow for updating manifest during flush:
    /// 1. Create BatchFileEntry with metadata
    /// 2. Update manifest file on disk
    /// 3. Update cache with new manifest
    ///
    /// # Arguments
    /// * `namespace` - Namespace ID
    /// * `table` - Table name
    /// * `user_id` - User ID for user tables, None for shared tables
    /// * `batch_number` - Batch number
    /// * `batch_filename` - Batch filename
    /// * `batch` - RecordBatch (for extracting stats)
    /// * `file_size_bytes` - Size of written Parquet file
    ///
    /// # Returns
    /// Updated ManifestFile
    pub fn update_manifest_after_flush(
        &self,
        namespace: &NamespaceId,
        table: &TableName,
        user_id: Option<&UserId>,
        batch_number: u64,
        batch_filename: String,
        batch: &RecordBatch,
        file_size_bytes: u64,
    ) -> Result<ManifestFile, KalamDbError> {
        // Extract metadata from batch
        let (min_seq, max_seq) = Self::extract_seq_range(batch);
        let column_min_max = Self::extract_column_stats(batch)?;
        let row_count = batch.num_rows() as u64;

        // Create batch entry
        let batch_entry = BatchFileEntry::new(
            batch_number,
            batch_filename,
            min_seq,
            max_seq,
            column_min_max,
            row_count,
            file_size_bytes,
            1, // schema_version
        );

        // Update manifest on disk
        let updated_manifest = self
            .manifest_service
            .update_manifest(namespace, table, user_id, batch_entry)
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
        let scope_str = user_id.map(|u| u.as_str().to_string()).unwrap_or_else(|| "shared".to_string());
        let manifest_path = format!("{}/{}/{}/manifest.json", namespace.as_str(), table.as_str(), scope_str);
        self.manifest_cache
            .update_after_flush(namespace, table, user_id, &updated_manifest, None, manifest_path)
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
        assert_eq!(FlushManifestHelper::generate_batch_filename(0), "batch-0.parquet");
        assert_eq!(FlushManifestHelper::generate_batch_filename(42), "batch-42.parquet");
        assert_eq!(FlushManifestHelper::generate_batch_filename(999), "batch-999.parquet");
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

    #[test]
    fn test_extract_column_stats() {
        let schema = StdArc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, true),
            Field::new("_seq", DataType::Int64, false),
        ]));

        let id_array = Int64Array::from(vec![1, 2, 3, 4]);
        let name_array = StringArray::from(vec![Some("alice"), Some("bob"), None, Some("charlie")]);
        let seq_array = Int64Array::from(vec![100, 200, 150, 175]);

        let batch = RecordBatch::try_new(
            schema,
            vec![
                StdArc::new(id_array),
                StdArc::new(name_array),
                StdArc::new(seq_array),
            ],
        )
        .unwrap();

        let stats = FlushManifestHelper::extract_column_stats(&batch).unwrap();

        // Check id column stats
        assert!(stats.contains_key("id"));
        let (min, max) = &stats["id"];
        assert_eq!(min, &serde_json::json!(1));
        assert_eq!(max, &serde_json::json!(4));

        // Check name column stats (skips nulls)
        assert!(stats.contains_key("name"));
        let (min, max) = &stats["name"];
        assert_eq!(min, &serde_json::json!("alice"));
        assert_eq!(max, &serde_json::json!("charlie"));

        // Check _seq column stats
        assert!(stats.contains_key("_seq"));
        let (min, max) = &stats["_seq"];
        assert_eq!(min, &serde_json::json!(100));
        assert_eq!(max, &serde_json::json!(200));
    }
}
