use crate::error::Result;
use kalamdb_commons::{NamespaceId, StorageId, TableName, UserId};
use std::path::{Path, PathBuf};

/// Utilities for generating consistent storage paths
pub struct PathUtils;

impl PathUtils {
    /// Sanitize a path component to prevent path traversal attacks.
    /// Rejects any component containing path separators or parent directory references.
    /// 
    /// # Security
    /// This function prevents:
    /// - Path traversal via `..`
    /// - Absolute path injection via leading `/` or `\`
    /// - Hidden file access via leading `.`
    /// - Path separator injection
    fn sanitize_path_component(component: &str, context: &str) -> Result<()> {
        // Check for empty string
        if component.is_empty() {
            return Err(crate::error::FilestoreError::PathTraversal(format!(
                "{} cannot be empty",
                context
            )));
        }

        // Check for path traversal attempts
        if component.contains("..") {
            return Err(crate::error::FilestoreError::PathTraversal(format!(
                "{} contains path traversal sequence '..': {}",
                context, component
            )));
        }

        // Check for path separators
        if component.contains('/') || component.contains('\\') {
            return Err(crate::error::FilestoreError::PathTraversal(format!(
                "{} contains path separator: {}",
                context, component
            )));
        }

        // Check for hidden files (starting with .)
        if component.starts_with('.') {
            return Err(crate::error::FilestoreError::PathTraversal(format!(
                "{} cannot start with '.': {}",
                context, component
            )));
        }

        // Check for null bytes (can bypass some security checks)
        if component.contains('\0') {
            return Err(crate::error::FilestoreError::PathTraversal(format!(
                "{} contains null byte: {}",
                context, component
            )));
        }

        Ok(())
    }

    /// Get the base storage path for a storage ID
    pub fn storage_base_path(storage_id: &StorageId) -> PathBuf {
        PathBuf::from(storage_id.as_str())
    }

    /// Get the path for a namespace within a storage
    pub fn namespace_path(storage_id: &StorageId, namespace_id: &NamespaceId) -> PathBuf {
        Self::storage_base_path(storage_id).join(namespace_id.as_str())
    }

    /// Get the path for a namespace within a storage (with validation)
    pub fn namespace_path_safe(storage_id: &StorageId, namespace_id: &NamespaceId) -> Result<PathBuf> {
        Self::sanitize_path_component(namespace_id.as_str(), "Namespace ID")?;
        Ok(Self::storage_base_path(storage_id).join(namespace_id.as_str()))
    }

    /// Get the path for a shared table
    pub fn shared_table_path(
        storage_id: &StorageId,
        namespace_id: &NamespaceId,
        table_name: &TableName,
    ) -> PathBuf {
        Self::namespace_path(storage_id, namespace_id)
            .join("shared")
            .join(table_name.as_str())
    }

    /// Get the path for a shared table (with validation)
    pub fn shared_table_path_safe(
        storage_id: &StorageId,
        namespace_id: &NamespaceId,
        table_name: &TableName,
    ) -> Result<PathBuf> {
        Self::sanitize_path_component(namespace_id.as_str(), "Namespace ID")?;
        Self::sanitize_path_component(table_name.as_str(), "Table name")?;
        Ok(Self::namespace_path(storage_id, namespace_id)
            .join("shared")
            .join(table_name.as_str()))
    }

    /// Get the path for a user table
    pub fn user_table_path(
        storage_id: &StorageId,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        user_id: &UserId,
    ) -> PathBuf {
        Self::namespace_path(storage_id, namespace_id)
            .join("users")
            .join(user_id.as_str())
            .join(table_name.as_str())
    }

    /// Get the path for a user table (with validation)
    pub fn user_table_path_safe(
        storage_id: &StorageId,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        user_id: &UserId,
    ) -> Result<PathBuf> {
        Self::sanitize_path_component(namespace_id.as_str(), "Namespace ID")?;
        Self::sanitize_path_component(table_name.as_str(), "Table name")?;
        Self::sanitize_path_component(user_id.as_str(), "User ID")?;
        Ok(Self::namespace_path(storage_id, namespace_id)
            .join("users")
            .join(user_id.as_str())
            .join(table_name.as_str()))
    }

    /// Get the path for a stream table
    pub fn stream_table_path(
        storage_id: &StorageId,
        namespace_id: &NamespaceId,
        table_name: &TableName,
    ) -> PathBuf {
        Self::namespace_path(storage_id, namespace_id)
            .join("streams")
            .join(table_name.as_str())
    }

    /// Get the path for a stream table (with validation)
    pub fn stream_table_path_safe(
        storage_id: &StorageId,
        namespace_id: &NamespaceId,
        table_name: &TableName,
    ) -> Result<PathBuf> {
        Self::sanitize_path_component(namespace_id.as_str(), "Namespace ID")?;
        Self::sanitize_path_component(table_name.as_str(), "Table name")?;
        Ok(Self::namespace_path(storage_id, namespace_id)
            .join("streams")
            .join(table_name.as_str()))
    }

    /// Generate batch file name with timestamp
    pub fn batch_filename(timestamp_ms: i64, batch_index: u64) -> String {
        format!("batch_{}_{:08}.parquet", timestamp_ms, batch_index)
    }

    /// Parse batch filename to extract timestamp and index
    pub fn parse_batch_filename(filename: &str) -> Result<(i64, u64)> {
        let parts: Vec<&str> = filename.trim_end_matches(".parquet").split('_').collect();

        if parts.len() != 3 || parts[0] != "batch" {
            return Err(crate::error::FilestoreError::InvalidBatchFile(format!(
                "Invalid batch filename format: {}",
                filename
            )));
        }

        let timestamp = parts[1].parse::<i64>().map_err(|_| {
            crate::error::FilestoreError::InvalidBatchFile(format!(
                "Invalid timestamp in filename: {}",
                filename
            ))
        })?;

        let index = parts[2].parse::<u64>().map_err(|_| {
            crate::error::FilestoreError::InvalidBatchFile(format!(
                "Invalid index in filename: {}",
                filename
            ))
        })?;

        Ok((timestamp, index))
    }

    /// Ensure directory exists, creating it if necessary
    pub fn ensure_directory(path: &Path) -> Result<()> {
        if !path.exists() {
            std::fs::create_dir_all(path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_filename_generation() {
        let filename = PathUtils::batch_filename(1699000000000, 42);
        assert_eq!(filename, "batch_1699000000000_00000042.parquet");
    }

    #[test]
    fn test_batch_filename_parsing() {
        let (timestamp, index) =
            PathUtils::parse_batch_filename("batch_1699000000000_00000042.parquet")
                .expect("Failed to parse batch filename");
        assert_eq!(timestamp, 1699000000000);
        assert_eq!(index, 42);
    }

    #[test]
    fn test_invalid_batch_filename() {
        let result = PathUtils::parse_batch_filename("invalid_filename.parquet");
        assert!(result.is_err());
    }

    #[test]
    fn test_path_traversal_prevention() {
        let storage_id = StorageId::new("storage");
        let namespace_id = NamespaceId::new("../etc");
        let table_name = TableName::new("passwd");
        
        // Should fail due to path traversal in namespace
        let result = PathUtils::shared_table_path_safe(&storage_id, &namespace_id, &table_name);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("path traversal"));
    }

    #[test]
    fn test_path_separator_prevention() {
        let storage_id = StorageId::new("storage");
        let namespace_id = NamespaceId::new("valid_ns");
        let table_name = TableName::new("../../secret");
        
        // Should fail due to path traversal in table name
        let result = PathUtils::shared_table_path_safe(&storage_id, &namespace_id, &table_name);
        assert!(result.is_err());
    }

    #[test]
    fn test_hidden_file_prevention() {
        let storage_id = StorageId::new("storage");
        let namespace_id = NamespaceId::new(".hidden");
        let table_name = TableName::new("table");
        
        // Should fail due to hidden file pattern
        let result = PathUtils::shared_table_path_safe(&storage_id, &namespace_id, &table_name);
        assert!(result.is_err());
    }

    #[test]
    fn test_valid_paths_allowed() {
        let storage_id = StorageId::new("storage");
        let namespace_id = NamespaceId::new("my_namespace");
        let table_name = TableName::new("users");
        let user_id = UserId::new("user123");
        
        // All these should succeed
        assert!(PathUtils::shared_table_path_safe(&storage_id, &namespace_id, &table_name).is_ok());
        assert!(PathUtils::user_table_path_safe(&storage_id, &namespace_id, &table_name, &user_id).is_ok());
        assert!(PathUtils::stream_table_path_safe(&storage_id, &namespace_id, &table_name).is_ok());
    }
}
