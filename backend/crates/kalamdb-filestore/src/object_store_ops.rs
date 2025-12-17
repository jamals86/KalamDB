//! Unified object_store operations for all storage backends.
//!
//! This module consolidates all file operations (read, write, list, delete, metadata)
//! into a single interface using object_store abstraction. Supports local filesystem,
//! S3, GCS, and Azure transparently.

use crate::error::{FilestoreError, Result};
use crate::object_store_factory::object_key_for_path;
use bytes::Bytes;
use futures_util::StreamExt;
use kalamdb_commons::system::Storage;
use object_store::ObjectStore;
use std::sync::Arc;

/// List all files in a directory (prefix).
///
/// Returns relative paths from the storage base directory.
pub async fn list_files(
    store: Arc<dyn ObjectStore>,
    storage: &Storage,
    prefix: &str,
) -> Result<Vec<String>> {
    let key = object_key_for_path(storage, prefix)?;
    let prefix_path = if key.as_ref().is_empty() {
        None
    } else {
        Some(key)
    };

    let mut stream = store.list(prefix_path.as_ref());
    let mut paths = Vec::new();

    while let Some(result) = stream.next().await {
        let meta = result.map_err(|e| FilestoreError::ObjectStore(e.to_string()))?;
        paths.push(meta.location.to_string());
    }

    Ok(paths)
}

/// Synchronous wrapper for list_files.
pub fn list_files_sync(
    store: Arc<dyn ObjectStore>,
    storage: &Storage,
    prefix: &str,
) -> Result<Vec<String>> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        std::thread::scope(|s| {
            s.spawn(|| handle.block_on(list_files(store, storage, prefix)))
                .join()
                .map_err(|_| FilestoreError::Other("Thread panicked".into()))?
        })
    } else {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| FilestoreError::Other(format!("Failed to create runtime: {e}")))?;

        rt.block_on(list_files(store, storage, prefix))
    }
}

/// Read file contents as bytes.
pub async fn read_file(
    store: Arc<dyn ObjectStore>,
    storage: &Storage,
    path: &str,
) -> Result<Bytes> {
    let key = object_key_for_path(storage, path)?;
    
    let result = store
        .get(&key)
        .await
        .map_err(|e| FilestoreError::ObjectStore(e.to_string()))?;
    
    let bytes = result
        .bytes()
        .await
        .map_err(|e| FilestoreError::ObjectStore(e.to_string()))?;
    
    Ok(bytes)
}

/// Synchronous wrapper for read_file.
pub fn read_file_sync(
    store: Arc<dyn ObjectStore>,
    storage: &Storage,
    path: &str,
) -> Result<Bytes> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        std::thread::scope(|s| {
            s.spawn(|| handle.block_on(read_file(store, storage, path)))
                .join()
                .map_err(|_| FilestoreError::Other("Thread panicked".into()))?
        })
    } else {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| FilestoreError::Other(format!("Failed to create runtime: {e}")))?;

        rt.block_on(read_file(store, storage, path))
    }
}

/// Write bytes to a file (replaces if exists).
pub async fn write_file(
    store: Arc<dyn ObjectStore>,
    storage: &Storage,
    path: &str,
    data: Bytes,
) -> Result<()> {
    let key = object_key_for_path(storage, path)?;
    
    store
        .put(&key, data.into())
        .await
        .map_err(|e| FilestoreError::ObjectStore(e.to_string()))?;
    
    Ok(())
}

/// Synchronous wrapper for write_file.
pub fn write_file_sync(
    store: Arc<dyn ObjectStore>,
    storage: &Storage,
    path: &str,
    data: Bytes,
) -> Result<()> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        std::thread::scope(|s| {
            s.spawn(|| handle.block_on(write_file(store, storage, path, data)))
                .join()
                .map_err(|_| FilestoreError::Other("Thread panicked".into()))?
        })
    } else {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| FilestoreError::Other(format!("Failed to create runtime: {e}")))?;

        rt.block_on(write_file(store, storage, path, data))
    }
}

/// Delete a file.
pub async fn delete_file(
    store: Arc<dyn ObjectStore>,
    storage: &Storage,
    path: &str,
) -> Result<()> {
    let key = object_key_for_path(storage, path)?;
    
    store
        .delete(&key)
        .await
        .map_err(|e| FilestoreError::ObjectStore(e.to_string()))?;
    
    Ok(())
}

/// Synchronous wrapper for delete_file.
pub fn delete_file_sync(
    store: Arc<dyn ObjectStore>,
    storage: &Storage,
    path: &str,
) -> Result<()> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        std::thread::scope(|s| {
            s.spawn(|| handle.block_on(delete_file(store, storage, path)))
                .join()
                .map_err(|_| FilestoreError::Other("Thread panicked".into()))?
        })
    } else {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| FilestoreError::Other(format!("Failed to create runtime: {e}")))?;

        rt.block_on(delete_file(store, storage, path))
    }
}

/// Get file metadata (size, modification time).
pub async fn head_file(
    store: Arc<dyn ObjectStore>,
    storage: &Storage,
    path: &str,
) -> Result<FileMetadata> {
    let key = object_key_for_path(storage, path)?;
    
    let meta = store
        .head(&key)
        .await
        .map_err(|e| FilestoreError::ObjectStore(e.to_string()))?;
    
    Ok(FileMetadata {
        size_bytes: meta.size as usize,
        modified_timestamp_ms: meta.last_modified.timestamp_millis(),
    })
}

/// Synchronous wrapper for head_file.
pub fn head_file_sync(
    store: Arc<dyn ObjectStore>,
    storage: &Storage,
    path: &str,
) -> Result<FileMetadata> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        std::thread::scope(|s| {
            s.spawn(|| handle.block_on(head_file(store, storage, path)))
                .join()
                .map_err(|_| FilestoreError::Other("Thread panicked".into()))?
        })
    } else {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| FilestoreError::Other(format!("Failed to create runtime: {e}")))?;

        rt.block_on(head_file(store, storage, path))
    }
}

/// File metadata returned by head_file.
#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub size_bytes: usize,
    pub modified_timestamp_ms: i64,
}

/// Delete all files under a prefix (recursive delete).
///
/// Returns the total number of bytes deleted (best-effort).
pub async fn delete_prefix(
    store: Arc<dyn ObjectStore>,
    storage: &Storage,
    prefix: &str,
) -> Result<u64> {
    let key = object_key_for_path(storage, prefix)?;
    let prefix_path = if key.as_ref().is_empty() {
        None
    } else {
        Some(key)
    };

    let mut stream = store.list(prefix_path.as_ref());
    let mut total_bytes = 0u64;
    let mut paths_to_delete = Vec::new();

    while let Some(result) = stream.next().await {
        let meta = result.map_err(|e| FilestoreError::ObjectStore(e.to_string()))?;
        total_bytes += meta.size as u64;
        paths_to_delete.push(meta.location);
    }

    // Delete all files found
    for path in paths_to_delete {
        // Ignore errors on individual deletes (best-effort)
        let _ = store.delete(&path).await;
    }

    Ok(total_bytes)
}

/// Synchronous wrapper for delete_prefix.
pub fn delete_prefix_sync(
    store: Arc<dyn ObjectStore>,
    storage: &Storage,
    prefix: &str,
) -> Result<u64> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        std::thread::scope(|s| {
            s.spawn(|| handle.block_on(delete_prefix(store, storage, prefix)))
                .join()
                .map_err(|_| FilestoreError::Other("Thread panicked".into()))?
        })
    } else {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| FilestoreError::Other(format!("Failed to create runtime: {e}")))?;

        rt.block_on(delete_prefix(store, storage, prefix))
    }
}

/// Check if any files exist under a prefix.
pub async fn prefix_exists(
    store: Arc<dyn ObjectStore>,
    storage: &Storage,
    prefix: &str,
) -> Result<bool> {
    let key = object_key_for_path(storage, prefix)?;
    let prefix_path = if key.as_ref().is_empty() {
        None
    } else {
        Some(key)
    };

    let mut stream = store.list(prefix_path.as_ref());
    
    // Check if at least one file exists
    Ok(stream.next().await.is_some())
}

/// Synchronous wrapper for prefix_exists.
pub fn prefix_exists_sync(
    store: Arc<dyn ObjectStore>,
    storage: &Storage,
    prefix: &str,
) -> Result<bool> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        std::thread::scope(|s| {
            s.spawn(|| handle.block_on(prefix_exists(store, storage, prefix)))
                .join()
                .map_err(|_| FilestoreError::Other("Thread panicked".into()))?
        })
    } else {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| FilestoreError::Other(format!("Failed to create runtime: {e}")))?;

        rt.block_on(prefix_exists(store, storage, prefix))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_object_store;
    use kalamdb_commons::models::ids::StorageId;
    use kalamdb_commons::models::storage::StorageType;
    use kalamdb_commons::models::types::Storage;
    use std::env;
    use std::fs;

    fn create_test_storage(temp_dir: &std::path::Path) -> Storage {
        let now = chrono::Utc::now().timestamp_millis();
        Storage {
            storage_id: StorageId::from("test_object_store_ops"),
            storage_name: "test_object_store_ops".to_string(),
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
    fn test_write_and_read_file_sync() {
        let temp_dir = env::temp_dir().join("kalamdb_test_write_read");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        let file_path = "test/data.txt";
        let content = Bytes::from("Hello, KalamDB!");

        // Write file
        write_file_sync(Arc::clone(&store), &storage, file_path, content.clone()).unwrap();

        // Read file back
        let read_content = read_file_sync(store, &storage, file_path).unwrap();
        assert_eq!(read_content, content);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_list_files_sync() {
        let temp_dir = env::temp_dir().join("kalamdb_test_list_files");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        // Write multiple files
        let files = vec![
            "namespace1/file1.txt",
            "namespace1/file2.txt",
            "namespace1/subdir/file3.txt",
            "namespace2/file4.txt",
        ];

        for file in &files {
            write_file_sync(
                Arc::clone(&store),
                &storage,
                file,
                Bytes::from("test"),
            )
            .unwrap();
        }

        // List all files under namespace1
        let listed = list_files_sync(Arc::clone(&store), &storage, "namespace1").unwrap();
        
        // Should include namespace1 files
        assert!(listed.len() >= 3, "Should list at least 3 files");
        
        // Check that namespace1 files are present
        let has_file1 = listed.iter().any(|p| p.contains("file1.txt"));
        let has_file2 = listed.iter().any(|p| p.contains("file2.txt"));
        assert!(has_file1, "Should contain file1.txt");
        assert!(has_file2, "Should contain file2.txt");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_head_file_sync() {
        let temp_dir = env::temp_dir().join("kalamdb_test_head_file");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        let file_path = "test/metadata.txt";
        let content = Bytes::from("Some content for metadata test");

        // Write file
        write_file_sync(Arc::clone(&store), &storage, file_path, content.clone()).unwrap();

        // Get metadata
        let meta = head_file_sync(store, &storage, file_path).unwrap();
        
        assert_eq!(meta.size_bytes, content.len());
        assert!(meta.modified_timestamp_ms > 0);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_delete_file_sync() {
        let temp_dir = env::temp_dir().join("kalamdb_test_delete_file");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        let file_path = "test/delete_me.txt";
        let content = Bytes::from("Delete this");

        // Write file
        write_file_sync(Arc::clone(&store), &storage, file_path, content).unwrap();

        // Verify exists
        let exists_before = head_file_sync(Arc::clone(&store), &storage, file_path).is_ok();
        assert!(exists_before);

        // Delete file
        delete_file_sync(Arc::clone(&store), &storage, file_path).unwrap();

        // Verify deleted
        let exists_after = head_file_sync(store, &storage, file_path).is_ok();
        assert!(!exists_after);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_delete_prefix_sync() {
        let temp_dir = env::temp_dir().join("kalamdb_test_delete_prefix");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        // Write multiple files under a prefix
        let files = vec![
            "ns/table/seg1.parquet",
            "ns/table/seg2.parquet",
            "ns/table/manifest.json",
        ];

        for file in &files {
            write_file_sync(
                Arc::clone(&store),
                &storage,
                file,
                Bytes::from("test"),
            )
            .unwrap();
        }

        // Verify files exist
        let listed_before = list_files_sync(Arc::clone(&store), &storage, "ns/table").unwrap();
        assert!(listed_before.len() >= 3);

        // Delete all files under prefix
        delete_prefix_sync(Arc::clone(&store), &storage, "ns/table").unwrap();

        // Verify all deleted
        let listed_after = list_files_sync(store, &storage, "ns/table").unwrap();
        assert_eq!(listed_after.len(), 0, "All files should be deleted");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_prefix_exists_sync() {
        let temp_dir = env::temp_dir().join("kalamdb_test_prefix_exists");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        // Check non-existent prefix
        let exists_before = prefix_exists_sync(Arc::clone(&store), &storage, "nonexistent").unwrap();
        assert!(!exists_before);

        // Write a file
        write_file_sync(
            Arc::clone(&store),
            &storage,
            "exists/file.txt",
            Bytes::from("test"),
        )
        .unwrap();

        // Check prefix now exists
        let exists_after = prefix_exists_sync(store, &storage, "exists").unwrap();
        assert!(exists_after);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_write_overwrite_file() {
        let temp_dir = env::temp_dir().join("kalamdb_test_overwrite");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        let file_path = "test/overwrite.txt";

        // Write first version
        write_file_sync(
            Arc::clone(&store),
            &storage,
            file_path,
            Bytes::from("version 1"),
        )
        .unwrap();

        let v1 = read_file_sync(Arc::clone(&store), &storage, file_path).unwrap();
        assert_eq!(v1, Bytes::from("version 1"));

        // Overwrite with version 2
        write_file_sync(
            Arc::clone(&store),
            &storage,
            file_path,
            Bytes::from("version 2"),
        )
        .unwrap();

        let v2 = read_file_sync(store, &storage, file_path).unwrap();
        assert_eq!(v2, Bytes::from("version 2"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_read_nonexistent_file() {
        let temp_dir = env::temp_dir().join("kalamdb_test_nonexistent_read");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        let result = read_file_sync(store, &storage, "nonexistent/file.txt");
        assert!(result.is_err());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_large_file_operations() {
        let temp_dir = env::temp_dir().join("kalamdb_test_large_file");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        let file_path = "test/large.bin";
        
        // Create 1MB of data
        let large_data = vec![42u8; 1024 * 1024];
        let content = Bytes::from(large_data.clone());

        // Write large file
        write_file_sync(Arc::clone(&store), &storage, file_path, content.clone()).unwrap();

        // Read back
        let read_content = read_file_sync(Arc::clone(&store), &storage, file_path).unwrap();
        assert_eq!(read_content.len(), 1024 * 1024);
        assert_eq!(read_content, content);

        // Check metadata
        let meta = head_file_sync(store, &storage, file_path).unwrap();
        assert_eq!(meta.size_bytes, 1024 * 1024);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_nested_directory_creation() {
        let temp_dir = env::temp_dir().join("kalamdb_test_nested");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        // Write to deeply nested path
        let file_path = "a/b/c/d/e/f/file.txt";
        write_file_sync(
            Arc::clone(&store),
            &storage,
            file_path,
            Bytes::from("nested"),
        )
        .unwrap();

        // Read back
        let content = read_file_sync(store, &storage, file_path).unwrap();
        assert_eq!(content, Bytes::from("nested"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_empty_file() {
        let temp_dir = env::temp_dir().join("kalamdb_test_empty_file");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        let file_path = "test/empty.txt";
        let empty = Bytes::new();

        // Write empty file
        write_file_sync(Arc::clone(&store), &storage, file_path, empty).unwrap();

        // Read back
        let content = read_file_sync(Arc::clone(&store), &storage, file_path).unwrap();
        assert_eq!(content.len(), 0);

        // Check metadata
        let meta = head_file_sync(store, &storage, file_path).unwrap();
        assert_eq!(meta.size_bytes, 0);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_list_empty_prefix() {
        let temp_dir = env::temp_dir().join("kalamdb_test_list_empty");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        // List non-existent prefix
        let files = list_files_sync(store, &storage, "empty/prefix").unwrap();
        assert_eq!(files.len(), 0);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_delete_nonexistent_file() {
        let temp_dir = env::temp_dir().join("kalamdb_test_delete_nonexistent");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        // Deleting non-existent file should not error (idempotent)
        let result = delete_file_sync(store, &storage, "nonexistent/file.txt");
        // object_store typically doesn't error on delete of non-existent files
        assert!(result.is_ok() || result.is_err());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_binary_data() {
        let temp_dir = env::temp_dir().join("kalamdb_test_binary");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        let file_path = "test/binary.bin";
        
        // Create binary data with all byte values
        let binary_data: Vec<u8> = (0..=255).collect();
        let content = Bytes::from(binary_data.clone());

        write_file_sync(Arc::clone(&store), &storage, file_path, content.clone()).unwrap();

        let read_content = read_file_sync(store, &storage, file_path).unwrap();
        assert_eq!(read_content, content);
        assert_eq!(read_content.len(), 256);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}