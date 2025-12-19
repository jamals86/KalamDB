//! Cached storage entry with lazy ObjectStore initialization
//!
//! This module provides `StorageCached`, which wraps a Storage configuration
//! with a lazily-initialized ObjectStore instance. This ensures that all tables
//! sharing the same storage use a single ObjectStore (100 tables = 1 ObjectStore).

use crate::error::KalamDbError;
use kalamdb_commons::system::Storage;
use object_store::ObjectStore;
use std::sync::{Arc, RwLock};

/// Cached storage entry containing both Storage config and lazily-initialized ObjectStore
///
/// This ensures that all tables sharing the same storage use a single ObjectStore instance,
/// avoiding the overhead of creating one per table (100 tables = 1 ObjectStore, not 100).
///
/// # Thread Safety
///
/// Uses double-check locking pattern for thread-safe lazy initialization of the ObjectStore.
/// Concurrent reads are allowed, exclusive write only during first initialization.
///
/// # Performance
///
/// - First call to `object_store()`: ~50-200μs (builds ObjectStore, slower for cloud backends)
/// - Subsequent calls: ~1μs (returns cached Arc clone)
///
/// # Example
///
/// ```ignore
/// let cached = StorageCached::new(storage);
/// let store = cached.object_store()?;  // First call builds, subsequent calls return cached
/// ```
#[derive(Debug)]
pub struct StorageCached {
    /// The storage configuration from system.storages
    pub storage: Arc<Storage>,
    /// Lazily-initialized ObjectStore instance
    /// Uses double-check locking for thread-safe initialization
    object_store: Arc<RwLock<Option<Arc<dyn ObjectStore>>>>,
}

impl StorageCached {
    /// Create a new StorageCached with the given storage configuration
    ///
    /// The ObjectStore is not built until first access via `object_store()`.
    pub fn new(storage: Storage) -> Self {
        Self {
            storage: Arc::new(storage),
            object_store: Arc::new(RwLock::new(None)),
        }
    }

    /// Get or compute ObjectStore instance with double-check locking
    ///
    /// **Performance**: First call builds store (~50-200μs for cloud), subsequent calls return cached Arc (~1μs)
    /// This achieves 50-200× speedup for repeated access, especially for cloud backends.
    ///
    /// # Returns
    /// Arc-wrapped ObjectStore for zero-copy sharing across all tables using this storage
    ///
    /// # Errors
    /// Returns error if ObjectStore construction fails (invalid credentials, network issues, etc.)
    ///
    /// # Panics
    /// Panics if RwLock is poisoned (unrecoverable lock corruption)
    pub fn object_store(&self) -> Result<Arc<dyn ObjectStore>, KalamDbError> {
        // Fast path: Check if already computed (concurrent reads allowed)
        {
            let read_guard = self
                .object_store
                .read()
                .expect("RwLock poisoned: object_store read lock failed");
            if let Some(store) = read_guard.as_ref() {
                return Ok(Arc::clone(store)); // ~1μs cached access
            }
        }

        // Slow path: Build and cache (exclusive write)
        {
            let mut write_guard = self
                .object_store
                .write()
                .expect("RwLock poisoned: object_store write lock failed");

            // Double-check: Another thread may have computed while we waited for write lock
            if let Some(store) = write_guard.as_ref() {
                return Ok(Arc::clone(store));
            }

            // Build ObjectStore from Storage configuration (~50-200μs first time)
            let store = crate::storage::build_object_store(&self.storage).map_err(|e| {
                KalamDbError::InvalidOperation(format!("Failed to build ObjectStore: {}", e))
            })?;

            // Cache for future access
            *write_guard = Some(Arc::clone(&store));

            Ok(store)
        }
    }

    /// Invalidate the cached ObjectStore instance
    ///
    /// Use this when storage configuration changes (e.g., ALTER STORAGE)
    /// to force rebuilding the ObjectStore on next access.
    ///
    /// # Thread Safety
    /// Takes exclusive write lock to clear the cached store.
    ///
    /// # Panics
    /// Panics if RwLock is poisoned (unrecoverable lock corruption)
    pub fn invalidate_object_store(&self) {
        let mut write_guard = self
            .object_store
            .write()
            .expect("RwLock poisoned: object_store write lock failed");
        *write_guard = None;
    }

    /// Check if the ObjectStore has been initialized
    ///
    /// Returns true if the ObjectStore has been built and cached.
    /// Useful for metrics and debugging.
    pub fn is_object_store_initialized(&self) -> bool {
        let read_guard = self
            .object_store
            .read()
            .expect("RwLock poisoned: object_store read lock failed");
        read_guard.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::models::ids::StorageId;
    use kalamdb_commons::models::storage::StorageType;
    use std::env;

    fn create_test_storage() -> Storage {
        let temp_dir = env::temp_dir().join("storage_cache_test");
        std::fs::create_dir_all(&temp_dir).ok();
        
        let now = chrono::Utc::now().timestamp_millis();
        Storage {
            storage_id: StorageId::from("test_storage"),
            storage_name: "test_storage".to_string(),
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
    fn test_storage_cached_new() {
        let storage = create_test_storage();
        let cached = StorageCached::new(storage);
        
        // Initially not initialized
        assert!(!cached.is_object_store_initialized());
    }

    #[test]
    fn test_storage_cached_object_store() {
        let storage = create_test_storage();
        let cached = StorageCached::new(storage);
        
        // First call should build
        let store1 = cached.object_store().expect("Failed to get object store");
        assert!(cached.is_object_store_initialized());
        
        // Second call should return cached
        let store2 = cached.object_store().expect("Failed to get object store");
        
        // Should be the same Arc (pointer equality)
        assert!(Arc::ptr_eq(&store1, &store2));
    }

    #[test]
    fn test_storage_cached_invalidate() {
        let storage = create_test_storage();
        let cached = StorageCached::new(storage);
        
        // Build the store
        let _store1 = cached.object_store().expect("Failed to get object store");
        assert!(cached.is_object_store_initialized());
        
        // Invalidate
        cached.invalidate_object_store();
        assert!(!cached.is_object_store_initialized());
        
        // Next call should rebuild
        let _store2 = cached.object_store().expect("Failed to get object store");
        assert!(cached.is_object_store_initialized());
    }
}
