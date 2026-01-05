//! Raft Storage Layer
//!
//! This module provides the storage implementation for openraft,
//! using the existing StorageBackend abstraction.
//!
//! ## Components
//!
//! - [`KalamRaftStorage`]: Combined Raft storage implementing `RaftStorage` trait
//! - [`KalamTypeConfig`]: OpenRaft type configuration for KalamDB
//! - [`KalamNode`]: Node information for cluster membership

pub mod log_store;
mod raft_store;

pub use log_store::{KalamNode, KalamTypeConfig};
pub use raft_store::{KalamRaftStorage, KalamLogReader, KalamSnapshotBuilder, StoredSnapshot};
