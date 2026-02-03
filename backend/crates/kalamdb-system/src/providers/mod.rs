//! System table providers
//!
//! This module contains all system table provider implementations.
//! Each provider implements the DataFusion TableProvider trait.
//!
//! **Architecture**:
//! - `*TableSchema` structs: Contain the `TableDefinition` (source of truth)
//! - `*TableProvider` structs: Implement DataFusion's `TableProvider` trait
//! - Each `*TableSchema` memoizes its Arrow schema (OnceLock)
//! - `base` module: Common traits for unified scan logic

pub mod base;
pub mod audit_logs;
pub mod jobs;
pub mod job_nodes;
pub mod live_queries;
pub mod manifest;
pub mod namespaces;
pub mod storages;
pub mod tables;
pub mod topic_offsets;
pub mod topics;
pub mod users;

// Re-export base traits
pub use base::{SystemTableScan, SimpleSystemTableScan, extract_filter_value, extract_range_filters};

// Re-export all providers
pub use audit_logs::AuditLogsTableProvider;
pub use jobs::JobsTableProvider;
pub use job_nodes::JobNodesTableProvider;
pub use live_queries::LiveQueriesTableProvider;
pub use manifest::{InMemoryChecker, ManifestTableProvider};
pub use namespaces::NamespacesTableProvider;
pub use storages::StoragesTableProvider;
pub use tables::SchemasTableProvider;
pub use topic_offsets::TopicOffsetsTableProvider;
pub use topics::TopicsTableProvider;
pub use users::UsersTableProvider;

// Re-export all schema definitions (source of truth for TableDefinition)
pub use audit_logs::AuditLogsTableSchema;
pub use jobs::JobsTableSchema;
pub use job_nodes::JobNodesTableSchema;
pub use live_queries::LiveQueriesTableSchema;
pub use manifest::ManifestTableSchema;
pub use namespaces::NamespacesTableSchema;
pub use storages::StoragesTableSchema;
pub use tables::SchemasTableSchema;
pub use topic_offsets::TopicOffsetsTableSchema;
pub use topics::TopicsTableSchema;
pub use users::UsersTableSchema;
