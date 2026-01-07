//! Cluster-specific tests for KalamDB
//!
//! These tests are designed to be run against a multi-node cluster
//! and can be executed separately from the main smoke tests.
//!
//! To run cluster tests only:
//!   cargo test --test cluster
//!
//! Environment variables:
//!   KALAMDB_CLUSTER_URLS - Comma-separated list of cluster node URLs
//!     Default: http://127.0.0.1:8081,http://127.0.0.1:8082,http://127.0.0.1:8083
//!
//!   KALAMDB_ROOT_PASSWORD - Root password for authentication
//!     Required for authenticated cluster access

mod common;

/// Cluster-specific common utilities
mod cluster_common {
    use crate::common::*;
    use kalam_link::{AuthProvider, KalamLinkClient, KalamLinkTimeouts, QueryResponse};
    use std::sync::OnceLock;
    use std::time::Duration;

    /// Get cluster node URLs from environment or use defaults
    pub fn cluster_urls() -> Vec<String> {
        let default_urls = "http://127.0.0.1:8081,http://127.0.0.1:8082,http://127.0.0.1:8083";
        std::env::var("KALAMDB_CLUSTER_URLS")
            .unwrap_or_else(|_| default_urls.to_string())
            .split(',')
            .map(|url| url.trim().to_string())
            .filter(|url| !url.is_empty())
            .collect()
    }

    /// Shared tokio runtime for cluster tests
    pub fn cluster_runtime() -> &'static tokio::runtime::Runtime {
        static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        RUNTIME.get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(4)
                .enable_all()
                .build()
                .expect("Failed to create cluster test runtime")
        })
    }

    /// Create a client connected to a specific cluster node
    pub fn create_cluster_client(base_url: &str) -> KalamLinkClient {
        let password = root_password().to_string();
        KalamLinkClient::builder()
            .base_url(base_url)
            .auth(AuthProvider::basic_auth("root".to_string(), password))
            .timeouts(
                KalamLinkTimeouts::builder()
                    .connection_timeout_secs(5)
                    .receive_timeout_secs(30)
                    .send_timeout_secs(10)
                    .subscribe_timeout_secs(10)
                    .auth_timeout_secs(10)
                    .initial_data_timeout(Duration::from_secs(30))
                    .build(),
            )
            .build()
            .expect("Failed to build cluster client")
    }

    /// Execute a query on a specific cluster node and return the count
    pub fn query_count_on_url(base_url: &str, sql: &str) -> i64 {
        let client = create_cluster_client(base_url);
        let sql = sql.to_string();

        cluster_runtime()
            .block_on(async move { client.execute_query(&sql, None, None).await })
            .map(|response| {
                let result = response
                    .results
                    .first()
                    .expect("Missing query result for count");
                let rows = result
                    .rows
                    .as_ref()
                    .and_then(|rows| rows.first())
                    .expect("Missing count row");
                let value = rows.first().expect("Missing count column");
                let unwrapped = extract_typed_value(value);
                match unwrapped {
                    serde_json::Value::String(s) => s.parse::<i64>().expect("Invalid count string"),
                    serde_json::Value::Number(n) => n.as_i64().expect("Invalid count number"),
                    other => panic!("Unexpected count value: {}", other),
                }
            })
            .expect("Cluster count query failed")
    }

    /// Execute SQL on a specific cluster node
    pub fn execute_on_node(base_url: &str, sql: &str) -> Result<String, String> {
        let client = create_cluster_client(base_url);
        let sql = sql.to_string();

        cluster_runtime()
            .block_on(async move { client.execute_query(&sql, None, None).await })
            .map(|response| {
                serde_json::to_string_pretty(&response)
                    .unwrap_or_else(|_| format!("{:?}", response))
            })
            .map_err(|e| e.to_string())
    }

    /// Execute SQL on a specific cluster node and return the structured response
    pub fn execute_on_node_response(base_url: &str, sql: &str) -> Result<QueryResponse, String> {
        let client = create_cluster_client(base_url);
        let sql = sql.to_string();

        cluster_runtime()
            .block_on(async move { client.execute_query(&sql, None, None).await })
            .map_err(|e| e.to_string())
    }

    /// Check if a cluster node is healthy
    pub fn is_node_healthy(base_url: &str) -> bool {
        let client = create_cluster_client(base_url);
        cluster_runtime()
            .block_on(async move { client.execute_query("SELECT 1", None, None).await })
            .is_ok()
    }

    /// Require cluster to be running (skip test if not available)
    pub fn require_cluster_running() {
        let urls = cluster_urls();
        if urls.is_empty() {
            panic!("No cluster URLs configured. Set KALAMDB_CLUSTER_URLS environment variable.");
        }

        // Check if at least one node is reachable
        let any_healthy = urls.iter().any(|url| is_node_healthy(url));
        if !any_healthy {
            panic!(
                "No cluster nodes are reachable. Start the cluster first.\n\
                 Expected nodes at: {:?}\n\
                 Use: ./docker/cluster/cluster.sh start  (Docker)\n\
                 Or:  ./scripts/cluster-local.sh start   (local)",
                urls
            );
        }
    }

    /// Wait for a table to be visible on all cluster nodes
    /// Returns true if table is visible on all nodes within timeout, false otherwise
    pub fn wait_for_table_on_all_nodes(namespace: &str, table_name: &str, timeout_ms: u64) -> bool {
        let urls = cluster_urls();
        let query = format!(
            "SELECT table_name FROM system.tables WHERE namespace_id = '{}' AND table_name = '{}'",
            namespace, table_name
        );
        
        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(timeout_ms);
        
        while start.elapsed() < timeout {
            let mut all_visible = true;
            for url in &urls {
                match execute_on_node(url, &query) {
                    Ok(result) if result.contains(table_name) => {}
                    _ => {
                        all_visible = false;
                        break;
                    }
                }
            }
            if all_visible {
                return true;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        false
    }

    /// Wait for a namespace to be visible on all cluster nodes
    pub fn wait_for_namespace_on_all_nodes(namespace: &str, timeout_ms: u64) -> bool {
        let urls = cluster_urls();
        let query = format!(
            "SELECT namespace_id FROM system.namespaces WHERE namespace_id = '{}'",
            namespace
        );
        
        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(timeout_ms);
        
        while start.elapsed() < timeout {
            let mut all_visible = true;
            for url in &urls {
                match execute_on_node(url, &query) {
                    Ok(result) if result.contains(namespace) => {}
                    _ => {
                        all_visible = false;
                        break;
                    }
                }
            }
            if all_visible {
                return true;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        false
    }

    /// Wait for row count to reach expected value on all nodes
    pub fn wait_for_row_count_on_all_nodes(full_table: &str, expected: i64, timeout_ms: u64) -> bool {
        let urls = cluster_urls();
        let query = format!("SELECT count(*) as count FROM {}", full_table);
        
        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(timeout_ms);
        
        while start.elapsed() < timeout {
            let mut all_match = true;
            for url in &urls {
                let count = query_count_on_url(url, &query);
                if count != expected {
                    all_match = false;
                    break;
                }
            }
            if all_match {
                return true;
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        false
    }
}

#[path = "cluster/cluster_test_consistency.rs"]
mod cluster_test_consistency;
#[path = "cluster/cluster_test_failover.rs"]
mod cluster_test_failover;
#[path = "cluster/cluster_test_data_digest.rs"]
mod cluster_test_data_digest;
#[path = "cluster/cluster_test_replication.rs"]
mod cluster_test_replication;
#[path = "cluster/cluster_test_ws_follower.rs"]
mod cluster_test_ws_follower;
#[path = "cluster/cluster_test_system_tables_replication.rs"]
mod cluster_test_system_tables_replication;
#[path = "cluster/cluster_test_subscription_nodes.rs"]
mod cluster_test_subscription_nodes;
#[path = "cluster/cluster_test_table_identity.rs"]
mod cluster_test_table_identity;
#[path = "cluster/cluster_test_multi_node_smoke.rs"]
mod cluster_test_multi_node_smoke;
#[path = "cluster/cluster_test_final_consistency.rs"]
mod cluster_test_final_consistency;
