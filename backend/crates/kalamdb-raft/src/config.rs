//! Cluster configuration types
//!
//! Configuration for Raft clustering, parsed from the `[cluster]` section
//! of server.toml.

use serde::{Deserialize, Serialize};

/// Complete cluster configuration
///
/// Parsed from the `[cluster]` section in server.toml.
/// If this section is absent, the server runs in standalone mode.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClusterConfig {
    /// Whether clustering is enabled
    pub enabled: bool,

    /// Unique identifier for this cluster
    pub cluster_id: String,

    /// This node's unique ID within the cluster (1, 2, 3, ...)
    pub node_id: u64,

    /// Raft-specific configuration
    pub raft: RaftConfig,

    /// Sharding configuration
    #[serde(default)]
    pub sharding: ShardingConfig,

    /// List of cluster members
    pub members: Vec<MemberConfig>,
}

/// Raft protocol configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RaftConfig {
    /// Address to bind Raft gRPC server to
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,

    /// Address to advertise to other nodes
    pub advertise_addr: String,

    /// Heartbeat interval in milliseconds
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval: u64,

    /// Minimum election timeout in milliseconds
    #[serde(default = "default_election_timeout_min")]
    pub election_timeout_min: u64,

    /// Maximum election timeout in milliseconds
    #[serde(default = "default_election_timeout_max")]
    pub election_timeout_max: u64,

    /// Number of log entries before taking a snapshot
    #[serde(default = "default_snapshot_threshold")]
    pub snapshot_threshold: u64,

    /// Number of entries to compact in a batch
    #[serde(default = "default_log_compaction_batch")]
    pub log_compaction_batch: u64,
}

/// Sharding configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ShardingConfig {
    /// Number of user data shards (default: 32)
    #[serde(default = "default_user_shards")]
    pub num_user_shards: u32,

    /// Number of shared data shards (default: 1)
    #[serde(default = "default_shared_shards")]
    pub num_shared_shards: u32,
}

impl Default for ShardingConfig {
    fn default() -> Self {
        Self {
            num_user_shards: default_user_shards(),
            num_shared_shards: default_shared_shards(),
        }
    }
}

/// Configuration for a single cluster member
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemberConfig {
    /// Node ID
    pub node_id: u64,

    /// Raft gRPC address
    pub raft_addr: String,

    /// HTTP API address
    pub api_addr: String,
}

// Default value functions for serde

fn default_bind_addr() -> String {
    "0.0.0.0:9090".to_string()
}

fn default_heartbeat_interval() -> u64 {
    100 // 100ms
}

fn default_election_timeout_min() -> u64 {
    300 // 300ms
}

fn default_election_timeout_max() -> u64 {
    500 // 500ms
}

fn default_snapshot_threshold() -> u64 {
    10_000 // 10k entries
}

fn default_log_compaction_batch() -> u64 {
    1_000
}

fn default_user_shards() -> u32 {
    32
}

fn default_shared_shards() -> u32 {
    1
}

impl Default for RaftConfig {
    fn default() -> Self {
        Self {
            bind_addr: default_bind_addr(),
            advertise_addr: "127.0.0.1:9090".to_string(),
            heartbeat_interval: default_heartbeat_interval(),
            election_timeout_min: default_election_timeout_min(),
            election_timeout_max: default_election_timeout_max(),
            snapshot_threshold: default_snapshot_threshold(),
            log_compaction_batch: default_log_compaction_batch(),
        }
    }
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

        if self.members.is_empty() {
            return Err("members list cannot be empty".to_string());
        }

        // Check this node is in the members list
        if !self.members.iter().any(|m| m.node_id == self.node_id) {
            return Err(format!(
                "node_id {} not found in members list",
                self.node_id
            ));
        }

        // Check election timeout > heartbeat
        if self.raft.election_timeout_min <= self.raft.heartbeat_interval {
            return Err(
                "election_timeout_min must be > heartbeat_interval".to_string()
            );
        }

        if self.raft.election_timeout_max <= self.raft.election_timeout_min {
            return Err(
                "election_timeout_max must be > election_timeout_min".to_string()
            );
        }

        // Check shard counts
        if self.sharding.num_user_shards == 0 {
            return Err("num_user_shards must be > 0".to_string());
        }

        if self.sharding.num_shared_shards == 0 {
            return Err("num_shared_shards must be > 0".to_string());
        }

        Ok(())
    }

    /// Get the total number of Raft groups
    pub fn total_groups(&self) -> usize {
        3 // metadata groups
            + self.sharding.num_user_shards as usize
            + self.sharding.num_shared_shards as usize
    }

    /// Find a member by node_id
    pub fn find_member(&self, node_id: u64) -> Option<&MemberConfig> {
        self.members.iter().find(|m| m.node_id == node_id)
    }

    /// Get this node's member configuration
    pub fn this_member(&self) -> Option<&MemberConfig> {
        self.find_member(self.node_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_config() -> ClusterConfig {
        ClusterConfig {
            enabled: true,
            cluster_id: "test-cluster".to_string(),
            node_id: 1,
            raft: RaftConfig::default(),
            sharding: ShardingConfig::default(),
            members: vec![
                MemberConfig {
                    node_id: 1,
                    raft_addr: "127.0.0.1:9090".to_string(),
                    api_addr: "http://127.0.0.1:8080".to_string(),
                },
                MemberConfig {
                    node_id: 2,
                    raft_addr: "127.0.0.2:9090".to_string(),
                    api_addr: "http://127.0.0.2:8080".to_string(),
                },
                MemberConfig {
                    node_id: 3,
                    raft_addr: "127.0.0.3:9090".to_string(),
                    api_addr: "http://127.0.0.3:8080".to_string(),
                },
            ],
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
    fn test_invalid_node_not_in_members() {
        let mut config = valid_config();
        config.node_id = 99;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_total_groups() {
        let config = valid_config();
        // 3 metadata + 32 user + 1 shared = 36
        assert_eq!(config.total_groups(), 36);
    }

    #[test]
    fn test_default_sharding() {
        let sharding = ShardingConfig::default();
        assert_eq!(sharding.num_user_shards, 32);
        assert_eq!(sharding.num_shared_shards, 1);
    }
}
