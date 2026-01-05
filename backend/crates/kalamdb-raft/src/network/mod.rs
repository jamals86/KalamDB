//! Raft Network Layer
//!
//! This module provides gRPC-based networking for Raft communication
//! between cluster nodes.
//!
//! ## Components
//!
//! - [`RaftNetwork`]: Network implementation for a single Raft group
//! - [`RaftNetworkFactory`]: Creates network instances for each group
//! - [`RaftService`]: gRPC service for handling incoming Raft RPCs

mod network;
mod service;

pub use network::{RaftNetwork, RaftNetworkFactory};
pub use service::RaftService;
