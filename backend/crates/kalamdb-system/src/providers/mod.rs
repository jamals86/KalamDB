//! System table providers
//!
//! This module contains all system table provider implementations.
//! Each provider implements the DataFusion TableProvider trait.

pub mod audit_logs;
pub mod jobs;
pub mod live_queries;
pub mod manifest;
pub mod namespaces;
pub mod storages;
pub mod stats;
pub mod tables;
pub mod users;

// Re-export all providers
pub use audit_logs::AuditLogsTableProvider;
pub use jobs::JobsTableProvider;
pub use live_queries::LiveQueriesTableProvider;
pub use manifest::ManifestTableProvider;
pub use namespaces::NamespacesTableProvider;
pub use storages::StoragesTableProvider;
pub use stats::StatsTableProvider;
pub use tables::TablesTableProvider;
pub use users::UsersTableProvider;
