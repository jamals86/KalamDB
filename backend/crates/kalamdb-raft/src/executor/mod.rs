//! CommandExecutor trait and implementations
//!
//! The CommandExecutor pattern provides a unified interface for executing
//! commands, eliminating if/else checks for cluster vs standalone mode.
//!
//! ```text
//! STANDALONE: Handler → DirectExecutor → Provider → RocksDB
//! CLUSTER:    Handler → RaftExecutor → Raft → StateMachine → Provider → RocksDB
//! ```

mod direct;
mod raft;
mod trait_def;

pub use direct::DirectExecutor;
pub use raft::RaftExecutor;
pub use trait_def::{ClusterInfo, ClusterNodeInfo, CommandExecutor};
