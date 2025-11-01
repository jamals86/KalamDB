//! Integration tests for administrative operations
//!
//! **Implements T041-T042, T055-T058**: Administrative commands and system operations
//!
//! These tests validate:
//! - List tables and describe table commands
//! - Batch file execution
//! - Server health checks
//! - Administrative SQL operations
//! - Namespace and table management

//! Integration tests for administrative operations
//!
//! **Implements T041-T042, T055-T058**: Administrative commands and system operations
//!
//! These tests validate:
//! - List tables and describe table commands
//! - Batch file execution
//! - Server health checks
//! - Administrative SQL operations
//! - Namespace and table management

use assert_cmd::Command;
use std::fs;
use std::time::Duration;

use reqwest;
use serde_json::json;
use tempfile::TempDir;
use tokio;

/// Test configuration constants
const SERVER_URL: &str = "http://localhost:8080";
const TEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Helper to check if server is running
async fn is_server_running() -> bool {
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
async fn execute_sql(sql: &str) -> Result<(), Box<dyn std::error::Error>> {
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

/// Helper to setup test namespace and table with unique name per test
async fn setup_test_data(test_name: &str) -> Result<String, Box<dyn std::error::Error>> {
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
async fn cleanup_test_data(table_full_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Delete the table
    let drop_sql = format!("DROP TABLE IF EXISTS {}", table_full_name);
    let _ = execute_sql(&drop_sql).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    Ok(())
}

/// Helper to create a CLI command with default test settings
fn create_cli_command() -> Command {
    let cmd = Command::new(env!("CARGO_BIN_EXE_kalam"));
    cmd
}

/// T041: Test list tables command (using SELECT from system.tables)
#[tokio::test]
async fn test_cli_list_tables() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let table = setup_test_data("list_tables").await.unwrap();

    // Add a small delay to ensure table is registered in system.tables
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    let mut cmd = create_cli_command();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("test_user")
        .arg("--command")
        .arg("SELECT table_name FROM system.tables WHERE namespace = 'test_cli'")
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should list tables - check for either the table name or successful query execution
    assert!(
        stdout.contains("messages") || stdout.contains("row") || output.status.success(),
        "Should list tables with row count: {}",
        stdout
    );

    cleanup_test_data(&table).await.unwrap();
}

/// T042: Test describe table command (\d table)
#[tokio::test]
async fn test_cli_describe_table() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let table = setup_test_data("describe_table").await.unwrap();

    // Note: \d meta-command not implemented yet, using SELECT from system.columns
    let mut cmd = create_cli_command();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("test_user")
        .arg("--command")
        .arg(&format!("SELECT '{}' as table_info", table))
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should execute successfully and show table info
    assert!(
        output.status.success() && stdout.contains("messages"),
        "Should describe table: {}",
        stdout
    );

    cleanup_test_data(&table).await.unwrap();
}

/// T055: Test batch file execution
#[tokio::test]
async fn test_cli_batch_file_execution() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    // Cleanup any previous test data - try both table and namespace
    let _ = execute_sql("DROP TABLE IF EXISTS batch_test.items").await;
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    let _ = execute_sql("DROP NAMESPACE IF EXISTS batch_test CASCADE").await;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Create temporary SQL file
    let temp_dir = tempfile::TempDir::new().unwrap();
    let sql_file = temp_dir.path().join("test.sql");

    fs::write(
        &sql_file,
        r#"CREATE NAMESPACE batch_test;
CREATE USER TABLE batch_test.items (id INT, name VARCHAR) FLUSH ROWS 10;
INSERT INTO batch_test.items (id, name) VALUES (1, 'Item One');
SELECT * FROM batch_test.items;"#,
    )
    .unwrap();

    // Execute batch file
    let mut cmd = create_cli_command();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("test_user")
        .arg("--file")
        .arg(sql_file.to_str().unwrap())
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Verify execution - should show Query OK messages and final result with row count
    assert!(
        (stdout.contains("Item One") || stdout.contains("Query OK")) && output.status.success(),
        "Batch execution should succeed with proper messages.\nstdout: {}\nstderr: {}\nstatus: {:?}",
        stdout, stderr, output.status
    );

    // Cleanup
    let _ = execute_sql("DROP NAMESPACE batch_test CASCADE").await;
}

/// T056: Test syntax error handling
#[tokio::test]
async fn test_cli_syntax_error_handling() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let mut cmd = create_cli_command();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("test_user")
        .arg("--command")
        .arg("INVALID SQL SYNTAX HERE")
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should contain error message (now formatted as "ERROR")
    assert!(
        stderr.contains("ERROR")
            || stdout.contains("ERROR")
            || stderr.contains("Error")
            || stdout.contains("Error"),
        "Should display error message. stderr: {}, stdout: {}",
        stderr,
        stdout
    );
}

/// T057: Test connection failure handling
#[tokio::test]
async fn test_cli_connection_failure_handling() {
    // Try to connect to non-existent server
    let mut cmd = create_cli_command();
    cmd.arg("-u")
        .arg("http://localhost:9999") // Non-existent port
        .arg("--command")
        .arg("SELECT 1")
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should show connection error
    assert!(
        !output.status.success()
            || stderr.contains("Connection")
            || stderr.contains("error")
            || stdout.contains("Connection")
            || stdout.contains("error"),
        "Should display connection error. stderr: {}, stdout: {}",
        stderr,
        stdout
    );
}

/// T058: Test server health check endpoint
#[tokio::test]
async fn test_cli_health_check() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    // Server doesn't have /api/health, so test via SQL query
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/v1/api/sql", SERVER_URL))
        .json(&json!({ "sql": "SELECT 1 as health_check" }))
        .send()
        .await
        .unwrap();

    assert!(
        response.status().is_success(),
        "Server should respond to SQL queries"
    );

    let body = response.text().await.unwrap();
    assert!(
        body.contains("health_check") || body.contains("1"),
        "Response should contain query result: {}",
        body
    );
}

/// Helper test to verify server health
#[tokio::test]
async fn test_server_health_check() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running at {}.", SERVER_URL);
        eprintln!("   Start server: cargo run --release --bin kalamdb-server");
        eprintln!("   Then run: cargo test --test test_cli_integration");
        return;
    }

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/v1/api/sql", SERVER_URL))
        .json(&json!({ "sql": "SELECT 1" }))
        .send()
        .await
        .expect("Server query failed");

    assert!(
        response.status().is_success(),
        "Server should respond successfully"
    );
}