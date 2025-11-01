//! Common utilities and helpers for CLI integration tests
//!
//! This module provides shared functionality used across all CLI test modules,
//! including server connectivity checks, SQL execution helpers, test data setup,
//! and specialized tools like subscription message listeners.

use assert_cmd::Command;
use reqwest;
use serde_json::json;
use std::fs;
use std::time::Duration;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use std::sync::Arc;

/// Test configuration constants
pub const SERVER_URL: &str = "http://localhost:8080";
pub const TEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Helper to check if server is running
pub async fn is_server_running() -> bool {
    // Try a simple SQL query instead of health endpoint
    reqwest::Client::new()
        .post(format!("{}/v1/api/sql", SERVER_URL))
        .json(&json!({ "sql": "SELECT 1" }))
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Helper to execute SQL via HTTP (for test setup)
pub async fn execute_sql(sql: &str) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/v1/api/sql", SERVER_URL))
        .basic_auth("root", Some(""))
        .json(&json!({ "sql": sql }))
        .send()
        .await?;

    let body = response.text().await?;
    let parsed: serde_json::Value = serde_json::from_str(&body)?;

    if parsed["status"] != "success" {
        return Err(format!("SQL failed: {}", body).into());
    }
    Ok(())
}

/// Helper to execute SQL with authentication
pub async fn execute_sql_as(
    username: &str,
    password: &str,
    sql: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/v1/api/sql", SERVER_URL))
        .basic_auth(username, Some(password))
        .json(&json!({ "sql": sql }))
        .send()
        .await?;

    let body = response.text().await?;
    let parsed: serde_json::Value = serde_json::from_str(&body)?;
    Ok(parsed)
}

/// Helper to execute SQL as root user (empty password for localhost)
pub async fn execute_sql_as_root(sql: &str) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    execute_sql_as("root", "", sql).await
}

/// Helper to setup test namespace and table with unique name per test
pub async fn setup_test_data(test_name: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Use test-specific table name to avoid conflicts
    let table_name = format!("messages_{}", test_name);
    let namespace = "test_cli";

    // Longer delay to avoid race conditions between tests
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Try to drop table first if it exists
    let drop_sql = format!("DROP TABLE IF EXISTS {}.{}", namespace, table_name);
    let _ = execute_sql(&drop_sql).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Create namespace (don't drop it - let it persist across tests)
    match execute_sql(&format!("CREATE NAMESPACE {}", namespace)).await {
        Ok(_) => {
            // Namespace created successfully
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
        Err(e) if e.to_string().contains("already exists") => {
            // Namespace exists, that's ok
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }
        Err(e) => return Err(e),
    }

    // Create test table using USER TABLE
    let create_sql = format!(
        r#"CREATE USER TABLE {}.{} (
            id INT AUTO_INCREMENT,
            content VARCHAR NOT NULL,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        ) FLUSH ROWS 10"#,
        namespace, table_name
    );

    match execute_sql(&create_sql).await {
        Ok(_) => {}
        Err(e) if e.to_string().contains("already exists") => {
            // Table exists, drop and recreate to ensure clean state
            execute_sql(&drop_sql).await?;
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            execute_sql(&create_sql).await?;
        }
        Err(e) => return Err(e),
    }

    // Small delay after table creation
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    Ok(format!("{}.{}", namespace, table_name))
}

/// Helper to cleanup test data
pub async fn cleanup_test_data(table_full_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Delete the table
    let drop_sql = format!("DROP TABLE IF EXISTS {}", table_full_name);
    let _ = execute_sql(&drop_sql).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    Ok(())
}

/// Helper to create a CLI command with default test settings
pub fn create_cli_command() -> Command {
    let cmd = Command::new(env!("CARGO_BIN_EXE_kalam"));
    cmd
}

/// Subscription message listener for testing WebSocket subscriptions
pub struct SubscriptionListener {
    receiver: mpsc::Receiver<serde_json::Value>,
    _handle: tokio::task::JoinHandle<()>,
}

impl SubscriptionListener {
    /// Create a new subscription listener for a given subscription URL
    pub async fn new(subscription_url: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let (tx, rx) = mpsc::channel(100);

        let url = subscription_url.to_string();
        let handle = tokio::spawn(async move {
            if let Ok((ws_stream, _)) = connect_async(&url).await {
                let (mut write, mut read) = ws_stream.split();

                while let Some(message) = read.next().await {
                    match message {
                        Ok(msg) => {
                            if let Ok(text) = msg.to_text() {
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(text) {
                                    let _ = tx.send(json).await;
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        });

        Ok(Self {
            receiver: rx,
            _handle: handle,
        })
    }

    /// Receive the next subscription message with timeout
    pub async fn receive_message(&mut self, timeout: Duration) -> Option<serde_json::Value> {
        tokio::time::timeout(timeout, self.receiver.recv()).await.ok().flatten()
    }

    /// Receive all available messages up to a maximum count
    pub async fn receive_all_messages(&mut self, max_count: usize, timeout: Duration) -> Vec<serde_json::Value> {
        let mut messages = Vec::new();
        let start_time = std::time::Instant::now();

        while messages.len() < max_count && start_time.elapsed() < timeout {
            if let Some(msg) = self.receive_message(Duration::from_millis(100)).await {
                messages.push(msg);
            } else {
                break;
            }
        }

        messages
    }
}

/// Helper to create a unique test namespace name
pub fn generate_unique_namespace(base_name: &str) -> String {
    format!(
        "{}_{}",
        base_name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    )
}

/// Helper to create a unique test table name
pub fn generate_unique_table(base_name: &str) -> String {
    format!(
        "{}_{}",
        base_name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    )
}

/// Helper to wait for a condition with timeout
pub async fn wait_for_condition<F, Fut>(
    mut condition: F,
    timeout: Duration,
    check_interval: Duration,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        if condition().await {
            return Ok(());
        }
        tokio::time::sleep(check_interval).await;
    }

    Err("Condition not met within timeout".into())
}

/// Helper to create temporary SQL file for batch execution tests
pub fn create_temp_sql_file(content: &str) -> Result<tempfile::NamedTempFile, Box<dyn std::error::Error>> {
    let mut temp_file = tempfile::NamedTempFile::new()?;
    std::io::Write::write_all(&mut temp_file, content.as_bytes())?;
    Ok(temp_file)
}

/// Helper to create temporary config file for config tests
pub fn create_temp_config_file(content: &str) -> Result<tempfile::NamedTempFile, Box<dyn std::error::Error>> {
    let mut temp_file = tempfile::NamedTempFile::new()?;
    std::io::Write::write_all(&mut temp_file, content.as_bytes())?;
    Ok(temp_file)
}