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
    use serde_json::Value;
    use std::sync::OnceLock;
    use std::time::Duration;

    /// Get cluster node URLs from environment or use defaults
    pub fn cluster_urls() -> Vec<String> {
        test_context().cluster_urls.clone()
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

    /// Create a client connected to a specific cluster node with custom credentials
    pub fn create_cluster_client_with_auth(
        base_url: &str,
        username: &str,
        password: &str,
    ) -> KalamLinkClient {
        KalamLinkClient::builder()
            .base_url(base_url)
            .auth(AuthProvider::basic_auth(username.to_string(), password.to_string()))
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
                let result = response.results.first().expect("Missing query result for count");
                let rows =
                    result.rows.as_ref().and_then(|rows| rows.first()).expect("Missing count row");
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
        let sql = sql.to_string();
        let urls = cluster_urls();
        let mut last_err: Option<String> = None;

        for _ in 0..3 {
            for url in std::iter::once(base_url.to_string()).chain(urls.iter().cloned()) {
                let client = create_cluster_client(&url);
                let sql_value = sql.clone();
                match cluster_runtime()
                    .block_on(async move { client.execute_query(&sql_value, None, None).await })
                {
                    Ok(response) => {
                        return Ok(
                            serde_json::to_string_pretty(&response)
                                .unwrap_or_else(|_| format!("{:?}", response)),
                        );
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        if is_leader_error(&msg) {
                            last_err = Some(msg);
                            continue;
                        }
                        return Err(msg);
                    }
                }
            }

            std::thread::sleep(Duration::from_millis(200));
        }

        Err(last_err.unwrap_or_else(|| "All cluster nodes failed".to_string()))
    }

    /// Execute SQL on a specific cluster node and return the structured response
    pub fn execute_on_node_response(base_url: &str, sql: &str) -> Result<QueryResponse, String> {
        let sql = sql.to_string();
        let urls = cluster_urls();
        let mut last_err: Option<String> = None;

        for _ in 0..3 {
            for url in std::iter::once(base_url.to_string()).chain(urls.iter().cloned()) {
                let client = create_cluster_client(&url);
                let sql_value = sql.clone();
                match cluster_runtime()
                    .block_on(async move { client.execute_query(&sql_value, None, None).await })
                {
                    Ok(response) => return Ok(response),
                    Err(e) => {
                        let msg = e.to_string();
                        if is_leader_error(&msg) {
                            last_err = Some(msg);
                            continue;
                        }
                        return Err(msg);
                    }
                }
            }

            std::thread::sleep(Duration::from_millis(200));
        }

        Err(last_err.unwrap_or_else(|| "All cluster nodes failed".to_string()))
    }

    /// Execute SQL on a specific cluster node as a custom user
    pub fn execute_on_node_as_user(
        base_url: &str,
        username: &str,
        password: &str,
        sql: &str,
    ) -> Result<String, String> {
        let sql = sql.to_string();
        let urls = cluster_urls();
        let mut last_err: Option<String> = None;

        for _ in 0..3 {
            for url in std::iter::once(base_url.to_string()).chain(urls.iter().cloned()) {
                let client = create_cluster_client_with_auth(&url, username, password);
                let sql_value = sql.clone();
                match cluster_runtime()
                    .block_on(async move { client.execute_query(&sql_value, None, None).await })
                {
                    Ok(response) => {
                        return Ok(
                            serde_json::to_string_pretty(&response)
                                .unwrap_or_else(|_| format!("{:?}", response)),
                        );
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        if is_leader_error(&msg) {
                            last_err = Some(msg);
                            continue;
                        }
                        return Err(msg);
                    }
                }
            }

            std::thread::sleep(Duration::from_millis(200));
        }

        Err(last_err.unwrap_or_else(|| "All cluster nodes failed".to_string()))
    }

    /// Execute SQL on a specific cluster node as a custom user and return the response
    pub fn execute_on_node_as_user_response(
        base_url: &str,
        username: &str,
        password: &str,
        sql: &str,
    ) -> Result<QueryResponse, String> {
        let sql = sql.to_string();
        let urls = cluster_urls();
        let mut last_err: Option<String> = None;

        for _ in 0..3 {
            for url in std::iter::once(base_url.to_string()).chain(urls.iter().cloned()) {
                let client = create_cluster_client_with_auth(&url, username, password);
                let sql_value = sql.clone();
                match cluster_runtime()
                    .block_on(async move { client.execute_query(&sql_value, None, None).await })
                {
                    Ok(response) => return Ok(response),
                    Err(e) => {
                        let msg = e.to_string();
                        if is_leader_error(&msg) {
                            last_err = Some(msg);
                            continue;
                        }
                        return Err(msg);
                    }
                }
            }

            std::thread::sleep(Duration::from_millis(200));
        }

        Err(last_err.unwrap_or_else(|| "All cluster nodes failed".to_string()))
    }

    fn normalize_rows(rows: &[Vec<Value>]) -> Vec<String> {
        let mut normalized: Vec<String> = rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|v| {
                        let extracted = extract_typed_value(v);
                        match extracted {
                            Value::Null => "NULL".to_string(),
                            Value::String(s) => s,
                            Value::Number(n) => n.to_string(),
                            Value::Bool(b) => b.to_string(),
                            other => other.to_string(),
                        }
                    })
                    .collect::<Vec<String>>()
                    .join("|")
            })
            .collect();

        normalized.sort();
        normalized
    }

    /// Fetch normalized row strings from a root-authenticated query
    pub fn fetch_normalized_rows(base_url: &str, sql: &str) -> Result<Vec<String>, String> {
        let response = execute_on_node_response(base_url, sql)?;
        let result = response.results.first().ok_or_else(|| "Missing query result".to_string())?;
        let rows = result.rows.as_ref().ok_or_else(|| "Missing row data".to_string())?;

        Ok(normalize_rows(rows))
    }

    /// Fetch normalized row strings from a user-authenticated query
    pub fn fetch_normalized_rows_as_user(
        base_url: &str,
        username: &str,
        password: &str,
        sql: &str,
    ) -> Result<Vec<String>, String> {
        let response = execute_on_node_as_user_response(base_url, username, password, sql)?;
        let result = response.results.first().ok_or_else(|| "Missing query result".to_string())?;
        let rows = result.rows.as_ref().ok_or_else(|| "Missing row data".to_string())?;

        Ok(normalize_rows(rows))
    }

    /// Check if a cluster node is healthy
    pub fn is_node_healthy(base_url: &str) -> bool {
        let client = create_cluster_client(base_url);
        cluster_runtime()
            .block_on(async move { client.execute_query("SELECT 1", None, None).await })
            .is_ok()
    }

    /// Require cluster to be running (skip test if not available)
    pub fn require_cluster_running() -> bool {
        if !crate::common::is_cluster_mode() {
            println!(
                "\n  Skipping: single-node server detected (cluster tests require multi-node)\n"
            );
            return false;
        }

        let urls = cluster_urls();
        if urls.is_empty() {
            println!("\n  Skipping: no cluster URLs configured (set KALAMDB_CLUSTER_URLS)\n");
            return false;
        }

        // Check if at least one node is reachable
        let any_healthy = urls.iter().any(|url| is_node_healthy(url));
        if !any_healthy {
            println!(
                "\n  Skipping: no cluster nodes are reachable. Expected nodes at: {:?}\n",
                urls
            );
            return false;
        }

        true
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
        let timeout = std::time::Duration::from_millis(timeout_ms);

        while start.elapsed() < timeout {
            let all_visible = urls.iter().all(|url| {
                matches!(execute_on_node(url, &query), Ok(result) if result.contains(table_name))
            });

            if all_visible {
                return true;
            }

            std::thread::sleep(std::time::Duration::from_millis(50));
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
        let timeout = std::time::Duration::from_millis(timeout_ms);

        while start.elapsed() < timeout {
            let all_visible = urls.iter().all(|url| {
                matches!(execute_on_node(url, &query), Ok(result) if result.contains(namespace))
            });

            if all_visible {
                return true;
            }

            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        false
    }

    /// Wait for row count to reach expected value on all nodes
    pub fn wait_for_row_count_on_all_nodes(
        full_table: &str,
        expected: i64,
        timeout_ms: u64,
    ) -> bool {
        let urls = cluster_urls();
        let query = format!("SELECT count(*) as count FROM {}", full_table);

        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(timeout_ms);

        while start.elapsed() < timeout {
            let all_match = urls
                .iter()
                .map(|url| query_count_on_url(url, &query))
                .all(|count| count == expected);

            if all_match {
                return true;
            }

            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        false
    }

    /// Wait for the latest job id for a job type to appear.
    pub fn wait_for_latest_job_id_by_type(
        base_url: &str,
        job_type: &str,
        timeout: Duration,
    ) -> Option<String> {
        let start = std::time::Instant::now();
        let sql = format!(
            "SELECT job_id FROM system.jobs WHERE job_type = '{}' ORDER BY created_at DESC LIMIT 1",
            job_type
        );

        while start.elapsed() < timeout {
            if let Ok(response) = execute_on_node_response(base_url, &sql) {
                if let Some(result) = response.results.first() {
                    if let Some(rows) = &result.rows {
                        if let Some(row) = rows.first() {
                            if let Some(value) = row.first() {
                                let extracted = extract_typed_value(value);
                                if let Some(job_id) = extracted.as_str() {
                                    return Some(job_id.to_string());
                                }
                            }
                        }
                    }
                }
            }

            std::thread::sleep(Duration::from_millis(200));
        }

        None
    }

    /// Wait for a job to reach a specific status.
    pub fn wait_for_job_status(
        base_url: &str,
        job_id: &str,
        status: &str,
        timeout: Duration,
    ) -> bool {
        let start = std::time::Instant::now();
        let sql = format!(
            "SELECT status FROM system.jobs WHERE job_id = '{}' LIMIT 1",
            job_id
        );

        while start.elapsed() < timeout {
            if let Ok(response) = execute_on_node_response(base_url, &sql) {
                if let Some(result) = response.results.first() {
                    if let Some(rows) = &result.rows {
                        if let Some(row) = rows.first() {
                            if let Some(value) = row.first() {
                                if extract_typed_value(value)
                                    .as_str()
                                    .map(|s| s.eq_ignore_ascii_case(status))
                                    .unwrap_or(false)
                                {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }

            std::thread::sleep(Duration::from_millis(200));
        }

        false
    }
}

#[path = "cluster/cluster_test_cluster_list.rs"]
mod cluster_test_cluster_list;
#[path = "cluster/cluster_test_consistency.rs"]
mod cluster_test_consistency;
#[path = "cluster/cluster_test_data_digest.rs"]
mod cluster_test_data_digest;
#[path = "cluster/cluster_test_failover.rs"]
mod cluster_test_failover;
#[path = "cluster/cluster_test_final_consistency.rs"]
mod cluster_test_final_consistency;
#[path = "cluster/cluster_test_flush.rs"]
mod cluster_test_flush;
#[path = "cluster/cluster_test_leader_jobs.rs"]
mod cluster_test_leader_jobs;
#[path = "cluster/cluster_test_multi_node_smoke.rs"]
mod cluster_test_multi_node_smoke;
#[path = "cluster/cluster_test_node_rejoin.rs"]
mod cluster_test_node_rejoin;
#[path = "cluster/cluster_test_replication.rs"]
mod cluster_test_replication;
#[path = "cluster/cluster_test_snapshot.rs"]
mod cluster_test_snapshot;
#[path = "cluster/cluster_test_subscription_nodes.rs"]
mod cluster_test_subscription_nodes;
#[path = "cluster/cluster_test_system_tables_replication.rs"]
mod cluster_test_system_tables_replication;
#[path = "cluster/cluster_test_table_crud_consistency.rs"]
mod cluster_test_table_crud_consistency;
#[path = "cluster/cluster_test_table_identity.rs"]
mod cluster_test_table_identity;
#[path = "cluster/cluster_test_ws_follower.rs"]
mod cluster_test_ws_follower;
