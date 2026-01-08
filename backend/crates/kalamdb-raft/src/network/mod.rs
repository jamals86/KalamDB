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
//! - [`start_rpc_server`]: Starts the gRPC server for incoming Raft RPCs
//! - [`ClientProposalRequest`], [`ClientProposalResponse`]: Types for leader forwarding

mod network;
pub mod service;

pub use network::{RaftNetwork, RaftNetworkFactory};
pub use service::{RaftService, start_rpc_server, ClientProposalRequest, ClientProposalResponse};
pub use service::raft_client::RaftClient;
