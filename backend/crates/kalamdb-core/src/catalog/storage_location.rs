//! Storage location entity
//!
//! Defines storage locations for table data with support for filesystem and S3.

use serde::{Deserialize, Serialize};

/// Storage location type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocationType {
    /// Local filesystem storage
    Filesystem,
    /// Amazon S3 storage
    S3,
}

/// Storage location entity
///
/// Defines a predefined storage location that can be referenced by tables.
///
/// # Examples
///
/// ```
/// use kalamdb_core::catalog::{StorageLocation, LocationType};
///
/// let location = StorageLocation {
///     location_name: "local_disk".to_string(),
///     location_type: LocationType::Filesystem,
///     path: "/data/${user_id}/tables".to_string(),
///     credentials_ref: None,
///     usage_count: 0,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageLocation {
    /// Unique location identifier
    pub location_name: String,

    /// Type of storage (Filesystem or S3)
    pub location_type: LocationType,

    /// Path template (may include ${user_id} placeholder)
    pub path: String,

    /// Reference to credentials (for S3)
    pub credentials_ref: Option<String>,

    /// Number of tables using this location
    #[serde(default)]
    pub usage_count: u32,
}

impl StorageLocation {
    /// Create a new storage location
    pub fn new(
        location_name: impl Into<String>,
        location_type: LocationType,
        path: impl Into<String>,
    ) -> Self {
        Self {
            location_name: location_name.into(),
            location_type,
            path: path.into(),
            credentials_ref: None,
            usage_count: 0,
        }
    }

    /// Validate location name format
    ///
    /// Name must match regex: ^[a-z][a-z0-9_]*$
    pub fn validate_name(name: &str) -> Result<(), String> {
        if !name.chars().next().is_some_and(|c| c.is_ascii_lowercase()) {
            return Err("Location name must start with a lowercase letter".to_string());
        }

        if !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(
                "Location name can only contain lowercase letters, digits, and underscores"
                    .to_string(),
            );
        }

        Ok(())
    }

    /// Check if this location can be deleted (not in use)
    pub fn can_delete(&self) -> bool {
        self.usage_count == 0
    }

    /// Increment the usage count
    pub fn increment_usage(&mut self) {
        self.usage_count += 1;
    }

    /// Decrement the usage count
    pub fn decrement_usage(&mut self) {
        if self.usage_count > 0 {
            self.usage_count -= 1;
        }
    }

    /// Substitute ${user_id} in the path template
    pub fn substitute_path(&self, user_id: &str) -> String {
        self.path.replace("${user_id}", user_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_location_creation() {
        let location = StorageLocation::new("local_disk", LocationType::Filesystem, "/data");
        assert_eq!(location.location_name, "local_disk");
        assert_eq!(location.location_type, LocationType::Filesystem);
        assert_eq!(location.path, "/data");
        assert_eq!(location.usage_count, 0);
        assert!(location.credentials_ref.is_none());
    }

    #[test]
    fn test_validate_name_valid() {
        assert!(StorageLocation::validate_name("local_disk").is_ok());
        assert!(StorageLocation::validate_name("s3_bucket").is_ok());
        assert!(StorageLocation::validate_name("storage123").is_ok());
    }

    #[test]
    fn test_validate_name_invalid() {
        assert!(StorageLocation::validate_name("Local").is_err());
        assert!(StorageLocation::validate_name("123storage").is_err());
        assert!(StorageLocation::validate_name("storage-name").is_err());
    }

    #[test]
    fn test_can_delete() {
        let mut location = StorageLocation::new("test", LocationType::Filesystem, "/data");
        assert!(location.can_delete());

        location.increment_usage();
        assert!(!location.can_delete());

        location.decrement_usage();
        assert!(location.can_delete());
    }

    #[test]
    fn test_usage_count_operations() {
        let mut location = StorageLocation::new("test", LocationType::Filesystem, "/data");
        assert_eq!(location.usage_count, 0);

        location.increment_usage();
        assert_eq!(location.usage_count, 1);

        location.increment_usage();
        assert_eq!(location.usage_count, 2);

        location.decrement_usage();
        assert_eq!(location.usage_count, 1);

        location.decrement_usage();
        assert_eq!(location.usage_count, 0);

        // Should not go below 0
        location.decrement_usage();
        assert_eq!(location.usage_count, 0);
    }

    #[test]
    fn test_substitute_path() {
        let location = StorageLocation::new(
            "user_storage",
            LocationType::Filesystem,
            "/data/${user_id}/tables",
        );

        assert_eq!(location.substitute_path("user123"), "/data/user123/tables");
    }

    #[test]
    fn test_substitute_path_no_placeholder() {
        let location =
            StorageLocation::new("shared_storage", LocationType::Filesystem, "/data/shared");

        assert_eq!(location.substitute_path("user123"), "/data/shared");
    }
}
