//! Cluster configuration types
//!
//! Re-exports cluster configuration from kalamdb-commons for backwards compatibility.
//! All cluster config types are now defined in kalamdb-commons/src/config/cluster.rs
//! to be shared across crates and avoid duplication.
// TODO: No need for this file at all directly use kalamdb-commons crate instead.
pub use kalamdb_commons::config::{ClusterConfig, PeerConfig};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_config_fields() {
        let peer = PeerConfig {
            node_id: 2,
            rpc_addr: "127.0.0.1:9082".to_string(),
            api_addr: "http://127.0.0.1:8082".to_string(),
        };

        assert_eq!(peer.node_id, 2);
        assert_eq!(peer.rpc_addr, "127.0.0.1:9082");
        assert_eq!(peer.api_addr, "http://127.0.0.1:8082");
    }

    #[test]
    fn test_cluster_config_construction() {
        let config = ClusterConfig {
            cluster_id: "test-cluster".to_string(),
            node_id: 1,
            rpc_addr: "0.0.0.0:9081".to_string(),
            api_addr: "http://127.0.0.1:8081".to_string(),
            peers: vec![
                PeerConfig {
                    node_id: 2,
                    rpc_addr: "127.0.0.1:9082".to_string(),
                    api_addr: "http://127.0.0.1:8082".to_string(),
                },
            ],
            user_shards: 12,
            shared_shards: 1,
            heartbeat_interval_ms: 250,
            election_timeout_ms: (150, 300),
            max_snapshots_to_keep: 5,
            snapshot_policy: "NONE".to_string(),
            replication_timeout_ms: 5000,
        };

        assert_eq!(config.node_id, 1);
        assert_eq!(config.user_shards, 12);
        assert_eq!(config.peers.len(), 1);
    }
}
