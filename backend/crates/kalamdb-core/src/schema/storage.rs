//! Schema directory structure creation
//!
//! This module handles creation of schema directory structures for tables.

use crate::catalog::{NamespaceId, TableName};
use crate::error::KalamDbError;
use std::fs;
use std::path::PathBuf;

/// Schema storage manager
///
/// Manages the directory structure for table schemas.
/// Format: /conf/{namespace}/schemas/{table_name}/
pub struct SchemaStorage {
    base_dir: PathBuf,
}

impl SchemaStorage {
    /// Create a new schema storage manager
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    /// Get the schema directory path for a table
    ///
    /// Format: {base_dir}/{namespace}/schemas/{table_name}/
    pub fn schema_dir(&self, namespace: &NamespaceId, table_name: &TableName) -> PathBuf {
        self.base_dir
            .join(namespace.as_str())
            .join("schemas")
            .join(table_name.as_str())
    }

    /// Create the schema directory structure for a table
    pub fn create_schema_dir(&self, namespace: &NamespaceId, table_name: &TableName) -> Result<PathBuf, KalamDbError> {
        let schema_dir = self.schema_dir(namespace, table_name);

        fs::create_dir_all(&schema_dir)
            .map_err(|e| KalamDbError::SchemaError(format!(
                "Failed to create schema directory for {}.{}: {}",
                namespace.as_str(),
                table_name.as_str(),
                e
            )))?;

        Ok(schema_dir)
    }

    /// Check if a schema directory exists
    pub fn schema_dir_exists(&self, namespace: &NamespaceId, table_name: &TableName) -> bool {
        self.schema_dir(namespace, table_name).exists()
    }

    /// Delete a schema directory and all its contents
    pub fn delete_schema_dir(&self, namespace: &NamespaceId, table_name: &TableName) -> Result<(), KalamDbError> {
        let schema_dir = self.schema_dir(namespace, table_name);

        if schema_dir.exists() {
            fs::remove_dir_all(&schema_dir)
                .map_err(|e| KalamDbError::SchemaError(format!(
                    "Failed to delete schema directory for {}.{}: {}",
                    namespace.as_str(),
                    table_name.as_str(),
                    e
                )))?;
        }

        Ok(())
    }

    /// Ensure the base directory exists
    pub fn ensure_base_dir(&self) -> Result<(), KalamDbError> {
        fs::create_dir_all(&self.base_dir)
            .map_err(|e| KalamDbError::SchemaError(format!("Failed to create base directory: {}", e)))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_schema_storage() {
        let temp_dir = env::temp_dir().join("kalamdb_schema_storage_test");
        let _ = fs::remove_dir_all(&temp_dir);

        let storage = SchemaStorage::new(&temp_dir);

        let namespace = NamespaceId::new("app");
        let table_name = TableName::new("messages");

        // Check directory doesn't exist initially
        assert!(!storage.schema_dir_exists(&namespace, &table_name));

        // Create directory
        let schema_dir = storage.create_schema_dir(&namespace, &table_name).unwrap();
        assert!(schema_dir.exists());
        assert!(storage.schema_dir_exists(&namespace, &table_name));

        // Verify path structure
        let expected_path = temp_dir.join("app").join("schemas").join("messages");
        assert_eq!(schema_dir, expected_path);

        // Delete directory
        storage.delete_schema_dir(&namespace, &table_name).unwrap();
        assert!(!storage.schema_dir_exists(&namespace, &table_name));

        // Cleanup
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_ensure_base_dir() {
        let temp_dir = env::temp_dir().join("kalamdb_base_dir_test");
        let _ = fs::remove_dir_all(&temp_dir);

        let storage = SchemaStorage::new(&temp_dir);
        storage.ensure_base_dir().unwrap();
        assert!(temp_dir.exists());

        // Cleanup
        let _ = fs::remove_dir_all(temp_dir);
    }
}
