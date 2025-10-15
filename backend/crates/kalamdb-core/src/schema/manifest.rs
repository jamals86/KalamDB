//! Schema manifest management
//!
//! This module manages the manifest.json file that tracks schema metadata.

use crate::error::KalamDbError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Schema manifest
///
/// Tracks metadata about table schemas, including current version and history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaManifest {
    /// Current schema version number
    pub current_version: u32,
    
    /// Total number of versions
    pub version_count: u32,
    
    /// When the current version was created
    pub current_version_created_at: String,
    
    /// Schema evolution history (version number -> creation timestamp)
    #[serde(default)]
    pub version_history: Vec<VersionHistoryEntry>,
}

/// Version history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionHistoryEntry {
    /// Version number
    pub version: u32,
    
    /// When this version was created
    pub created_at: String,
    
    /// Optional description of changes
    pub description: Option<String>,
}

impl SchemaManifest {
    /// Create a new manifest for version 1
    pub fn new() -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            current_version: 1,
            version_count: 1,
            current_version_created_at: now.clone(),
            version_history: vec![VersionHistoryEntry {
                version: 1,
                created_at: now,
                description: Some("Initial schema".to_string()),
            }],
        }
    }

    /// Update to a new version
    pub fn add_version(&mut self, description: Option<String>) {
        let now = chrono::Utc::now().to_rfc3339();
        self.current_version += 1;
        self.version_count += 1;
        self.current_version_created_at = now.clone();
        
        self.version_history.push(VersionHistoryEntry {
            version: self.current_version,
            created_at: now,
            description,
        });
    }

    /// Get the path to the manifest file
    pub fn manifest_path(schema_dir: impl AsRef<Path>) -> PathBuf {
        schema_dir.as_ref().join("manifest.json")
    }

    /// Save manifest to file
    pub fn save(&self, schema_dir: impl AsRef<Path>) -> Result<(), KalamDbError> {
        let path = Self::manifest_path(schema_dir);
        
        // Ensure directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| KalamDbError::SchemaError(format!("Failed to create directory: {}", e)))?;
        }

        let json = serde_json::to_string_pretty(&self)
            .map_err(|e| KalamDbError::SchemaError(format!("Failed to serialize manifest: {}", e)))?;

        fs::write(&path, json)
            .map_err(|e| KalamDbError::SchemaError(format!("Failed to write manifest: {}", e)))?;

        Ok(())
    }

    /// Load manifest from file
    pub fn load(schema_dir: impl AsRef<Path>) -> Result<Self, KalamDbError> {
        let path = Self::manifest_path(schema_dir);

        if !path.exists() {
            return Err(KalamDbError::SchemaError("Manifest file not found".to_string()));
        }

        let json = fs::read_to_string(&path)
            .map_err(|e| KalamDbError::SchemaError(format!("Failed to read manifest: {}", e)))?;

        serde_json::from_str(&json)
            .map_err(|e| KalamDbError::SchemaError(format!("Failed to parse manifest: {}", e)))
    }

    /// Load manifest or create a new one if it doesn't exist
    pub fn load_or_create(schema_dir: impl AsRef<Path>) -> Result<Self, KalamDbError> {
        match Self::load(&schema_dir) {
            Ok(manifest) => Ok(manifest),
            Err(_) => {
                let manifest = Self::new();
                manifest.save(&schema_dir)?;
                Ok(manifest)
            }
        }
    }
}

impl Default for SchemaManifest {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_schema_manifest() {
        let temp_dir = env::temp_dir().join("kalamdb_manifest_test");
        let _ = fs::remove_dir_all(&temp_dir);

        // Create new manifest
        let mut manifest = SchemaManifest::new();
        assert_eq!(manifest.current_version, 1);
        assert_eq!(manifest.version_count, 1);

        // Save
        manifest.save(&temp_dir).unwrap();
        assert!(SchemaManifest::manifest_path(&temp_dir).exists());

        // Load
        let loaded = SchemaManifest::load(&temp_dir).unwrap();
        assert_eq!(loaded.current_version, 1);
        assert_eq!(loaded.version_history.len(), 1);

        // Add version
        manifest.add_version(Some("Added new field".to_string()));
        assert_eq!(manifest.current_version, 2);
        assert_eq!(manifest.version_count, 2);
        assert_eq!(manifest.version_history.len(), 2);

        manifest.save(&temp_dir).unwrap();

        // Load again
        let loaded = SchemaManifest::load(&temp_dir).unwrap();
        assert_eq!(loaded.current_version, 2);
        assert_eq!(loaded.version_history.len(), 2);

        // Cleanup
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_load_or_create() {
        let temp_dir = env::temp_dir().join("kalamdb_manifest_load_or_create_test");
        let _ = fs::remove_dir_all(&temp_dir);

        // Should create new manifest
        let manifest = SchemaManifest::load_or_create(&temp_dir).unwrap();
        assert_eq!(manifest.current_version, 1);

        // Should load existing manifest
        let manifest2 = SchemaManifest::load_or_create(&temp_dir).unwrap();
        assert_eq!(manifest2.current_version, 1);

        // Cleanup
        let _ = fs::remove_dir_all(temp_dir);
    }
}
