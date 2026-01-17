//! OpenRaft Type Configuration
//!
//! This module defines the type configuration for KalamDB's Raft implementation.
//! These types are used throughout the Raft layer for node identification,
//! log entries, and snapshot data.

use std::io::Cursor;

use openraft::{Entry, RaftTypeConfig};
use serde::{Deserialize, Serialize};

/// Type configuration for KalamDB Raft
///
/// Defines the associated types used by OpenRaft:
/// - `D`: Log entry data (serialized commands)
/// - `R`: Response data (serialized responses)  
/// - `NodeId`: Unique identifier for nodes (u64)
/// - `Node`: Node metadata for cluster membership
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct KalamTypeConfig;

impl RaftTypeConfig for KalamTypeConfig {
    type D = Vec<u8>; // Log entry data (serialized command)
    type R = Vec<u8>; // Response data (serialized response)
    type NodeId = u64;
    type Node = KalamNode;
    type Entry = Entry<Self>;
    type SnapshotData = Cursor<Vec<u8>>;
    type AsyncRuntime = openraft::TokioRuntime;
    type Responder = openraft::impls::OneshotResponder<Self>;
}

/// Node information for cluster membership
///
/// Contains the network addresses needed to communicate with a node:
/// - `rpc_addr`: gRPC address for Raft consensus messages
/// - `api_addr`: HTTP address for client requests
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct KalamNode {
    /// gRPC address for Raft communication (e.g., "127.0.0.1:9100")
    pub rpc_addr: String,
    /// HTTP address for client requests (e.g., "127.0.0.1:8080")
    pub api_addr: String,
}

impl KalamNode {
    /// Create a new KalamNode with the given addresses
    pub fn new(rpc_addr: impl Into<String>, api_addr: impl Into<String>) -> Self {
        Self {
            rpc_addr: rpc_addr.into(),
            api_addr: api_addr.into(),
        }
    }
}

impl std::fmt::Display for KalamNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}|{}", self.rpc_addr, self.api_addr)
    }
}

impl std::error::Error for KalamNode {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kalam_node_display() {
        let node = KalamNode::new("127.0.0.1:9100", "127.0.0.1:8080");
        assert_eq!(format!("{}", node), "127.0.0.1:9100|127.0.0.1:8080");
    }

    #[test]
    fn test_kalam_node_default() {
        let node = KalamNode::default();
        assert_eq!(node.rpc_addr, "");
        assert_eq!(node.api_addr, "");
    }
}
