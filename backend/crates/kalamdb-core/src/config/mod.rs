//! Configuration module for JSON file persistence
//!
//! This module manages configuration files including namespaces and storage locations.

pub mod file_manager;
pub mod namespaces_config;
pub mod startup_loader;
pub mod storage_locations_config;

pub use file_manager::FileManager;
pub use namespaces_config::NamespacesConfig;
pub use startup_loader::{StartupConfig, StartupLoader};
pub use storage_locations_config::StorageLocationsConfig;
