//! Command Executor implementations for kalamdb-core
//!
//! This module provides the concrete executor implementations that are wired
//! to the actual providers in kalamdb-core.
//!
//! - [`StandaloneExecutor`]: Wired DirectExecutor that calls providers directly
//! - Future: `ClusterExecutor` that routes through Raft

mod standalone;

pub use standalone::StandaloneExecutor;

// Re-export the trait from kalamdb-raft for convenience
pub use kalamdb_raft::{CommandExecutor, GroupId, ShardRouter};
