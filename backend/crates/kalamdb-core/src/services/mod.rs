//! Services module for business logic
//!
//! This module contains service layers that orchestrate operations across
//! multiple components like storage, catalog, and configuration.

pub mod namespace_service;
pub mod storage_location_service;
pub mod table_deletion_service;
pub mod user_table_service;

pub use namespace_service::NamespaceService;
pub use storage_location_service::StorageLocationService;
pub use table_deletion_service::{TableDeletionResult, TableDeletionService};
pub use user_table_service::UserTableService;
