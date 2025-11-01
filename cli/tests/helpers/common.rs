//! Common utilities and helpers for CLI integration tests
//!
//! This module provides shared functionality used across all CLI test modules,
//! including server connectivity checks, CLI command helpers, test data setup,
//! and specialized tools like subscription message listeners.

use assert_cmd::Command;
use std::time::Duration;

/// Test configuration constants
pub const SERVER_URL: &str = "http://localhost:8080";
pub const TEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Helper to check if server is running via CLI
pub fn is_server_running() -> bool {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_kalam"));
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--command")
        .arg("SELECT 1")
        .timeout(Duration::from_secs(2));

    cmd.output().map(|output| output.status.success()).unwrap_or(false)
}

/// Helper to create a CLI command with default test settings
pub fn create_cli_command() -> Command {
    let cmd = Command::new(env!("CARGO_BIN_EXE_kalam"));
    cmd
}

/// Helper to execute SQL via CLI command
pub fn execute_sql_via_cli(sql: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut cmd = create_cli_command();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--command")
        .arg(sql)
        .timeout(TEST_TIMEOUT);

    let output = cmd.output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(format!("CLI command failed: {}", String::from_utf8_lossy(&output.stderr)).into())
    }
}

/// Helper to execute SQL via CLI with authentication
pub fn execute_sql_via_cli_as(username: &str, password: &str, sql: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut cmd = create_cli_command();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--username")
        .arg(username)
        .arg("--password")
        .arg(password)
        .arg("--command")
        .arg(sql)
        .timeout(TEST_TIMEOUT);

    let output = cmd.output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(format!("CLI command failed: {}", String::from_utf8_lossy(&output.stderr)).into())
    }
}

/// Helper to execute SQL as root user via CLI (empty password for localhost)
pub fn execute_sql_as_root_via_cli(sql: &str) -> Result<String, Box<dyn std::error::Error>> {
    execute_sql_via_cli_as("root", "", sql)
}

/// Helper to generate unique namespace name
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

/// Helper to generate unique table name
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