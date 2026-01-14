//! Cluster-based integration tests for KalamDB.
//!
//! These tests validate KalamDB's ability to maintain consistency across a 3-node cluster.
//! Each test verifies that operations on any node are properly replicated to other nodes.

#[path = "../common/testserver/mod.rs"]
#[allow(dead_code)]
mod test_support;

// pub mod test_cluster_basic_operations;
// pub mod test_cluster_node_recovery_and_sync;
