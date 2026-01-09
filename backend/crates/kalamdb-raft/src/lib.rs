//! KalamDB Raft Consensus Layer
//!
//! This crate provides Multi-Raft consensus for KalamDB, enabling multi-node
//! clustering with strong consistency guarantees.
//!
//! ## Architecture
//!
//! - **34 Raft Groups**: 1 unified metadata group + 32 user data shards + 1 shared data shard
//! - **CommandExecutor Pattern**: Generic abstraction eliminating if/else for cluster vs standalone
//! - **Leader-Only Jobs**: Background jobs (flush, compaction) run only on the leader
//! - **Meta→Data Watermarking**: Data commands carry `required_meta_index` for ordering
//!
//! ## Key Components
//!
//! - [`GroupId`]: Identifies which of the 34 Raft groups a command belongs to
//! - [`ShardRouter`]: Routes operations to the correct shard based on user_id
//! - [`MetaCommand`]: Unified metadata command (namespaces, tables, users, jobs)
//! - [`MetaStateMachine`]: Unified state machine for all metadata
//! - [`CommandExecutor`]: Generic trait for executing commands (standalone or cluster)
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
//! ctx.executor().execute_meta(MetaCommand::CreateTable { ... }).await?;
//! ```

pub mod applier;
pub mod cluster_types;
pub mod config;
pub mod error;
pub mod executor;
pub mod group_id;
pub mod commands;
pub mod state_machine;
pub mod storage;
pub mod network;
pub mod manager;

// Re-exports - Meta layer
pub use applier::{MetaApplier, NoOpMetaApplier};
pub use commands::{MetaCommand, MetaResponse};
pub use state_machine::MetaStateMachine;

// Re-exports - Data layer
pub use applier::{
    NoOpSharedDataApplier, NoOpUserDataApplier,
    SharedDataApplier, UserDataApplier,
};
pub use commands::{UserDataCommand, SharedDataCommand, DataResponse};
pub use state_machine::{UserDataStateMachine, SharedDataStateMachine};

// Re-exports - Core types
pub use cluster_types::{NodeRole, NodeStatus, ServerStateExt};
pub use config::{ClusterConfig as RaftClusterConfig, PeerConfig, ReplicationMode};
pub use error::{RaftError, Result};
pub use executor::{ClusterInfo, ClusterNodeInfo, CommandExecutor, DirectExecutor, RaftExecutor};
pub use group_id::{GroupId, ShardRouter};
pub use state_machine::{KalamStateMachine, StateMachineSnapshot, ApplyResult};
pub use storage::{KalamRaftStorage, KalamTypeConfig, KalamNode};
pub use network::{RaftNetwork, RaftNetworkFactory, RaftService, start_rpc_server};
pub use manager::{RaftManager, RaftGroup, RaftManagerConfig, PeerNode, DEFAULT_USER_DATA_SHARDS, DEFAULT_SHARED_DATA_SHARDS};

