//! Namespace service for business logic
//!
//! This service handles all namespace-related operations including:
//! - Creating new namespaces with validation
//! - Listing all namespaces
//! - Updating namespace options
//! - Deleting namespaces (with table count validation)
//!
//! **REFACTORED**: Now uses kalamdb-sql crate for RocksDB persistence instead of JSON config files

use crate::error::KalamDbError;
use kalamdb_commons::models::{NamespaceId, UserId};
use kalamdb_commons::system::Namespace;
use kalamdb_sql::KalamSql;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;

/// Namespace service
///
/// Coordinates namespace operations using kalamdb-sql for RocksDB persistence
pub struct NamespaceService {
    kalam_sql: Arc<KalamSql>,
}

impl NamespaceService {
    /// Create a new namespace service
    ///
    /// # Arguments
    /// * `kalam_sql` - KalamSQL instance for system table operations
    pub fn new(kalam_sql: Arc<KalamSql>) -> Self {
        Self { kalam_sql }
    }

    /// Create a new namespace
    ///
    /// # Arguments
    /// * `name` - Namespace name to create
    /// * `if_not_exists` - If true, don't error if namespace already exists
    ///
    /// # Returns
    /// * `Ok(true)` - Namespace was created
    /// * `Ok(false)` - Namespace already existed and if_not_exists was true
    /// * `Err(_)` - Validation or I/O error
    pub fn create(
        &self,
        name: impl Into<NamespaceId>,
        if_not_exists: bool,
    ) -> Result<bool, KalamDbError> {
        let name = name.into();

        // Validate namespace name
        Namespace::validate_name(name.as_str())?;

        // Check if namespace already exists
        let namespace_id = NamespaceId::new(name.as_str());
        let existing = self.kalam_sql.get_namespace(&namespace_id).map_err(|e| {
            KalamDbError::IoError(format!("Failed to check namespace existence: {}", e))
        })?;

        if existing.is_some() {
            if if_not_exists {
                return Ok(false);
            } else {
                return Err(KalamDbError::AlreadyExists(format!(
                    "Namespace '{}' already exists",
                    name.as_str()
                )));
            }
        }

        // Create namespace entity with system as default owner
        let namespace = Namespace::new(name.as_str());

        self.kalam_sql
            .insert_namespace_struct(&namespace)
            .map_err(|e| KalamDbError::IoError(format!("Failed to insert namespace: {}", e)))?;

        Ok(true)
    }

    /// Check whether a namespace already exists.
    pub fn namespace_exists(&self, name: &str) -> Result<bool, KalamDbError> {
        let namespace_id = NamespaceId::new(name);
        self.kalam_sql.namespace_exists(&namespace_id).map_err(|e| {
            KalamDbError::IoError(format!("Failed to check namespace existence: {}", e))
        })
    }

    /// List all namespaces
    pub fn list(&self) -> Result<Vec<Namespace>, KalamDbError> {
        self.kalam_sql
            .scan_all_namespaces()
            .map_err(|e| KalamDbError::IoError(format!("Failed to scan namespaces: {}", e)))
    }

    /// Get a specific namespace by name
    pub fn get(&self, name: &str) -> Result<Option<Namespace>, KalamDbError> {
        let namespace_id = NamespaceId::new(name);
        self.kalam_sql
            .get_namespace(&namespace_id)
            .map_err(|e| KalamDbError::IoError(format!("Failed to get namespace: {}", e)))
    }

    /// Update namespace options
    ///
    /// # Arguments
    /// * `name` - Namespace name
    /// * `options` - New options to set (merges with existing options)
    ///
    /// **TODO**: Implement update_namespace() in kalamdb-sql adapter
    pub fn update_options(
        &self,
        name: impl Into<NamespaceId>,
        options: HashMap<String, JsonValue>,
    ) -> Result<(), KalamDbError> {
        let name = name.into();

        // Load existing namespace
        let mut namespace = self.get(name.as_str())?.ok_or_else(|| {
            KalamDbError::NotFound(format!("Namespace '{}' not found", name.as_str()))
        })?;

        // Merge options - parse existing, merge, serialize back
        let mut existing_options: HashMap<String, JsonValue> = namespace
            .options
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        for (key, value) in options {
            existing_options.insert(key, value);
        }

        namespace.options =
            Some(serde_json::to_string(&existing_options).map_err(|e| {
                KalamDbError::IoError(format!("Failed to serialize options: {}", e))
            })?);

        self.kalam_sql
            .insert_namespace_struct(&namespace)
            .map_err(|e| KalamDbError::IoError(format!("Failed to update namespace: {}", e)))?;

        Ok(())
    }

    /// Delete a namespace
    ///
    /// # Arguments
    /// * `name` - Namespace name to delete
    /// * `if_exists` - If true, don't error if namespace doesn't exist
    ///
    /// # Returns
    /// * `Ok(true)` - Namespace was deleted
    /// * `Ok(false)` - Namespace didn't exist and if_exists was true
    /// * `Err(_)` - Namespace has tables or I/O error
    pub fn delete(
        &self,
        name: impl Into<NamespaceId>,
        if_exists: bool,
    ) -> Result<bool, KalamDbError> {
        let name = name.into();

        // Check if namespace exists
        let namespace = match self.get(name.as_str())? {
            Some(ns) => ns,
            None => {
                if if_exists {
                    return Ok(false);
                } else {
                    return Err(KalamDbError::NotFound(format!(
                        "Namespace '{}' not found",
                        name.as_str()
                    )));
                }
            }
        };

        // Check if namespace has tables
        if !namespace.can_delete() {
            return Err(KalamDbError::InvalidOperation(format!(
                "Cannot drop namespace '{}': namespace contains {} table(s). Drop all tables first.",
                name.as_str(),
                namespace.table_count
            )));
        }

        let namespace_id = NamespaceId::new(name.as_str());
        self.kalam_sql
            .delete_namespace(&namespace_id)
            .map_err(|e| KalamDbError::IoError(format!("Failed to delete namespace: {}", e)))?;

        Ok(true)
    }

    /// Increment table count for a namespace
    ///
    /// Called when a table is created in this namespace
    ///
    /// **TODO**: Implement update_namespace() in kalamdb-sql adapter for atomic counter updates
    pub fn increment_table_count(&self, name: &str) -> Result<(), KalamDbError> {
        let mut namespace = self
            .get(name)?
            .ok_or_else(|| KalamDbError::NotFound(format!("Namespace '{}' not found", name)))?;

        namespace.increment_table_count();

        self.kalam_sql
            .insert_namespace_struct(&namespace)
            .map_err(|e| KalamDbError::IoError(format!("Failed to update table count: {}", e)))?;

        Ok(())
    }

    /// Decrement table count for a namespace
    ///
    /// Called when a table is dropped from this namespace
    ///
    /// **TODO**: Implement update_namespace() in kalamdb-sql adapter for atomic counter updates
    pub fn decrement_table_count(&self, name: &str) -> Result<(), KalamDbError> {
        let mut namespace = self
            .get(name)?
            .ok_or_else(|| KalamDbError::NotFound(format!("Namespace '{}' not found", name)))?;

        namespace.decrement_table_count();

        self.kalam_sql
            .insert_namespace_struct(&namespace)
            .map_err(|e| KalamDbError::IoError(format!("Failed to update table count: {}", e)))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_store::test_utils::TestDb;
    use kalamdb_store::{kalamdb_commons::storage::StorageBackend, RocksDBBackend};

    fn setup_test_service() -> NamespaceService {
        let test_db = TestDb::new(&["system_namespaces"]).unwrap();
        let backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(test_db.db.clone()));
        let kalam_sql = Arc::new(KalamSql::new(backend).unwrap());
        NamespaceService::new(kalam_sql)
    }

    #[test]
    fn test_create_namespace() {
        let service = setup_test_service();

        let created = service.create("app", false).unwrap();
        assert!(created);

        // Verify namespace exists
        let namespace = service.get("app").unwrap().unwrap();
        assert_eq!(namespace.name, "app");
        assert_eq!(namespace.table_count, 0);
    }

    #[test]
    fn test_create_namespace_if_not_exists() {
        let service = setup_test_service();

        let created1 = service.create("app", false).unwrap();
        assert!(created1);

        let created2 = service.create("app", true).unwrap();
        assert!(!created2);
    }

    #[test]
    fn test_create_namespace_duplicate_error() {
        let service = setup_test_service();

        service.create("app", false).unwrap();
        let result = service.create("app", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_create_namespace_invalid_name() {
        let service = setup_test_service();

        let result = service.create("system", false);
        assert!(result.is_err());

        let result = service.create("Invalid-Name", false);
        assert!(result.is_err());
    }

    #[test]
    #[ignore] // TODO: Implement scan_all_namespaces() in kalamdb-sql first
    fn test_list_namespaces() {
        let service = setup_test_service();

        service.create("app1", false).unwrap();
        service.create("app2", false).unwrap();

        let namespaces = service.list().unwrap();
        assert_eq!(namespaces.len(), 2);
        assert_eq!(namespaces[0].name, "app1");
        assert_eq!(namespaces[1].name, "app2");
    }

    #[test]
    fn test_update_options() {
        let service = setup_test_service();

        service.create("app", false).unwrap();

        let mut options = HashMap::new();
        options.insert("max_tables".to_string(), JsonValue::Number(100.into()));
        options.insert(
            "region".to_string(),
            JsonValue::String("us-west".to_string()),
        );

        service.update_options("app", options).unwrap();

        let namespace = service.get("app").unwrap().unwrap();
        let parsed_options: HashMap<String, JsonValue> =
            serde_json::from_str(namespace.options.as_ref().unwrap()).unwrap();
        assert_eq!(parsed_options.len(), 2);
        assert_eq!(
            parsed_options.get("max_tables"),
            Some(&JsonValue::Number(100.into()))
        );
    }

    #[test]
    #[ignore] // TODO: Implement delete_namespace() in kalamdb-sql first
    fn test_delete_namespace() {
        let service = setup_test_service();

        service.create("app", false).unwrap();
        let deleted = service.delete("app", false).unwrap();
        assert!(deleted);

        let namespace = service.get("app").unwrap();
        assert!(namespace.is_none());
    }

    #[test]
    fn test_delete_namespace_with_tables() {
        let service = setup_test_service();

        service.create("app", false).unwrap();
        service.increment_table_count("app").unwrap();

        let result = service.delete("app", false);
        assert!(result.is_err());
    }

    #[test]
    #[ignore] // TODO: Implement delete_namespace() in kalamdb-sql first
    fn test_delete_namespace_if_exists() {
        let service = setup_test_service();

        let deleted = service.delete("nonexistent", true).unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_table_count_management() {
        let service = setup_test_service();

        service.create("app", false).unwrap();

        service.increment_table_count("app").unwrap();
        let namespace = service.get("app").unwrap().unwrap();
        assert_eq!(namespace.table_count, 1);

        service.increment_table_count("app").unwrap();
        let namespace = service.get("app").unwrap().unwrap();
        assert_eq!(namespace.table_count, 2);

        service.decrement_table_count("app").unwrap();
        let namespace = service.get("app").unwrap().unwrap();
        assert_eq!(namespace.table_count, 1);
    }
}
