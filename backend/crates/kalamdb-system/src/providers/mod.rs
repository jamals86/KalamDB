//! System table providers
//!
//! This module contains all system table provider implementations.
//! Each provider implements the DataFusion TableProvider trait.

pub mod audit_logs;
pub mod jobs;
pub mod live_queries;
pub mod manifest;
pub mod namespaces;
pub mod server_logs;
pub mod stats;
pub mod storages;
pub mod tables;
pub mod users;

// Re-export all providers
pub use audit_logs::AuditLogsTableProvider;
pub use jobs::JobsTableProvider;
pub use live_queries::LiveQueriesTableProvider;
pub use manifest::{InMemoryChecker, ManifestTableProvider};
pub use namespaces::NamespacesTableProvider;
pub use server_logs::ServerLogsTableProvider;
pub use stats::StatsTableProvider;
pub use storages::StoragesTableProvider;
pub use tables::TablesTableProvider;
pub use users::UsersTableProvider;
