//! Parquet writer with _seq column bloom filter
//!
//! Migrated from kalamdb-core/src/storage/parquet_writer.rs

use crate::error::{FilestoreError, Result};
use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use datafusion::parquet::arrow::ArrowWriter;
use datafusion::parquet::file::properties::{EnabledStatistics, WriterProperties};
use datafusion::parquet::schema::types::ColumnPath;
use kalamdb_commons::constants::SystemColumnNames;
use std::fs::File;
use std::path::Path;

/// Parquet writer with bloom filter support
pub struct ParquetWriter {
    /// Output file path
    file_path: String,
}

impl ParquetWriter {
    /// Create a new Parquet writer
    pub fn new(file_path: impl Into<String>) -> Self {
        Self {
            file_path: file_path.into(),
        }
    }

    /// Write RecordBatches to a Parquet file with bloom filters
    ///
    /// # Arguments
    /// * `schema` - Arrow schema for the data
    /// * `batches` - Record batches to write
    /// * `bloom_filter_columns` - Optional list of columns to enable Bloom filters on.
    ///   If None, defaults to [_seq] only.
    ///   Common usage: Pass PRIMARY KEY columns + _seq for optimal point query performance.
    ///
    /// # Bloom Filter Configuration
    /// - **False Positive Rate**: 1% (0.01) - balances space overhead vs accuracy
    /// - **Estimated NDV**: 100,000 - heuristic for filter sizing
    /// - **Target Columns**: _seq (default) + user-specified columns (typically PRIMARY KEY)
    ///
    /// # Example
    /// ```rust,ignore
    /// // Enable Bloom filters for PRIMARY KEY ("id") and _seq
    /// let bloom_cols = vec!["id".to_string(), "_seq".to_string()];
    /// writer.write_with_bloom_filter(schema, batches, Some(bloom_cols))?;
    /// ```
    pub fn write_with_bloom_filter(
        &self,
        schema: SchemaRef,
        batches: Vec<RecordBatch>,
        bloom_filter_columns: Option<Vec<String>>,
    ) -> Result<()> {
        let path = Path::new(&self.file_path);

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(FilestoreError::Io)?;
        }

        let file = File::create(path).map_err(FilestoreError::Io)?;

        // Configure writer properties with bloom filters
        let mut props_builder = WriterProperties::builder()
            .set_compression(datafusion::parquet::basic::Compression::SNAPPY)
            // Enable statistics for all columns (helps with query planning)
            .set_statistics_enabled(EnabledStatistics::Chunk)
            // Set row group size (optimize for time-range queries)
            .set_max_row_group_size(100_000);

        // Determine which columns should have Bloom filters
        let bloom_columns = bloom_filter_columns.unwrap_or_else(|| {
            // Default: only _seq column
            vec![SystemColumnNames::SEQ.to_string()]
        });

        // Enable bloom filters for specified columns
        for column_name in &bloom_columns {
            // Check if column exists in schema
            if schema.fields().iter().any(|f| f.name() == column_name) {
                // Enable bloom filter with 0.01 FPP (1% false positive rate)
                // ColumnPath::from() accepts a str and converts it to the proper type
                let column_path = ColumnPath::from(column_name.as_str());
                props_builder = props_builder
                    .set_column_bloom_filter_enabled(column_path.clone(), true)
                    .set_column_bloom_filter_fpp(column_path.clone(), 0.01)
                    .set_column_bloom_filter_ndv(column_path.clone(), 100_000)
                    // Enable per-page statistics so row selections can skip data pages later.
                    .set_column_statistics_enabled(column_path, EnabledStatistics::Page);

                log::debug!("✅ Enabled Bloom filter for column: {}", column_name);
            } else {
                log::warn!(
                    "⚠️  Column {} not found in schema, skipping Bloom filter",
                    column_name
                );
            }
        }

        let props = props_builder.build();

        let mut writer = ArrowWriter::try_new(file, schema, Some(props)).map_err(|e| {
            FilestoreError::Parquet(format!("Failed to create Parquet writer: {}", e))
        })?;

        // Write all batches
        for batch in batches {
            writer
                .write(&batch)
                .map_err(|e| FilestoreError::Parquet(format!("Failed to write batch: {}", e)))?;
        }

        writer
            .close()
            .map_err(|e| FilestoreError::Parquet(format!("Failed to close writer: {}", e)))?;

        Ok(())
    }

    /// Write RecordBatches to a Parquet file with default Bloom filters (only _seq)
    ///
    /// For custom Bloom filter configuration (e.g., including PRIMARY KEY columns),
    /// use `write_with_bloom_filter()` instead.
    pub fn write(&self, schema: SchemaRef, batches: Vec<RecordBatch>) -> Result<()> {
        self.write_with_bloom_filter(schema, batches, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{Int64Array, StringArray, TimestampMicrosecondArray};
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use datafusion::parquet::file::reader::{FileReader, SerializedFileReader};
    use std::env;
    use std::fs;
    use std::sync::Arc;

    #[test]
    fn test_parquet_writer() {
        let temp_dir = env::temp_dir().join("kalamdb_filestore_parquet_writer_test");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("content", DataType::Utf8, true),
        ]));

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(Int64Array::from(vec![1, 2, 3])),
                Arc::new(StringArray::from(vec!["a", "b", "c"])),
            ],
        )
        .unwrap();

        let file_path = temp_dir.join("test.parquet");
        let writer = ParquetWriter::new(file_path.to_str().unwrap());

        let result = writer.write(schema, vec![batch]);
        assert!(result.is_ok());
        assert!(file_path.exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_parquet_writer_creates_directory() {
        let temp_dir = env::temp_dir().join("kalamdb_filestore_parquet_writer_dir_test");
        let _ = fs::remove_dir_all(&temp_dir);

        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

        let batch = RecordBatch::try_new(schema.clone(), vec![Arc::new(Int64Array::from(vec![1]))])
            .unwrap();

        let file_path = temp_dir.join("nested/dir/test.parquet");
        let writer = ParquetWriter::new(file_path.to_str().unwrap());

        let result = writer.write(schema, vec![batch]);
        assert!(result.is_ok());
        assert!(file_path.exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_bloom_filter_enabled_for_seq_column() {
        let temp_dir = env::temp_dir().join("kalamdb_filestore_bloom_filter_test");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // Schema with _seq column
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("content", DataType::Utf8, true),
            Field::new("_seq", DataType::Int64, false),
        ]));

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(Int64Array::from(vec![1, 2, 3])),
                Arc::new(StringArray::from(vec!["a", "b", "c"])),
                Arc::new(Int64Array::from(vec![1000000, 2000000, 3000000])),
            ],
        )
        .unwrap();

        let file_path = temp_dir.join("with_bloom.parquet");
        let writer = ParquetWriter::new(file_path.to_str().unwrap());

        let result = writer.write_with_bloom_filter(schema, vec![batch], None);
        assert!(result.is_ok());
        assert!(file_path.exists());

        // Verify bloom filter is present in Parquet metadata
        let file = std::fs::File::open(&file_path).unwrap();
        let reader = SerializedFileReader::new(file).unwrap();
        let metadata = reader.metadata();

        // Check that file has row groups
        assert!(metadata.num_row_groups() > 0);

        // Get first row group metadata
        let row_group = metadata.row_group(0);

        // Find _seq column index
        let seq_col_idx = row_group
            .columns()
            .iter()
            .position(|col| col.column_path().string() == SystemColumnNames::SEQ)
            .expect("_seq column should exist");

        // Check that _seq column has bloom filter
        let seq_col_metadata = row_group.column(seq_col_idx);
        assert!(
            seq_col_metadata.bloom_filter_offset().is_some(),
            "_seq column should have bloom filter"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_no_bloom_filter_without_seq_column() {
        let temp_dir = env::temp_dir().join("kalamdb_filestore_no_bloom_filter_test");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // Schema without _seq column
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("content", DataType::Utf8, true),
        ]));

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(Int64Array::from(vec![1, 2, 3])),
                Arc::new(StringArray::from(vec!["a", "b", "c"])),
            ],
        )
        .unwrap();

        let file_path = temp_dir.join("without_bloom.parquet");
        let writer = ParquetWriter::new(file_path.to_str().unwrap());

        let result = writer.write_with_bloom_filter(schema, vec![batch], None);
        assert!(result.is_ok());
        assert!(file_path.exists());

        // Verify no bloom filters are present
        let file = std::fs::File::open(&file_path).unwrap();
        let reader = SerializedFileReader::new(file).unwrap();
        let metadata = reader.metadata();

        // Check that columns don't have bloom filters
        let row_group = metadata.row_group(0);
        for col_metadata in row_group.columns() {
            assert!(
                col_metadata.bloom_filter_offset().is_none(),
                "Column {} should not have bloom filter",
                col_metadata.column_path().string()
            );
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_statistics_enabled() {
        let temp_dir = env::temp_dir().join("kalamdb_filestore_statistics_test");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("_seq", DataType::Int64, false),
        ]));

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(Int64Array::from(vec![1, 2, 3, 4, 5])),
                Arc::new(Int64Array::from(vec![
                    1000000, 2000000, 3000000, 4000000, 5000000,
                ])),
            ],
        )
        .unwrap();

        let file_path = temp_dir.join("with_stats.parquet");
        let writer = ParquetWriter::new(file_path.to_str().unwrap());

        let result = writer.write_with_bloom_filter(schema, vec![batch], None);
        assert!(result.is_ok());

        // Verify statistics are present
        let file = std::fs::File::open(&file_path).unwrap();
        let reader = SerializedFileReader::new(file).unwrap();
        let metadata = reader.metadata();

        let row_group = metadata.row_group(0);
        for col_metadata in row_group.columns() {
            assert!(
                col_metadata.statistics().is_some(),
                "Column {} should have statistics",
                col_metadata.column_path().string()
            );
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_compression_is_snappy() {
        let temp_dir = env::temp_dir().join("kalamdb_filestore_compression_test");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

        let batch = RecordBatch::try_new(schema.clone(), vec![Arc::new(Int64Array::from(vec![1]))])
            .unwrap();

        let file_path = temp_dir.join("compressed.parquet");
        let writer = ParquetWriter::new(file_path.to_str().unwrap());

        let result = writer.write_with_bloom_filter(schema, vec![batch], None);
        assert!(result.is_ok());

        // Verify compression is SNAPPY
        let file = std::fs::File::open(&file_path).unwrap();
        let reader = SerializedFileReader::new(file).unwrap();
        let metadata = reader.metadata();

        let row_group = metadata.row_group(0);
        for col_metadata in row_group.columns() {
            assert_eq!(
                col_metadata.compression(),
                datafusion::parquet::basic::Compression::SNAPPY,
                "Column {} should use SNAPPY compression",
                col_metadata.column_path().string()
            );
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
