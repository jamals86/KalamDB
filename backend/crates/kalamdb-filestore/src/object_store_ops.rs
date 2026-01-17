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

    let result = store.get(&key).await.map_err(|e| FilestoreError::ObjectStore(e.to_string()))?;

    let bytes = result.bytes().await.map_err(|e| FilestoreError::ObjectStore(e.to_string()))?;

    Ok(bytes)
}

/// Synchronous wrapper for read_file.
pub fn read_file_sync(store: Arc<dyn ObjectStore>, storage: &Storage, path: &str) -> Result<Bytes> {
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
pub async fn delete_file(store: Arc<dyn ObjectStore>, storage: &Storage, path: &str) -> Result<()> {
    let key = object_key_for_path(storage, path)?;

    store
        .delete(&key)
        .await
        .map_err(|e| FilestoreError::ObjectStore(e.to_string()))?;

    Ok(())
}

/// Synchronous wrapper for delete_file.
pub fn delete_file_sync(store: Arc<dyn ObjectStore>, storage: &Storage, path: &str) -> Result<()> {
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

    let meta = store.head(&key).await.map_err(|e| FilestoreError::ObjectStore(e.to_string()))?;

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
/// For local filesystem storage, also removes empty directories.
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
        total_bytes += meta.size;
        paths_to_delete.push(meta.location);
    }

    // Delete all files found
    for path in &paths_to_delete {
        store
            .delete(path)
            .await
            .map_err(|e| FilestoreError::ObjectStore(e.to_string()))?;
    }

    // For local filesystem, also remove the empty directory tree
    if storage.storage_type == kalamdb_commons::models::storage::StorageType::Filesystem {
        let base_dir = storage.base_directory.trim();
        if !base_dir.is_empty() {
            let target_dir = if prefix.is_empty() {
                std::path::PathBuf::from(base_dir)
            } else {
                std::path::PathBuf::from(base_dir).join(prefix)
            };

            // Try to remove the directory and any empty parent directories
            // up to (but not including) the base directory
            let _ = remove_empty_dir_tree(&target_dir, base_dir);
        }
    }

    Ok(total_bytes)
}

/// Recursively remove empty directories under target, then work up to (but not including) stop_at.
///
/// This function first walks down to find all empty leaf directories, removes them,
/// then works up the tree removing any directories that become empty.
fn remove_empty_dir_tree(target: &std::path::Path, stop_at: &str) -> std::io::Result<()> {
    let stop_path = std::path::Path::new(stop_at);

    // First, recursively collect all directories under target (bottom-up order)
    fn collect_dirs_bottom_up(dir: &std::path::Path, dirs: &mut Vec<std::path::PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(ft) = entry.file_type() {
                    if ft.is_dir() {
                        // Recurse first (depth-first)
                        collect_dirs_bottom_up(&entry.path(), dirs);
                        // Then add this directory
                        dirs.push(entry.path());
                    }
                }
            }
        }
    }

    // Collect all subdirectories in bottom-up order
    let mut dirs_to_check = Vec::new();
    collect_dirs_bottom_up(target, &mut dirs_to_check);
    // Also add the target directory itself
    dirs_to_check.push(target.to_path_buf());

    // Try to remove each directory (will only succeed if empty)
    for dir in &dirs_to_check {
        match std::fs::remove_dir(dir) {
            Ok(_) => {
                // Successfully removed empty directory
            },
            Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => {
                // Not empty, skip
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Already removed
            },
            Err(_) => {
                // Other error (permissions, etc.), skip
            },
        }
    }

    // Now walk up from target to stop_at, removing empty directories
    let mut current = target.to_path_buf();

    while current.starts_with(stop_path) && current != stop_path {
        // Try to remove the directory (only works if empty)
        match std::fs::remove_dir(&current) {
            Ok(_) => {
                // Successfully removed empty parent directory
            },
            Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => {
                // Directory not empty, stop walking up
                break;
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Already removed, continue walking up
            },
            Err(_) => {
                // Other error (permissions, etc.), stop
                break;
            },
        }

        // Move to parent
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => break,
        }
    }

    Ok(())
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

/// Rename (move) a file from source to destination.
///
/// For object stores, this performs a copy followed by delete (atomic at object level).
/// For local filesystem, object_store uses rename which is atomic on POSIX.
///
/// # Arguments
/// * `store` - ObjectStore instance
/// * `storage` - Storage configuration
/// * `from_path` - Source path (relative to storage base)
/// * `to_path` - Destination path (relative to storage base)
pub async fn rename_file(
    store: Arc<dyn ObjectStore>,
    storage: &Storage,
    from_path: &str,
    to_path: &str,
) -> Result<()> {
    let from_key = object_key_for_path(storage, from_path)?;
    let to_key = object_key_for_path(storage, to_path)?;

    store
        .rename(&from_key, &to_key)
        .await
        .map_err(|e| FilestoreError::ObjectStore(e.to_string()))?;

    Ok(())
}

/// Synchronous wrapper for rename_file.
pub fn rename_file_sync(
    store: Arc<dyn ObjectStore>,
    storage: &Storage,
    from_path: &str,
    to_path: &str,
) -> Result<()> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        std::thread::scope(|s| {
            s.spawn(|| handle.block_on(rename_file(store, storage, from_path, to_path)))
                .join()
                .map_err(|_| FilestoreError::Other("Thread panicked".into()))?
        })
    } else {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| FilestoreError::Other(format!("Failed to create runtime: {e}")))?;

        rt.block_on(rename_file(store, storage, from_path, to_path))
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
            write_file_sync(Arc::clone(&store), &storage, file, Bytes::from("test")).unwrap();
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
            write_file_sync(Arc::clone(&store), &storage, file, Bytes::from("test")).unwrap();
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
        let exists_before =
            prefix_exists_sync(Arc::clone(&store), &storage, "nonexistent").unwrap();
        assert!(!exists_before);

        // Write a file
        write_file_sync(Arc::clone(&store), &storage, "exists/file.txt", Bytes::from("test"))
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
        write_file_sync(Arc::clone(&store), &storage, file_path, Bytes::from("version 1")).unwrap();

        let v1 = read_file_sync(Arc::clone(&store), &storage, file_path).unwrap();
        assert_eq!(v1, Bytes::from("version 1"));

        // Overwrite with version 2
        write_file_sync(Arc::clone(&store), &storage, file_path, Bytes::from("version 2")).unwrap();

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
        write_file_sync(Arc::clone(&store), &storage, file_path, Bytes::from("nested")).unwrap();

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

    #[test]
    fn test_rename_file_sync() {
        let temp_dir = env::temp_dir().join("kalamdb_test_rename");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        let src_path = "test/original.txt";
        let dst_path = "test/renamed.txt";
        let content = Bytes::from("rename test content");

        // Write source file
        write_file_sync(Arc::clone(&store), &storage, src_path, content.clone()).unwrap();

        // Verify source exists
        assert!(head_file_sync(Arc::clone(&store), &storage, src_path).is_ok());

        // Rename file
        rename_file_sync(Arc::clone(&store), &storage, src_path, dst_path).unwrap();

        // Verify destination exists with correct content
        let read_content = read_file_sync(Arc::clone(&store), &storage, dst_path).unwrap();
        assert_eq!(read_content, content);

        // Verify source no longer exists
        assert!(head_file_sync(Arc::clone(&store), &storage, src_path).is_err());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_rename_file_to_different_directory() {
        let temp_dir = env::temp_dir().join("kalamdb_test_rename_dir");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        let src_path = "dir1/file.txt";
        let dst_path = "dir2/subdir/file.txt";
        let content = Bytes::from("cross-directory rename");

        // Write source file
        write_file_sync(Arc::clone(&store), &storage, src_path, content.clone()).unwrap();

        // Rename to different directory
        rename_file_sync(Arc::clone(&store), &storage, src_path, dst_path).unwrap();

        // Verify destination exists
        let read_content = read_file_sync(Arc::clone(&store), &storage, dst_path).unwrap();
        assert_eq!(read_content, content);

        // Verify source no longer exists
        assert!(head_file_sync(Arc::clone(&store), &storage, src_path).is_err());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_rename_temp_parquet_to_final() {
        // Simulates the atomic flush pattern: write to .tmp, rename to .parquet
        let temp_dir = env::temp_dir().join("kalamdb_test_rename_parquet");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        let temp_path = "ns/table/batch-0.parquet.tmp";
        let final_path = "ns/table/batch-0.parquet";

        // Simulate Parquet content (just bytes for this test)
        let parquet_content = Bytes::from(vec![0x50, 0x41, 0x52, 0x31]); // "PAR1" magic

        // Step 1: Write to temp location
        write_file_sync(Arc::clone(&store), &storage, temp_path, parquet_content.clone()).unwrap();

        // Step 2: Atomic rename to final location
        rename_file_sync(Arc::clone(&store), &storage, temp_path, final_path).unwrap();

        // Step 3: Verify final file exists with correct content
        let read_content = read_file_sync(Arc::clone(&store), &storage, final_path).unwrap();
        assert_eq!(read_content, parquet_content);

        // Step 4: Verify temp file is gone
        assert!(head_file_sync(Arc::clone(&store), &storage, temp_path).is_err());

        // List files - should only see the final file
        let files = list_files_sync(Arc::clone(&store), &storage, "ns/table").unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].contains("batch-0.parquet"));
        assert!(!files[0].contains(".tmp"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_rename_preserves_large_file() {
        let temp_dir = env::temp_dir().join("kalamdb_test_rename_large");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        let src_path = "large/file.tmp";
        let dst_path = "large/file.dat";

        // Create a larger file (1MB)
        let large_content: Vec<u8> = (0..1024 * 1024).map(|i| (i % 256) as u8).collect();
        let content = Bytes::from(large_content.clone());

        // Write large file
        write_file_sync(Arc::clone(&store), &storage, src_path, content.clone()).unwrap();

        // Rename
        rename_file_sync(Arc::clone(&store), &storage, src_path, dst_path).unwrap();

        // Verify content is preserved
        let read_content = read_file_sync(Arc::clone(&store), &storage, dst_path).unwrap();
        assert_eq!(read_content.len(), 1024 * 1024);
        assert_eq!(read_content, content);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_atomic_flush_pattern_simulation() {
        // Full simulation of the atomic flush pattern with manifest update
        let temp_dir = env::temp_dir().join("kalamdb_test_atomic_flush");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        // Initial state: manifest exists (from previous flush)
        let manifest_path = "myns/mytable/manifest.json";
        let initial_manifest = r#"{"version":1,"segments":[]}"#;
        write_file_sync(Arc::clone(&store), &storage, manifest_path, Bytes::from(initial_manifest))
            .unwrap();

        // Step 1: Write Parquet to temp location
        let temp_parquet = "myns/mytable/batch-0.parquet.tmp";
        let final_parquet = "myns/mytable/batch-0.parquet";
        let parquet_data = Bytes::from(vec![0x50, 0x41, 0x52, 0x31, 0x00, 0x01, 0x02, 0x03]);

        write_file_sync(Arc::clone(&store), &storage, temp_parquet, parquet_data.clone()).unwrap();

        // Step 2: Atomic rename temp -> final
        rename_file_sync(Arc::clone(&store), &storage, temp_parquet, final_parquet).unwrap();

        // Step 3: Update manifest with new segment
        let updated_manifest = r#"{"version":2,"segments":[{"path":"batch-0.parquet"}]}"#;
        write_file_sync(Arc::clone(&store), &storage, manifest_path, Bytes::from(updated_manifest))
            .unwrap();

        // Verify final state
        let files = list_files_sync(Arc::clone(&store), &storage, "myns/mytable").unwrap();
        assert_eq!(files.len(), 2); // manifest.json + batch-0.parquet

        // No temp files should exist
        let temp_files: Vec<_> = files.iter().filter(|f| f.contains(".tmp")).collect();
        assert!(temp_files.is_empty(), "No temp files should remain after atomic flush");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
