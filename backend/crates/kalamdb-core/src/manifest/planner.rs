//! Manifest-driven access planner (Phase: pruning integration)
//!
//! Provides utilities to translate `Manifest` metadata into
//! concrete file/row-group selections for efficient reads.

use crate::error::KalamDbError;
use crate::error_extensions::KalamDbResultExt;
use crate::app_context::AppContext;
use datafusion::arrow::compute::cast;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use kalamdb_commons::types::Manifest;
use kalamdb_commons::TableId;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

/// Planned selection for a single Parquet file
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowGroupSelection {
    /// Relative file path (e.g., "batch-0.parquet")
    pub file_path: String,
    /// Row-group indexes to read from that file
    pub row_groups: Vec<usize>,
}

impl RowGroupSelection {
    pub fn new(file_path: String, row_groups: Vec<usize>) -> Self {
        Self {
            file_path,
            row_groups,
        }
    }
}

/// Planner that produces pruning-aware selections from the manifest
#[derive(Debug, Default)]
pub struct ManifestAccessPlanner;

impl ManifestAccessPlanner {
    pub fn new() -> Self {
        Self
    }

    /// Plan file selections (all files, no row-group pruning)
    ///
    /// Returns a list of all batch files to scan.
    pub fn plan_all_files(&self, manifest: &Manifest) -> Vec<String> {
        manifest.segments.iter().map(|s| s.path.clone()).collect()
    }

    /// Unified scan method: returns combined RecordBatch from Parquet files
    ///
    /// Handles manifest-based pruning, file loading, schema evolution, and batch concatenation.
    ///
    /// # Arguments
    /// * `manifest_opt` - Optional manifest for metadata-driven selection
    /// * `storage_dir` - Base directory containing Parquet files
    /// * `seq_range` - Optional (min, max) seq range for pruning
    /// * `use_degraded_mode` - If true, skip manifest and list directory
    /// * `schema` - Current Arrow schema (target schema for projection)
    /// * `table_id` - Table identifier for retrieving historical schemas
    /// * `app_context` - AppContext for accessing system tables
    ///
    /// # Returns
    /// (batch: RecordBatch, stats: (total_batches, skipped, scanned))
    pub fn scan_parquet_files(
        &self,
        manifest_opt: Option<&Manifest>,
        storage_dir: &PathBuf,
        seq_range: Option<(i64, i64)>,
        use_degraded_mode: bool,
        schema: SchemaRef,
        table_id: &TableId,
        app_context: &Arc<AppContext>,
    ) -> Result<(RecordBatch, (usize, usize, usize)), KalamDbError> {
        let mut parquet_files: Vec<PathBuf> = Vec::new();
        let mut file_schema_versions: std::collections::HashMap<PathBuf, u32> = std::collections::HashMap::new();
        let (mut total_batches, mut skipped, mut scanned) = (0usize, 0usize, 0usize);

        if !use_degraded_mode {
            if let Some(manifest) = manifest_opt {
                total_batches = manifest.segments.len();

                let selected_files: Vec<String> = if let Some((min_seq, max_seq)) = seq_range {
                    // Use file level pruning
                    let selections = self.plan_by_seq_range(manifest, min_seq, max_seq);
                    selections.into_iter().map(|s| s.file_path).collect()
                } else {
                    // No seq filter - select all files
                    self.plan_all_files(manifest)
                };

                scanned = selected_files.len();
                skipped = total_batches.saturating_sub(scanned);

                for file_path in selected_files {
                    let full_path = storage_dir.join(&file_path);
                    // Find the schema_version for this file from manifest
                    if let Some(segment) = manifest.segments.iter().find(|s| s.path == file_path) {
                        file_schema_versions.insert(full_path.clone(), segment.schema_version);
                    }
                    parquet_files.push(full_path);
                }
            }
        }

        // Fallback: only when no manifest (or degraded mode)
        if parquet_files.is_empty()
            && (manifest_opt.is_none() || use_degraded_mode)
        {
            let entries = fs::read_dir(storage_dir).map_err(KalamDbError::Io)?;
            for entry in entries {
                let entry = entry.map_err(KalamDbError::Io)?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("parquet") {
                    parquet_files.push(path);
                }
            }
        }

        // Return empty batch if no files found
        if parquet_files.is_empty() {
            return Ok((
                RecordBatch::new_empty(schema),
                (total_batches, skipped, scanned),
            ));
        }

        // Read all Parquet files and merge batches
        let mut all_batches = Vec::new();

        for parquet_file in &parquet_files {
            let file = fs::File::open(parquet_file).map_err(KalamDbError::Io)?;

            let builder = ParquetRecordBatchReaderBuilder::try_new(file)
                .into_arrow_error_ctx("Failed to create Parquet reader")?;

            let reader = builder.build()
                .into_arrow_error_ctx("Failed to build Parquet reader")?;

            // Get the schema version for this file (default to 1 if not found)
            let file_schema_version = file_schema_versions.get(parquet_file).copied().unwrap_or(1);
            
            // Check if schema version matches current version
            let current_version = app_context
                .schema_registry()
                .get(table_id)
                .map(|cached| cached.table.schema_version)
                .unwrap_or(1);

            // Read all batches from this file
            for batch_result in reader {
                let batch = batch_result
                    .into_arrow_error_ctx("Failed to read Parquet batch")?;
                
                // If schema versions differ, project the batch to current schema
                let projected_batch = if file_schema_version != current_version {
                    self.project_batch_to_current_schema(
                        batch,
                        file_schema_version,
                        &schema,
                        table_id,
                        app_context,
                    )?
                } else {
                    batch
                };
                
                all_batches.push(projected_batch);
            }
        }

        // Return empty batch if all files were empty
        if all_batches.is_empty() {
            return Ok((
                RecordBatch::new_empty(schema),
                (total_batches, skipped, scanned),
            ));
        }

        // Concatenate all batches
        let combined =
            datafusion::arrow::compute::concat_batches(&schema, &all_batches)
                .into_arrow_error_ctx("Failed to concatenate Parquet batches")?;

        Ok((combined, (total_batches, skipped, scanned)))
    }

    /// Simple planner: select files overlapping a given `_seq` range
    ///
    /// This is a first step towards full predicate-based pruning.
    pub fn plan_by_seq_range(
        &self,
        manifest: &Manifest,
        min_seq: i64,
        max_seq: i64,
    ) -> Vec<RowGroupSelection> {
        if manifest.segments.is_empty() {
            return Vec::new();
        }

        let mut selections: Vec<RowGroupSelection> = Vec::new();

        for segment in &manifest.segments {
            // Skip segments that don't overlap at all
            if segment.max_seq < min_seq || segment.min_seq > max_seq {
                continue;
            }

            // We don't have row group stats anymore, so we select the whole file
            selections.push(RowGroupSelection::new(segment.path.clone(), Vec::new()));
        }

        selections
    }

    /// Project a RecordBatch from an old schema version to the current schema
    ///
    /// Handles:
    /// - New columns added after flush (filled with NULLs)
    /// - Dropped columns (removed from projection)
    /// - Column reordering
    ///
    /// # Arguments
    /// * `batch` - RecordBatch with old schema
    /// * `old_schema_version` - Schema version used when data was flushed
    /// * `current_schema` - Target Arrow schema (current version)
    /// * `table_id` - Table identifier
    /// * `app_context` - AppContext for accessing historical schemas
    fn project_batch_to_current_schema(
        &self,
        batch: RecordBatch,
        old_schema_version: u32,
        current_schema: &SchemaRef,
        table_id: &TableId,
        app_context: &Arc<AppContext>,
    ) -> Result<RecordBatch, KalamDbError> {
        // Get the historical schema definition
        let old_table_def = app_context
            .system_tables()
            .tables()
            .get_version(table_id, old_schema_version)
            .into_kalamdb_error(&format!("Failed to retrieve schema version {}", old_schema_version))?
            .ok_or_else(|| {
                KalamDbError::Other(format!(
                    "Schema version {} not found for table {}",
                    old_schema_version, table_id
                ))
            })?;

        let old_schema = old_table_def.to_arrow_schema()
            .into_arrow_error_ctx("Failed to convert old schema to Arrow")?;

        // If schemas are identical, no projection needed
        if old_schema.fields() == current_schema.fields() {
            return Ok(batch);
        }

        log::debug!(
            "[Schema Evolution] Projecting batch from schema v{} to current schema for table {}",
            old_schema_version,
            table_id
        );

        // Build projection: for each field in current_schema, find it in old_schema or create NULL array
        let mut projected_columns: Vec<Arc<dyn datafusion::arrow::array::Array>> = Vec::new();

        for current_field in current_schema.fields() {
            // Check if field exists in old schema
            if let Ok(old_col_index) = old_schema.index_of(current_field.name()) {
                // Column existed in old schema - extract it
                let old_column = batch.column(old_col_index).clone();
                
                // Check if data types match
                let old_field = old_schema.field(old_col_index);
                if old_field.data_type() == current_field.data_type() {
                    // Types match - use as-is
                    projected_columns.push(old_column);
                } else {
                    // Type changed - attempt cast
                    let casted = cast(&old_column, current_field.data_type())
                        .into_arrow_error_ctx(&format!(
                            "Failed to cast column '{}' from {:?} to {:?}",
                            current_field.name(),
                            old_field.data_type(),
                            current_field.data_type()
                        ))?;
                    projected_columns.push(casted);
                }
            } else {
                // Column didn't exist in old schema - create NULL array
                use datafusion::arrow::array::{new_null_array, ArrayRef};
                let null_array: ArrayRef = new_null_array(current_field.data_type(), batch.num_rows());
                projected_columns.push(null_array);
                
                log::trace!(
                    "[Schema Evolution] Column '{}' not in old schema v{}, filled with NULLs",
                    current_field.name(),
                    old_schema_version
                );
            }
        }

        // Create new RecordBatch with projected columns
        let projected_batch = RecordBatch::try_new(current_schema.clone(), projected_columns)
            .into_arrow_error_ctx("Failed to create projected RecordBatch")?;

        Ok(projected_batch)
    }

    /// Prune segments that definitely cannot contain a PK value based on column_stats min/max
    ///
    /// Returns segments where the PK value could exist (i.e., value is within [min, max] range).
    /// If a segment has no column_stats for the PK column, it's included (conservative).
    ///
    /// # Arguments
    /// * `manifest` - The manifest containing segment metadata
    /// * `pk_column` - Name of the primary key column
    /// * `pk_value` - The PK value to search for (as string for comparison)
    ///
    /// # Returns
    /// List of segment file paths that could contain the PK value
    pub fn plan_by_pk_value(
        &self,
        manifest: &Manifest,
        pk_column: &str,
        pk_value: &str,
    ) -> Vec<String> {
        if manifest.segments.is_empty() {
            return Vec::new();
        }

        let mut selected_paths: Vec<String> = Vec::new();

        for segment in &manifest.segments {
            // Skip tombstoned segments
            if segment.tombstone {
                continue;
            }

            // Check if segment has column_stats for the PK column
            if let Some(stats) = segment.column_stats.get(pk_column) {
                // Check if PK value could be in this segment's range
                if !Self::pk_value_in_range(pk_value, stats) {
                    // Definitely not in this segment, skip
                    continue;
                }
            }
            // No column_stats for PK column = conservative, include the segment

            selected_paths.push(segment.path.clone());
        }

        selected_paths
    }

    /// Check if a PK value could be within the min/max range of column stats
    ///
    /// Supports string and numeric comparisons.
    fn pk_value_in_range(pk_value: &str, stats: &kalamdb_commons::types::ColumnStats) -> bool {
        // If no min/max stats, conservatively assume it could be in range
        let (Some(min), Some(max)) = (&stats.min, &stats.max) else {
            return true;
        };

        // Try numeric comparison first (most common for PKs)
        if let Ok(pk_num) = pk_value.parse::<i64>() {
            let min_num = Self::json_value_as_i64(min);
            let max_num = Self::json_value_as_i64(max);

            if let (Some(min_n), Some(max_n)) = (min_num, max_num) {
                return pk_num >= min_n && pk_num <= max_n;
            }
        }

        // Fall back to string comparison
        let min_str = Self::json_value_as_str(min);
        let max_str = Self::json_value_as_str(max);

        if let (Some(min_s), Some(max_s)) = (min_str, max_str) {
            return pk_value >= min_s.as_str() && pk_value <= max_s.as_str();
        }

        // Can't compare, conservatively include
        true
    }

    /// Extract i64 from serde_json::Value
    fn json_value_as_i64(value: &serde_json::Value) -> Option<i64> {
        match value {
            serde_json::Value::Number(n) => n.as_i64(),
            serde_json::Value::String(s) => s.parse::<i64>().ok(),
            _ => None,
        }
    }

    /// Extract String from serde_json::Value
    fn json_value_as_str(value: &serde_json::Value) -> Option<String> {
        match value {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_context::AppContext;
    use datafusion::arrow::array::{Int32Array, Int64Array};
    use datafusion::arrow::datatypes::Schema;
    use datafusion::arrow::record_batch::RecordBatch as ArrowRecordBatch;
    use kalamdb_commons::types::{Manifest, SegmentMetadata};
    use kalamdb_commons::{NamespaceId, TableId, TableName};
    use parquet::arrow::arrow_writer::ArrowWriter;
    use std::fs as stdfs;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_manifest_with_segments() -> Manifest {
        use std::collections::HashMap;

        let table_id = TableId::new(NamespaceId::new("ns"), TableName::new("tbl"));
        let mut mf = Manifest::new(table_id, None);

        let s0 = SegmentMetadata::new(
            "uuid-0".to_string(),
            "batch-0.parquet".to_string(),
            HashMap::new(),
            0,
            99,
            100,
            1024,
        );
        mf.add_segment(s0);

        let s1 = SegmentMetadata::new(
            "uuid-1".to_string(),
            "batch-1.parquet".to_string(),
            HashMap::new(),
            100,
            199,
            100,
            2048,
        );
        mf.add_segment(s1);

        mf
    }

    fn make_test_app_context() -> Arc<AppContext> {
        let base = std::env::temp_dir();
        let unique = format!(
            "kalamdb_test_ctx_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let dir = base.join(unique);
        stdfs::create_dir_all(&dir).unwrap();
        
        Arc::new(AppContext::new_test())
    }

    #[test]
    fn test_plan_by_seq_range_overlaps() {
        let mf = make_manifest_with_segments();
        let planner = ManifestAccessPlanner::new();

        let plan = planner.plan_by_seq_range(&mf, 25, 150);
        assert_eq!(plan.len(), 2);

        let p0 = plan
            .iter()
            .find(|p| p.file_path == "batch-0.parquet")
            .unwrap();
        assert!(p0.row_groups.is_empty());

        let p1 = plan
            .iter()
            .find(|p| p.file_path == "batch-1.parquet")
            .unwrap();
        assert!(p1.row_groups.is_empty());
    }

    #[test]
    fn test_plan_by_seq_range_no_overlap() {
        let mf = make_manifest_with_segments();
        let planner = ManifestAccessPlanner::new();

        let plan = planner.plan_by_seq_range(&mf, 300, 400);
        assert!(plan.is_empty());
    }

    #[tokio::test]
    async fn test_scan_parquet_files_empty_dir_returns_empty_batch() {
        // Create a unique temp directory without any parquet files
        let base = std::env::temp_dir();
        let unique = format!(
            "kalamdb_test_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let dir = base.join(unique);
        stdfs::create_dir_all(&dir).unwrap();

        // Empty schema
        let schema: SchemaRef = Arc::new(Schema::new(
            vec![] as Vec<datafusion::arrow::datatypes::Field>
        ));

        let app_context = make_test_app_context();
        let table_id = TableId::new(NamespaceId::new("ns"), TableName::new("tbl"));
        
        let planner = ManifestAccessPlanner::new();
        let (batch, (_total, _skipped, _scanned)) = planner
            .scan_parquet_files(None, &dir, None, false, schema.clone(), &table_id, &app_context)
            .expect("planner should handle empty directory");

        assert_eq!(batch.num_rows(), 0, "Expected empty batch for empty dir");

        // Cleanup
        stdfs::remove_dir_all(&dir).unwrap();
    }

    fn write_parquet_with_rows(path: &std::path::Path, schema: &SchemaRef, rows: usize) {
        let file = stdfs::File::create(path).unwrap();
        let mut writer = ArrowWriter::try_new(file, schema.clone(), None).unwrap();
        if rows > 0 {
            let seq_values: Vec<i64> = (0..rows as i64).collect();
            let val_values: Vec<i32> = (0..rows as i32).collect();
            let seq = Int64Array::from(seq_values);
            let val = Int32Array::from(val_values);
            let batch =
                ArrowRecordBatch::try_new(schema.clone(), vec![Arc::new(seq), Arc::new(val)])
                    .unwrap();
            writer.write(&batch).unwrap();
        }
        writer.close().unwrap();
    }

    #[tokio::test]
    async fn test_pruning_stats_with_manifest_and_seq_ranges() {
        // Temp dir
        let base = std::env::temp_dir();
        let unique = format!(
            "kalamdb_test_prune_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let dir = base.join(unique);
        stdfs::create_dir_all(&dir).unwrap();

        // Schema: _seq: Int64, val: Int32
        use datafusion::arrow::datatypes::{DataType, Field};
        let schema: SchemaRef = Arc::new(Schema::new(vec![
            Field::new("_seq", DataType::Int64, false),
            Field::new("val", DataType::Int32, false),
        ]));

        // Create two files
        let f0 = dir.join("batch-0.parquet");
        let f1 = dir.join("batch-1.parquet");
        write_parquet_with_rows(&f0, &schema, 10);
        write_parquet_with_rows(&f1, &schema, 10);

        // Manifest with two segments
        use std::collections::HashMap;
        let table_id = TableId::new(NamespaceId::new("ns"), TableName::new("tbl"));
        let mut mf = Manifest::new(table_id.clone(), None);

        let s0 = SegmentMetadata::new(
            "uuid-0".to_string(),
            "batch-0.parquet".to_string(),
            HashMap::new(),
            0,
            99,
            10,
            123,
        );
        mf.add_segment(s0);
        let s1 = SegmentMetadata::new(
            "uuid-1".to_string(),
            "batch-1.parquet".to_string(),
            HashMap::new(),
            100,
            199,
            10,
            123,
        );
        mf.add_segment(s1);

        let app_context = make_test_app_context();
        let planner = ManifestAccessPlanner::new();

        // Range overlaps only first file
        let (batch, (total, skipped, scanned)) = planner
            .scan_parquet_files(Some(&mf), &dir, Some((0, 90)), false, schema.clone(), &table_id, &app_context)
            .expect("planner should read files");
        assert_eq!(total, 2);
        assert_eq!(scanned, 1);
        assert_eq!(skipped, 1);
        // rows: full file read (we don't prune row-groups yet)
        assert_eq!(batch.num_rows(), 10);

        // Range overlaps only second file
        let (batch2, (total2, skipped2, scanned2)) = planner
            .scan_parquet_files(Some(&mf), &dir, Some((150, 160)), false, schema.clone(), &table_id, &app_context)
            .expect("planner should read files");
        assert_eq!(total2, 2);
        assert_eq!(scanned2, 1);
        assert_eq!(skipped2, 1);
        assert_eq!(batch2.num_rows(), 10);

        // Range overlaps none -> scanned 0, empty batch, no fallback because manifest exists
        let (batch3, (total3, skipped3, scanned3)) = planner
            .scan_parquet_files(Some(&mf), &dir, Some((300, 400)), false, schema.clone(), &table_id, &app_context)
            .expect("planner should handle no-overlap");
        assert_eq!(total3, 2);
        assert_eq!(scanned3, 0);
        assert_eq!(skipped3, 2);
        assert_eq!(batch3.num_rows(), 0);

        // Cleanup
        stdfs::remove_dir_all(&dir).unwrap();
    }
}
