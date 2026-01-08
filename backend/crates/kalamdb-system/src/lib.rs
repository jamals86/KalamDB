//! # kalamdb-system
//!
//! System table providers and metadata management for KalamDB.
//!
//! This crate contains all system table implementations:
//! - UsersTableProvider: System users and authentication
//! - JobsTableProvider: Background job status and history
//! - NamespacesTableProvider: Database namespace catalog
//! - TablesTableProvider: Table metadata and schema registry
//! - StoragesTableProvider: Storage backend configuration
//! - LiveQueriesTableProvider: Active live query subscriptions
//! - AuditLogsTableProvider: System audit log entries
//! - StatsTableProvider: Database statistics and metrics
//!
//! ## Architecture
//!
//! System tables are read-only views over KalamDB's internal metadata stored
//! in RocksDB via EntityStore. They provide SQL-queryable access to:
//! - User management and permissions
//! - Job execution status and history
//! - Schema evolution and versioning
//! - Storage configuration
//! - Active subscriptions
//! - Audit trails
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use kalamdb_system::{SystemTablesRegistry, UsersTableProvider};
//!
//! // Register all system tables
//! let registry = SystemTablesRegistry::new(app_context);
//! registry.register_all(session_state)?;
//!
//! // Query system tables via SQL
//! // SELECT * FROM system.users WHERE role = 'dba';
//! ```

#[macro_use]
pub mod macros;

pub mod error;
pub mod initialization;
pub mod providers;
pub mod registry;
pub mod system_table_definitions;
pub mod system_table_store;
pub mod system_table_trait;

// Re-export main types
pub use error::{Result, SystemError};
pub use initialization::initialize_system_tables;
pub use registry::SystemTablesRegistry;
pub use system_table_trait::SystemTableProviderExt;

// Re-export all providers
pub use providers::{
    AuditLogsTableProvider, ClusterTableProvider, InMemoryChecker, JobsTableProvider, 
    LiveQueriesTableProvider, ManifestTableProvider, NamespacesTableProvider, ServerLogsTableProvider, 
    StatsTableProvider, StoragesTableProvider, TablesTableProvider, UsersTableProvider,
};
