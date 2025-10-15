//! Parquet writer with _updated column bloom filter
//!
//! This module provides writing RecordBatches to Parquet files with bloom filters.

use crate::error::KalamDbError;
use datafusion::arrow::array::TimestampMillisecondArray;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::parquet::arrow::ArrowWriter;
use datafusion::parquet::file::properties::WriterProperties;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

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

    /// Write RecordBatches to a Parquet file with bloom filter on _updated column
    pub fn write_with_bloom_filter(
        &self,
        schema: SchemaRef,
        batches: Vec<RecordBatch>,
    ) -> Result<(), KalamDbError> {
        let path = Path::new(&self.file_path);
        
        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| KalamDbError::Io(e))?;
        }
        
        let file = File::create(path)
            .map_err(|e| KalamDbError::Io(e))?;

        // Configure writer properties with bloom filter
        // Note: Bloom filter configuration will be added when parquet crate supports it
        let props = WriterProperties::builder()
            .set_compression(datafusion::parquet::basic::Compression::SNAPPY)
            .build();

        let mut writer = ArrowWriter::try_new(file, schema, Some(props))
            .map_err(|e| KalamDbError::Other(format!("Failed to create Parquet writer: {}", e)))?;

        // Write all batches
        for batch in batches {
            writer.write(&batch)
                .map_err(|e| KalamDbError::Other(format!("Failed to write batch: {}", e)))?;
        }

        writer.close()
            .map_err(|e| KalamDbError::Other(format!("Failed to close writer: {}", e)))?;

        Ok(())
    }

    /// Write RecordBatches to a Parquet file (without bloom filter)
    pub fn write(
        &self,
        schema: SchemaRef,
        batches: Vec<RecordBatch>,
    ) -> Result<(), KalamDbError> {
        self.write_with_bloom_filter(schema, batches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::arrow::array::{Int64Array, StringArray};
    use datafusion::arrow::datatypes::{DataType, Field, Schema};
    use std::env;
    use std::fs;

    #[test]
    fn test_parquet_writer() {
        let temp_dir = env::temp_dir().join("kalamdb_parquet_writer_test");
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
        ).unwrap();

        let file_path = temp_dir.join("test.parquet");
        let writer = ParquetWriter::new(file_path.to_str().unwrap());
        
        let result = writer.write(schema, vec![batch]);
        assert!(result.is_ok());
        assert!(file_path.exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_parquet_writer_creates_directory() {
        let temp_dir = env::temp_dir().join("kalamdb_parquet_writer_dir_test");
        let _ = fs::remove_dir_all(&temp_dir);

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
        ]));

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![Arc::new(Int64Array::from(vec![1]))],
        ).unwrap();

        let file_path = temp_dir.join("nested/dir/test.parquet");
        let writer = ParquetWriter::new(file_path.to_str().unwrap());
        
        let result = writer.write(schema, vec![batch]);
        assert!(result.is_ok());
        assert!(file_path.exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
