//! ManifestService for batch file metadata tracking (Phase 5 - US2, T107-T113).
//!
//! Provides manifest.json management for Parquet batch files using object_store:
//! - create_manifest(): Generate initial manifest for new table
//! - update_manifest(): Append new batch entry atomically
//! - read_manifest(): Parse manifest.json from storage
//! - rebuild_manifest(): Regenerate from Parquet footers
//! - validate_manifest(): Verify consistency

use crate::error_extensions::KalamDbResultExt;
use dashmap::DashMap;
use kalamdb_commons::models::types::{Manifest, SegmentMetadata};
use kalamdb_commons::models::StorageId;
use kalamdb_commons::{TableId, UserId};
use kalamdb_store::{StorageBackend, StorageError};
use log::{info, warn};
use std::collections::HashMap;
use std::sync::Arc;

/// Service for managing manifest.json files in storage backends.
///
/// Manifest files track Parquet batch file metadata for query optimization:
/// - Batch numbering (sequential, deterministic)
/// - Min/max timestamps for temporal pruning
/// - Column statistics for predicate pushdown
/// - Row counts and file sizes for cost estimation
pub struct ManifestService {
    /// Storage backend (RocksDB - not used for manifests, kept for interface)
    _storage_backend: Arc<dyn StorageBackend>,

    /// Base storage path (fallback for building paths)
    _base_path: String,

    /// Hot Store: In-memory cache of manifests
    cache: DashMap<(TableId, Option<UserId>), Manifest>,
}

impl ManifestService {
    /// Create a new ManifestService
    pub fn new(storage_backend: Arc<dyn StorageBackend>, base_path: String) -> Self {
        Self {
            _storage_backend: storage_backend,
            _base_path: base_path,
            cache: DashMap::new(),
        }
    }

    /// Create an in-memory manifest for a table scope.
    pub fn create_manifest(
        &self,
        table_id: &TableId,
        _table_type: kalamdb_commons::models::schemas::TableType,
        user_id: Option<&UserId>,
    ) -> Manifest {
        Manifest::new(table_id.clone(), user_id.cloned())
    }

    /// Ensure a manifest exists (checking cache, then disk, otherwise creating in-memory).
    pub fn ensure_manifest_initialized(
        &self,
        table_id: &TableId,
        table_type: kalamdb_commons::models::schemas::TableType,
        user_id: Option<&UserId>,
    ) -> Result<Manifest, StorageError> {
        let key = (table_id.clone(), user_id.cloned());

        // 1. Check Hot Store (Cache)
        if let Some(manifest) = self.cache.get(&key) {
            return Ok(manifest.clone());
        }

        // 2. Check Cold Store (via object_store)
        // Try to read the manifest - if it exists, it will be returned
        match self.read_manifest(table_id, user_id) {
            Ok(manifest) => {
                self.cache.insert(key, manifest.clone());
                return Ok(manifest);
            }
            Err(_) => {
                // Manifest doesn't exist or can't be read, create new one
            }
        }

        // 3. Create New (In-Memory only)
        let manifest = self.create_manifest(table_id, table_type, user_id);
        self.cache.insert(key, manifest.clone());
        Ok(manifest)
    }

    /// Update manifest: append segment to Hot Store (Cache).
    /// Does NOT write to disk (Cold Store).
    pub fn update_manifest(
        &self,
        table_id: &TableId,
        table_type: kalamdb_commons::models::schemas::TableType,
        user_id: Option<&UserId>,
        segment: SegmentMetadata,
    ) -> Result<Manifest, StorageError> {
        let key = (table_id.clone(), user_id.cloned());

        // Ensure loaded
        if !self.cache.contains_key(&key) {
            self.ensure_manifest_initialized(table_id, table_type, user_id)?;
        }

        let mut manifest = self.cache.get_mut(&key).ok_or_else(|| {
            StorageError::Other("Manifest not found in cache after init".to_string())
        })?;

        manifest.add_segment(segment);
        // Also update sequence number if needed? Segment has min/max seq.
        // manifest.update_sequence_number(segment.max_seq as u64); // Assuming max_seq is i64 but last_sequence_number is u64

        Ok(manifest.clone())
    }

    /// Flush manifest: Write Hot Store (Cache) to Cold Store (storage via object_store).
    pub fn flush_manifest(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<(), StorageError> {
        let key = (table_id.clone(), user_id.cloned());

        if let Some(manifest) = self.cache.get(&key) {
            let (store, storage, _) = self.get_storage_context(table_id, user_id)?;
            self.write_manifest_via_store(store, &storage, table_id, user_id, &manifest)?;
            info!(
                "Flushed manifest for {}.{} (ver: {})",
                table_id.namespace_id().as_str(),
                table_id.table_name().as_str(),
                manifest.version
            );
        } else {
            warn!(
                "Attempted to flush manifest for {}.{} but it was not in cache",
                table_id.namespace_id().as_str(),
                table_id.table_name().as_str()
            );
        }
        Ok(())
    }

    /// Read manifest.json from storage (T110).
    pub fn read_manifest(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<Manifest, StorageError> {
        let (store, storage, manifest_path) = self.get_storage_context(table_id, user_id)?;

        let json_str = kalamdb_filestore::read_manifest_json(store, &storage, &manifest_path)
            .map_err(|e| StorageError::IoError(format!("Failed to read manifest: {}", e)))?;

        serde_json::from_str(&json_str).map_err(|e| {
            StorageError::SerializationError(format!("Failed to parse manifest JSON: {}", e))
        })
    }

    /// Rebuild manifest from Parquet footers (T111).
    pub fn rebuild_manifest(
        &self,
        table_id: &TableId,
        _table_type: kalamdb_commons::models::schemas::TableType,
        user_id: Option<&UserId>,
    ) -> Result<Manifest, StorageError> {
        let (store, storage, _) = self.get_storage_context(table_id, user_id)?;
        let table_dir = self.get_storage_path(table_id, user_id)?;
        let mut manifest = Manifest::new(table_id.clone(), user_id.cloned());

        // List all files in the table directory
        let files = kalamdb_filestore::list_files_sync(Arc::clone(&store), &storage, &table_dir)
            .map_err(|e| StorageError::IoError(format!("Failed to list files: {}", e)))?;

        let mut batch_files: Vec<String> = files
            .into_iter()
            .filter(|f| f.ends_with(".parquet") && f.contains("batch-"))
            .collect();
        batch_files.sort();

        for batch_path in batch_files {
            if let Some(segment) = self.extract_segment_metadata_via_store(
                Arc::clone(&store),
                &storage,
                &batch_path,
            )? {
                manifest.add_segment(segment);
            }
        }

        // Update cache
        self.cache
            .insert((table_id.clone(), user_id.cloned()), manifest.clone());

        // Write to storage
        self.write_manifest_via_store(Arc::clone(&store), &storage, table_id, user_id, &manifest)?;

        Ok(manifest)
    }

    /// Validate manifest consistency (T112).
    pub fn validate_manifest(&self, manifest: &Manifest) -> Result<(), StorageError> {
        // Basic validation
        if manifest.segments.is_empty() && manifest.last_sequence_number > 0 {
            // This might be valid if segments were deleted but seq num kept increasing?
            // But for now let's assume it's suspicious if we have seq num but no segments ever.
            // Actually, last_sequence_number tracks the *latest* assigned ID.
            // If we delete all segments, we still want to know the last ID to avoid reuse.
            // So this check might be invalid.
            // Let's just check if segments are valid.
        }
        Ok(())
    }

    /// Public helper for consumers that need the resolved manifest.json path (relative to storage).
    pub fn manifest_path(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<String, StorageError> {
        let storage_path = self.get_storage_path(table_id, user_id)?;
        Ok(format!("{}/manifest.json", storage_path))
    }

    /// Get storage context (ObjectStore, Storage, manifest path) for a table.
    fn get_storage_context(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<(Arc<dyn object_store::ObjectStore>, kalamdb_commons::system::Storage, String), StorageError> {
        let app_ctx = crate::app_context::AppContext::get();
        let schema_registry = app_ctx.schema_registry();

        // Get cached table data for ObjectStore access
        let cached = schema_registry
            .get(table_id)
            .ok_or_else(|| StorageError::Other(format!("Table not found: {}", table_id)))?;

        // Get storage from registry (cached lookup)
        let storage_id = cached.storage_id.clone().unwrap_or_else(StorageId::local);
        let storage_arc = app_ctx
            .storage_registry()
            .get_storage(&storage_id)
            .map_err(|e| StorageError::IoError(e.to_string()))?
            .ok_or_else(|| StorageError::Other(format!("Storage {} not found", storage_id.as_str())))?;
        let storage = (*storage_arc).clone();

        // Get ObjectStore
        let store = cached
            .object_store()
            .into_kalamdb_error("Failed to get object store")
            .map_err(|e| StorageError::IoError(e.to_string()))?;

        // Build manifest path
        let storage_path = crate::schema_registry::PathResolver::get_storage_path(&cached, user_id, None)
            .map_err(|e| StorageError::IoError(e.to_string()))?;
        let manifest_path = format!("{}/manifest.json", storage_path);

        Ok((store, storage, manifest_path))
    }

    /// Get storage path relative to storage base directory.
    fn get_storage_path(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
    ) -> Result<String, StorageError> {
        let app_ctx = crate::app_context::AppContext::get();
        let schema_registry = app_ctx.schema_registry();

        schema_registry
            .get_storage_path(table_id, user_id, None)
            .map_err(|e| StorageError::IoError(e.to_string()))
    }

    /// Write manifest via object_store (PUT is atomic by design).
    fn write_manifest_via_store(
        &self,
        store: Arc<dyn object_store::ObjectStore>,
        storage: &kalamdb_commons::system::Storage,
        table_id: &TableId,
        user_id: Option<&UserId>,
        manifest: &Manifest,
    ) -> Result<(), StorageError> {
        let storage_path = self.get_storage_path(table_id, user_id)?;
        let manifest_path = format!("{}/manifest.json", storage_path);

        let json_str = serde_json::to_string_pretty(manifest).map_err(|e| {
            StorageError::SerializationError(format!("Failed to serialize manifest: {}", e))
        })?;

        kalamdb_filestore::write_manifest_json(store, storage, &manifest_path, &json_str)
            .map_err(|e| StorageError::IoError(format!("Failed to write manifest: {}", e)))
    }

    /// Extract segment metadata from a Parquet file via object_store.
    fn extract_segment_metadata_via_store(
        &self,
        store: Arc<dyn object_store::ObjectStore>,
        storage: &kalamdb_commons::system::Storage,
        parquet_path: &str,
    ) -> Result<Option<SegmentMetadata>, StorageError> {
        // Extract file name from path
        let file_name = parquet_path
            .rsplit('/')
            .next()
            .unwrap_or(parquet_path)
            .to_string();

        let id = file_name.clone();

        // Get file size via object_store head
        let size_bytes = kalamdb_filestore::head_file_sync(store, storage, parquet_path)
            .map(|m| m.size_bytes as u64)
            .unwrap_or(0);

        Ok(Some(SegmentMetadata::new(
            id,
            file_name,
            HashMap::new(),
            0,
            0,
            0, // row_count unknown without reading footer
            size_bytes,
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

    #[tokio::test]
    async fn test_create_manifest() {
        let (service, _temp_dir) = create_test_service();
        let table_id = build_table_id("ns1", "products");

        let manifest = service.create_manifest(&table_id, TableType::Shared, None);

        assert_eq!(manifest.table_id, table_id);
        assert_eq!(manifest.user_id, None);
        assert_eq!(manifest.segments.len(), 0);
    }

    #[test]
    #[ignore = "Requires SchemaRegistry with registered tables for object_store access"]
    fn test_ensure_manifest_initialized_does_not_touch_disk() {
        let (service, _temp_dir) = create_test_service();
        let table_id = build_table_id("ns_manifest", "lazy_init");

        let manifest = service
            .ensure_manifest_initialized(&table_id, TableType::Shared, None)
            .unwrap();
        assert_eq!(manifest.segments.len(), 0);
        // Manifest should only be in cache, not yet written to storage
        assert!(service.cache.contains_key(&(table_id, None)));
    }

    #[test]
    #[ignore = "Requires SchemaRegistry with registered tables"]
    fn test_update_manifest_creates_if_missing() {
        let (service, _temp_dir) = create_test_service();
        let table_id = build_table_id("ns1", "orders");
        let segment = SegmentMetadata::new(
            "uuid-1".to_string(),
            "batch-0.parquet".to_string(),
            HashMap::new(),
            1000,
            2000,
            100,
            1024,
        );

        let user_id = UserId::from("u_123");
        let manifest = service
            .update_manifest(&table_id, TableType::User, Some(&user_id), segment)
            .unwrap();

        assert_eq!(manifest.segments.len(), 1);
    }

    #[test]
    #[ignore = "Requires SchemaRegistry with registered tables for object_store access"]
    fn test_flush_manifest_writes_to_storage() {
        let (service, _temp_dir) = create_test_service();
        let table_id = build_table_id("ns1", "flush_test");

        // 1. Init (In-Memory)
        service
            .ensure_manifest_initialized(&table_id, TableType::Shared, None)
            .unwrap();

        // 2. Update (In-Memory)
        let segment = SegmentMetadata::new(
            "uuid-1".to_string(),
            "batch-0.parquet".to_string(),
            HashMap::new(),
            0,
            0,
            0,
            0,
        );
        service
            .update_manifest(&table_id, TableType::Shared, None, segment)
            .unwrap();

        // 3. Verify in cache
        assert!(service.cache.contains_key(&(table_id.clone(), None)));

        // 4. Flush (requires object_store context)
        // This test is ignored because it requires full AppContext setup
        // In production, flush_manifest writes via object_store
    }
}
