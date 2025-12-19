use crate::error::{FilestoreError, Result};
use crate::object_store_ops::{delete_file_sync, head_file_sync, list_files_sync};
use kalamdb_commons::system::Storage;
use object_store::ObjectStore;
use std::path::PathBuf;
use std::sync::Arc;

/// Metadata about a batch file
#[derive(Debug, Clone)]
pub struct BatchFile {
    pub path: PathBuf,
    pub timestamp_ms: i64,
    pub batch_index: u64,
    pub size_bytes: u64,
}

/// Parse batch filename to extract timestamp and index.
/// Format: batch_{timestamp_ms}_{index:08}.parquet
fn parse_batch_filename(filename: &str) -> Result<(i64, u64)> {
    if !filename.starts_with("batch_") || !filename.ends_with(".parquet") {
        return Err(FilestoreError::InvalidBatchFile(format!(
            "Invalid batch filename format: {}",
            filename
        )));
    }

    let without_prefix = filename.strip_prefix("batch_").unwrap();
    let without_suffix = without_prefix.strip_suffix(".parquet").unwrap();
    let parts: Vec<&str> = without_suffix.split('_').collect();

    if parts.len() != 2 {
        return Err(FilestoreError::InvalidBatchFile(format!(
            "Expected format: batch_{{timestamp}}_{{index}}.parquet, got: {}",
            filename
        )));
    }

    let timestamp_ms = parts[0].parse::<i64>().map_err(|e| {
        FilestoreError::InvalidBatchFile(format!("Invalid timestamp in filename: {}", e))
    })?;

    let batch_index = parts[1].parse::<u64>().map_err(|e| {
        FilestoreError::InvalidBatchFile(format!("Invalid batch index in filename: {}", e))
    })?;

    Ok((timestamp_ms, batch_index))
}

/// ObjectStore-based batch manager for cloud/local storage abstraction.
///
/// Use this for remote storage (S3/GCS/Azure) or when you want unified API.
pub struct ObjectStoreBatchManager {
    store: Arc<dyn ObjectStore>,
    storage: Storage,
}

impl ObjectStoreBatchManager {
    pub fn new(store: Arc<dyn ObjectStore>, storage: Storage) -> Self {
        Self { store, storage }
    }

    /// List all batch files in a prefix (directory).
    pub fn list_batches(&self, prefix: &str) -> Result<Vec<BatchFile>> {
        let paths = list_files_sync(Arc::clone(&self.store), &self.storage, prefix)?;
        let mut batches = Vec::new();

        for path in paths {
            // Only process .parquet files
            if !path.ends_with(".parquet") {
                continue;
            }

            // Extract filename from full path
            let filename = std::path::Path::new(&path)
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| {
                    FilestoreError::InvalidBatchFile(format!("Invalid filename: {}", path))
                })?;

            // Parse batch metadata from filename
            let (timestamp_ms, batch_index) = parse_batch_filename(filename)?;

            // Get file metadata
            let meta = head_file_sync(Arc::clone(&self.store), &self.storage, &path)?;

            batches.push(BatchFile {
                path: PathBuf::from(path),
                timestamp_ms,
                batch_index,
                size_bytes: meta.size_bytes as u64,
            });
        }

        // Sort by timestamp, then by batch index
        batches.sort_by_key(|b| (b.timestamp_ms, b.batch_index));

        Ok(batches)
    }

    /// Delete batch files older than a given timestamp.
    pub fn delete_batches_before(&self, prefix: &str, cutoff_timestamp_ms: i64) -> Result<usize> {
        let batches = self.list_batches(prefix)?;
        let mut deleted_count = 0;

        for batch in batches {
            if batch.timestamp_ms < cutoff_timestamp_ms {
                let path = batch.path.to_string_lossy().to_string();
                delete_file_sync(Arc::clone(&self.store), &self.storage, &path)?;
                deleted_count += 1;
                log::info!("Deleted old batch file: {}", path);
            }
        }

        Ok(deleted_count)
    }

    /// Get total size of all batch files in a prefix.
    pub fn get_total_size(&self, prefix: &str) -> Result<u64> {
        let batches = self.list_batches(prefix)?;
        Ok(batches.iter().map(|b| b.size_bytes).sum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_object_store;
    use crate::object_store_ops::write_file_sync;
    use bytes::Bytes;
    use kalamdb_commons::models::ids::StorageId;
    use kalamdb_commons::models::storage::StorageType;
    use kalamdb_commons::models::types::Storage;
    use std::env;
    use std::fs;

    fn create_test_storage(temp_dir: &std::path::Path) -> Storage {
        let now = chrono::Utc::now().timestamp_millis();
        Storage {
            storage_id: StorageId::from("test_batch_manager"),
            storage_name: "test_batch_manager".to_string(),
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

    #[test]
    fn test_parse_batch_filename_valid() {
        let result = parse_batch_filename("batch_1234567890_00000001.parquet");
        assert!(result.is_ok());
        let (timestamp, index) = result.unwrap();
        assert_eq!(timestamp, 1234567890);
        assert_eq!(index, 1);
    }

    #[test]
    fn test_parse_batch_filename_invalid_format() {
        assert!(parse_batch_filename("invalid.parquet").is_err());
        assert!(parse_batch_filename("batch_123.parquet").is_err());
        assert!(parse_batch_filename("batch_abc_123.parquet").is_err());
    }

    #[test]
    fn test_list_batches() {
        let temp_dir = env::temp_dir().join("kalamdb_test_batch_list");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");
        let manager = ObjectStoreBatchManager::new(Arc::clone(&store), storage.clone());

        let prefix = "namespace1/table";

        // Create batch files with different timestamps
        let batches = vec![
            ("batch_1000000000_00000001.parquet", Bytes::from("data1")),
            ("batch_2000000000_00000001.parquet", Bytes::from("data22")),
            ("batch_1500000000_00000002.parquet", Bytes::from("data333")),
        ];

        for (filename, data) in &batches {
            let path = format!("{}/{}", prefix, filename);
            write_file_sync(Arc::clone(&store), &storage, &path, data.clone()).unwrap();
        }

        // List batches
        let listed = manager.list_batches(prefix).unwrap();
        assert_eq!(listed.len(), 3);

        // Verify sorted by timestamp, then index
        assert_eq!(listed[0].timestamp_ms, 1000000000);
        assert_eq!(listed[0].batch_index, 1);
        assert_eq!(listed[1].timestamp_ms, 1500000000);
        assert_eq!(listed[1].batch_index, 2);
        assert_eq!(listed[2].timestamp_ms, 2000000000);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_list_batches_empty_prefix() {
        let temp_dir = env::temp_dir().join("kalamdb_test_batch_empty");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");
        let manager = ObjectStoreBatchManager::new(store, storage);

        let listed = manager.list_batches("empty/prefix").unwrap();
        assert_eq!(listed.len(), 0);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_list_batches_ignores_non_parquet() {
        let temp_dir = env::temp_dir().join("kalamdb_test_batch_filter");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");
        let manager = ObjectStoreBatchManager::new(Arc::clone(&store), storage.clone());

        let prefix = "namespace1/table";

        // Create batch files and non-parquet files
        let files = vec![
            ("batch_1000000000_00000001.parquet", "data"),
            ("manifest.json", "metadata"),
            ("README.txt", "text"),
        ];

        for (filename, data) in &files {
            let path = format!("{}/{}", prefix, filename);
            write_file_sync(Arc::clone(&store), &storage, &path, Bytes::from(*data)).unwrap();
        }

        // List should only return parquet files
        let listed = manager.list_batches(prefix).unwrap();
        assert_eq!(listed.len(), 1, "Should only list .parquet files");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_delete_batches_before() {
        let temp_dir = env::temp_dir().join("kalamdb_test_batch_delete");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");
        let manager = ObjectStoreBatchManager::new(Arc::clone(&store), storage.clone());

        let prefix = "namespace1/table";

        // Create batch files with different timestamps
        let batches = vec![
            ("batch_1000000000_00000001.parquet", "old1"),
            ("batch_2000000000_00000001.parquet", "old2"),
            ("batch_3000000000_00000001.parquet", "new1"),
            ("batch_4000000000_00000001.parquet", "new2"),
        ];

        for (filename, data) in &batches {
            let path = format!("{}/{}", prefix, filename);
            write_file_sync(Arc::clone(&store), &storage, &path, Bytes::from(*data)).unwrap();
        }

        // Delete batches before timestamp 2500000000
        let cutoff = 2500000000;
        let deleted_count = manager.delete_batches_before(prefix, cutoff).unwrap();
        assert_eq!(deleted_count, 2, "Should delete 2 old batches");

        // Verify remaining batches
        let remaining = manager.list_batches(prefix).unwrap();
        assert_eq!(remaining.len(), 2);
        assert!(remaining[0].timestamp_ms >= cutoff);
        assert!(remaining[1].timestamp_ms >= cutoff);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_delete_batches_before_no_matches() {
        let temp_dir = env::temp_dir().join("kalamdb_test_batch_delete_none");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");
        let manager = ObjectStoreBatchManager::new(Arc::clone(&store), storage.clone());

        let prefix = "namespace1/table";

        // Create new batch files
        let path = format!("{}/batch_9000000000_00000001.parquet", prefix);
        write_file_sync(Arc::clone(&store), &storage, &path, Bytes::from("data")).unwrap();

        // Try to delete batches before an old timestamp
        let deleted_count = manager.delete_batches_before(prefix, 1000000000).unwrap();
        assert_eq!(deleted_count, 0, "Should delete nothing");

        // Verify batch still exists
        let remaining = manager.list_batches(prefix).unwrap();
        assert_eq!(remaining.len(), 1);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_get_total_size() {
        let temp_dir = env::temp_dir().join("kalamdb_test_batch_size");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");
        let manager = ObjectStoreBatchManager::new(Arc::clone(&store), storage.clone());

        let prefix = "namespace1/table";

        // Create batch files with known sizes
        let batches = vec![
            ("batch_1000000000_00000001.parquet", vec![0u8; 100]),
            ("batch_2000000000_00000001.parquet", vec![0u8; 200]),
            ("batch_3000000000_00000001.parquet", vec![0u8; 300]),
        ];

        for (filename, data) in &batches {
            let path = format!("{}/{}", prefix, filename);
            write_file_sync(Arc::clone(&store), &storage, &path, Bytes::from(data.clone()))
                .unwrap();
        }

        // Get total size
        let total_size = manager.get_total_size(prefix).unwrap();
        assert_eq!(total_size, 600, "Total size should be 100+200+300");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_get_total_size_empty() {
        let temp_dir = env::temp_dir().join("kalamdb_test_batch_size_empty");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");
        let manager = ObjectStoreBatchManager::new(store, storage);

        let total_size = manager.get_total_size("empty/prefix").unwrap();
        assert_eq!(total_size, 0);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_batch_file_metadata() {
        let temp_dir = env::temp_dir().join("kalamdb_test_batch_metadata");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");
        let manager = ObjectStoreBatchManager::new(Arc::clone(&store), storage.clone());

        let prefix = "namespace1/table";
        let filename = "batch_1234567890_00000042.parquet";
        let data = vec![0u8; 512];
        let path = format!("{}/{}", prefix, filename);

        write_file_sync(Arc::clone(&store), &storage, &path, Bytes::from(data)).unwrap();

        let batches = manager.list_batches(prefix).unwrap();
        assert_eq!(batches.len(), 1);

        let batch = &batches[0];
        assert_eq!(batch.timestamp_ms, 1234567890);
        assert_eq!(batch.batch_index, 42);
        assert_eq!(batch.size_bytes, 512);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}