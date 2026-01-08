//! Raft Manager
//!
//! This module provides the central orchestration for all Raft groups.
//!
//! ## Components
//!
//! - [`RaftManager`]: Manages all Raft group instances
//! - [`RaftGroup`]: A single Raft consensus group
//! - [`RaftManagerConfig`]: Configuration for the Raft manager
//! - [`PeerNode`]: Configuration for a peer node

mod config;
mod raft_manager;
mod raft_group;

pub use config::{RaftManagerConfig, PeerNode, DEFAULT_USER_DATA_SHARDS, DEFAULT_SHARED_DATA_SHARDS};
pub use raft_manager::RaftManager;
pub use raft_group::RaftGroup;
