//! Storage models for system.storages.

// Core storage table model
mod storage;
mod storage_type;

// Storage mode
mod storage_mode;

// Location-specific configurations
mod location_azure;
mod location_config;
mod location_error;
mod location_gcs;
mod location_local;
mod location_s3;

// Re-export all types
pub use location_azure::AzureStorageConfig;
pub use location_config::StorageLocationConfig;
pub use location_error::StorageLocationConfigError;
pub use location_gcs::GcsStorageConfig;
pub use location_local::LocalStorageConfig;
pub use location_s3::S3StorageConfig;
pub use storage::Storage;
pub use storage_mode::StorageMode;
pub use storage_type::StorageType;
