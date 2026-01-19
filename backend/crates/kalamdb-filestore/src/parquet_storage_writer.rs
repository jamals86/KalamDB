//! Unified Parquet writer for all storage backends.
//!
//! Uses `object_store` for all storage types (local, S3, GCS, Azure).
//! Writes Parquet data directly via the ObjectStore::put() API.

use crate::error::{FilestoreError, Result};
use crate::object_store_factory::{build_object_store, object_key_for_path};
use arrow::array::{Array, Int64Array};
use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use datafusion::arrow::compute::{self, SortOptions};
use kalamdb_commons::constants::SystemColumnNames;
use kalamdb_commons::system::Storage;
use object_store::ObjectStore;
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::WriterProperties;
use std::sync::Arc;

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

/// Write RecordBatches to Parquet using a pre-built ObjectStore (cached variant).
///
/// This variant avoids the overhead of `build_object_store()` on each call by
/// accepting a pre-built `Arc<dyn ObjectStore>`. Use this for high-frequency
/// operations where the ObjectStore is cached (e.g., in CachedTableData).
///
/// **Performance**: Saves ~50-200μs per write for cloud backends.
pub async fn write_parquet_with_store(
    store: Arc<dyn ObjectStore>,
    storage: &Storage,
    destination_path: &str,
    schema: SchemaRef,
    batches: Vec<RecordBatch>,
    bloom_filter_columns: Option<Vec<String>>,
) -> Result<ParquetWriteResult> {
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

/// Synchronous wrapper for write_parquet_with_store.
///
/// Use this when you have a cached ObjectStore and need synchronous execution.
pub fn write_parquet_with_store_sync(
    store: Arc<dyn ObjectStore>,
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
                handle.block_on(write_parquet_with_store(
                    store,
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

        rt.block_on(write_parquet_with_store(
            store,
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
    const MIN_ROWS_FOR_BLOOM_FILTERS: u64 = 1024;

    let batches = sort_batches_by_seq(batches)?;
    let total_rows: u64 = batches.iter().map(|b| b.num_rows() as u64).sum();
    let bloom_ndv_estimate = total_rows.max(1);
    let bloom_filter_columns = if total_rows < MIN_ROWS_FOR_BLOOM_FILTERS {
        None
    } else {
        bloom_filter_columns
    };

    // Build writer properties
    let mut props_builder = WriterProperties::builder()
        .set_compression(Compression::ZSTD(zstd_level()))
        .set_max_row_group_size(128 * 1024); // 128K rows per group

    // Add bloom filters for specified columns
    if let Some(cols) = bloom_filter_columns {
        for col in cols {
            let col_path: parquet::schema::types::ColumnPath = col.into();
            props_builder = props_builder.set_column_bloom_filter_enabled(col_path.clone(), true);
            props_builder = props_builder.set_column_bloom_filter_fpp(col_path.clone(), 0.01);
            props_builder = props_builder.set_column_bloom_filter_ndv(col_path, bloom_ndv_estimate);
        }
    }

    let props = props_builder.build();

    // Write to in-memory buffer
    let mut buffer = Vec::with_capacity(1024 * 1024); // 1MB initial capacity
    {
        let mut writer = ArrowWriter::try_new(&mut buffer, schema, Some(props))
            .map_err(|e| FilestoreError::Parquet(e.to_string()))?;

        for batch in batches {
            writer.write(&batch).map_err(|e| FilestoreError::Parquet(e.to_string()))?;
        }

        writer.close().map_err(|e| FilestoreError::Parquet(e.to_string()))?;
    }

    Ok(Bytes::from(buffer))
}

fn zstd_level() -> ZstdLevel {
    // Prefer a reasonable default level; keep this small to avoid heavy CPU on flush.
    // If the Parquet crate changes accepted ranges, fall back to default.
    ZstdLevel::try_new(1).unwrap_or_default()
}

fn sort_batches_by_seq(batches: Vec<RecordBatch>) -> Result<Vec<RecordBatch>> {
    batches.into_iter().map(sort_record_batch_by_seq).collect()
}

fn sort_record_batch_by_seq(batch: RecordBatch) -> Result<RecordBatch> {
    let Some(seq_idx) =
        batch.schema().fields().iter().position(|f| f.name() == SystemColumnNames::SEQ)
    else {
        return Ok(batch);
    };

    if is_sorted_by_seq(&batch, seq_idx)? {
        return Ok(batch);
    }

    let seq_col = batch.column(seq_idx);
    let indices = compute::sort_to_indices(
        seq_col.as_ref(),
        Some(SortOptions {
            descending: false,
            nulls_first: false,
        }),
        None,
    )
    .map_err(|e| FilestoreError::Other(format!("Failed to sort by _seq: {e}")))?;

    let new_columns: Result<Vec<_>> = batch
        .columns()
        .iter()
        .map(|col| {
            compute::take(col.as_ref(), &indices, None)
                .map_err(|e| FilestoreError::Other(format!("Failed to apply sort indices: {e}")))
        })
        .collect();

    RecordBatch::try_new(batch.schema(), new_columns?)
        .map_err(|e| FilestoreError::Other(format!("Failed to build sorted RecordBatch: {e}")))
}

fn is_sorted_by_seq(batch: &RecordBatch, seq_idx: usize) -> Result<bool> {
    if batch.num_rows() <= 1 {
        return Ok(true);
    }

    let seq_col = batch.column(seq_idx);
    let Some(seq_array) = seq_col.as_any().downcast_ref::<Int64Array>() else {
        // If _seq isn't Int64 in this batch, fall back to sorting.
        return Ok(false);
    };

    if seq_array.null_count() > 0 {
        // System column should be non-null; treat nulls as unsorted.
        return Ok(false);
    }

    let mut prev = seq_array.value(0);
    for i in 1..seq_array.len() {
        let cur = seq_array.value(i);
        if cur < prev {
            return Ok(false);
        }
        prev = cur;
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::StringArray;
    use kalamdb_commons::arrow_utils::{field_int64, field_utf8, schema};
    use std::sync::Arc;

    fn make_test_batch() -> (SchemaRef, Vec<RecordBatch>) {
        let schema = schema(vec![field_utf8("name", false)]);
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

    #[test]
    fn test_write_parquet_to_storage_sync() {
        use crate::build_object_store;
        use kalamdb_commons::models::ids::StorageId;
        use kalamdb_commons::models::storage::StorageType;
        use kalamdb_commons::models::types::Storage;
        use std::env;
        use std::fs;

        let temp_dir = env::temp_dir().join("kalamdb_test_write_storage");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        let storage = Storage {
            storage_id: StorageId::from("test_write"),
            storage_name: "test_write".to_string(),
            description: None,
            storage_type: StorageType::Filesystem,
            base_directory: temp_dir.to_string_lossy().to_string(),
            credentials: None,
            config_json: None,
            shared_tables_template: "{namespace}/{table}".to_string(),
            user_tables_template: "{namespace}/{user}/{table}".to_string(),
            created_at: now,
            updated_at: now,
        };

        let (schema, batches) = make_test_batch();
        let file_path = "test/output.parquet";

        let result = write_parquet_to_storage_sync(&storage, file_path, schema, batches, None);
        assert!(result.is_ok(), "Failed to write parquet");

        let write_result = result.unwrap();
        assert!(write_result.size_bytes > 0, "Should have written bytes");

        // Verify file exists
        let store = build_object_store(&storage).unwrap();
        use crate::object_store_ops::head_file_sync;
        let meta = head_file_sync(store, &storage, file_path);
        assert!(meta.is_ok(), "File should exist");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_write_parquet_with_store_sync() {
        use crate::build_object_store;
        use kalamdb_commons::models::ids::StorageId;
        use kalamdb_commons::models::storage::StorageType;
        use kalamdb_commons::models::types::Storage;
        use std::env;
        use std::fs;
        use std::sync::Arc;

        let temp_dir = env::temp_dir().join("kalamdb_test_write_with_store");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        let storage = Storage {
            storage_id: StorageId::from("test_write_store"),
            storage_name: "test_write_store".to_string(),
            description: None,
            storage_type: StorageType::Filesystem,
            base_directory: temp_dir.to_string_lossy().to_string(),
            credentials: None,
            config_json: None,
            shared_tables_template: "{namespace}/{table}".to_string(),
            user_tables_template: "{namespace}/{user}/{table}".to_string(),
            created_at: now,
            updated_at: now,
        };

        let store = build_object_store(&storage).unwrap();
        let (schema, batches) = make_test_batch();
        let file_path = "test/with_store.parquet";

        let result = write_parquet_with_store_sync(
            Arc::clone(&store),
            &storage,
            file_path,
            schema,
            batches,
            None,
        );
        assert!(result.is_ok());

        // Verify file exists and has content
        use crate::object_store_ops::head_file_sync;
        let meta = head_file_sync(store, &storage, file_path).unwrap();
        assert!(meta.size_bytes > 0);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_write_parquet_with_bloom_filters() {
        use crate::build_object_store;
        use arrow::array::Int64Array;
        use arrow::datatypes::{DataType, Field, Schema};
        use kalamdb_commons::models::ids::StorageId;
        use kalamdb_commons::models::storage::StorageType;
        use kalamdb_commons::models::types::Storage;
        use std::env;
        use std::fs;
        use std::sync::Arc;

        let temp_dir = env::temp_dir().join("kalamdb_test_write_bloom");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        let storage = Storage {
            storage_id: StorageId::from("test_bloom"),
            storage_name: "test_bloom".to_string(),
            description: None,
            storage_type: StorageType::Filesystem,
            base_directory: temp_dir.to_string_lossy().to_string(),
            credentials: None,
            config_json: None,
            shared_tables_template: "{namespace}/{table}".to_string(),
            user_tables_template: "{namespace}/{user}/{table}".to_string(),
            created_at: now,
            updated_at: now,
        };

        let store = build_object_store(&storage).unwrap();

        // Create batch with 1024+ rows for bloom filters
        let schema = schema(vec![
            field_int64("id", false),
            field_int64("_seq", false),
        ]);

        let ids: Vec<i64> = (0..1024).collect();
        let seqs: Vec<i64> = (0..1024).map(|i| i * 1000).collect();

        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(Int64Array::from(ids)),
                Arc::new(Int64Array::from(seqs)),
            ],
        )
        .unwrap();

        let bloom_columns = vec!["id".to_string(), "_seq".to_string()];
        let file_path = "test/with_bloom.parquet";

        let result = write_parquet_with_store_sync(
            Arc::clone(&store),
            &storage,
            file_path,
            schema,
            vec![batch],
            Some(bloom_columns),
        );
        assert!(result.is_ok(), "Should write with bloom filters");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_write_large_parquet() {
        use crate::build_object_store;
        use arrow::array::Int64Array;
        use arrow::datatypes::{DataType, Field, Schema};
        use kalamdb_commons::models::ids::StorageId;
        use kalamdb_commons::models::storage::StorageType;
        use kalamdb_commons::models::types::Storage;
        use std::env;
        use std::fs;
        use std::sync::Arc;

        let temp_dir = env::temp_dir().join("kalamdb_test_write_large");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        let storage = Storage {
            storage_id: StorageId::from("test_large"),
            storage_name: "test_large".to_string(),
            description: None,
            storage_type: StorageType::Filesystem,
            base_directory: temp_dir.to_string_lossy().to_string(),
            credentials: None,
            config_json: None,
            shared_tables_template: "{namespace}/{table}".to_string(),
            user_tables_template: "{namespace}/{user}/{table}".to_string(),
            created_at: now,
            updated_at: now,
        };

        let store = build_object_store(&storage).unwrap();

        // Create large batch (50K rows)
        let schema = schema(vec![field_int64("value", false)]);

        let values: Vec<i64> = (0..50_000).collect();
        let batch =
            RecordBatch::try_new(Arc::clone(&schema), vec![Arc::new(Int64Array::from(values))])
                .unwrap();

        let file_path = "test/large.parquet";

        let result =
            write_parquet_with_store_sync(store, &storage, file_path, schema, vec![batch], None);
        assert!(result.is_ok());

        let write_result = result.unwrap();
        assert!(write_result.size_bytes > 50_000, "Large file should have substantial size");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_write_creates_directories() {
        use kalamdb_commons::models::ids::StorageId;
        use kalamdb_commons::models::storage::StorageType;
        use kalamdb_commons::models::types::Storage;
        use std::env;
        use std::fs;

        let temp_dir = env::temp_dir().join("kalamdb_test_write_mkdir");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        let storage = Storage {
            storage_id: StorageId::from("test_mkdir"),
            storage_name: "test_mkdir".to_string(),
            description: None,
            storage_type: StorageType::Filesystem,
            base_directory: temp_dir.to_string_lossy().to_string(),
            credentials: None,
            config_json: None,
            shared_tables_template: "{namespace}/{table}".to_string(),
            user_tables_template: "{namespace}/{user}/{table}".to_string(),
            created_at: now,
            updated_at: now,
        };

        let (schema, batches) = make_test_batch();

        // Deep nested path
        let file_path = "deep/nested/path/to/file.parquet";

        let result = write_parquet_to_storage_sync(&storage, file_path, schema, batches, None);
        assert!(result.is_ok(), "Should create nested directories");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
