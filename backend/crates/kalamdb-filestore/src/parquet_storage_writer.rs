//! Unified Parquet writer for all storage backends.
//!
//! Uses `object_store` for all storage types (local, S3, GCS, Azure).
//! Writes Parquet data directly via the ObjectStore::put() API.

use crate::error::{FilestoreError, Result};
use crate::object_store_factory::{build_object_store, object_key_for_path};
use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use kalamdb_commons::system::Storage;
use object_store::ObjectStore;
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, Encoding};
use parquet::file::properties::WriterProperties;

/// Result of a Parquet write operation.
#[derive(Debug, Clone)]
pub struct ParquetWriteResult {
    /// Size of the written data in bytes.
    pub size_bytes: u64,
}

/// Write RecordBatches to Parquet format and store via object_store.
///
/// This function:
/// 1. Serializes the Arrow data to Parquet format in memory
/// 2. Uploads the bytes via ObjectStore::put() (works for local and remote)
///
/// The `destination_path` is a full path (local) or URL (remote) that will be
/// mapped to an object key relative to the storage's base directory.
pub async fn write_parquet_to_storage(
    storage: &Storage,
    destination_path: &str,
    schema: SchemaRef,
    batches: Vec<RecordBatch>,
    bloom_filter_columns: Option<Vec<String>>,
) -> Result<ParquetWriteResult> {
    // Build the object store for this storage
    let store = build_object_store(storage)?;

    // Compute the object key relative to the store's root
    let key = object_key_for_path(storage, destination_path)?;

    // Serialize batches to Parquet bytes in memory
    let parquet_bytes = serialize_to_parquet(schema, batches, bloom_filter_columns)?;
    let size_bytes = parquet_bytes.len() as u64;

    // Write via object_store (atomic for all backends)
    store
        .put(&key, parquet_bytes.into())
        .await
        .map_err(|e| FilestoreError::ObjectStore(e.to_string()))?;

    Ok(ParquetWriteResult { size_bytes })
}

/// Synchronous wrapper for use in non-async contexts.
pub fn write_parquet_to_storage_sync(
    storage: &Storage,
    destination_path: &str,
    schema: SchemaRef,
    batches: Vec<RecordBatch>,
    bloom_filter_columns: Option<Vec<String>>,
) -> Result<ParquetWriteResult> {
    // Try to use existing tokio runtime, or create a new one
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        // Use spawn_blocking to avoid blocking the runtime
        std::thread::scope(|s| {
            s.spawn(|| {
                handle.block_on(write_parquet_to_storage(
                    storage,
                    destination_path,
                    schema,
                    batches,
                    bloom_filter_columns,
                ))
            })
            .join()
            .map_err(|_| FilestoreError::Other("Thread panicked".into()))?
        })
    } else {
        // No runtime available, create a minimal one
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| FilestoreError::Other(format!("Failed to create runtime: {e}")))?;

        rt.block_on(write_parquet_to_storage(
            storage,
            destination_path,
            schema,
            batches,
            bloom_filter_columns,
        ))
    }
}

/// Serialize Arrow RecordBatches to Parquet format in memory.
fn serialize_to_parquet(
    schema: SchemaRef,
    batches: Vec<RecordBatch>,
    bloom_filter_columns: Option<Vec<String>>,
) -> Result<Bytes> {
    // Build writer properties
    let mut props_builder = WriterProperties::builder()
        .set_compression(Compression::SNAPPY)
        .set_encoding(Encoding::PLAIN)
        .set_max_row_group_size(128 * 1024); // 128K rows per group

    // Add bloom filters for specified columns
    if let Some(cols) = bloom_filter_columns {
        for col in cols {
            let col_path: parquet::schema::types::ColumnPath = col.into();
            props_builder = props_builder.set_column_bloom_filter_enabled(col_path.clone(), true);
            props_builder = props_builder.set_column_bloom_filter_fpp(col_path.clone(), 0.01);
            props_builder = props_builder.set_column_bloom_filter_ndv(col_path, 1_000_000);
        }
    }

    let props = props_builder.build();

    // Write to in-memory buffer
    let mut buffer = Vec::with_capacity(1024 * 1024); // 1MB initial capacity
    {
        let mut writer = ArrowWriter::try_new(&mut buffer, schema, Some(props))
            .map_err(|e| FilestoreError::Parquet(e.to_string()))?;

        for batch in batches {
            writer
                .write(&batch)
                .map_err(|e| FilestoreError::Parquet(e.to_string()))?;
        }

        writer
            .close()
            .map_err(|e| FilestoreError::Parquet(e.to_string()))?;
    }

    Ok(Bytes::from(buffer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::StringArray;
    use arrow::datatypes::{DataType, Field, Schema};
    use std::sync::Arc;

    fn make_test_batch() -> (SchemaRef, Vec<RecordBatch>) {
        let schema = Arc::new(Schema::new(vec![Field::new("name", DataType::Utf8, false)]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(StringArray::from(vec!["alice", "bob", "charlie"]))],
        )
        .unwrap();
        (schema, vec![batch])
    }

    #[test]
    fn test_serialize_to_parquet() {
        let (schema, batches) = make_test_batch();
        let bytes = serialize_to_parquet(schema, batches, None).unwrap();
        assert!(!bytes.is_empty());
        // Parquet magic number at start
        assert_eq!(&bytes[0..4], b"PAR1");
    }
}
