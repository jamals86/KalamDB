//! Storage backend configuration types
//!
//! This module contains all storage-related types including:
//! - Storage type enum
//! - Storage mode
//! - Storage configuration
//! - Location-specific configs (S3, GCS, Azure, Local)
//! - Location config enum and errors

mod config;
mod location_azure;
mod location_config;
mod location_error;
mod location_gcs;
mod location_local;
mod location_s3;
mod mode;
mod type_;

// Re-export all types
pub use config::StorageConfig;
pub use location_azure::AzureStorageConfig;
pub use location_config::StorageLocationConfig;
pub use location_error::StorageLocationConfigError;
pub use location_gcs::GcsStorageConfig;
pub use location_local::LocalStorageConfig;
pub use location_s3::S3StorageConfig;
pub use mode::StorageMode;
pub use type_::StorageType;
