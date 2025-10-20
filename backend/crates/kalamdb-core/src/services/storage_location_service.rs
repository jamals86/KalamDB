//! Storage location service for business logic
//!
//! This service handles all storage location-related operations including:
//! - Adding new storage locations with validation
//! - Updating existing locations
//! - Deleting locations (with usage count validation)
//! - Resolving location references
//! - Managing usage counts

use crate::catalog::{LocationType, StorageLocation, UserId};
use crate::error::KalamDbError;
use crate::tables::system::{StorageLocationRecord, StorageLocationsTableProvider};
use std::path::Path;
use std::sync::Arc;

/// Storage location service
///
/// Coordinates storage location operations with validation and persistence.
pub struct StorageLocationService {
    provider: Arc<StorageLocationsTableProvider>,
}

impl StorageLocationService {
    /// Create a new storage location service
    pub fn new(provider: Arc<StorageLocationsTableProvider>) -> Self {
        Self { provider }
    }

    /// Add a new storage location
    ///
    /// # Arguments
    /// * `location_name` - Unique identifier for the location
    /// * `location_type` - Type of storage (Filesystem or S3)
    /// * `path` - Path template (may include ${user_id})
    /// * `credentials_ref` - Optional reference to credentials (for S3)
    /// * `validate_accessibility` - If true, test path accessibility before adding
    ///
    /// # Returns
    /// * `Ok(())` - Location was created
    /// * `Err(_)` - Validation error or location already exists
    pub fn add(
        &self,
        location_name: impl Into<String>,
        location_type: LocationType,
        path: impl Into<String>,
        credentials_ref: Option<String>,
        validate_accessibility: bool,
    ) -> Result<(), KalamDbError> {
        let location_name = location_name.into();
        let path = path.into();

        // Validate location name format
        StorageLocation::validate_name(&location_name)?;

        // Validate path accessibility if requested
        if validate_accessibility {
            self.validate_path_accessibility(&path, &location_type)?;
        }

        let now = chrono::Utc::now().timestamp_millis();
        let record = StorageLocationRecord {
            location_name,
            location_type: format!("{:?}", location_type),
            path,
            credentials_ref,
            usage_count: 0,
            created_at: now,
            updated_at: now,
        };

        self.provider.insert_location(record)
    }

    /// Update an existing storage location
    ///
    /// # Arguments
    /// * `location_name` - Location to update
    /// * `path` - New path template (optional)
    /// * `credentials_ref` - New credentials reference (optional)
    /// * `validate_accessibility` - If true, test new path accessibility
    pub fn update(
        &self,
        location_name: impl Into<String>,
        path: Option<String>,
        credentials_ref: Option<Option<String>>,
        validate_accessibility: bool,
    ) -> Result<(), KalamDbError> {
        let location_name = location_name.into();

        // Get existing location
        let mut location = self.provider.get_location(&location_name)?.ok_or_else(|| {
            KalamDbError::NotFound(format!("Storage location '{}' not found", location_name))
        })?;

        // Update fields if provided
        if let Some(new_path) = path {
            if validate_accessibility {
                let location_type = match location.location_type.as_str() {
                    "Filesystem" => LocationType::Filesystem,
                    "S3" => LocationType::S3,
                    _ => LocationType::Filesystem,
                };
                self.validate_path_accessibility(&new_path, &location_type)?;
            }
            location.path = new_path;
        }

        if let Some(new_creds) = credentials_ref {
            location.credentials_ref = new_creds;
        }

        location.updated_at = chrono::Utc::now().timestamp_millis();

        self.provider.update_location(location)
    }

    /// Delete a storage location
    ///
    /// # Arguments
    /// * `location_name` - Location to delete
    ///
    /// # Returns
    /// * `Ok(())` - Location was deleted
    /// * `Err(_)` - Location not found or still in use
    pub fn delete(&self, location_name: impl Into<String>) -> Result<(), KalamDbError> {
        let location_name = location_name.into();
        self.provider.delete_location(&location_name)
    }

    /// Get a storage location by name
    pub fn get(
        &self,
        location_name: impl Into<String>,
    ) -> Result<Option<StorageLocationRecord>, KalamDbError> {
        let location_name = location_name.into();
        self.provider.get_location(&location_name)
    }

    /// List all storage locations
    pub fn list(&self) -> Result<Vec<StorageLocationRecord>, KalamDbError> {
        let batch = self.provider.scan_all_locations()?;

        // Extract locations from RecordBatch
        let mut locations = Vec::new();

        use datafusion::arrow::array::{Array, Int64Array, StringArray, TimestampMillisecondArray};

        let location_names = batch
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let location_types = batch
            .column(1)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let paths = batch
            .column(2)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let credentials_refs = batch
            .column(3)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let usage_counts = batch
            .column(4)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        let created_ats = batch
            .column(5)
            .as_any()
            .downcast_ref::<TimestampMillisecondArray>()
            .unwrap();
        let updated_ats = batch
            .column(6)
            .as_any()
            .downcast_ref::<TimestampMillisecondArray>()
            .unwrap();

        for i in 0..batch.num_rows() {
            locations.push(StorageLocationRecord {
                location_name: location_names.value(i).to_string(),
                location_type: location_types.value(i).to_string(),
                path: paths.value(i).to_string(),
                credentials_ref: if credentials_refs.is_null(i) {
                    None
                } else {
                    Some(credentials_refs.value(i).to_string())
                },
                usage_count: usage_counts.value(i) as u32,
                created_at: created_ats.value(i),
                updated_at: updated_ats.value(i),
            });
        }

        Ok(locations)
    }

    /// Resolve a location reference to its path template
    ///
    /// # Arguments
    /// * `location_name` - Location reference to resolve
    ///
    /// # Returns
    /// * `Ok(path)` - Resolved path template
    /// * `Err(_)` - Location not found
    pub fn resolve(&self, location_name: impl Into<String>) -> Result<String, KalamDbError> {
        let location_name = location_name.into();
        let location = self.provider.get_location(&location_name)?.ok_or_else(|| {
            KalamDbError::NotFound(format!("Storage location '{}' not found", location_name))
        })?;

        Ok(location.path)
    }

    /// Resolve a location reference and substitute user_id in the path
    ///
    /// # Arguments
    /// * `location_name` - Location reference to resolve
    /// * `user_id` - User ID to substitute in the path template
    ///
    /// # Returns
    /// * `Ok(path)` - Resolved and substituted path
    /// * `Err(_)` - Location not found
    pub fn resolve_for_user(
        &self,
        location_name: impl Into<String>,
        user_id: &UserId,
    ) -> Result<String, KalamDbError> {
        let path = self.resolve(location_name)?;
        Ok(path.replace("${user_id}", user_id.as_ref()))
    }

    /// Increment usage count for a location
    ///
    /// This should be called when a table is created using this location.
    pub fn increment_usage(&self, location_name: impl Into<String>) -> Result<(), KalamDbError> {
        let location_name = location_name.into();
        self.provider.increment_usage(&location_name)
    }

    /// Decrement usage count for a location
    ///
    /// This should be called when a table using this location is dropped.
    pub fn decrement_usage(&self, location_name: impl Into<String>) -> Result<(), KalamDbError> {
        let location_name = location_name.into();
        self.provider.decrement_usage(&location_name)
    }

    /// Validate path accessibility
    ///
    /// For Filesystem: Check if parent directory exists or can be created
    /// For S3: Basic format validation (TODO: actual S3 connectivity check)
    fn validate_path_accessibility(
        &self,
        path: &str,
        location_type: &LocationType,
    ) -> Result<(), KalamDbError> {
        match location_type {
            LocationType::Filesystem => {
                // Remove ${user_id} placeholder for validation
                let test_path = path.replace("${user_id}", "test_user");
                let path_obj = Path::new(&test_path);

                // Check if parent directory exists
                if let Some(parent) = path_obj.parent() {
                    if !parent.exists() {
                        return Err(KalamDbError::InvalidOperation(format!(
                            "Parent directory does not exist: {}",
                            parent.display()
                        )));
                    }
                }

                Ok(())
            }
            LocationType::S3 => {
                // Basic S3 path validation
                if !path.starts_with("s3://") {
                    return Err(KalamDbError::InvalidOperation(
                        "S3 path must start with 's3://'".to_string(),
                    ));
                }

                // TODO: Implement actual S3 connectivity check
                // For now, just validate the format

                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::RocksDbInit;
    use kalamdb_sql::KalamSql;
    use tempfile::TempDir;

    fn setup_test_service() -> (StorageLocationService, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let init = RocksDbInit::new(temp_dir.path().to_str().unwrap());
        let db = init.open().unwrap();
        let kalam_sql = Arc::new(KalamSql::new(db).unwrap());
        let provider = Arc::new(StorageLocationsTableProvider::new(kalam_sql));
        let service = StorageLocationService::new(provider);
        (service, temp_dir)
    }

    #[test]
    fn test_add_location() {
        let (service, temp_dir) = setup_test_service();

        service
            .add(
                "local_disk",
                LocationType::Filesystem,
                temp_dir.path().join("${user_id}").to_str().unwrap(),
                None,
                false, // Don't validate accessibility
            )
            .unwrap();

        let location = service.get("local_disk").unwrap().unwrap();
        assert_eq!(location.location_name, "local_disk");
    }

    #[test]
    fn test_add_duplicate_location() {
        let (service, temp_dir) = setup_test_service();

        let path = temp_dir
            .path()
            .join("${user_id}")
            .to_str()
            .unwrap()
            .to_string();
        service
            .add("local_disk", LocationType::Filesystem, &path, None, false)
            .unwrap();
        let result = service.add("local_disk", LocationType::Filesystem, path, None, false);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            KalamDbError::AlreadyExists(_)
        ));
    }

    #[test]
    fn test_update_location() {
        let (service, temp_dir) = setup_test_service();

        let path = temp_dir
            .path()
            .join("${user_id}")
            .to_str()
            .unwrap()
            .to_string();
        service
            .add("local_disk", LocationType::Filesystem, &path, None, false)
            .unwrap();

        let new_path = temp_dir
            .path()
            .join("new/${user_id}")
            .to_str()
            .unwrap()
            .to_string();
        service
            .update("local_disk", Some(new_path.clone()), None, false)
            .unwrap();

        let location = service.get("local_disk").unwrap().unwrap();
        assert_eq!(location.path, new_path);
    }

    #[test]
    fn test_delete_location() {
        let (service, temp_dir) = setup_test_service();

        let path = temp_dir
            .path()
            .join("${user_id}")
            .to_str()
            .unwrap()
            .to_string();
        service
            .add("local_disk", LocationType::Filesystem, path, None, false)
            .unwrap();
        service.delete("local_disk").unwrap();

        let location = service.get("local_disk").unwrap();
        assert!(location.is_none());
    }

    #[test]
    fn test_delete_location_with_usage() {
        let (service, temp_dir) = setup_test_service();

        let path = temp_dir
            .path()
            .join("${user_id}")
            .to_str()
            .unwrap()
            .to_string();
        service
            .add("local_disk", LocationType::Filesystem, path, None, false)
            .unwrap();
        service.increment_usage("local_disk").unwrap();

        let result = service.delete("local_disk");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            KalamDbError::InvalidOperation(_)
        ));
    }

    #[test]
    fn test_list_locations() {
        let (service, temp_dir) = setup_test_service();

        let path1 = temp_dir
            .path()
            .join("loc1/${user_id}")
            .to_str()
            .unwrap()
            .to_string();
        let path2 = temp_dir
            .path()
            .join("loc2/${user_id}")
            .to_str()
            .unwrap()
            .to_string();

        service
            .add("location1", LocationType::Filesystem, path1, None, false)
            .unwrap();
        service
            .add(
                "location2",
                LocationType::S3,
                path2,
                Some("creds".to_string()),
                false,
            )
            .unwrap();

        let locations = service.list().unwrap();
        assert_eq!(locations.len(), 2);
    }

    #[test]
    fn test_resolve_location() {
        let (service, temp_dir) = setup_test_service();

        let path = temp_dir
            .path()
            .join("${user_id}")
            .to_str()
            .unwrap()
            .to_string();
        service
            .add("local_disk", LocationType::Filesystem, &path, None, false)
            .unwrap();

        let resolved = service.resolve("local_disk").unwrap();
        assert_eq!(resolved, path);
    }

    #[test]
    fn test_resolve_for_user() {
        let (service, temp_dir) = setup_test_service();

        let path = temp_dir
            .path()
            .join("${user_id}/tables")
            .to_str()
            .unwrap()
            .to_string();
        service
            .add("local_disk", LocationType::Filesystem, &path, None, false)
            .unwrap();

        let user_id = UserId::new("user123".to_string());
        let resolved = service.resolve_for_user("local_disk", &user_id).unwrap();

        let expected = temp_dir
            .path()
            .join("user123/tables")
            .to_str()
            .unwrap()
            .to_string();
        assert_eq!(resolved, expected);
    }

    #[test]
    fn test_usage_count_tracking() {
        let (service, temp_dir) = setup_test_service();

        let path = temp_dir
            .path()
            .join("${user_id}")
            .to_str()
            .unwrap()
            .to_string();
        service
            .add("local_disk", LocationType::Filesystem, path, None, false)
            .unwrap();

        service.increment_usage("local_disk").unwrap();
        service.increment_usage("local_disk").unwrap();

        let location = service.get("local_disk").unwrap().unwrap();
        assert_eq!(location.usage_count, 2);

        service.decrement_usage("local_disk").unwrap();

        let location = service.get("local_disk").unwrap().unwrap();
        assert_eq!(location.usage_count, 1);
    }

    #[test]
    fn test_validate_s3_path() {
        let (service, _temp_dir) = setup_test_service();

        // Valid S3 path
        let result = service.add(
            "s3_location",
            LocationType::S3,
            "s3://bucket/path/${user_id}",
            None,
            true, // Validate accessibility
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_invalid_s3_path() {
        let (service, _temp_dir) = setup_test_service();

        // Invalid S3 path (missing s3:// prefix)
        let result = service.add(
            "s3_location",
            LocationType::S3,
            "bucket/path/${user_id}",
            None,
            true, // Validate accessibility
        );
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            KalamDbError::InvalidOperation(_)
        ));
    }
}
