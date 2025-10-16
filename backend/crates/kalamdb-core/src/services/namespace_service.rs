//! Namespace service for business logic
//!
//! This service handles all namespace-related operations including:
//! - Creating new namespaces with validation
//! - Listing all namespaces
//! - Updating namespace options
//! - Deleting namespaces (with table count validation)

use crate::catalog::{Namespace, NamespaceId};
use crate::config::NamespacesConfig;
use crate::error::KalamDbError;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::Path;
use std::fs;

/// Namespace service
///
/// Coordinates namespace operations across configuration persistence
/// and filesystem structure.
pub struct NamespaceService {
    config: NamespacesConfig,
    base_config_path: String,
}

impl NamespaceService {
    /// Create a new namespace service
    ///
    /// # Arguments
    /// * `namespaces_config_path` - Path to namespaces.json file
    /// * `base_config_path` - Base path for configuration (e.g., "backend/config")
    pub fn new(namespaces_config_path: impl Into<String>, base_config_path: impl Into<String>) -> Self {
        Self {
            config: NamespacesConfig::new(namespaces_config_path),
            base_config_path: base_config_path.into(),
        }
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
        if self.config.exists(name.as_str())? {
            if if_not_exists {
                return Ok(false);
            } else {
                return Err(KalamDbError::AlreadyExists(format!(
                    "Namespace '{}' already exists",
                    name.as_str()
                )));
            }
        }

        // Create namespace entity
        let namespace = Namespace::new(name.clone());

        // Save to configuration
        self.config.upsert(namespace)?;

        // Create namespace directory structure: /config/{namespace}/schemas/
        let namespace_path = Path::new(&self.base_config_path).join(name.as_str());
        let schemas_path = namespace_path.join("schemas");
        
        fs::create_dir_all(&schemas_path).map_err(|e| {
            KalamDbError::IoError(format!(
                "Failed to create namespace directory '{}': {}",
                schemas_path.display(),
                e
            ))
        })?;

        Ok(true)
    }

    /// List all namespaces
    pub fn list(&self) -> Result<Vec<Namespace>, KalamDbError> {
        let namespaces = self.config.load()?;
        let mut list: Vec<_> = namespaces.into_values().collect();
        list.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
        Ok(list)
    }

    /// Get a specific namespace by name
    pub fn get(&self, name: &str) -> Result<Option<Namespace>, KalamDbError> {
        self.config.get(name)
    }

    /// Update namespace options
    ///
    /// # Arguments
    /// * `name` - Namespace name
    /// * `options` - New options to set (merges with existing options)
    pub fn update_options(
        &self,
        name: impl Into<NamespaceId>,
        options: HashMap<String, JsonValue>,
    ) -> Result<(), KalamDbError> {
        let name = name.into();
        
        // Load existing namespace
        let mut namespace = self.config.get(name.as_str())?
            .ok_or_else(|| KalamDbError::NotFound(format!(
                "Namespace '{}' not found",
                name.as_str()
            )))?;

        // Merge options
        for (key, value) in options {
            namespace.options.insert(key, value);
        }

        // Save updated namespace
        self.config.upsert(namespace)?;

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
        let namespace = match self.config.get(name.as_str())? {
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

        // Remove from configuration
        self.config.remove(name.as_str())?;

        // Note: We don't delete the filesystem directory to preserve any data
        // Users can manually delete if needed

        Ok(true)
    }

    /// Increment table count for a namespace
    ///
    /// Called when a table is created in this namespace
    pub fn increment_table_count(&self, name: &str) -> Result<(), KalamDbError> {
        let mut namespace = self.config.get(name)?
            .ok_or_else(|| KalamDbError::NotFound(format!(
                "Namespace '{}' not found",
                name
            )))?;

        namespace.increment_table_count();
        self.config.upsert(namespace)?;

        Ok(())
    }

    /// Decrement table count for a namespace
    ///
    /// Called when a table is dropped from this namespace
    pub fn decrement_table_count(&self, name: &str) -> Result<(), KalamDbError> {
        let mut namespace = self.config.get(name)?
            .ok_or_else(|| KalamDbError::NotFound(format!(
                "Namespace '{}' not found",
                name
            )))?;

        namespace.decrement_table_count();
        self.config.upsert(namespace)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;

    fn setup_test_service() -> (NamespaceService, String, String) {
        let temp_dir = env::temp_dir();
        let test_id = format!("ns_service_test_{}_{}", std::process::id(), uuid::Uuid::new_v4());
        let config_file = temp_dir.join(format!("{}.json", test_id));
        let config_path = temp_dir.join(&test_id);
        
        // Cleanup
        let _ = fs::remove_file(&config_file);
        let _ = fs::remove_dir_all(&config_path);
        
        let service = NamespaceService::new(
            config_file.to_str().unwrap(),
            config_path.to_str().unwrap(),
        );
        
        (service, config_file.to_str().unwrap().to_string(), config_path.to_str().unwrap().to_string())
    }

    #[test]
    fn test_create_namespace() {
        let (service, _, config_path) = setup_test_service();
        
        let created = service.create("app", false).unwrap();
        assert!(created);
        
        // Verify namespace exists
        let namespace = service.get("app").unwrap().unwrap();
        assert_eq!(namespace.name.as_str(), "app");
        assert_eq!(namespace.table_count, 0);
        
        // Verify directory was created
        let schemas_path = Path::new(&config_path).join("app").join("schemas");
        assert!(schemas_path.exists());
    }

    #[test]
    fn test_create_namespace_if_not_exists() {
        let (service, _, _) = setup_test_service();
        
        let created1 = service.create("app", false).unwrap();
        assert!(created1);
        
        let created2 = service.create("app", true).unwrap();
        assert!(!created2);
    }

    #[test]
    fn test_create_namespace_duplicate_error() {
        let (service, _, _) = setup_test_service();
        
        service.create("app", false).unwrap();
        let result = service.create("app", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_create_namespace_invalid_name() {
        let (service, _, _) = setup_test_service();
        
        let result = service.create("system", false);
        assert!(result.is_err());
        
        let result = service.create("Invalid-Name", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_list_namespaces() {
        let (service, _, _) = setup_test_service();
        
        service.create("app1", false).unwrap();
        service.create("app2", false).unwrap();
        
        let namespaces = service.list().unwrap();
        assert_eq!(namespaces.len(), 2);
        assert_eq!(namespaces[0].name.as_str(), "app1");
        assert_eq!(namespaces[1].name.as_str(), "app2");
    }

    #[test]
    fn test_update_options() {
        let (service, _, _) = setup_test_service();
        
        service.create("app", false).unwrap();
        
        let mut options = HashMap::new();
        options.insert("max_tables".to_string(), JsonValue::Number(100.into()));
        options.insert("region".to_string(), JsonValue::String("us-west".to_string()));
        
        service.update_options("app", options).unwrap();
        
        let namespace = service.get("app").unwrap().unwrap();
        assert_eq!(namespace.options.len(), 2);
        assert_eq!(namespace.options.get("max_tables"), Some(&JsonValue::Number(100.into())));
    }

    #[test]
    fn test_delete_namespace() {
        let (service, _, _) = setup_test_service();
        
        service.create("app", false).unwrap();
        let deleted = service.delete("app", false).unwrap();
        assert!(deleted);
        
        let namespace = service.get("app").unwrap();
        assert!(namespace.is_none());
    }

    #[test]
    fn test_delete_namespace_with_tables() {
        let (service, _, _) = setup_test_service();
        
        service.create("app", false).unwrap();
        service.increment_table_count("app").unwrap();
        
        let result = service.delete("app", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_namespace_if_exists() {
        let (service, _, _) = setup_test_service();
        
        let deleted = service.delete("nonexistent", true).unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_table_count_management() {
        let (service, _, _) = setup_test_service();
        
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
