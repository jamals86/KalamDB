//! Cluster management command handlers
//!
//! Handles cluster-level operations:
//! - CLUSTER SNAPSHOT: Force logs to snapshot
//! - CLUSTER PURGE: Purge logs up to index
//! - CLUSTER TRIGGER ELECTION: Trigger leader election
//! - CLUSTER TRANSFER-LEADER: Transfer leadership
//! - CLUSTER JOIN: Add a node at runtime
//! - CLUSTER REBALANCE: Best-effort data leader redistribution
//! - CLUSTER STEPDOWN: Attempt leader stepdown
//! - CLUSTER CLEAR: Clear old snapshots

pub mod clear;
pub mod join;
pub mod purge;
pub mod rebalance;
mod result_rows;
pub mod snapshot;
pub mod stepdown;
pub mod transfer_leader;
pub mod trigger_election;

pub use clear::ClusterClearHandler;
pub use join::ClusterJoinHandler;
pub use purge::ClusterPurgeHandler;
pub use rebalance::ClusterRebalanceHandler;
pub use snapshot::ClusterSnapshotHandler;
pub use stepdown::ClusterStepdownHandler;
pub use transfer_leader::ClusterTransferLeaderHandler;
pub use trigger_election::ClusterTriggerElectionHandler;
