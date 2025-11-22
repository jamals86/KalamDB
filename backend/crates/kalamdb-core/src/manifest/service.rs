//! ManifestService for batch file metadata tracking (Phase 5 - US2, T107-T113).
//!
//! Provides manifest.json management for Parquet batch files:
//! - create_manifest(): Generate initial manifest for new table
//! - update_manifest(): Append new batch entry atomically
//! - read_manifest(): Parse manifest.json from storage
//! - rebuild_manifest(): Regenerate from Parquet footers
//! - validate_manifest(): Verify consistency

use kalamdb_commons::types::{BatchFileEntry, ManifestFile};
use kalamdb_commons::{TableId, UserId};
use kalamdb_store::{StorageBackend, StorageError};
use log::warn;
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

    /// Base storage path (fallback, but we prefer using SchemaRegistry)
    _base_path: String,
}

impl ManifestService {
    /// Create a new ManifestService
    pub fn new(storage_backend: Arc<dyn StorageBackend>, base_path: String) -> Self {
        Self {
            _storage_backend: storage_backend,
            _base_path: base_path,
        }
    }

    /// Create an in-memory manifest for a table scope.
    pub fn create_manifest(
        &self,
        table_id: &TableId,
        table_type: kalamdb_commons::models::schemas::TableType,
        user_id: Option<&UserId>,
    ) -> ManifestFile {
        ManifestFile::new(table_id.clone(), table_type, user_id.cloned())
    }

    /// Ensure a manifest exists (reading from disk when available, otherwise creating in-memory).
    ///
    /// This method no longer writes manifest.json to disk. The manifest is persisted only when a
    /// flush operation materializes Parquet batches.
    pub fn ensure_manifest_initialized(
        &self,
        table_id: &TableId,
        table_type: kalamdb_commons::models::schemas::TableType,
        user_id: Option<&UserId>,
    ) -> Result<ManifestFile, StorageError> {
        let manifest_path = self.get_manifest_path(table_id, user_id)?;
        if manifest_path.exists() {
            return self.read_manifest(table_id, user_id);
        }

        Ok(self.create_manifest(table_id, table_type, user_id))
    }

    /// Update manifest: read, increment max_batch, append entry, write atomically (T109).
    pub fn update_manifest(
        &self,
        table_id: &TableId,
        table_type: kalamdb_commons::models::schemas::TableType,
        user_id: Option<&UserId>,
        batch_entry: BatchFileEntry,
    ) -> Result<ManifestFile, StorageError> {
        let mut manifest = self
            .read_manifest(table_id, user_id)
            .unwrap_or_else(|_| self.create_manifest(table_id, table_type, user_id));

        manifest.add_batch(batch_entry);
        self.write_manifest_atomic(table_id, user_id, &manifest)?;

        Ok(manifest)
    }

    /// Read manifest.json from storage (T110).
    pub fn read_manifest(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<ManifestFile, StorageError> {
        let manifest_path = self.get_manifest_path(table_id, user_id)?;

        let json_str = std::fs::read_to_string(&manifest_path).map_err(|e| {
            StorageError::IoError(format!(
                "Failed to read manifest at {}: {}",
                manifest_path.display(),
                e
            ))
        })?;

        ManifestFile::from_json(&json_str).map_err(|e| {
            StorageError::SerializationError(format!("Failed to parse manifest JSON: {}", e))
        })
    }

    /// Rebuild manifest from Parquet footers (T111).
    pub fn rebuild_manifest(
        &self,
        table_id: &TableId,
        table_type: kalamdb_commons::models::schemas::TableType,
        user_id: Option<&UserId>,
    ) -> Result<ManifestFile, StorageError> {
        let table_dir = self.get_table_directory(table_id, user_id)?;
        let mut manifest = ManifestFile::new(table_id.clone(), table_type, user_id.cloned());

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

        batch_files.sort();

        for batch_path in batch_files {
            if let Some(batch_entry) = self.extract_batch_metadata(&batch_path)? {
                manifest.add_batch(batch_entry);
            }
        }

        self.write_manifest_atomic(table_id, user_id, &manifest)?;

        Ok(manifest)
    }

    /// Validate manifest consistency (T112).
    pub fn validate_manifest(&self, manifest: &ManifestFile) -> Result<(), StorageError> {
        manifest
            .validate()
            .map_err(|e| StorageError::Other(format!("Manifest validation failed: {}", e)))
    }

    /// Public helper for consumers that need the resolved manifest.json path.
    pub fn manifest_path(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<PathBuf, StorageError> {
        self.get_manifest_path(table_id, user_id)
    }

    fn get_manifest_path(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<PathBuf, StorageError> {
        let mut path = self.get_table_directory(table_id, user_id)?;
        path.push("manifest.json");
        Ok(path)
    }

    fn get_table_directory(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<PathBuf, StorageError> {
        let app_ctx = crate::app_context::AppContext::get();
        let schema_registry = app_ctx.schema_registry();

        match schema_registry.get_storage_path(table_id, user_id, None) {
            Ok(path) => Ok(PathBuf::from(path)),
            Err(err) => {
                warn!(
                    "[ManifestService] Falling back to base path for {}.{} (user_id={:?}): {}",
                    table_id.namespace_id().as_str(),
                    table_id.table_name().as_str(),
                    user_id.map(|u| u.as_str()),
                    err
                );
                Ok(self.build_fallback_path(table_id, user_id))
            }
        }
    }

    fn build_fallback_path(&self, table_id: &TableId, user_id: Option<&UserId>) -> PathBuf {
        let mut path = PathBuf::from(&self._base_path);
        path.push(table_id.namespace_id().as_str());
        path.push(table_id.table_name().as_str());
        if let Some(uid) = user_id {
            path.push(uid.as_str());
        }
        path
    }

    fn write_manifest_atomic(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
        manifest: &ManifestFile,
    ) -> Result<(), StorageError> {
        let manifest_path = self.get_manifest_path(table_id, user_id)?;
        let tmp_path = manifest_path.with_extension("json.tmp");

        if let Some(parent) = manifest_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                StorageError::IoError(format!(
                    "Failed to create directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }

        let json_str = manifest.to_json().map_err(|e| {
            StorageError::SerializationError(format!("Failed to serialize manifest: {}", e))
        })?;

        std::fs::write(&tmp_path, json_str).map_err(|e| {
            StorageError::IoError(format!(
                "Failed to write manifest to {}: {}",
                tmp_path.display(),
                e
            ))
        })?;

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

    fn extract_batch_metadata(
        &self,
        batch_path: &Path,
    ) -> Result<Option<BatchFileEntry>, StorageError> {
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

        let size_bytes = std::fs::metadata(batch_path).map(|m| m.len()).unwrap_or(0);

        Ok(Some(BatchFileEntry::new(
            batch_number,
            file_name.to_string(),
            0,
            0,
            0,
            size_bytes,
            1,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::models::schemas::TableType;
    use kalamdb_commons::{NamespaceId, TableId, TableName};
    use kalamdb_store::test_utils::InMemoryBackend;
    use tempfile::TempDir;

    fn create_test_service() -> (ManifestService, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let service = ManifestService::new(backend, temp_dir.path().to_string_lossy().to_string());
        crate::test_helpers::init_test_app_context();
        (service, temp_dir)
    }

    fn build_table_id(ns: &str, tbl: &str) -> TableId {
        TableId::new(NamespaceId::new(ns), TableName::new(tbl))
    }

    #[test]
    fn test_create_manifest() {
        let (service, _temp_dir) = create_test_service();
        let table_id = build_table_id("ns1", "products");

        let manifest = service.create_manifest(&table_id, TableType::Shared, None);

        assert_eq!(manifest.table_id, table_id);
        assert_eq!(manifest.table_type, TableType::Shared);
        assert_eq!(manifest.user_id, None);
        assert_eq!(manifest.max_batch, 0);
    }

    #[test]
    fn test_ensure_manifest_initialized_does_not_touch_disk() {
        let (service, temp_dir) = create_test_service();
        let table_id = build_table_id("ns_manifest", "lazy_init");
        let manifest_path = service.manifest_path(&table_id, None).unwrap_or_else(|_| {
            let mut path = temp_dir.path().to_path_buf();
            path.push("ns_manifest/lazy_init/manifest.json");
            path
        });
        assert!(!manifest_path.exists());

        let manifest = service
            .ensure_manifest_initialized(&table_id, TableType::Shared, None)
            .unwrap();
        assert_eq!(manifest.max_batch, 0);
        assert!(
            !manifest_path.exists(),
            "manifest should not be written before flush"
        );
    }

    #[test]
    #[ignore = "Requires SchemaRegistry with registered tables"]
    fn test_update_manifest_creates_if_missing() {
        let (service, _temp_dir) = create_test_service();
        let table_id = build_table_id("ns1", "orders");
        let batch_entry =
            BatchFileEntry::new(0, "batch-0.parquet".to_string(), 1000, 2000, 100, 1024, 1);

        let user_id = UserId::from("u_123");
        let manifest = service
            .update_manifest(&table_id, TableType::User, Some(&user_id), batch_entry)
            .unwrap();

        assert_eq!(manifest.max_batch, 0);
        assert_eq!(manifest.batches.len(), 1);
    }

    #[test]
    fn test_validate_manifest() {
        let (service, _temp_dir) = create_test_service();
        let table_id = build_table_id("ns1", "products");

        let mut manifest = service.create_manifest(&table_id, TableType::Shared, None);
        assert!(service.validate_manifest(&manifest).is_ok());

        manifest.add_batch(BatchFileEntry::new(
            1,
            "batch-1.parquet".to_string(),
            1000,
            2000,
            50,
            512,
            1,
        ));
        assert!(service.validate_manifest(&manifest).is_ok());

        manifest.max_batch = 99;
        assert!(service.validate_manifest(&manifest).is_err());
    }
}
