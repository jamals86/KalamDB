//! Schema versioning logic
//!
//! This module handles versioning of table schemas by creating schema_v{N}.json files.

use crate::error::KalamDbError;
use crate::schema::ArrowSchemaWithOptions;
use std::fs;
use std::path::PathBuf;

/// Schema versioning manager
///
/// Manages schema version files in the format: schema_v{N}.json
pub struct SchemaVersioning {
    schema_dir: PathBuf,
}

impl SchemaVersioning {
    /// Create a new schema versioning manager
    pub fn new(schema_dir: impl Into<PathBuf>) -> Self {
        Self {
            schema_dir: schema_dir.into(),
        }
    }

    /// Get the path for a specific schema version
    ///
    /// Format: {schema_dir}/schema_v{version}.json
    pub fn version_path(&self, version: u32) -> PathBuf {
        self.schema_dir.join(format!("schema_v{}.json", version))
    }

    /// Save a schema version
    pub fn save_version(&self, version: u32, schema: &ArrowSchemaWithOptions) -> Result<(), KalamDbError> {
        // Ensure directory exists
        fs::create_dir_all(&self.schema_dir)
            .map_err(|e| KalamDbError::SchemaError(format!("Failed to create schema directory: {}", e)))?;

        let path = self.version_path(version);
        let json_str = schema.to_json_string()?;

        fs::write(&path, json_str)
            .map_err(|e| KalamDbError::SchemaError(format!("Failed to write schema version {}: {}", version, e)))?;

        Ok(())
    }

    /// Load a schema version
    pub fn load_version(&self, version: u32) -> Result<ArrowSchemaWithOptions, KalamDbError> {
        let path = self.version_path(version);

        if !path.exists() {
            return Err(KalamDbError::SchemaError(format!("Schema version {} not found", version)));
        }

        let json_str = fs::read_to_string(&path)
            .map_err(|e| KalamDbError::SchemaError(format!("Failed to read schema version {}: {}", version, e)))?;

        ArrowSchemaWithOptions::from_json_string(&json_str)
    }

    /// Check if a version exists
    pub fn version_exists(&self, version: u32) -> bool {
        self.version_path(version).exists()
    }

    /// Get the latest version number
    ///
    /// Returns 0 if no versions exist.
    pub fn get_latest_version(&self) -> Result<u32, KalamDbError> {
        if !self.schema_dir.exists() {
            return Ok(0);
        }

        let entries = fs::read_dir(&self.schema_dir)
            .map_err(|e| KalamDbError::SchemaError(format!("Failed to read schema directory: {}", e)))?;

        let mut max_version = 0;
        for entry in entries {
            let entry = entry.map_err(|e| KalamDbError::SchemaError(format!("Failed to read directory entry: {}", e)))?;
            let filename = entry.file_name();
            let filename_str = filename.to_string_lossy();

            // Parse schema_v{N}.json
            if let Some(version_str) = filename_str.strip_prefix("schema_v").and_then(|s| s.strip_suffix(".json")) {
                if let Ok(version) = version_str.parse::<u32>() {
                    max_version = max_version.max(version);
                }
            }
        }

        Ok(max_version)
    }

    /// Create a new version (increments from latest)
    pub fn create_new_version(&self, schema: &ArrowSchemaWithOptions) -> Result<u32, KalamDbError> {
        let new_version = self.get_latest_version()? + 1;
        self.save_version(new_version, schema)?;
        Ok(new_version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::datatypes::{DataType, Field, Schema};
    use std::env;
    use std::sync::Arc;

    #[test]
    fn test_schema_versioning() {
        let temp_dir = env::temp_dir().join("kalamdb_schema_versioning_test");
        let _ = fs::remove_dir_all(&temp_dir);

        let versioning = SchemaVersioning::new(&temp_dir);

        // Create version 1
        let schema1 = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
        ]));
        let schema_with_opts1 = ArrowSchemaWithOptions::new(schema1);

        versioning.save_version(1, &schema_with_opts1).unwrap();
        assert!(versioning.version_exists(1));

        // Load version 1
        let loaded1 = versioning.load_version(1).unwrap();
        assert_eq!(loaded1.schema.fields().len(), 1);

        // Create version 2
        let schema2 = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, true),
        ]));
        let schema_with_opts2 = ArrowSchemaWithOptions::new(schema2);

        versioning.save_version(2, &schema_with_opts2).unwrap();

        // Get latest version
        assert_eq!(versioning.get_latest_version().unwrap(), 2);

        // Create new version (should be 3)
        let schema3 = Arc::new(Schema::new(vec![
            Field::new("value", DataType::Float64, false),
        ]));
        let schema_with_opts3 = ArrowSchemaWithOptions::new(schema3);

        let new_version = versioning.create_new_version(&schema_with_opts3).unwrap();
        assert_eq!(new_version, 3);

        // Cleanup
        let _ = fs::remove_dir_all(temp_dir);
    }
}
