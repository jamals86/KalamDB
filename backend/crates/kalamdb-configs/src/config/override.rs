use super::cluster::{ClusterConfig, PeerConfig};
use super::types::ServerConfig;
use std::env;
use std::path::Path;

impl ServerConfig {
    /// Apply environment variable overrides for sensitive configuration
    ///
    /// Supported environment variables (T030):
    /// - KALAMDB_SERVER_HOST: Override server.host
    /// - KALAMDB_SERVER_PORT: Override server.port
    /// - KALAMDB_LOG_LEVEL: Override logging.level
    /// - KALAMDB_LOGS_DIR: Override logging.logs_path
    /// - KALAMDB_LOG_FILE: Override logging.file_path (legacy, extracts parent dir)
    /// - KALAMDB_LOG_TO_CONSOLE: Override logging.log_to_console
    /// - KALAMDB_DATA_DIR: Override storage.data_path (base directory for rocksdb, storage, snapshots)
    /// - KALAMDB_ROCKSDB_PATH: Override storage.data_path (legacy, extracts parent dir)
    /// - KALAMDB_LOG_FILE_PATH: Override logging.file_path (legacy, prefer KALAMDB_LOG_FILE)
    /// - KALAMDB_HOST: Override server.host (legacy, prefer KALAMDB_SERVER_HOST)
    /// - KALAMDB_PORT: Override server.port (legacy, prefer KALAMDB_SERVER_PORT)
    /// - KALAMDB_CLUSTER_ID: Override cluster.cluster_id
    /// - KALAMDB_NODE_ID: Override cluster.node_id (alias: KALAMDB_CLUSTER_NODE_ID)
    /// - KALAMDB_CLUSTER_RPC_ADDR: Override cluster.rpc_addr
    /// - KALAMDB_CLUSTER_API_ADDR: Override cluster.api_addr
    /// - KALAMDB_CLUSTER_PEERS: Override cluster.peers
    ///
    /// Environment variables take precedence over server.toml values (T031)
    pub fn apply_env_overrides(&mut self) -> anyhow::Result<()> {
        // Server host (new naming convention)
        if let Ok(host) = env::var("KALAMDB_SERVER_HOST") {
            self.server.host = host;
        } else if let Ok(host) = env::var("KALAMDB_HOST") {
            // Legacy fallback
            self.server.host = host;
        }

        // Server port (new naming convention)
        if let Ok(port_str) = env::var("KALAMDB_SERVER_PORT") {
            self.server.port = port_str
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid KALAMDB_SERVER_PORT value: {}", port_str))?;
        } else if let Ok(port_str) = env::var("KALAMDB_PORT") {
            // Legacy fallback
            self.server.port = port_str
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid KALAMDB_PORT value: {}", port_str))?;
        }

        // Log level
        if let Ok(level) = env::var("KALAMDB_LOG_LEVEL") {
            self.logging.level = level;
        }

        // Logs directory path (new naming convention)
        if let Ok(path) = env::var("KALAMDB_LOGS_DIR") {
            self.logging.logs_path = path;
        } else if let Ok(path) = env::var("KALAMDB_LOG_FILE") {
            // Legacy fallback - extract directory from file path
            if let Some(parent) = Path::new(&path).parent() {
                self.logging.logs_path = parent.to_string_lossy().to_string();
            }
        } else if let Ok(path) = env::var("KALAMDB_LOG_FILE_PATH") {
            // Legacy fallback - extract directory from file path
            if let Some(parent) = Path::new(&path).parent() {
                self.logging.logs_path = parent.to_string_lossy().to_string();
            }
        }

        // Log to console
        if let Ok(val) = env::var("KALAMDB_LOG_TO_CONSOLE") {
            self.logging.log_to_console =
                val.to_lowercase() == "true" || val == "1" || val.to_lowercase() == "yes";
        }

        // Data directory (new naming convention)
        if let Ok(path) = env::var("KALAMDB_DATA_DIR") {
            self.storage.data_path = path;
        } else if let Ok(path) = env::var("KALAMDB_ROCKSDB_PATH") {
            // Legacy fallback - KALAMDB_ROCKSDB_PATH now sets the parent data dir
            if let Some(parent) = Path::new(&path).parent() {
                self.storage.data_path = parent.to_string_lossy().to_string();
            }
        }

        // Cluster overrides
        let cluster_id = env::var("KALAMDB_CLUSTER_ID").ok();
        let node_id = env::var("KALAMDB_NODE_ID")
            .ok()
            .or_else(|| env::var("KALAMDB_CLUSTER_NODE_ID").ok());
        let rpc_addr = env::var("KALAMDB_CLUSTER_RPC_ADDR").ok();
        let api_addr = env::var("KALAMDB_CLUSTER_API_ADDR").ok();
        let peers_env = env::var("KALAMDB_CLUSTER_PEERS").ok();

        let has_cluster_env = cluster_id.is_some()
            || node_id.is_some()
            || rpc_addr.is_some()
            || api_addr.is_some()
            || peers_env.is_some();

        if has_cluster_env {
            let parsed_node_id = match node_id {
                Some(ref val) => val
                    .parse::<u64>()
                    .map_err(|_| anyhow::anyhow!("Invalid KALAMDB_NODE_ID value: {}", val))?,
                None => self.cluster.as_ref().map(|c| c.node_id).ok_or_else(|| {
                    anyhow::anyhow!("KALAMDB_NODE_ID is required when overriding cluster settings")
                })?,
            };

            let cluster = self.cluster.get_or_insert_with(|| ClusterConfig {
                cluster_id: cluster_id.clone().unwrap_or_else(|| "cluster".to_string()),
                node_id: parsed_node_id,
                rpc_addr: rpc_addr
                    .clone()
                    .unwrap_or_else(|| "0.0.0.0:9100".to_string()),
                api_addr: api_addr
                    .clone()
                    .unwrap_or_else(|| "0.0.0.0:8080".to_string()),
                peers: Vec::new(),
                user_shards: 12,
                shared_shards: 1,
                heartbeat_interval_ms: 250,
                election_timeout_ms: (500, 1000),
                snapshot_policy: "LogsSinceLast(1000)".to_string(),
                max_snapshots_to_keep: 3,
                replication_timeout_ms: 5000,
                reconnect_interval_ms: 3000,
            });

            if let Some(val) = cluster_id {
                cluster.cluster_id = val;
            }

            cluster.node_id = parsed_node_id;

            if let Some(val) = rpc_addr {
                cluster.rpc_addr = val;
            }

            if let Some(val) = api_addr {
                cluster.api_addr = val;
            }

            if let Some(val) = peers_env {
                cluster.peers = parse_cluster_peers(&val)?;
            }
        }

        Ok(())
    }
}

fn parse_cluster_peers(value: &str) -> anyhow::Result<Vec<PeerConfig>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let mut peers = Vec::new();
    for entry in trimmed.split(';') {
        let parts: Vec<&str> = entry.split('@').collect();
        if parts.len() != 3 {
            return Err(anyhow::anyhow!(
                "Invalid KALAMDB_CLUSTER_PEERS entry '{}'. Expected format: node_id@rpc_addr@api_addr",
                entry
            ));
        }

        let node_id = parts[0]
            .trim()
            .parse::<u64>()
            .map_err(|_| anyhow::anyhow!("Invalid peer node_id in '{}': {}", entry, parts[0]))?;

        peers.push(PeerConfig {
            node_id,
            rpc_addr: parts[1].trim().to_string(),
            api_addr: parts[2].trim().to_string(),
        });
    }

    Ok(peers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    static ENV_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

    fn acquire_env_lock() -> MutexGuard<'static, ()> {
        ENV_MUTEX.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    #[test]
    fn test_env_override_server_host() {
        let _guard = acquire_env_lock();
        env::set_var("KALAMDB_SERVER_HOST", "0.0.0.0");

        let mut config = ServerConfig::default();
        config.apply_env_overrides().unwrap();

        assert_eq!(config.server.host, "0.0.0.0");

        env::remove_var("KALAMDB_SERVER_HOST");
    }

    #[test]
    fn test_env_override_server_port() {
        let _guard = acquire_env_lock();
        env::set_var("KALAMDB_SERVER_PORT", "9090");

        let mut config = ServerConfig::default();
        config.apply_env_overrides().unwrap();

        assert_eq!(config.server.port, 9090);

        env::remove_var("KALAMDB_SERVER_PORT");
    }

    #[test]
    fn test_env_override_log_level() {
        let _guard = acquire_env_lock();
        env::set_var("KALAMDB_LOG_LEVEL", "debug");

        let mut config = ServerConfig::default();
        config.apply_env_overrides().unwrap();

        assert_eq!(config.logging.level, "debug");

        env::remove_var("KALAMDB_LOG_LEVEL");
    }
}
