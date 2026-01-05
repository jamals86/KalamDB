//! Cluster nodes system table provider
//!
//! Provides a virtual view of the Raft cluster nodes with their status,
//! role (leader/follower), addresses, and other cluster metadata.

mod cluster_nodes_provider;

pub use cluster_nodes_provider::ClusterNodesTableProvider;
