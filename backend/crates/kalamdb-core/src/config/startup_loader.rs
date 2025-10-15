//! Server startup configuration loader
//!
//! This module loads all configuration from JSON files into in-memory catalog at server startup.

use crate::catalog::{Namespace, StorageLocation};
use crate::config::{FileManager, NamespacesConfig, StorageLocationsConfig};
use crate::error::KalamDbError;
use std::collections::HashMap;
use std::path::Path;

/// Server startup configuration loader
///
/// Loads all configuration from JSON files at server startup.
/// Note: RocksDB is only used for table data buffering, not for configuration storage.
pub struct StartupLoader {
    config_dir: String,
}

impl StartupLoader {
    /// Create a new startup loader
    pub fn new(config_dir: impl Into<String>) -> Self {
        Self {
            config_dir: config_dir.into(),
        }
    }

    /// Load all namespaces from namespaces.json
    pub fn load_namespaces(&self) -> Result<HashMap<String, Namespace>, KalamDbError> {
        let namespaces_path = Path::new(&self.config_dir).join("namespaces.json");
        let config = NamespacesConfig::new(namespaces_path.to_str().unwrap());
        config.load()
    }

    /// Load all storage locations from storage_locations.json
    pub fn load_storage_locations(&self) -> Result<HashMap<String, StorageLocation>, KalamDbError> {
        let locations_path = Path::new(&self.config_dir).join("storage_locations.json");
        let config = StorageLocationsConfig::new(locations_path.to_str().unwrap());
        config.load()
    }

    /// Ensure all required directories exist
    pub fn ensure_directories(&self) -> Result<(), KalamDbError> {
        // Create main config directory
        FileManager::ensure_directory(&self.config_dir)?;
        
        // Create subdirectories for namespaces
        // (Individual namespace directories will be created when namespaces are created)
        
        Ok(())
    }

    /// Load all configuration at startup
    pub fn load_all(&self) -> Result<StartupConfig, KalamDbError> {
        self.ensure_directories()?;

        let namespaces = self.load_namespaces()?;
        let storage_locations = self.load_storage_locations()?;

        Ok(StartupConfig {
            namespaces,
            storage_locations,
        })
    }
}

/// Configuration loaded at startup
#[derive(Debug)]
pub struct StartupConfig {
    /// All namespaces loaded from namespaces.json
    pub namespaces: HashMap<String, Namespace>,
    
    /// All storage locations loaded from storage_locations.json
    pub storage_locations: HashMap<String, StorageLocation>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{LocationType, NamespaceId};
    use std::env;
    use std::fs;

    #[test]
    fn test_startup_loader() {
        let temp_dir = env::temp_dir().join("kalamdb_startup_test");
        
        // Cleanup before test
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let loader = StartupLoader::new(temp_dir.to_str().unwrap());

        // Ensure directories
        loader.ensure_directories().unwrap();
        assert!(temp_dir.exists());

        // Create some test data
        let namespaces_path = temp_dir.join("namespaces.json");
        let ns_config = NamespacesConfig::new(namespaces_path.to_str().unwrap());
        ns_config.upsert(Namespace::new(NamespaceId::new("app"))).unwrap();

        let locations_path = temp_dir.join("storage_locations.json");
        let loc_config = StorageLocationsConfig::new(locations_path.to_str().unwrap());
        loc_config.upsert(StorageLocation::new("local", LocationType::Filesystem, "/data")).unwrap();

        // Load all
        let config = loader.load_all().unwrap();
        assert_eq!(config.namespaces.len(), 1);
        assert_eq!(config.storage_locations.len(), 1);

        // Cleanup
        let _ = fs::remove_dir_all(temp_dir);
    }
}
