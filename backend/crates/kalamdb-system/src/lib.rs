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
//! System tables use `SecuredSystemTableProvider` from `kalamdb-session` to
//! enforce permission checks at scan time. This provides defense-in-depth
//! security even for nested queries or subqueries.
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
pub mod impls;
pub mod providers;
pub mod registry;
pub mod services;
pub mod system_table_store;
pub mod system_table_trait;

// Re-export main types
pub use error::{Result, SystemError};
pub use initialization::initialize_system_tables;
pub use impls::{ClusterCoordinator, LiveQueryManager, ManifestService, SchemaRegistry};
pub use registry::SystemTablesRegistry;
pub use services::SystemColumnsService;
pub use system_table_trait::SystemTableProviderExt;
// Re-export SystemTable and StoragePartition from kalamdb_commons for consistent usage
pub use kalamdb_commons::{StoragePartition, SystemTable};

// Re-export session types for convenience
pub use kalamdb_session::{
    check_system_table_access, secure_provider, SecuredSystemTableProvider, SessionUserContext,
};

// Re-export all providers
pub use providers::{
    AuditLogsTableProvider, InMemoryChecker, JobNodesTableProvider, JobsTableProvider,
    LiveQueriesTableProvider, ManifestTableProvider, NamespacesTableProvider,
    StoragesTableProvider, TablesTableProvider, UsersTableProvider,
};
