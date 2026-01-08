//! Raft Manager Configuration
//!
//! This module contains configuration types for the RaftManager,
//! including runtime configuration and peer node definitions.

use std::time::Duration;

use kalamdb_commons::models::NodeId;

use crate::config::ReplicationMode;

/// Default number of user data shards
pub const DEFAULT_USER_DATA_SHARDS: u32 = 32;

/// Default number of shared data shards
pub const DEFAULT_SHARED_DATA_SHARDS: u32 = 1;

/// Runtime configuration for the Raft Manager
///
/// This is the internal runtime config used by RaftManager, distinct from the
/// TOML-parseable `kalamdb_commons::config::ClusterConfig` which uses primitives
/// like `u64` for TOML compatibility. This struct uses types like `Duration`
/// for runtime convenience.
///
/// Construct this from `kalamdb_commons::config::ClusterConfig` using `From` trait.
#[derive(Debug, Clone)]
pub struct RaftManagerConfig {
    /// This node's ID (must be >= 1)
    pub node_id: NodeId,
    
    /// This node's RPC address for Raft communication
    pub rpc_addr: String,
    
    /// This node's API address for client requests
    pub api_addr: String,
    
    /// Peer nodes in the cluster
    pub peers: Vec<PeerNode>,
    
    /// Number of user data shards (default: 32)
    pub user_shards: u32,
    
    /// Number of shared data shards (default: 1)
    pub shared_shards: u32,
    
    /// Raft heartbeat interval in milliseconds
    pub heartbeat_interval_ms: u64,
    
    /// Raft election timeout range (min, max) in milliseconds
    pub election_timeout_ms: (u64, u64),

    /// Replication mode: Quorum (fast) or All (strong consistency)
    pub replication_mode: ReplicationMode,
    
    /// Timeout for waiting for all replicas (when replication_mode = All)
    pub replication_timeout: Duration,
}

impl Default for RaftManagerConfig {
    fn default() -> Self {
        Self {
            node_id: NodeId::new(1),
            rpc_addr: "127.0.0.1:9100".to_string(),
            api_addr: "127.0.0.1:8080".to_string(),
            peers: vec![],
            user_shards: DEFAULT_USER_DATA_SHARDS,
            shared_shards: DEFAULT_SHARED_DATA_SHARDS,
            heartbeat_interval_ms: 250,
            election_timeout_ms: (500, 1000),
            replication_mode: ReplicationMode::Quorum,
            replication_timeout: Duration::from_secs(5),
        }
    }
}

/// Convert from the TOML-parseable ClusterConfig to runtime RaftManagerConfig
impl From<kalamdb_commons::config::ClusterConfig> for RaftManagerConfig {
    fn from(config: kalamdb_commons::config::ClusterConfig) -> Self {
        let replication_mode = match config.replication_mode.as_str() {
            "all" => ReplicationMode::All,
            _ => ReplicationMode::Quorum,
        };
        
        Self {
            node_id: NodeId::new(config.node_id),
            rpc_addr: config.rpc_addr,
            api_addr: config.api_addr,
            peers: config.peers.into_iter().map(PeerNode::from).collect(),
            user_shards: config.user_shards,
            shared_shards: config.shared_shards,
            heartbeat_interval_ms: config.heartbeat_interval_ms,
            election_timeout_ms: config.election_timeout_ms,
            replication_mode,
            replication_timeout: Duration::from_millis(config.replication_timeout_ms),
        }
    }
}

/// Runtime configuration for a peer node
#[derive(Debug, Clone)]
pub struct PeerNode {
    /// Peer's node ID
    pub node_id: NodeId,
    
    /// Peer's RPC address for Raft communication
    pub rpc_addr: String,
    
    /// Peer's API address for client requests
    pub api_addr: String,
}

impl From<kalamdb_commons::config::PeerConfig> for PeerNode {
    fn from(peer: kalamdb_commons::config::PeerConfig) -> Self {
        Self {
            node_id: NodeId::new(peer.node_id),
            rpc_addr: peer.rpc_addr,
            api_addr: peer.api_addr,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RaftManagerConfig::default();
        
        assert_eq!(config.node_id, NodeId::new(1));
        assert_eq!(config.user_shards, DEFAULT_USER_DATA_SHARDS);
        assert_eq!(config.shared_shards, DEFAULT_SHARED_DATA_SHARDS);
        assert!(config.peers.is_empty());
    }

    #[test]
    fn test_peer_node_from() {
        let peer_config = kalamdb_commons::config::PeerConfig {
            node_id: 2,
            rpc_addr: "127.0.0.1:9101".to_string(),
            api_addr: "127.0.0.1:8081".to_string(),
        };
        
        let peer = PeerNode::from(peer_config);
        
        assert_eq!(peer.node_id, NodeId::new(2));
        assert_eq!(peer.rpc_addr, "127.0.0.1:9101");
        assert_eq!(peer.api_addr, "127.0.0.1:8081");
    }
}
