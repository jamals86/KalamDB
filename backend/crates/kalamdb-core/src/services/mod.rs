//! Services module for business logic
//!
//! This module contains service layers that orchestrate operations across
//! multiple components like storage, catalog, and configuration.
//!
//! **Phase 8 COMPLETE**: Removed all table management services
//! (NamespaceService, UserTableService, SharedTableService, StreamTableService, TableDeletionService, SchemaEvolutionService)
//! All business logic inlined into DDL handlers.
//!
//! **Phase 10 - KalamSql Removal**: Backup/restore services temporarily disabled (need refactoring to remove KalamSql)

// TODO: Refactor these services to use SystemTablesRegistry providers instead of KalamSql
// pub mod backup_service;
// pub mod restore_service;
// pub use backup_service::{BackupManifest, BackupResult, BackupService, BackupStatistics};
// pub use restore_service::{RestoreResult, RestoreService};
