//! Storage Registry for managing storage backends
//!
//! Provides centralized access to storage configurations and path template validation.

use crate::error::KalamDbError;
use crate::tables::system::StoragesTableProvider;
use kalamdb_commons::models::StorageId;
use kalamdb_commons::system::Storage;
use std::sync::Arc;

/// Registry for managing storage backends
///
/// Provides methods to:
/// - Retrieve storage configurations by ID
/// - List all available storages
/// - Validate path templates for correctness
pub struct StorageRegistry {
    storages_provider: Arc<StoragesTableProvider>,
    /// Default base path for local filesystem storage when base_directory is empty
    /// Comes from server config: storage.default_storage_path (e.g., "/data/storage")
    default_storage_path: String,
}

impl StorageRegistry {
    /// Create a new StorageRegistry
    pub fn new(storages_provider: Arc<StoragesTableProvider>, default_storage_path: String) -> Self {
        use std::path::{Path, PathBuf};
        // Normalize default path: if relative, resolve against current working directory
        let normalized = if Path::new(&default_storage_path).is_absolute() {
            default_storage_path
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(default_storage_path)
                .to_string_lossy()
                .into_owned()
        };
        Self {
            storages_provider,
            default_storage_path: normalized,
        }
    }

    /// Get a storage configuration by ID
    ///
    /// # Arguments
    /// * `storage_id` - The unique storage identifier
    ///
    /// # Returns
    /// * `Ok(Some(Storage))` - Storage found
    /// * `Ok(None)` - Storage not found
    /// * `Err` - Database error
    pub fn get_storage_by_id(&self, storage_id: &StorageId) -> Result<Option<Storage>, KalamDbError> {
        self.storages_provider
            .get_storage(storage_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to get storage: {}", e)))
    }

    /// Backward compatible helper that accepts a raw storage ID string.
    pub fn get_storage_config(&self, storage_id: &str) -> Result<Option<Storage>, KalamDbError> {
        let storage_id = StorageId::from(storage_id);
        self.get_storage_by_id(&storage_id)
    }

    /// List all storage configurations
    ///
    /// Returns storages ordered with 'local' first, then alphabetically by storage_id.
    ///
    /// # Returns
    /// * `Ok(Vec<Storage>)` - List of all storages
    /// * `Err` - Database error
    ///
    /// # Example
    /// ```no_run
    /// # use kalamdb_core::storage::StorageRegistry;
    /// # fn example(registry: &StorageRegistry) -> Result<(), kalamdb_core::error::KalamDbError> {
    /// let storages = registry.list_storages()?;
    /// for storage in storages {
    ///     println!("Storage: {} ({})", storage.storage_name, storage.storage_id);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn list_storages(&self) -> Result<Vec<Storage>, KalamDbError> {
        let mut storages = self
            .storages_provider
            .list_storages()
            .map_err(|e| KalamDbError::Other(format!("Failed to list storages: {}", e)))?;

        // Sort: 'local' first, then alphabetically
        storages.sort_by(|a, b| {
            if a.storage_id.is_local() {
                std::cmp::Ordering::Less
            } else if b.storage_id.is_local() {
                std::cmp::Ordering::Greater
            } else {
                a.storage_id.as_str().cmp(b.storage_id.as_str())
            }
        });

        Ok(storages)
    }

    /// Validate a path template for correctness
    ///
    /// # Template Variables
    /// - `{namespace}` - Namespace ID
    /// - `{tableName}` - Table name
    /// - `{shard}` - Shard identifier (optional for user tables)
    /// - `{userId}` - User ID (required for user tables)
    ///
    /// # Validation Rules
    /// 1. **Shared tables**: `{namespace}` must appear before `{tableName}`
    /// 2. **User tables**: Enforce ordering `{namespace}` → `{tableName}` → `{shard}` → `{userId}`
    /// 3. **User tables**: `{userId}` is required
    ///
    /// # Arguments
    /// * `template` - The path template string
    /// * `is_user_table` - True for user table templates, false for shared table templates
    ///
    /// # Returns
    /// * `Ok(())` - Template is valid
    /// * `Err` - Template validation failed with specific error message
    ///
    /// # Examples
    /// ```no_run
    /// # use kalamdb_core::storage::StorageRegistry;
    /// # fn example(registry: &StorageRegistry) -> Result<(), kalamdb_core::error::KalamDbError> {
    /// // Valid shared table template
    /// registry.validate_template("{namespace}/{tableName}", false)?;
    ///
    /// // Valid user table template
    /// registry.validate_template("{namespace}/{tableName}/{userId}", true)?;
    ///
    /// // Invalid: {userId} missing for user table
    /// // registry.validate_template("{namespace}/{tableName}", true)?; // Error!
    /// # Ok(())
    /// # }
    /// ```
    pub fn validate_template(
        &self,
        template: &str,
        is_user_table: bool,
    ) -> Result<(), KalamDbError> {
        if is_user_table {
            // User table template validation
            self.validate_user_table_template(template)
        } else {
            // Shared table template validation
            self.validate_shared_table_template(template)
        }
    }

    /// Validate shared table template
    ///
    /// Rule: {namespace} must appear before {tableName}
    fn validate_shared_table_template(&self, template: &str) -> Result<(), KalamDbError> {
        let namespace_pos = template.find("{namespace}");
        let table_name_pos = template.find("{tableName}");

        match (namespace_pos, table_name_pos) {
            (Some(ns_pos), Some(tn_pos)) => {
                if ns_pos < tn_pos {
                    Ok(())
                } else {
                    Err(KalamDbError::InvalidOperation(
                        "Shared table template: {namespace} must appear before {tableName}"
                            .to_string(),
                    ))
                }
            }
            (None, Some(_)) => Err(KalamDbError::InvalidOperation(
                "Shared table template: {namespace} is required".to_string(),
            )),
            (Some(_), None) => Err(KalamDbError::InvalidOperation(
                "Shared table template: {tableName} is required".to_string(),
            )),
            (None, None) => Err(KalamDbError::InvalidOperation(
                "Shared table template: Both {namespace} and {tableName} are required".to_string(),
            )),
        }
    }

    /// Validate user table template
    ///
    /// Rules:
    /// 1. {userId} is required
    /// 2. Enforce ordering: {namespace} → {tableName} → {shard} → {userId}
    fn validate_user_table_template(&self, template: &str) -> Result<(), KalamDbError> {
        let namespace_pos = template.find("{namespace}");
        let table_name_pos = template.find("{tableName}");
        let shard_pos = template.find("{shard}");
        let user_id_pos = template.find("{userId}");

        // Rule 1: {userId} is required
        let user_id_pos = match user_id_pos {
            Some(pos) => pos,
            None => {
                return Err(KalamDbError::InvalidOperation(
                    "User table template: {userId} is required".to_string(),
                ))
            }
        };

        // Rule 2: Enforce ordering
        if let Some(ns_pos) = namespace_pos {
            if let Some(tn_pos) = table_name_pos {
                if ns_pos >= tn_pos {
                    return Err(KalamDbError::InvalidOperation(
                        "User table template: {namespace} must appear before {tableName}"
                            .to_string(),
                    ));
                }
            }
        }

        if let (Some(tn_pos), Some(shard_pos)) = (table_name_pos, shard_pos) {
            if tn_pos >= shard_pos {
                return Err(KalamDbError::InvalidOperation(
                    "User table template: {tableName} must appear before {shard}".to_string(),
                ));
            }
        }

        if let (Some(shard_pos), user_id_pos) = (shard_pos, user_id_pos) {
            if shard_pos >= user_id_pos {
                return Err(KalamDbError::InvalidOperation(
                    "User table template: {shard} must appear before {userId}".to_string(),
                ));
            }
        }

        if let (Some(tn_pos), None) = (table_name_pos, shard_pos) {
            // No {shard}, check {tableName} → {userId} ordering
            if tn_pos >= user_id_pos {
                return Err(KalamDbError::InvalidOperation(
                    "User table template: {tableName} must appear before {userId}".to_string(),
                ));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_store::RocksDbInit;
    use once_cell::sync::Lazy;
    use std::fs;
    use std::sync::{Arc, Mutex};
    use tempfile::Builder;

    // Note: Full integration tests require a RocksDB instance
    // These are unit tests for template validation logic

    #[test]
    fn test_validate_shared_template_valid() {
        // Mock registry (validation doesn't need DB access)
        let registry = create_mock_registry();

        // Valid shared table templates
        assert!(registry
            .validate_template("{namespace}/{tableName}", false)
            .is_ok());
        assert!(registry
            .validate_template("{namespace}/tables/{tableName}", false)
            .is_ok());
    }

    #[test]
    fn test_validate_shared_template_wrong_order() {
        let registry = create_mock_registry();

        // Invalid: {tableName} before {namespace}
        assert!(registry
            .validate_template("{tableName}/{namespace}", false)
            .is_err());
    }

    #[test]
    fn test_validate_user_template_valid() {
        let registry = create_mock_registry();

        // Valid user table templates
        assert!(registry
            .validate_template("{namespace}/{tableName}/{userId}", true)
            .is_ok());
        assert!(registry
            .validate_template("{namespace}/{tableName}/{shard}/{userId}", true)
            .is_ok());
    }

    #[test]
    fn test_validate_user_template_missing_user_id() {
        let registry = create_mock_registry();

        // Invalid: {userId} missing
        assert!(registry
            .validate_template("{namespace}/{tableName}", true)
            .is_err());
    }

    #[test]
    fn test_validate_user_template_wrong_order() {
        let registry = create_mock_registry();

        // Invalid: {userId} before {tableName}
        assert!(registry
            .validate_template("{namespace}/{userId}/{tableName}", true)
            .is_err());

        // Invalid: {shard} before {tableName}
        assert!(registry
            .validate_template("{namespace}/{shard}/{tableName}/{userId}", true)
            .is_err());
    }

    // Helper to create a mock registry for tests that don't need DB
    fn create_mock_registry() -> StorageRegistry {
        static TEST_DIRS: Lazy<Mutex<Vec<tempfile::TempDir>>> =
            Lazy::new(|| Mutex::new(Vec::new()));

        let temp_dir = Builder::new()
            .prefix("kalamdb-storage-registry-tests")
            .tempdir()
            .expect("Failed to create temp directory for storage registry tests");

        let db_path = temp_dir.path().join("rocksdb");
        fs::create_dir_all(&db_path)
            .expect("Failed to create RocksDB directory for storage registry tests");

        let db_path_string = db_path.to_string_lossy().into_owned();
        TEST_DIRS
            .lock()
            .expect("Temp dir mutex poisoned")
            .push(temp_dir);

        let db_init = RocksDbInit::new(db_path_string);
        let db = db_init
            .open()
            .expect("Failed to open RocksDB for storage registry tests");

        let backend: Arc<dyn kalamdb_store::StorageBackend> =
            Arc::new(kalamdb_store::RocksDBBackend::new(db.clone()));
        
        // Create StoragesTableProvider for tests
        let storages_provider = Arc::new(crate::tables::system::StoragesTableProvider::new(backend));

        // Use a temp storage base under the temp dir for tests
        let default_storage_path = db_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join("storage")
            .to_string_lossy()
            .into_owned();
        StorageRegistry::new(storages_provider, default_storage_path)
    }
}
