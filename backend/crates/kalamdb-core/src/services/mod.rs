//! Services module for business logic
//!
//! This module contains service layers that orchestrate operations across
//! multiple components like storage, catalog, and configuration.
//!
//! **Phase 8 COMPLETE**: Removed all table management services
//! (NamespaceService, UserTableService, SharedTableService, StreamTableService, TableDeletionService, SchemaEvolutionService)
//! All business logic inlined into DDL handlers. Only backup/restore services remain.

pub mod backup_service;
pub mod restore_service;

pub use backup_service::{BackupManifest, BackupResult, BackupService, BackupStatistics};
pub use restore_service::{RestoreResult, RestoreService};
