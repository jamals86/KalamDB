//! Cluster table provider module
//!
//! Provides the `system.cluster` virtual table showing OpenRaft cluster status.

mod cluster_provider;

pub use cluster_provider::ClusterTableProvider;
