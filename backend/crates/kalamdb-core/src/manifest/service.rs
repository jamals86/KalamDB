//! ManifestService for batch file metadata tracking (Phase 5 - US2, T107-T113).
//!
//! Provides manifest.json management for Parquet batch files:
//! - create_manifest(): Generate initial manifest for new table
//! - update_manifest(): Append new batch entry atomically
//! - read_manifest(): Parse manifest.json from storage
//! - rebuild_manifest(): Regenerate from Parquet footers
//! - validate_manifest(): Verify consistency

use kalamdb_commons::types::{BatchFileEntry, ManifestFile};
use kalamdb_commons::{NamespaceId, TableName};
use kalamdb_store::{StorageBackend, StorageError};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Service for managing manifest.json files in storage backends.
///
/// Manifest files track Parquet batch file metadata for query optimization:
/// - Batch numbering (sequential, deterministic)
/// - Min/max timestamps for temporal pruning
/// - Column statistics for predicate pushdown
/// - Row counts and file sizes for cost estimation
pub struct ManifestService {
    /// Storage backend (filesystem or S3)
    _storage_backend: Arc<dyn StorageBackend>,
    
    /// Base storage path
    base_path: String,
}

impl ManifestService {
    /// Create a new ManifestService
    ///
    /// # Arguments
    /// * `storage_backend` - Storage backend for reading/writing files
    /// * `base_path` - Base directory for table storage
    pub fn new(storage_backend: Arc<dyn StorageBackend>, base_path: String) -> Self {
        Self {
            _storage_backend: storage_backend,
            base_path,
        }
    }

    /// Create initial manifest.json for a new table (T108).
    ///
    /// Generates an empty manifest with version=1, max_batch=0.
    ///
    /// # Arguments
    /// * `namespace` - Namespace ID
    /// * `table` - Table name
    /// * `scope` - Scope ("shared" or user_id)
    ///
    /// # Returns
    /// ManifestFile ready to be written to storage
    pub fn create_manifest(
        &self,
        namespace: &NamespaceId,
        table: &TableName,
        scope: &str,
    ) -> ManifestFile {
        let table_id = format!("{}.{}", namespace.as_str(), table.as_str());
        ManifestFile::new(table_id, scope.to_string())
    }

    /// Update manifest: read, increment max_batch, append entry, write atomically (T109).
    ///
    /// Flow:
    /// 1. Read current manifest.json from storage
    /// 2. Add new BatchFileEntry
    /// 3. Increment max_batch
    /// 4. Write to manifest.json.tmp
    /// 5. Rename manifest.json.tmp → manifest.json (atomic)
    ///
    /// # Arguments
    /// * `namespace` - Namespace ID
    /// * `table` - Table name
    /// * `scope` - Scope ("shared" or user_id)
    /// * `batch_entry` - New batch file entry to append
    ///
    /// # Returns
    /// Updated ManifestFile
    pub fn update_manifest(
        &self,
        namespace: &NamespaceId,
        table: &TableName,
        scope: &str,
        batch_entry: BatchFileEntry,
    ) -> Result<ManifestFile, StorageError> {
        // Read current manifest (or create new if doesn't exist)
        let mut manifest = self
            .read_manifest(namespace, table, scope)
            .unwrap_or_else(|_| self.create_manifest(namespace, table, scope));

        // Add batch and update metadata
        manifest.add_batch(batch_entry);

        // Write atomically
        self.write_manifest_atomic(namespace, table, scope, &manifest)?;

        Ok(manifest)
    }

    /// Read manifest.json from storage (T110).
    ///
    /// # Arguments
    /// * `namespace` - Namespace ID
    /// * `table` - Table name
    /// * `scope` - Scope ("shared" or user_id)
    ///
    /// # Returns
    /// ManifestFile parsed from JSON
    pub fn read_manifest(
        &self,
        namespace: &NamespaceId,
        table: &TableName,
        scope: &str,
    ) -> Result<ManifestFile, StorageError> {
        let manifest_path = self.get_manifest_path(namespace, table, scope);
        
        // Read file contents
        let json_str = std::fs::read_to_string(&manifest_path).map_err(|e| {
            StorageError::IoError(format!(
                "Failed to read manifest at {}: {}",
                manifest_path.display(),
                e
            ))
        })?;

        // Parse JSON
        ManifestFile::from_json(&json_str).map_err(|e| {
            StorageError::SerializationError(format!("Failed to parse manifest JSON: {}", e))
        })
    }

    /// Rebuild manifest from Parquet footers (T111).
    ///
    /// Scans all batch-*.parquet files in directory, extracts metadata from
    /// Parquet footers, regenerates manifest.json.
    ///
    /// Used for:
    /// - Recovery after manifest corruption
    /// - Migration from legacy storage
    ///
    /// # Arguments
    /// * `namespace` - Namespace ID
    /// * `table` - Table name
    /// * `scope` - Scope ("shared" or user_id)
    ///
    /// # Returns
    /// Regenerated ManifestFile
    pub fn rebuild_manifest(
        &self,
        namespace: &NamespaceId,
        table: &TableName,
        scope: &str,
    ) -> Result<ManifestFile, StorageError> {
        let table_dir = self.get_table_directory(namespace, table, scope);
        let table_id = format!("{}.{}", namespace.as_str(), table.as_str());
        
        let mut manifest = ManifestFile::new(table_id, scope.to_string());

        // Scan directory for batch-*.parquet files
        let entries = std::fs::read_dir(&table_dir).map_err(|e| {
            StorageError::IoError(format!(
                "Failed to read table directory {}: {}",
                table_dir.display(),
                e
            ))
        })?;

        let mut batch_files = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                if file_name.starts_with("batch-") && file_name.ends_with(".parquet") {
                    batch_files.push(path);
                }
            }
        }

        // Sort by batch number
        batch_files.sort();

        // Extract metadata from each batch file
        for batch_path in batch_files {
            if let Some(batch_entry) = self.extract_batch_metadata(&batch_path)? {
                manifest.add_batch(batch_entry);
            }
        }

        // Write rebuilt manifest
        self.write_manifest_atomic(namespace, table, scope, &manifest)?;

        Ok(manifest)
    }

    /// Validate manifest consistency (T112).
    ///
    /// Checks:
    /// - max_batch == max(batches.batch_number)
    /// - JSON schema is valid
    /// - All referenced batch files exist
    ///
    /// # Arguments
    /// * `manifest` - ManifestFile to validate
    ///
    /// # Returns
    /// Ok(()) if valid, Err with details if invalid
    pub fn validate_manifest(&self, manifest: &ManifestFile) -> Result<(), StorageError> {
        // Use ManifestFile's built-in validation
        manifest
            .validate()
            .map_err(|e| StorageError::Other(format!("Manifest validation failed: {}", e)))
    }

    // Helper methods

    /// Get manifest.json file path
    fn get_manifest_path(
        &self,
        namespace: &NamespaceId,
        table: &TableName,
        scope: &str,
    ) -> PathBuf {
        let mut path = self.get_table_directory(namespace, table, scope);
        path.push("manifest.json");
        path
    }

    /// Get table directory path
    fn get_table_directory(
        &self,
        namespace: &NamespaceId,
        table: &TableName,
        scope: &str,
    ) -> PathBuf {
        PathBuf::from(&self.base_path)
            .join(namespace.as_str())
            .join(table.as_str())
            .join(scope)
    }

    /// Write manifest atomically: manifest.json.tmp → rename to manifest.json (T113).
    fn write_manifest_atomic(
        &self,
        namespace: &NamespaceId,
        table: &TableName,
        scope: &str,
        manifest: &ManifestFile,
    ) -> Result<(), StorageError> {
        let manifest_path = self.get_manifest_path(namespace, table, scope);
        let tmp_path = manifest_path.with_extension("json.tmp");

        // Ensure directory exists
        if let Some(parent) = manifest_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                StorageError::IoError(format!(
                    "Failed to create directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }

        // Serialize to JSON
        let json_str = manifest.to_json().map_err(|e| {
            StorageError::SerializationError(format!("Failed to serialize manifest: {}", e))
        })?;

        // Write to temp file
        std::fs::write(&tmp_path, json_str).map_err(|e| {
            StorageError::IoError(format!(
                "Failed to write manifest to {}: {}",
                tmp_path.display(),
                e
            ))
        })?;

        // Atomic rename
        std::fs::rename(&tmp_path, &manifest_path).map_err(|e| {
            StorageError::IoError(format!(
                "Failed to rename {} to {}: {}",
                tmp_path.display(),
                manifest_path.display(),
                e
            ))
        })?;

        Ok(())
    }

    /// Extract metadata from Parquet file footer (helper for rebuild_manifest).
    ///
    /// Returns None if file cannot be parsed (skip corrupted files).
    fn extract_batch_metadata(&self, batch_path: &Path) -> Result<Option<BatchFileEntry>, StorageError> {
        // Extract batch number from filename (e.g., "batch-0.parquet" → 0)
        let file_name = batch_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| {
                StorageError::Other(format!("Invalid batch file path: {:?}", batch_path))
            })?;

        let batch_number = file_name
            .strip_prefix("batch-")
            .and_then(|s| s.strip_suffix(".parquet"))
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| {
                StorageError::Other(format!("Invalid batch file name: {}", file_name))
            })?;

        // Get file size
        let size_bytes = std::fs::metadata(batch_path)
            .map(|m| m.len())
            .unwrap_or(0);

        // For now, return a basic entry
        // TODO: Parse Parquet footer to extract min/max _seq and column stats
        Ok(Some(BatchFileEntry::new(
            batch_number,
            file_name.to_string(),
            0, // min_seq (TODO: extract from Parquet footer)
            0, // max_seq (TODO: extract from Parquet footer)
            std::collections::HashMap::new(), // column_min_max (TODO: extract)
            0, // row_count (TODO: extract from Parquet footer)
            size_bytes,
            1, // schema_version
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_store::test_utils::InMemoryBackend;
    use tempfile::TempDir;

    fn create_test_service() -> (ManifestService, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let service = ManifestService::new(backend, temp_dir.path().to_string_lossy().to_string());
        (service, temp_dir)
    }

    #[test]
    fn test_create_manifest() {
        let (service, _temp_dir) = create_test_service();
        let namespace = NamespaceId::new("ns1");
        let table = TableName::new("products");

        let manifest = service.create_manifest(&namespace, &table, "shared");

        assert_eq!(manifest.table_id, "ns1.products");
        assert_eq!(manifest.scope, "shared");
        assert_eq!(manifest.version, 1);
        assert_eq!(manifest.max_batch, 0);
        assert_eq!(manifest.batches.len(), 0);
    }

    #[test]
    fn test_update_manifest_creates_if_missing() {
        let (service, _temp_dir) = create_test_service();
        let namespace = NamespaceId::new("ns1");
        let table = TableName::new("orders");

        let batch_entry = BatchFileEntry::new(
            0,
            "batch-0.parquet".to_string(),
            1000,
            2000,
            std::collections::HashMap::new(),
            100,
            1024,
            1,
        );

        let manifest = service
            .update_manifest(&namespace, &table, "u_123", batch_entry)
            .unwrap();

        assert_eq!(manifest.max_batch, 0);
        assert_eq!(manifest.batches.len(), 1);
        assert_eq!(manifest.batches[0].batch_number, 0);
    }

    #[test]
    fn test_validate_manifest() {
        let (service, _temp_dir) = create_test_service();
        let namespace = NamespaceId::new("ns1");
        let table = TableName::new("products");

        let mut manifest = service.create_manifest(&namespace, &table, "shared");

        // Valid empty manifest
        assert!(service.validate_manifest(&manifest).is_ok());

        // Add batch
        let batch = BatchFileEntry::new(
            1,
            "batch-1.parquet".to_string(),
            1000,
            2000,
            std::collections::HashMap::new(),
            50,
            512,
            1,
        );
        manifest.add_batch(batch);

        // Valid manifest with batch
        assert!(service.validate_manifest(&manifest).is_ok());

        // Corrupt max_batch
        manifest.max_batch = 99;
        assert!(service.validate_manifest(&manifest).is_err());
    }

    #[test]
    fn test_write_read_manifest() {
        let (service, _temp_dir) = create_test_service();
        let namespace = NamespaceId::new("ns1");
        let table = TableName::new("products");

        let batch_entry = BatchFileEntry::new(
            0,
            "batch-0.parquet".to_string(),
            1000,
            2000,
            std::collections::HashMap::new(),
            100,
            1024,
            1,
        );

        // Update (creates and writes)
        service
            .update_manifest(&namespace, &table, "shared", batch_entry)
            .unwrap();

        // Read back
        let manifest = service.read_manifest(&namespace, &table, "shared").unwrap();

        assert_eq!(manifest.table_id, "ns1.products");
        assert_eq!(manifest.scope, "shared");
        assert_eq!(manifest.max_batch, 0);
        assert_eq!(manifest.batches.len(), 1);
    }
}
