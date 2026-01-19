//! Parquet reading operations using object_store.
//!
//! Provides utilities to read Parquet files from any storage backend
//! (local, S3, GCS, Azure) using DataFusion's ParquetExec.

use crate::error::{FilestoreError, Result};
use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use datafusion::parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use kalamdb_commons::system::Storage;
use object_store::ObjectStore;
use std::sync::Arc;

/// Read all RecordBatches from a Parquet file using object_store.
///
/// This reads the entire file into memory, so use carefully for large files.
pub async fn read_parquet_batches(
    store: Arc<dyn ObjectStore>,
    storage: &Storage,
    parquet_path: &str,
) -> Result<Vec<RecordBatch>> {
    use crate::object_store_ops::read_file;

    // Read file bytes
    let bytes = read_file(Arc::clone(&store), storage, parquet_path).await?;

    // Parse Parquet from bytes (Bytes implements ChunkReader directly)
    let builder = ParquetRecordBatchReaderBuilder::try_new(bytes)
        .map_err(|e| FilestoreError::Parquet(e.to_string()))?;

    let row_group_count = builder.metadata().num_row_groups();
    let reader = builder.build().map_err(|e| FilestoreError::Parquet(e.to_string()))?;

    let mut batches = Vec::with_capacity(row_group_count);
    for batch_result in reader {
        let batch = batch_result.map_err(|e| FilestoreError::Parquet(e.to_string()))?;
        batches.push(batch);
    }

    Ok(batches)
}

/// Synchronous wrapper for read_parquet_batches.
pub fn read_parquet_batches_sync(
    store: Arc<dyn ObjectStore>,
    storage: &Storage,
    parquet_path: &str,
) -> Result<Vec<RecordBatch>> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        std::thread::scope(|s| {
            s.spawn(|| handle.block_on(read_parquet_batches(store, storage, parquet_path)))
                .join()
                .map_err(|_| FilestoreError::Other("Thread panicked".into()))?
        })
    } else {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| FilestoreError::Other(format!("Failed to create runtime: {e}")))?;

        rt.block_on(read_parquet_batches(store, storage, parquet_path))
    }
}

/// Read Parquet file schema without reading data.
pub async fn read_parquet_schema(
    store: Arc<dyn ObjectStore>,
    storage: &Storage,
    parquet_path: &str,
) -> Result<SchemaRef> {
    use crate::object_store_ops::read_file;

    // Read file bytes
    let bytes = read_file(Arc::clone(&store), storage, parquet_path).await?;

    // Parse schema from bytes (Bytes implements ChunkReader directly)
    let builder = ParquetRecordBatchReaderBuilder::try_new(bytes)
        .map_err(|e| FilestoreError::Parquet(e.to_string()))?;

    Ok(builder.schema().clone())
}

/// Synchronous wrapper for read_parquet_schema.
pub fn read_parquet_schema_sync(
    store: Arc<dyn ObjectStore>,
    storage: &Storage,
    parquet_path: &str,
) -> Result<SchemaRef> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        std::thread::scope(|s| {
            s.spawn(|| handle.block_on(read_parquet_schema(store, storage, parquet_path)))
                .join()
                .map_err(|_| FilestoreError::Other("Thread panicked".into()))?
        })
    } else {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| FilestoreError::Other(format!("Failed to create runtime: {e}")))?;

        rt.block_on(read_parquet_schema(store, storage, parquet_path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_object_store;
    use crate::parquet_storage_writer::write_parquet_with_store_sync;
    use arrow::array::{Array, BooleanArray, Float64Array, Int64Array, StringArray};
    use arrow::record_batch::RecordBatch;
    use kalamdb_commons::arrow_utils::{
        field_boolean, field_float64, field_int64, field_utf8, schema,
    };
    use kalamdb_commons::models::ids::StorageId;
    use kalamdb_commons::models::storage::StorageType;
    use kalamdb_commons::models::types::Storage;
    use std::env;
    use std::fs;

    fn create_test_storage(temp_dir: &std::path::Path) -> Storage {
        let now = chrono::Utc::now().timestamp_millis();
        Storage {
            storage_id: StorageId::from("test_parquet_read"),
            storage_name: "test_parquet_read".to_string(),
            description: None,
            storage_type: StorageType::Filesystem,
            base_directory: temp_dir.to_string_lossy().to_string(),
            credentials: None,
            config_json: None,
            shared_tables_template: "{namespace}/{table}".to_string(),
            user_tables_template: "{namespace}/{user}/{table}".to_string(),
            created_at: now,
            updated_at: now,
        }
    }

    fn create_simple_batch(num_rows: usize) -> RecordBatch {
        let schema = schema(vec![
            field_int64("id", false),
            field_utf8("name", true),
            field_int64("_seq", false),
        ]);

        let ids: Vec<i64> = (0..num_rows as i64).collect();
        let names: Vec<String> = (0..num_rows).map(|i| format!("name_{}", i)).collect();
        let seqs: Vec<i64> = (0..num_rows as i64).map(|i| i * 1000).collect();

        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(Int64Array::from(ids)),
                Arc::new(StringArray::from(names)),
                Arc::new(Int64Array::from(seqs)),
            ],
        )
        .unwrap()
    }

    #[test]
    fn test_read_parquet_batches_sync_simple() {
        let temp_dir = env::temp_dir().join("kalamdb_test_read_batches");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        // Write a test parquet file
        let batch = create_simple_batch(100);
        let schema = batch.schema();
        let file_path = "test/data.parquet";

        write_parquet_with_store_sync(
            Arc::clone(&store),
            &storage,
            file_path,
            schema,
            vec![batch.clone()],
            None,
        )
        .unwrap();

        // Read it back
        let read_batches = read_parquet_batches_sync(store, &storage, file_path).unwrap();

        assert_eq!(read_batches.len(), 1);
        assert_eq!(read_batches[0].num_rows(), 100);
        assert_eq!(read_batches[0].num_columns(), 3);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_read_parquet_schema_sync() {
        let temp_dir = env::temp_dir().join("kalamdb_test_read_schema");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        // Write a test parquet file
        let batch = create_simple_batch(50);
        let original_schema = batch.schema();
        let file_path = "test/schema_test.parquet";

        write_parquet_with_store_sync(
            Arc::clone(&store),
            &storage,
            file_path,
            Arc::clone(&original_schema),
            vec![batch],
            None,
        )
        .unwrap();

        // Read schema back (without reading data)
        let read_schema = read_parquet_schema_sync(store, &storage, file_path).unwrap();

        assert_eq!(read_schema.fields().len(), 3);
        assert_eq!(read_schema.field(0).name(), "id");
        assert_eq!(read_schema.field(1).name(), "name");
        assert_eq!(read_schema.field(2).name(), "_seq");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_read_empty_parquet_file() {
        let temp_dir = env::temp_dir().join("kalamdb_test_empty_parquet");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        // Create empty batch (0 rows)
        let schema = schema(vec![
            field_int64("id", false),
            field_utf8("value", true),
        ]);

        let empty_batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(Int64Array::from(Vec::<i64>::new())),
                Arc::new(StringArray::from(Vec::<String>::new())),
            ],
        )
        .unwrap();

        let file_path = "test/empty.parquet";
        write_parquet_with_store_sync(
            Arc::clone(&store),
            &storage,
            file_path,
            schema,
            vec![empty_batch],
            None,
        )
        .unwrap();

        // Read it back - empty files may return 0 batches or 1 batch with 0 rows
        let read_batches = read_parquet_batches_sync(store, &storage, file_path).unwrap();

        // Either no batches or one empty batch is acceptable
        if !read_batches.is_empty() {
            assert_eq!(read_batches[0].num_rows(), 0);
        }
        let total_rows: usize = read_batches.iter().map(|b| b.num_rows()).sum();
        assert_eq!(total_rows, 0, "Should have 0 total rows");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_read_multiple_batches() {
        let temp_dir = env::temp_dir().join("kalamdb_test_multi_batches");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        // Create multiple batches
        let batch1 = create_simple_batch(50);
        let batch2 = create_simple_batch(75);
        let batch3 = create_simple_batch(100);

        let schema = batch1.schema();
        let file_path = "test/multi_batch.parquet";

        write_parquet_with_store_sync(
            Arc::clone(&store),
            &storage,
            file_path,
            schema,
            vec![batch1, batch2, batch3],
            None,
        )
        .unwrap();

        // Read back
        let read_batches = read_parquet_batches_sync(store, &storage, file_path).unwrap();

        // Should be combined into batches based on row group size
        assert!(!read_batches.is_empty());

        let total_rows: usize = read_batches.iter().map(|b| b.num_rows()).sum();
        assert_eq!(total_rows, 50 + 75 + 100);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_read_parquet_with_different_types() {
        let temp_dir = env::temp_dir().join("kalamdb_test_types");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        // Schema with multiple data types
        let schema = schema(vec![
            field_int64("int_col", false),
            field_utf8("str_col", true),
            field_float64("float_col", true),
            field_boolean("bool_col", false),
        ]);

        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(Int64Array::from(vec![1, 2, 3])),
                Arc::new(StringArray::from(vec!["a", "b", "c"])),
                Arc::new(Float64Array::from(vec![1.1, 2.2, 3.3])),
                Arc::new(BooleanArray::from(vec![true, false, true])),
            ],
        )
        .unwrap();

        let file_path = "test/types.parquet";
        write_parquet_with_store_sync(
            Arc::clone(&store),
            &storage,
            file_path,
            schema,
            vec![batch.clone()],
            None,
        )
        .unwrap();

        // Read back
        let read_batches = read_parquet_batches_sync(store, &storage, file_path).unwrap();

        assert_eq!(read_batches.len(), 1);
        assert_eq!(read_batches[0].num_columns(), 4);
        assert_eq!(read_batches[0].num_rows(), 3);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_read_nonexistent_parquet() {
        let temp_dir = env::temp_dir().join("kalamdb_test_nonexistent");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        let result = read_parquet_batches_sync(store, &storage, "nonexistent/file.parquet");

        assert!(result.is_err(), "Should fail for nonexistent file");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_read_parquet_with_nulls() {
        let temp_dir = env::temp_dir().join("kalamdb_test_nulls");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        let schema = schema(vec![
            field_int64("id", false),
            field_utf8("nullable_str", true),
        ]);

        // Batch with null values
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(Int64Array::from(vec![1, 2, 3, 4])),
                Arc::new(StringArray::from(vec![Some("a"), None, Some("c"), None])),
            ],
        )
        .unwrap();

        let file_path = "test/nulls.parquet";
        write_parquet_with_store_sync(
            Arc::clone(&store),
            &storage,
            file_path,
            schema,
            vec![batch],
            None,
        )
        .unwrap();

        // Read back
        let read_batches = read_parquet_batches_sync(store, &storage, file_path).unwrap();

        assert_eq!(read_batches.len(), 1);
        assert_eq!(read_batches[0].num_rows(), 4);

        // Verify null handling
        let str_array = read_batches[0].column(1).as_any().downcast_ref::<StringArray>().unwrap();
        assert!(str_array.is_null(1));
        assert!(str_array.is_null(3));
        assert!(!str_array.is_null(0));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_read_large_parquet_file() {
        let temp_dir = env::temp_dir().join("kalamdb_test_large");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        // Create a large batch (10K rows)
        let large_batch = create_simple_batch(10_000);
        let schema = large_batch.schema();
        let file_path = "test/large.parquet";

        write_parquet_with_store_sync(
            Arc::clone(&store),
            &storage,
            file_path,
            schema,
            vec![large_batch],
            None,
        )
        .unwrap();

        // Read back
        let read_batches = read_parquet_batches_sync(store, &storage, file_path).unwrap();

        let total_rows: usize = read_batches.iter().map(|b| b.num_rows()).sum();
        assert_eq!(total_rows, 10_000);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
