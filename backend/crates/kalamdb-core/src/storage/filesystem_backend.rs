//! Filesystem storage backend for Parquet files
//!
//! This module handles writing and reading Parquet files to/from the local filesystem.

use crate::error::KalamDbError;
use std::fs;
use std::path::{Path, PathBuf};

/// Filesystem storage backend
pub struct FilesystemBackend {
    /// Base storage directory
    base_path: PathBuf,
}

impl FilesystemBackend {
    /// Create a new filesystem backend
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
        }
    }

    /// Ensure a directory exists, creating it if necessary
    pub fn ensure_directory(&self, relative_path: &str) -> Result<PathBuf, KalamDbError> {
        let full_path = self.base_path.join(relative_path);
        fs::create_dir_all(&full_path).map_err(KalamDbError::Io)?;
        Ok(full_path)
    }

    /// Get the full path for a file
    pub fn get_file_path(&self, relative_path: &str) -> PathBuf {
        self.base_path.join(relative_path)
    }

    /// Check if a file exists
    pub fn file_exists(&self, relative_path: &str) -> bool {
        self.get_file_path(relative_path).exists()
    }

    /// Read file contents
    pub fn read_file(&self, relative_path: &str) -> Result<Vec<u8>, KalamDbError> {
        let path = self.get_file_path(relative_path);
        fs::read(&path).map_err(KalamDbError::Io)
    }

    /// Write file contents
    pub fn write_file(&self, relative_path: &str, contents: &[u8]) -> Result<(), KalamDbError> {
        let path = self.get_file_path(relative_path);

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(KalamDbError::Io)?;
        }

        fs::write(&path, contents).map_err(KalamDbError::Io)
    }

    /// Delete a file
    pub fn delete_file(&self, relative_path: &str) -> Result<(), KalamDbError> {
        let path = self.get_file_path(relative_path);
        fs::remove_file(&path).map_err(KalamDbError::Io)
    }

    /// List files in a directory
    pub fn list_files(&self, relative_path: &str) -> Result<Vec<String>, KalamDbError> {
        let dir_path = self.get_file_path(relative_path);
        let entries = fs::read_dir(&dir_path).map_err(KalamDbError::Io)?;

        let mut files = Vec::new();
        for entry in entries {
            let entry = entry.map_err(KalamDbError::Io)?;
            if entry.file_type().map_err(KalamDbError::Io)?.is_file() {
                if let Some(file_name) = entry.file_name().to_str() {
                    files.push(file_name.to_string());
                }
            }
        }

        Ok(files)
    }

    /// Get the base path
    pub fn base_path(&self) -> &Path {
        &self.base_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_filesystem_backend_creation() {
        let temp_dir = env::temp_dir().join("kalamdb_fs_backend_test");
        let backend = FilesystemBackend::new(&temp_dir);
        assert_eq!(backend.base_path(), temp_dir.as_path());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ensure_directory() {
        let temp_dir = env::temp_dir().join("kalamdb_fs_ensure_dir_test");
        let _ = fs::remove_dir_all(&temp_dir);

        let backend = FilesystemBackend::new(&temp_dir);
        let result = backend.ensure_directory("subdir/nested");
        assert!(result.is_ok());
        assert!(temp_dir.join("subdir/nested").exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_write_and_read_file() {
        let temp_dir = env::temp_dir().join("kalamdb_fs_write_read_test");
        let _ = fs::remove_dir_all(&temp_dir);

        let backend = FilesystemBackend::new(&temp_dir);
        let test_data = b"Hello, KalamDB!";

        // Write file
        backend.write_file("test.txt", test_data).unwrap();
        assert!(backend.file_exists("test.txt"));

        // Read file
        let read_data = backend.read_file("test.txt").unwrap();
        assert_eq!(read_data, test_data);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_delete_file() {
        let temp_dir = env::temp_dir().join("kalamdb_fs_delete_test");
        let _ = fs::remove_dir_all(&temp_dir);

        let backend = FilesystemBackend::new(&temp_dir);
        backend.write_file("test.txt", b"test").unwrap();
        assert!(backend.file_exists("test.txt"));

        backend.delete_file("test.txt").unwrap();
        assert!(!backend.file_exists("test.txt"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_list_files() {
        let temp_dir = env::temp_dir().join("kalamdb_fs_list_test");
        let _ = fs::remove_dir_all(&temp_dir);

        let backend = FilesystemBackend::new(&temp_dir);
        backend.write_file("dir/file1.txt", b"test1").unwrap();
        backend.write_file("dir/file2.txt", b"test2").unwrap();
        backend.write_file("dir/file3.txt", b"test3").unwrap();

        let files = backend.list_files("dir").unwrap();
        assert_eq!(files.len(), 3);
        assert!(files.contains(&"file1.txt".to_string()));
        assert!(files.contains(&"file2.txt".to_string()));
        assert!(files.contains(&"file3.txt".to_string()));

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
