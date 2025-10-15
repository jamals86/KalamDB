//! Namespaces configuration persistence
//!
//! This module handles loading and saving namespace data to/from namespaces.json.

use crate::catalog::Namespace;
use crate::config::FileManager;
use crate::error::KalamDbError;
use std::collections::HashMap;
use std::path::Path;

/// Namespaces configuration handler
///
/// Manages the namespaces.json file that stores all namespace metadata.
pub struct NamespacesConfig {
    file_path: String,
}

impl NamespacesConfig {
    /// Create a new namespaces config handler
    pub fn new(file_path: impl Into<String>) -> Self {
        Self {
            file_path: file_path.into(),
        }
    }

    /// Load all namespaces from the JSON file
    ///
    /// Returns an empty HashMap if the file doesn't exist.
    pub fn load(&self) -> Result<HashMap<String, Namespace>, KalamDbError> {
        let path = Path::new(&self.file_path);
        
        if !FileManager::exists(path) {
            return Ok(HashMap::new());
        }

        FileManager::read_json(path)
    }

    /// Save all namespaces to the JSON file
    pub fn save(&self, namespaces: &HashMap<String, Namespace>) -> Result<(), KalamDbError> {
        FileManager::write_json(&self.file_path, namespaces)
    }

    /// Add or update a namespace
    pub fn upsert(&self, namespace: Namespace) -> Result<(), KalamDbError> {
        let mut namespaces = self.load()?;
        namespaces.insert(namespace.name.as_str().to_string(), namespace);
        self.save(&namespaces)
    }

    /// Remove a namespace
    pub fn remove(&self, name: &str) -> Result<Option<Namespace>, KalamDbError> {
        let mut namespaces = self.load()?;
        let removed = namespaces.remove(name);
        self.save(&namespaces)?;
        Ok(removed)
    }

    /// Get a specific namespace
    pub fn get(&self, name: &str) -> Result<Option<Namespace>, KalamDbError> {
        let namespaces = self.load()?;
        Ok(namespaces.get(name).cloned())
    }

    /// Check if a namespace exists
    pub fn exists(&self, name: &str) -> Result<bool, KalamDbError> {
        let namespaces = self.load()?;
        Ok(namespaces.contains_key(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::NamespaceId;
    use std::env;
    use std::fs;

    #[test]
    fn test_namespaces_config() {
        let temp_dir = env::temp_dir();
        let test_file = temp_dir.join("test_namespaces.json");
        
        // Cleanup before test
        let _ = fs::remove_file(&test_file);

        let config = NamespacesConfig::new(test_file.to_str().unwrap());

        // Load empty
        let namespaces = config.load().unwrap();
        assert!(namespaces.is_empty());

        // Add namespace
        let ns1 = Namespace::new(NamespaceId::new("app"));
        config.upsert(ns1.clone()).unwrap();

        // Verify it exists
        assert!(config.exists("app").unwrap());

        // Get namespace
        let loaded = config.get("app").unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().name.as_str(), "app");

        // Add another namespace
        let ns2 = Namespace::new(NamespaceId::new("analytics"));
        config.upsert(ns2).unwrap();

        // Load all
        let all = config.load().unwrap();
        assert_eq!(all.len(), 2);

        // Remove namespace
        let removed = config.remove("app").unwrap();
        assert!(removed.is_some());
        assert!(!config.exists("app").unwrap());

        // Cleanup
        let _ = fs::remove_file(test_file);
    }
}
