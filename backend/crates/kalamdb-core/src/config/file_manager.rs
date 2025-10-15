//! Configuration file management
//!
//! This module provides utilities for managing JSON configuration files
//! with atomic updates and error handling.

use crate::error::KalamDbError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Configuration file manager
///
/// Provides atomic file updates for JSON configuration files.
pub struct FileManager;

impl FileManager {
    /// Read a JSON file and deserialize it
    pub fn read_json<T: for<'de> Deserialize<'de>>(path: impl AsRef<Path>) -> Result<T, KalamDbError> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .map_err(|e| KalamDbError::ConfigError(format!("Failed to read {}: {}", path.display(), e)))?;
        
        serde_json::from_str(&content)
            .map_err(|e| KalamDbError::ConfigError(format!("Failed to parse {}: {}", path.display(), e)))
    }

    /// Write data to a JSON file atomically
    ///
    /// Uses write-to-temp-then-rename pattern for atomicity
    pub fn write_json<T: Serialize>(path: impl AsRef<Path>, data: &T) -> Result<(), KalamDbError> {
        let path = path.as_ref();
        
        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| KalamDbError::ConfigError(format!("Failed to create directory {}: {}", parent.display(), e)))?;
        }

        // Serialize to JSON with pretty printing
        let json = serde_json::to_string_pretty(data)
            .map_err(|e| KalamDbError::ConfigError(format!("Failed to serialize data: {}", e)))?;

        // Write to temporary file first
        let temp_path = path.with_extension("tmp");
        fs::write(&temp_path, json)
            .map_err(|e| KalamDbError::ConfigError(format!("Failed to write to {}: {}", temp_path.display(), e)))?;

        // Rename to final path (atomic on POSIX systems)
        fs::rename(&temp_path, path)
            .map_err(|e| KalamDbError::ConfigError(format!("Failed to rename {} to {}: {}", temp_path.display(), path.display(), e)))?;

        Ok(())
    }

    /// Check if a file exists
    pub fn exists(path: impl AsRef<Path>) -> bool {
        path.as_ref().exists()
    }

    /// Create a directory if it doesn't exist
    pub fn ensure_directory(path: impl AsRef<Path>) -> Result<(), KalamDbError> {
        let path = path.as_ref();
        if !path.exists() {
            fs::create_dir_all(path)
                .map_err(|e| KalamDbError::ConfigError(format!("Failed to create directory {}: {}", path.display(), e)))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::env;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestConfig {
        name: String,
        value: i32,
    }

    #[test]
    fn test_write_and_read_json() {
        let temp_dir = env::temp_dir();
        let test_file = temp_dir.join("test_config.json");

        let config = TestConfig {
            name: "test".to_string(),
            value: 42,
        };

        // Write
        FileManager::write_json(&test_file, &config).unwrap();

        // Read
        let loaded: TestConfig = FileManager::read_json(&test_file).unwrap();

        assert_eq!(config, loaded);

        // Cleanup
        let _ = fs::remove_file(test_file);
    }

    #[test]
    fn test_ensure_directory() {
        let temp_dir = env::temp_dir();
        let test_dir = temp_dir.join("kalamdb_test_dir");

        FileManager::ensure_directory(&test_dir).unwrap();
        assert!(test_dir.exists());

        // Cleanup
        let _ = fs::remove_dir(test_dir);
    }
}
