//! Storage Registry for managing storage backends
//!
//! Provides centralized access to storage configurations and path template validation.

use crate::error::KalamDbError;
use kalamdb_sql::{KalamSql, Storage};
use std::sync::Arc;

/// Registry for managing storage backends
///
/// Provides methods to:
/// - Retrieve storage configurations by ID
/// - List all available storages
/// - Validate path templates for correctness
pub struct StorageRegistry {
    kalam_sql: Arc<KalamSql>,
}

impl StorageRegistry {
    /// Create a new StorageRegistry
    pub fn new(kalam_sql: Arc<KalamSql>) -> Self {
        Self { kalam_sql }
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
    ///
    /// # Example
    /// ```no_run
    /// # use kalamdb_core::storage::StorageRegistry;
    /// # fn example(registry: &StorageRegistry) -> Result<(), kalamdb_core::error::KalamDbError> {
    /// let storage = registry.get_storage("local")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_storage(&self, storage_id: &str) -> Result<Option<Storage>, KalamDbError> {
        self.kalam_sql
            .get_storage(storage_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to get storage '{}': {}", storage_id, e)))
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
            .kalam_sql
            .scan_all_storages()
            .map_err(|e| KalamDbError::Other(format!("Failed to list storages: {}", e)))?;

        // Sort: 'local' first, then alphabetically
        storages.sort_by(|a, b| {
            if a.storage_id == "local" {
                std::cmp::Ordering::Less
            } else if b.storage_id == "local" {
                std::cmp::Ordering::Greater
            } else {
                a.storage_id.cmp(&b.storage_id)
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
    pub fn validate_template(&self, template: &str, is_user_table: bool) -> Result<(), KalamDbError> {
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
                        "Shared table template: {namespace} must appear before {tableName}".to_string(),
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
                        "User table template: {namespace} must appear before {tableName}".to_string(),
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

    /// Resolve the storage ID for a user based on table configuration
    ///
    /// Implements the 5-step storage lookup chain (T169-T169e):
    /// 1. If table.use_user_storage=false, return table.storage_id
    /// 2. If table.use_user_storage=true, query user.storage_mode
    /// 3. If user.storage_mode='region', return user.storage_id
    /// 4. If user.storage_mode='table' (or NULL), fallback to table.storage_id
    /// 5. If table.storage_id is NULL, fallback to storage_id='local'
    ///
    /// # Arguments
    /// * `table_id` - The table identifier (namespace:table_name)
    /// * `user_id` - The user identifier
    ///
    /// # Returns
    /// * `Ok(String)` - Resolved storage ID
    /// * `Err` - Database error or invalid configuration
    ///
    /// # Example
    /// ```no_run
    /// # use kalamdb_core::storage::StorageRegistry;
    /// # async fn example(registry: &StorageRegistry) -> Result<(), kalamdb_core::error::KalamDbError> {
    /// let storage_id = registry.resolve_storage_for_user("ns:users", "user123").await?;
    /// println!("Using storage: {}", storage_id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn resolve_storage_for_user(
        &self,
        table_id: &str,
        user_id: &str,
    ) -> Result<String, KalamDbError> {
        // Get table configuration
        let table = self
            .kalam_sql
            .get_table(table_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to get table '{}': {}", table_id, e)))?
            .ok_or_else(|| KalamDbError::NotFound(format!("Table '{}' not found", table_id)))?;

        // T169a: Step 1 - If table.use_user_storage=false, return table.storage_id
        if !table.use_user_storage {
            return Ok(table.storage_id.unwrap_or_else(|| "local".to_string()));
        }

        // T169b: Step 2 - If table.use_user_storage=true, query user.storage_mode
        let user = self
            .kalam_sql
            .get_user(user_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to get user '{}': {}", user_id, e)))?
            .ok_or_else(|| KalamDbError::NotFound(format!("User '{}' not found", user_id)))?;

        // T169c: Step 3 - If user.storage_mode='region', return user.storage_id
        if let Some(storage_mode) = &user.storage_mode {
            if storage_mode == "region" {
                if let Some(user_storage_id) = &user.storage_id {
                    return Ok(user_storage_id.clone());
                }
            }
        }

        // T169d: Step 4 - If user.storage_mode='table' (or NULL), fallback to table.storage_id
        // T169e: Step 5 - If table.storage_id is NULL, fallback to storage_id='local'
        Ok(table.storage_id.unwrap_or_else(|| "local".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::RocksDbInit;
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
        assert!(registry.validate_template("{namespace}/{tableName}", false).is_ok());
        assert!(registry.validate_template("{namespace}/tables/{tableName}", false).is_ok());
    }

    #[test]
    fn test_validate_shared_template_wrong_order() {
        let registry = create_mock_registry();

        // Invalid: {tableName} before {namespace}
        assert!(registry.validate_template("{tableName}/{namespace}", false).is_err());
    }

    #[test]
    fn test_validate_user_template_valid() {
        let registry = create_mock_registry();

        // Valid user table templates
        assert!(registry.validate_template("{namespace}/{tableName}/{userId}", true).is_ok());
        assert!(registry
            .validate_template("{namespace}/{tableName}/{shard}/{userId}", true)
            .is_ok());
    }

    #[test]
    fn test_validate_user_template_missing_user_id() {
        let registry = create_mock_registry();

        // Invalid: {userId} missing
        assert!(registry.validate_template("{namespace}/{tableName}", true).is_err());
    }

    #[test]
    fn test_validate_user_template_wrong_order() {
        let registry = create_mock_registry();

        // Invalid: {userId} before {tableName}
        assert!(registry.validate_template("{namespace}/{userId}/{tableName}", true).is_err());

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

        let kalam_sql = Arc::new(
            KalamSql::new(db)
                .expect("Failed to create KalamSQL for storage registry tests"),
        );

        StorageRegistry::new(kalam_sql)
    }
}
