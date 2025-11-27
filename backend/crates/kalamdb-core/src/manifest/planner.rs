//! Manifest-driven access planner (Phase: pruning integration)
//!
//! Provides utilities to translate `Manifest` metadata into
//! concrete file/row-group selections for efficient reads.

use crate::error::KalamDbError;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use kalamdb_commons::types::Manifest;
use std::fs;
use std::path::PathBuf;

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
    /// Handles manifest-based pruning, file loading, and batch concatenation.
    ///
    /// # Arguments
    /// * `manifest_opt` - Optional manifest for metadata-driven selection
    /// * `storage_dir` - Base directory containing Parquet files
    /// * `seq_range` - Optional (min, max) seq range for pruning
    /// * `use_degraded_mode` - If true, skip manifest and list directory
    /// * `schema` - Arrow schema for empty batch if no files found
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
    ) -> Result<(RecordBatch, (usize, usize, usize)), KalamDbError> {
        let mut parquet_files: Vec<PathBuf> = Vec::new();
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
                    parquet_files.push(storage_dir.join(&file_path));
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

            let builder = ParquetRecordBatchReaderBuilder::try_new(file).map_err(|e| {
                KalamDbError::Other(format!("Failed to create Parquet reader: {}", e))
            })?;

            let reader = builder.build().map_err(|e| {
                KalamDbError::Other(format!("Failed to build Parquet reader: {}", e))
            })?;

            // Read all batches from this file
            for batch_result in reader {
                let batch = batch_result.map_err(|e| {
                    KalamDbError::Other(format!("Failed to read Parquet batch: {}", e))
                })?;
                all_batches.push(batch);
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
            datafusion::arrow::compute::concat_batches(&schema, &all_batches).map_err(|e| {
                KalamDbError::Other(format!("Failed to concatenate Parquet batches: {}", e))
            })?;

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::arrow::array::{Int32Array, Int64Array};
    use datafusion::arrow::datatypes::Schema;
    use datafusion::arrow::record_batch::RecordBatch as ArrowRecordBatch;
    use kalamdb_commons::types::{Manifest, SegmentMetadata};
    use parquet::arrow::arrow_writer::ArrowWriter;
    use std::fs as stdfs;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_manifest_with_segments() -> Manifest {
        use kalamdb_commons::{NamespaceId, TableId, TableName};
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

    #[test]
    fn test_scan_parquet_files_empty_dir_returns_empty_batch() {
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

        let planner = ManifestAccessPlanner::new();
        let (batch, (_total, _skipped, _scanned)) = planner
            .scan_parquet_files(None, &dir, None, false, schema.clone())
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

    #[test]
    fn test_pruning_stats_with_manifest_and_seq_ranges() {
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
        use kalamdb_commons::{NamespaceId, TableId, TableName};
        use std::collections::HashMap;
        let table_id = TableId::new(NamespaceId::new("ns"), TableName::new("tbl"));
        let mut mf = Manifest::new(table_id, None);

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

        let planner = ManifestAccessPlanner::new();

        // Range overlaps only first file
        let (batch, (total, skipped, scanned)) = planner
            .scan_parquet_files(Some(&mf), &dir, Some((0, 90)), false, schema.clone())
            .expect("planner should read files");
        assert_eq!(total, 2);
        assert_eq!(scanned, 1);
        assert_eq!(skipped, 1);
        // rows: full file read (we don't prune row-groups yet)
        assert_eq!(batch.num_rows(), 10);

        // Range overlaps only second file
        let (batch2, (total2, skipped2, scanned2)) = planner
            .scan_parquet_files(Some(&mf), &dir, Some((150, 160)), false, schema.clone())
            .expect("planner should read files");
        assert_eq!(total2, 2);
        assert_eq!(scanned2, 1);
        assert_eq!(skipped2, 1);
        assert_eq!(batch2.num_rows(), 10);

        // Range overlaps none -> scanned 0, empty batch, no fallback because manifest exists
        let (batch3, (total3, skipped3, scanned3)) = planner
            .scan_parquet_files(Some(&mf), &dir, Some((300, 400)), false, schema.clone())
            .expect("planner should handle no-overlap");
        assert_eq!(total3, 2);
        assert_eq!(scanned3, 0);
        assert_eq!(skipped3, 2);
        assert_eq!(batch3.num_rows(), 0);

        // Cleanup
        stdfs::remove_dir_all(&dir).unwrap();
    }
}
