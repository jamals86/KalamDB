//! Services module for business logic
//!
//! This module contains service layers that orchestrate operations across
//! multiple components like storage, catalog, and configuration.

pub mod backup_service;
pub mod namespace_service;
pub mod restore_service;
pub mod schema_evolution_service;
pub mod shared_table_service;
pub mod storage_location_service;
pub mod stream_table_service;
pub mod table_deletion_service;
pub mod user_table_service;

pub use backup_service::{BackupManifest, BackupResult, BackupService, BackupStatistics};
pub use namespace_service::NamespaceService;
pub use restore_service::{RestoreResult, RestoreService};
pub use schema_evolution_service::{SchemaEvolutionResult, SchemaEvolutionService};
pub use shared_table_service::SharedTableService;
pub use storage_location_service::StorageLocationService;
pub use stream_table_service::StreamTableService;
pub use table_deletion_service::{TableDeletionResult, TableDeletionService};
pub use user_table_service::UserTableService;
