//! Cluster management command handlers
//!
//! Handles cluster-level operations:
//! - CLUSTER FLUSH: Force logs to snapshot
//! - CLUSTER CLEAR: Clear old snapshots
//! - CLUSTER LIST: List cluster nodes
//! - CLUSTER JOIN: Join an existing cluster (stub - not implemented)
//! - CLUSTER LEAVE: Leave the cluster (stub - not implemented)

pub mod flush;
pub mod clear;
pub mod list;
pub mod join;
pub mod leave;

pub use flush::ClusterFlushHandler;
pub use clear::ClusterClearHandler;
pub use list::ClusterListHandler;
pub use join::ClusterJoinHandler;
pub use leave::ClusterLeaveHandler;
