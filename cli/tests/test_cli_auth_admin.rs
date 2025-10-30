//! CLI integration tests for authentication and admin operations
//!
//! Tests proper authentication flow and admin-level SQL commands using real credentials.
//!
//! # Running Tests
//!
//! ```bash
//! # Start server in one terminal
//! cargo run --release --bin kalamdb-server
//!
//! # Run tests in another terminal
//! cargo test --test test_cli_auth_admin -- --test-threads=1
//! ```

use assert_cmd::Command;
use reqwest;
use serde_json::json;
use std::time::Duration;

const SERVER_URL: &str = "http://localhost:8080";
const TEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Helper to check if server is running
async fn is_server_running() -> bool {
    reqwest::Client::new()
        .post(format!("{}/v1/api/sql", SERVER_URL))
        .basic_auth("root", Some(""))
        .json(&json!({ "sql": "SELECT 1" }))
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Helper to execute SQL with authentication
async fn execute_sql_as(
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
async fn execute_sql_as_root(sql: &str) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    execute_sql_as("root", "", sql).await
}

/// Test that root user can create namespaces
#[tokio::test]
async fn test_root_can_create_namespace() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running at {}. Skipping test.", SERVER_URL);
        return;
    }

    // Clean up any existing namespace
    let _ = execute_sql_as_root("DROP NAMESPACE IF EXISTS test_admin CASCADE").await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Create namespace as root
    let result = execute_sql_as_root("CREATE NAMESPACE test_admin").await.unwrap();

    // Should succeed or already exist
    assert!(
        result["status"] == "success" 
            || (result["status"] == "error" && result["error"].as_str().unwrap_or("").contains("already exists")),
        "Root user should be able to create namespaces: {:?}",
        result
    );

    // Verify namespace was created
    let result = execute_sql_as_root(
        "SELECT namespace_name FROM system.namespaces WHERE namespace_name = 'test_admin'",
    )
    .await
    .unwrap();

    assert_eq!(result["status"], "success");
    assert!(
        result["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["namespace_name"] == "test_admin"),
        "Namespace should exist in system.namespaces"
    );

    // Cleanup
    let _ = execute_sql_as_root("DROP NAMESPACE test_admin CASCADE").await;
}

/// Test that root user can create and drop tables
#[tokio::test]
async fn test_root_can_create_drop_tables() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    // Ensure namespace exists
    let _ = execute_sql_as_root("CREATE NAMESPACE IF NOT EXISTS test_admin").await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create table as root
    let result = execute_sql_as_root(
        "CREATE USER TABLE test_admin.test_table (id INT, name VARCHAR) FLUSH ROWS 10",
    )
    .await
    .unwrap();

    assert_eq!(
        result["status"], "success",
        "Root user should be able to create tables: {:?}",
        result
    );

    // Drop table
    let result = execute_sql_as_root("DROP TABLE test_admin.test_table").await.unwrap();

    assert_eq!(result["status"], "success", "Root user should be able to drop tables");

    // Cleanup
    let _ = execute_sql_as_root("DROP NAMESPACE test_admin CASCADE").await;
}

/// Test CREATE NAMESPACE via CLI with root authentication
#[tokio::test]
async fn test_cli_create_namespace_as_root() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    // Clean up any existing namespace
    let _ = execute_sql_as_root("DROP NAMESPACE IF EXISTS cli_test_ns CASCADE").await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Execute CREATE NAMESPACE via CLI (auto-authenticates as root for localhost)
    let mut cmd = Command::cargo_bin("kalam").unwrap();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--command")
        .arg("CREATE NAMESPACE cli_test_ns")
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "CLI should succeed when creating namespace as root.\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );

    // Verify namespace was created
    let result = execute_sql_as_root(
        "SELECT namespace_name FROM system.namespaces WHERE namespace_name = 'cli_test_ns'",
    )
    .await
    .unwrap();

    assert_eq!(result["status"], "success");
    assert!(
        result["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["namespace_name"] == "cli_test_ns"),
        "Namespace should exist after CLI creation"
    );

    // Cleanup
    let _ = execute_sql_as_root("DROP NAMESPACE cli_test_ns CASCADE").await;
}

/// Test that non-admin users cannot create namespaces
#[tokio::test]
async fn test_regular_user_cannot_create_namespace() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    // First, create a regular user as root
    let _ = execute_sql_as_root("DROP USER IF EXISTS testuser").await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let result =
        execute_sql_as_root("CREATE USER testuser PASSWORD 'testpass' ROLE user").await;

    if result.is_err() || result.as_ref().unwrap()["status"] != "success" {
        eprintln!("⚠️  Failed to create test user, skipping test");
        return;
    }

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Try to create namespace as regular user
    let result = execute_sql_as("testuser", "testpass", "CREATE NAMESPACE user_test_ns").await;

    // Should fail with authorization error
    if let Ok(response) = result {
        assert!(
            response["status"] == "error"
                && (response["error"]
                    .as_str()
                    .unwrap_or("")
                    .contains("Admin privileges")
                    || response["error"]
                        .as_str()
                        .unwrap_or("")
                        .contains("Unauthorized")),
            "Regular user should not be able to create namespaces: {:?}",
            response
        );
    }

    // Cleanup
    let _ = execute_sql_as_root("DROP USER testuser").await;
}

/// Test CLI with explicit username/password
#[tokio::test]
async fn test_cli_with_explicit_credentials() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    // Execute query with explicit root credentials
    let mut cmd = Command::cargo_bin("kalam").unwrap();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--username")
        .arg("root")
        .arg("--password")
        .arg("")
        .arg("--command")
        .arg("SELECT 1 as test")
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success() && stdout.contains("test"),
        "CLI should work with explicit root credentials: {}",
        stdout
    );
}

/// Test admin operations via CLI
#[tokio::test]
async fn test_cli_admin_operations() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    // Clean up
    let _ = execute_sql_as_root("DROP NAMESPACE IF EXISTS admin_test CASCADE").await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Test batch SQL with multiple admin commands
    let sql_batch = r#"
CREATE NAMESPACE admin_test;
CREATE USER TABLE admin_test.users (id INT, name VARCHAR) FLUSH ROWS 10;
INSERT INTO admin_test.users (id, name) VALUES (1, 'Alice');
SELECT * FROM admin_test.users;
"#;

    let mut cmd = Command::cargo_bin("kalam").unwrap();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--command")
        .arg(sql_batch)
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Batch admin commands should succeed.\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );

    assert!(
        stdout.contains("Alice") || stdout.contains("Query OK"),
        "Output should show successful execution"
    );

    // Cleanup
    let _ = execute_sql_as_root("DROP NAMESPACE admin_test CASCADE").await;
}

/// Test SHOW NAMESPACES command
#[tokio::test]
async fn test_cli_show_namespaces() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let mut cmd = Command::cargo_bin("kalam").unwrap();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--command")
        .arg("SHOW NAMESPACES")
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success() && (stdout.contains("system") || stdout.contains("default")),
        "SHOW NAMESPACES should list system namespaces: {}",
        stdout
    );
}
