//! Storage configuration entity for system.storages table.

use crate::models::ids::StorageId;
use crate::models::storage::{StorageLocationConfig, StorageLocationConfigError, StorageType};
use serde::{Deserialize, Serialize};

/// Storage configuration in system_storages table
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Storage {
    pub storage_id: StorageId, // PK
    pub storage_name: String,
    pub description: Option<String>,
    pub storage_type: StorageType, // Enum: Filesystem, S3, Gcs, Azure
    pub base_directory: String,
    #[serde(default)]
    pub credentials: Option<String>,
    /// Storage backend parameters encoded as JSON.
    ///
    /// This is the canonical place for backend-specific configuration (S3/GCS/Azure/local).
    ///
    /// Example (S3):
    /// `{ "type": "s3", "region": "us-east-1", "endpoint": "https://s3.amazonaws.com" }`
    #[serde(default)]
    pub config_json: Option<String>,
    pub shared_tables_template: String,
    pub user_tables_template: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Storage {
    /// Decode `config_json` into a type-safe `StorageLocationConfig`.
    pub fn location_config(&self) -> Result<StorageLocationConfig, StorageLocationConfigError> {
        let raw = self
            .config_json
            .as_deref()
            .ok_or(StorageLocationConfigError::MissingConfigJson)?;

        serde_json::from_str::<StorageLocationConfig>(raw)
            .map_err(|e| StorageLocationConfigError::InvalidJson(e.to_string()))
    }
}

// KSerializable implementation for EntityStore support
impl crate::serialization::KSerializable for Storage {}
