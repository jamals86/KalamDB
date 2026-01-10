//! CommandExecutor trait and implementations
//!
//! The CommandExecutor pattern provides a unified interface for executing
//! commands through Raft consensus (Phase 20 - Unified Raft Executor).
//!
//! ```text
//! SINGLE-NODE: Handler → RaftExecutor → Single-node Raft → StateMachine → Provider → RocksDB
//! CLUSTER:     Handler → RaftExecutor → Multi-node Raft → StateMachine → Provider → RocksDB
//! ```
//!
//! Both modes use the same RaftExecutor, ensuring consistent behavior and testing.

mod raft;
mod trait_def;

pub use raft::RaftExecutor;
pub use trait_def::{ClusterInfo, ClusterNodeInfo, CommandExecutor};
