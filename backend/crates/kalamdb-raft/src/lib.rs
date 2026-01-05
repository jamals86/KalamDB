//! KalamDB Raft Consensus Layer
//!
//! This crate provides Multi-Raft consensus for KalamDB, enabling multi-node
//! clustering with strong consistency guarantees.
//!
//! ## Architecture
//!
//! - **36 Raft Groups**: 3 metadata groups + 32 user data shards + 1 shared data shard
//! - **CommandExecutor Pattern**: Generic abstraction eliminating if/else for cluster vs standalone
//! - **Leader-Only Jobs**: Background jobs (flush, compaction) run only on the leader
//!
//! ## Key Components
//!
//! - [`GroupId`]: Identifies which of the 36 Raft groups a command belongs to
//! - [`ShardRouter`]: Routes operations to the correct shard based on user_id
//! - [`CommandExecutor`]: Generic trait for executing commands (standalone or cluster)
//! - [`DirectExecutor`]: Standalone mode - direct provider calls, zero overhead
//! - [`RaftExecutor`]: Cluster mode - commands go through Raft consensus
//!
//! ## Usage
//!
//! ```rust,ignore
//! // In standalone mode (no [cluster] config):
//! let executor = DirectExecutor::new(providers);
//!
//! // In cluster mode:
//! let executor = RaftExecutor::new(raft_manager);
//!
//! // Handlers use the same interface:
//! ctx.executor().execute_system(SystemCommand::CreateTable { ... }).await?;
//! ```

pub mod applier;
pub mod config;
pub mod error;
pub mod executor;
pub mod group_id;
pub mod commands;
pub mod state_machine;
pub mod storage;
pub mod network;
pub mod manager;

// Re-exports
pub use applier::{SystemApplier, NoOpSystemApplier};
pub use config::{ClusterConfig as RaftClusterConfig, RaftConfig, ShardingConfig};
pub use error::{RaftError, Result};
pub use executor::{ClusterInfo, ClusterNodeInfo, CommandExecutor, DirectExecutor, RaftExecutor};
pub use group_id::{GroupId, ShardRouter};
pub use commands::{
    SystemCommand, SystemResponse,
    UsersCommand, UsersResponse,
    JobsCommand, JobsResponse,
    UserDataCommand, SharedDataCommand, DataResponse,
};
pub use state_machine::{
    KalamStateMachine, StateMachineSnapshot, ApplyResult,
    SystemStateMachine, UsersStateMachine, JobsStateMachine,
    UserDataStateMachine, SharedDataStateMachine,
};
pub use storage::{KalamRaftStorage, KalamTypeConfig, KalamNode};
pub use network::{RaftNetwork, RaftNetworkFactory, RaftService};
pub use manager::{RaftManager, RaftGroup, ClusterConfig, PeerConfig, DEFAULT_USER_DATA_SHARDS, DEFAULT_SHARED_DATA_SHARDS};
