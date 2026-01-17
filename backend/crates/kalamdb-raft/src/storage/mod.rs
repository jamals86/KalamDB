//! Raft Storage Layer
//!
//! This module provides the storage implementation for openraft,
//! using in-memory storage with the `RaftStorage` v1 API.
//!
//! ## Components
//!
//! - [`KalamRaftStorage`]: Combined Raft storage implementing `RaftStorage` trait
//! - [`KalamTypeConfig`]: OpenRaft type configuration for KalamDB
//! - [`KalamNode`]: Node information for cluster membership

mod raft_store;
mod types;

pub use raft_store::{KalamLogReader, KalamRaftStorage, KalamSnapshotBuilder, StoredSnapshot};
pub use types::{KalamNode, KalamTypeConfig};
