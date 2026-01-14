//! KalamDB Raft Consensus Layer
//!
//! This crate provides Multi-Raft consensus for KalamDB, enabling multi-node
//! clustering with strong consistency guarantees.
//!
//! ## Architecture (Phase 20 - Unified Raft Executor)
//!
//! - **34 Raft Groups**: 1 unified metadata group + 32 user data shards + 1 shared data shard
//! - **Unified Code Path**: Both single-node and cluster modes use RaftExecutor
//! - **Leader-Only Jobs**: Background jobs (flush, compaction) run only on the leader
//! - **Meta→Data Watermarking**: Data commands carry `required_meta_index` for ordering
//!
//! ## Key Components
//!
//! - [`GroupId`]: Identifies which of the 34 Raft groups a command belongs to
//! - [`ShardRouter`]: Routes operations to the correct shard based on user_id
//! - [`MetaCommand`]: Unified metadata command (namespaces, tables, users, jobs)
//! - [`MetaStateMachine`]: Unified state machine for all metadata
//! - [`CommandExecutor`]: Generic trait for executing commands
//! - [`RaftExecutor`]: The only executor implementation (handles both single-node and cluster)
//!
//! ## Usage
//!
//! ```rust,ignore
//! // Single-node mode (no [cluster] config):
//! let config = RaftManagerConfig::for_single_node("127.0.0.1:8080".to_string());
//! let manager = RaftManager::new(config);
//! let executor = RaftExecutor::new(manager);
//!
//! // Cluster mode:
//! let config = RaftManagerConfig::from(cluster_config);
//! let manager = RaftManager::new(config);
//! let executor = RaftExecutor::new(manager);
//!
//! // Same interface in both modes:
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
pub use commands::{MetaCommand, MetaResponse, RaftCommand, RaftResponse};
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
pub use config::{ClusterConfig as RaftClusterConfig, PeerConfig};
pub use error::{RaftError, Result};
pub use executor::{ClusterInfo, ClusterNodeInfo, CommandExecutor, RaftExecutor};
pub use group_id::{GroupId, ShardRouter};
pub use state_machine::{KalamStateMachine, StateMachineSnapshot, ApplyResult, serde_helpers};
pub use storage::{KalamRaftStorage, KalamTypeConfig, KalamNode};
pub use network::{RaftNetwork, RaftNetworkFactory, RaftService, start_rpc_server};
pub use manager::{RaftManager, RaftGroup, RaftManagerConfig, PeerNode, SnapshotInfo, SnapshotsSummary, DEFAULT_USER_DATA_SHARDS, DEFAULT_SHARED_DATA_SHARDS};

