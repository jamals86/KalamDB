//! Cluster Command and View Tests
//!
//! Tests covering:
//! - CLUSTER commands over HTTP
//! - system.cluster and system.cluster_groups views
//! - Snapshot creation and reuse

#[path = "../../common/testserver/mod.rs"]
#[allow(dead_code)]
pub(super) mod test_support;

mod test_cluster_commands_http;
mod test_cluster_views_http;
mod test_cluster_snapshots_http;
