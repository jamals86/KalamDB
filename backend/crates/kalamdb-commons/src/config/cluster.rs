//! Cluster configuration types
//!
//! Configuration for Raft clustering, parsed from the `[cluster]` section
//! of server.toml. These types are shared across kalamdb-raft, kalamdb-core,
//! and other crates that need cluster configuration.
//!
//! The cluster section in server.toml has a FLAT structure (no nesting),
//! so these types reflect that flat structure for proper TOML deserialization.

use serde::{Deserialize, Serialize};

/// Complete cluster configuration (FLAT structure matching server.toml)
///
/// Parsed from the `[cluster]` section in server.toml.
/// If this section is absent, the server runs in standalone mode.
///
/// Example server.toml:
/// ```toml
/// [cluster]
/// cluster_id = "kalamdb-cluster"
/// node_id = 1
/// rpc_addr = "0.0.0.0:9100"
/// api_addr = "0.0.0.0:8080"
/// user_shards = 12
/// shared_shards = 1
/// heartbeat_interval_ms = 50
/// election_timeout_ms = [150, 300]
/// snapshot_threshold = 10000
/// replication_mode = "quorum"  # or "all"
/// replication_timeout_ms = 5000
///
/// [[cluster.peers]]
/// node_id = 2
/// rpc_addr = "10.0.0.2:9100"
/// api_addr = "http://10.0.0.2:8080"
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClusterConfig {
    /// Unique identifier for this cluster (used for Raft group prefixes)
    #[serde(default = "default_cluster_id")]
    pub cluster_id: String,
    
    /// This node's unique ID within the cluster (must be >= 1)
    /// This is the authoritative node ID for the server.
    pub node_id: u64,
    
    /// RPC address for Raft inter-node communication (e.g., "0.0.0.0:9100")
    #[serde(default = "default_rpc_addr")]
    pub rpc_addr: String,
    
    /// API address for client HTTP requests (e.g., "0.0.0.0:8080")
    /// This should match the server.host:server.port
    #[serde(default = "default_api_addr")]
    pub api_addr: String,
    
    /// List of peer nodes in the cluster
    /// Each peer should have node_id, rpc_addr, and api_addr
    #[serde(default)]
    pub peers: Vec<PeerConfig>,
    
    /// Number of user data shards (default: 12)
    /// Each shard is a separate Raft group for user table data.
    #[serde(default = "default_user_shards")]
    pub user_shards: u32,
    
    /// Number of shared data shards (default: 1)
    /// Each shard is a separate Raft group for shared table data.
    #[serde(default = "default_shared_shards")]
    pub shared_shards: u32,
    
    /// Raft heartbeat interval in milliseconds (default: 50)
    #[serde(default = "default_heartbeat_interval_ms")]
    pub heartbeat_interval_ms: u64,
    
    /// Raft election timeout range (min, max) in milliseconds (default: 150-300)
    /// Election timeout is randomly chosen from this range.
    #[serde(default = "default_election_timeout_ms")]
    pub election_timeout_ms: (u64, u64),
    
    /// Maximum entries per Raft snapshot (default: 10000)
    #[serde(default = "default_snapshot_threshold")]
    pub snapshot_threshold: u64,
    
    /// Replication mode: "quorum" (fast, default) or "all" (strong consistency)
    /// - quorum: ACK after majority commits (may have brief inconsistency on followers)
    /// - all: ACK only after ALL nodes commit (guarantees no stale reads anywhere)
    #[serde(default = "default_replication_mode")]
    pub replication_mode: String,
    
    /// Timeout in milliseconds to wait for all replicas when replication_mode = "all"
    #[serde(default = "default_replication_timeout_ms")]
    pub replication_timeout_ms: u64,
}

/// Configuration for a peer node in the cluster
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerConfig {
    /// Peer's unique node ID (must be >= 1)
    pub node_id: u64,
    /// Peer's RPC address for Raft communication (e.g., "10.0.0.2:9100")
    pub rpc_addr: String,
    /// Peer's API address for client requests (e.g., "10.0.0.2:8080")
    pub api_addr: String,
}

/// Replication mode for command acknowledgment
///
/// Controls when the leader acknowledges a write to the client:
/// - `Quorum`: ACK after majority of nodes commit (fast, default Raft behavior)
/// - `All`: ACK only after ALL nodes have committed (strong consistency, slower)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReplicationMode {
    /// Standard Raft quorum (majority) - fast but may have stale reads on followers
    #[default]
    Quorum,
    /// Wait for ALL nodes to commit - slower but guarantees no stale state
    All,
}

impl std::fmt::Display for ReplicationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplicationMode::Quorum => write!(f, "quorum"),
            ReplicationMode::All => write!(f, "all"),
        }
    }
}

// Default value functions for serde

fn default_cluster_id() -> String {
    "kalamdb-cluster".to_string()
}

fn default_rpc_addr() -> String {
    "0.0.0.0:9100".to_string()
}

fn default_api_addr() -> String {
    "0.0.0.0:8080".to_string()
}

fn default_user_shards() -> u32 {
    12
}

fn default_shared_shards() -> u32 {
    1
}

fn default_heartbeat_interval_ms() -> u64 {
    250
}

fn default_election_timeout_ms() -> (u64, u64) {
    (500, 1000)
}

fn default_snapshot_threshold() -> u64 {
    10000
}

fn default_replication_mode() -> String {
    "quorum".to_string()
}

fn default_replication_timeout_ms() -> u64 {
    5000 // 5 seconds
}

impl ClusterConfig {
    /// Check if this configuration is valid
    pub fn validate(&self) -> Result<(), String> {
        if self.cluster_id.is_empty() {
            return Err("cluster_id cannot be empty".to_string());
        }

        if self.node_id == 0 {
            return Err("node_id must be > 0".to_string());
        }

        // Check election timeout > heartbeat
        if self.election_timeout_ms.0 <= self.heartbeat_interval_ms {
            return Err(
                "election_timeout_min must be > heartbeat_interval".to_string()
            );
        }

        if self.election_timeout_ms.1 <= self.election_timeout_ms.0 {
            return Err(
                "election_timeout_max must be > election_timeout_min".to_string()
            );
        }

        // Check shard counts
        if self.user_shards == 0 {
            return Err("user_shards must be > 0".to_string());
        }

        if self.shared_shards == 0 {
            return Err("shared_shards must be > 0".to_string());
        }

        Ok(())
    }

    /// Get the total number of Raft groups
    pub fn total_groups(&self) -> usize {
        3 // metadata groups (system, users, jobs)
            + self.user_shards as usize
            + self.shared_shards as usize
    }

    /// Find a peer by node_id
    pub fn find_peer(&self, node_id: u64) -> Option<&PeerConfig> {
        self.peers.iter().find(|p| p.node_id == node_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_config() -> ClusterConfig {
        ClusterConfig {
            cluster_id: "test-cluster".to_string(),
            node_id: 1,
            rpc_addr: "127.0.0.1:9100".to_string(),
            api_addr: "127.0.0.1:8080".to_string(),
            peers: vec![
                PeerConfig {
                    node_id: 2,
                    rpc_addr: "127.0.0.2:9100".to_string(),
                    api_addr: "127.0.0.2:8080".to_string(),
                },
                PeerConfig {
                    node_id: 3,
                    rpc_addr: "127.0.0.3:9100".to_string(),
                    api_addr: "127.0.0.3:8080".to_string(),
                },
            ],
            user_shards: 32,
            shared_shards: 1,
            heartbeat_interval_ms: 50,
            election_timeout_ms: (150, 300),
            snapshot_threshold: 10000,
            replication_mode: "quorum".to_string(),
            replication_timeout_ms: 5000,
        }
    }

    #[test]
    fn test_valid_config() {
        let config = valid_config();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_invalid_empty_cluster_id() {
        let mut config = valid_config();
        config.cluster_id = "".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_invalid_node_id_zero() {
        let mut config = valid_config();
        config.node_id = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_total_groups() {
        let config = valid_config();
        // 3 metadata + 32 user + 1 shared = 36
        assert_eq!(config.total_groups(), 36);
    }
}

