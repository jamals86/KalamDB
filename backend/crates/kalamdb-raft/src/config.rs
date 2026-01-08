//! Cluster configuration types
//!
//! Re-exports cluster configuration from kalamdb-commons for backwards compatibility.
//! All cluster config types are now defined in kalamdb-commons/src/config/cluster.rs
//! to be shared across crates and avoid duplication.

pub use kalamdb_commons::config::{ClusterConfig, PeerConfig, ReplicationMode};

