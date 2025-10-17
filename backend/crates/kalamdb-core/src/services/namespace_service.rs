//! Namespace service for business logic
//!
//! This service handles all namespace-related operations including:
//! - Creating new namespaces with validation
//! - Listing all namespaces
//! - Updating namespace options
//! - Deleting namespaces (with table count validation)
//!
//! **REFACTORED**: Now uses kalamdb-sql crate for RocksDB persistence instead of JSON config files

use crate::catalog::{Namespace, NamespaceId};
use crate::error::KalamDbError;
use kalamdb_sql::{KalamSql, Namespace as SqlNamespace};
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
        let existing = self.kalam_sql.get_namespace(name.as_str())
            .map_err(|e| KalamDbError::IoError(format!("Failed to check namespace existence: {}", e)))?;
        
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

        // Create namespace entity
        let namespace = Namespace::new(name.clone());

        // Convert to SQL model and save to RocksDB via kalamdb-sql
        let options_json = serde_json::to_string(&namespace.options)
            .map_err(|e| KalamDbError::IoError(format!("Failed to serialize options: {}", e)))?;
        
        self.kalam_sql.insert_namespace(name.as_str(), &options_json)
            .map_err(|e| KalamDbError::IoError(format!("Failed to insert namespace: {}", e)))?;

        Ok(true)
    }

    /// List all namespaces
    ///
    /// **TODO**: Implement scan_all_namespaces() in kalamdb-sql adapter
    pub fn list(&self) -> Result<Vec<Namespace>, KalamDbError> {
        // TODO: When kalamdb-sql adds scan_all_namespaces(), use that instead
        // For now, return empty list with warning
        log::warn!("NamespaceService::list() not fully implemented - kalamdb-sql needs scan_all_namespaces()");
        Ok(Vec::new())
    }

    /// Get a specific namespace by name
    pub fn get(&self, name: &str) -> Result<Option<Namespace>, KalamDbError> {
        let sql_namespace = self.kalam_sql.get_namespace(name)
            .map_err(|e| KalamDbError::IoError(format!("Failed to get namespace: {}", e)))?;
        
        match sql_namespace {
            Some(ns) => {
                // Convert SQL model to core model
                let options: HashMap<String, JsonValue> = serde_json::from_str(&ns.options)
                    .unwrap_or_default();
                
                Ok(Some(Namespace {
                    name: NamespaceId::from(ns.name),
                    created_at: chrono::DateTime::from_timestamp(ns.created_at, 0)
                        .ok_or_else(|| KalamDbError::IoError("Invalid timestamp".to_string()))?,
                    options,
                    table_count: ns.table_count as u32,
                }))
            }
            None => Ok(None),
        }
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
        let mut namespace = self.get(name.as_str())?
            .ok_or_else(|| KalamDbError::NotFound(format!(
                "Namespace '{}' not found",
                name.as_str()
            )))?;

        // Merge options
        for (key, value) in options {
            namespace.options.insert(key, value);
        }

        // TODO: When kalamdb-sql adds update_namespace(), use that
        // For now, re-insert the namespace
        let options_json = serde_json::to_string(&namespace.options)
            .map_err(|e| KalamDbError::IoError(format!("Failed to serialize options: {}", e)))?;
        
        let sql_namespace = SqlNamespace {
            namespace_id: name.as_str().to_string(),
            name: name.as_str().to_string(),
            created_at: namespace.created_at.timestamp(),
            options: options_json,
            table_count: namespace.table_count as i32,
        };
        
        self.kalam_sql.insert_namespace(name.as_str(), &sql_namespace.options)
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
    ///
    /// **TODO**: Implement delete_namespace() in kalamdb-sql adapter
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

        // TODO: When kalamdb-sql adds delete_namespace(), use that
        log::warn!("NamespaceService::delete() not fully implemented - kalamdb-sql needs delete_namespace()");

        Ok(true)
    }

    /// Increment table count for a namespace
    ///
    /// Called when a table is created in this namespace
    ///
    /// **TODO**: Implement update_namespace() in kalamdb-sql adapter for atomic counter updates
    pub fn increment_table_count(&self, name: &str) -> Result<(), KalamDbError> {
        let mut namespace = self.get(name)?
            .ok_or_else(|| KalamDbError::NotFound(format!(
                "Namespace '{}' not found",
                name
            )))?;

        namespace.increment_table_count();
        
        // TODO: This should be an atomic update operation
        let options_json = serde_json::to_string(&namespace.options)
            .map_err(|e| KalamDbError::IoError(format!("Failed to serialize options: {}", e)))?;
        
        let sql_namespace = SqlNamespace {
            namespace_id: name.to_string(),
            name: name.to_string(),
            created_at: namespace.created_at.timestamp(),
            options: options_json,
            table_count: namespace.table_count as i32,
        };
        
        self.kalam_sql.insert_namespace(name, &sql_namespace.options)
            .map_err(|e| KalamDbError::IoError(format!("Failed to update table count: {}", e)))?;

        Ok(())
    }

    /// Decrement table count for a namespace
    ///
    /// Called when a table is dropped from this namespace
    ///
    /// **TODO**: Implement update_namespace() in kalamdb-sql adapter for atomic counter updates
    pub fn decrement_table_count(&self, name: &str) -> Result<(), KalamDbError> {
        let mut namespace = self.get(name)?
            .ok_or_else(|| KalamDbError::NotFound(format!(
                "Namespace '{}' not found",
                name
            )))?;

        namespace.decrement_table_count();
        
        // TODO: This should be an atomic update operation
        let options_json = serde_json::to_string(&namespace.options)
            .map_err(|e| KalamDbError::IoError(format!("Failed to serialize options: {}", e)))?;
        
        let sql_namespace = SqlNamespace {
            namespace_id: name.to_string(),
            name: name.to_string(),
            created_at: namespace.created_at.timestamp(),
            options: options_json,
            table_count: namespace.table_count as i32,
        };
        
        self.kalam_sql.insert_namespace(name, &sql_namespace.options)
            .map_err(|e| KalamDbError::IoError(format!("Failed to update table count: {}", e)))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocksdb::{DB, Options};
    use std::env;
    use std::path::PathBuf;

    fn setup_test_service() -> (NamespaceService, PathBuf) {
        let temp_dir = env::temp_dir();
        let test_id = format!("ns_service_test_{}_{}", std::process::id(), uuid::Uuid::new_v4());
        let db_path = temp_dir.join(&test_id);
        
        // Cleanup
        let _ = std::fs::remove_dir_all(&db_path);
        
        // Create RocksDB with system_namespaces column family
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        
        let db = Arc::new(DB::open_cf(&opts, &db_path, vec!["system_namespaces"]).unwrap());
        let kalam_sql = Arc::new(KalamSql::new(db).unwrap());
        
        let service = NamespaceService::new(kalam_sql);
        
        (service, db_path)
    }

    #[test]
    fn test_create_namespace() {
        let (service, _db_path) = setup_test_service();
        
        let created = service.create("app", false).unwrap();
        assert!(created);
        
        // Verify namespace exists
        let namespace = service.get("app").unwrap().unwrap();
        assert_eq!(namespace.name.as_str(), "app");
        assert_eq!(namespace.table_count, 0);
    }

    #[test]
    fn test_create_namespace_if_not_exists() {
        let (service, _) = setup_test_service();
        
        let created1 = service.create("app", false).unwrap();
        assert!(created1);
        
        let created2 = service.create("app", true).unwrap();
        assert!(!created2);
    }

    #[test]
    fn test_create_namespace_duplicate_error() {
        let (service, _) = setup_test_service();
        
        service.create("app", false).unwrap();
        let result = service.create("app", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_create_namespace_invalid_name() {
        let (service, _) = setup_test_service();
        
        let result = service.create("system", false);
        assert!(result.is_err());
        
        let result = service.create("Invalid-Name", false);
        assert!(result.is_err());
    }

    #[test]
    #[ignore] // TODO: Implement scan_all_namespaces() in kalamdb-sql first
    fn test_list_namespaces() {
        let (service, _) = setup_test_service();
        
        service.create("app1", false).unwrap();
        service.create("app2", false).unwrap();
        
        let namespaces = service.list().unwrap();
        assert_eq!(namespaces.len(), 2);
        assert_eq!(namespaces[0].name.as_str(), "app1");
        assert_eq!(namespaces[1].name.as_str(), "app2");
    }

    #[test]
    fn test_update_options() {
        let (service, _) = setup_test_service();
        
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
    #[ignore] // TODO: Implement delete_namespace() in kalamdb-sql first
    fn test_delete_namespace() {
        let (service, _) = setup_test_service();
        
        service.create("app", false).unwrap();
        let deleted = service.delete("app", false).unwrap();
        assert!(deleted);
        
        let namespace = service.get("app").unwrap();
        assert!(namespace.is_none());
    }

    #[test]
    fn test_delete_namespace_with_tables() {
        let (service, _) = setup_test_service();
        
        service.create("app", false).unwrap();
        service.increment_table_count("app").unwrap();
        
        let result = service.delete("app", false);
        assert!(result.is_err());
    }

    #[test]
    #[ignore] // TODO: Implement delete_namespace() in kalamdb-sql first
    fn test_delete_namespace_if_exists() {
        let (service, _) = setup_test_service();
        
        let deleted = service.delete("nonexistent", true).unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_table_count_management() {
        let (service, _) = setup_test_service();
        
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
