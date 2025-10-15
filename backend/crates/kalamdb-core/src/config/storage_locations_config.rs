//! Storage locations configuration persistence
//!
//! This module handles loading and saving storage location data to/from storage_locations.json.

use crate::catalog::StorageLocation;
use crate::config::FileManager;
use crate::error::KalamDbError;
use std::collections::HashMap;
use std::path::Path;

/// Storage locations configuration handler
///
/// Manages the storage_locations.json file that stores all storage location metadata.
pub struct StorageLocationsConfig {
    file_path: String,
}

impl StorageLocationsConfig {
    /// Create a new storage locations config handler
    pub fn new(file_path: impl Into<String>) -> Self {
        Self {
            file_path: file_path.into(),
        }
    }

    /// Load all storage locations from the JSON file
    ///
    /// Returns an empty HashMap if the file doesn't exist.
    pub fn load(&self) -> Result<HashMap<String, StorageLocation>, KalamDbError> {
        let path = Path::new(&self.file_path);
        
        if !FileManager::exists(path) {
            return Ok(HashMap::new());
        }

        FileManager::read_json(path)
    }

    /// Save all storage locations to the JSON file
    pub fn save(&self, locations: &HashMap<String, StorageLocation>) -> Result<(), KalamDbError> {
        FileManager::write_json(&self.file_path, locations)
    }

    /// Add or update a storage location
    pub fn upsert(&self, location: StorageLocation) -> Result<(), KalamDbError> {
        let mut locations = self.load()?;
        locations.insert(location.location_name.clone(), location);
        self.save(&locations)
    }

    /// Remove a storage location
    pub fn remove(&self, name: &str) -> Result<Option<StorageLocation>, KalamDbError> {
        let mut locations = self.load()?;
        let removed = locations.remove(name);
        self.save(&locations)?;
        Ok(removed)
    }

    /// Get a specific storage location
    pub fn get(&self, name: &str) -> Result<Option<StorageLocation>, KalamDbError> {
        let locations = self.load()?;
        Ok(locations.get(name).cloned())
    }

    /// Check if a storage location exists
    pub fn exists(&self, name: &str) -> Result<bool, KalamDbError> {
        let locations = self.load()?;
        Ok(locations.contains_key(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::LocationType;
    use std::env;
    use std::fs;

    #[test]
    fn test_storage_locations_config() {
        let temp_dir = env::temp_dir();
        let test_file = temp_dir.join("test_storage_locations.json");
        
        // Cleanup before test
        let _ = fs::remove_file(&test_file);

        let config = StorageLocationsConfig::new(test_file.to_str().unwrap());

        // Load empty
        let locations = config.load().unwrap();
        assert!(locations.is_empty());

        // Add location
        let loc1 = StorageLocation::new("local_disk", LocationType::Filesystem, "/data");
        config.upsert(loc1.clone()).unwrap();

        // Verify it exists
        assert!(config.exists("local_disk").unwrap());

        // Get location
        let loaded = config.get("local_disk").unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().location_name, "local_disk");

        // Add another location
        let loc2 = StorageLocation::new("s3_bucket", LocationType::S3, "s3://my-bucket/data");
        config.upsert(loc2).unwrap();

        // Load all
        let all = config.load().unwrap();
        assert_eq!(all.len(), 2);

        // Remove location
        let removed = config.remove("local_disk").unwrap();
        assert!(removed.is_some());
        assert!(!config.exists("local_disk").unwrap());

        // Cleanup
        let _ = fs::remove_file(test_file);
    }
}
