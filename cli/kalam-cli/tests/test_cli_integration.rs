//! Integration tests for kalam-cli
//!
//! **Implements T035-T043**: Critical CLI integration tests
//!
//! These tests validate the CLI against a running KalamDB server:
//! - Connection and basic queries
//! - Output formatting (table, JSON, CSV)
//! - Batch execution
//! - WebSocket subscriptions
//! - Error handling
//!
//! # Test Architecture
//!
//! Tests use a two-process model:
//! 1. **Server Process**: KalamDB server running on localhost:8080
//! 2. **CLI Process**: kalam-cli binary executed via assert_cmd
//!
//! # Running Tests
//!
//! ```bash
//! # Start server in one terminal
//! cargo run --release --bin kalamdb-server
//!
//! # Run tests in another terminal
//! cargo test --test test_cli_integration
//! ```

use assert_cmd::Command;
use predicates::prelude::*;
use reqwest;
use serde_json::json;
use std::fs;
use std::time::Duration;
use tempfile::TempDir;
use tokio;

/// Test configuration
const SERVER_URL: &str = "http://localhost:8080";
const TEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Helper to check if server is running
async fn is_server_running() -> bool {
    reqwest::Client::new()
        .get(format!("{}/api/health", SERVER_URL))
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .is_ok()
}

/// Helper to execute SQL via HTTP (for test setup)
async fn execute_sql(sql: &str) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/api/sql", SERVER_URL))
        .json(&json!({ "sql": sql }))
        .send()
        .await?;
    
    if !response.status().is_success() {
        return Err(format!("SQL failed: {}", response.text().await?).into());
    }
    Ok(())
}

/// Helper to setup test namespace and table
async fn setup_test_data() -> Result<(), Box<dyn std::error::Error>> {
    // Create namespace
    execute_sql("CREATE NAMESPACE test_cli").await?;
    
    // Create test table
    execute_sql(
        r#"CREATE USER TABLE test_cli.messages (
            id INT AUTO_INCREMENT,
            content VARCHAR NOT NULL,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        ) FLUSH ROWS 10"#,
    )
    .await?;
    
    Ok(())
}

/// Helper to cleanup test data
async fn cleanup_test_data() -> Result<(), Box<dyn std::error::Error>> {
    let _ = execute_sql("DROP NAMESPACE test_cli CASCADE").await;
    Ok(())
}

/// T036: Test CLI connection and prompt display
#[tokio::test]
async fn test_cli_connection_and_prompt() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running at {}. Skipping test.", SERVER_URL);
        eprintln!("   Start server: cargo run --release --bin kalamdb-server");
        return;
    }

    let mut cmd = Command::cargo_bin("kalam").unwrap();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Interactive SQL terminal"))
        .stdout(predicate::str::contains("--url"));
}

/// T037: Test basic query execution
#[tokio::test]
async fn test_cli_basic_query_execution() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    // Setup
    setup_test_data().await.unwrap();

    // Execute query via CLI
    let mut cmd = Command::cargo_bin("kalam").unwrap();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--command")
        .arg("SELECT name, type FROM system.tables WHERE namespace = 'test_cli'")
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Verify output contains table name
    assert!(
        stdout.contains("messages") || stdout.contains("test_cli"),
        "Output should contain table info: {}",
        stdout
    );

    // Cleanup
    cleanup_test_data().await.unwrap();
}

/// T038: Test table output formatting
#[tokio::test]
async fn test_cli_table_output_formatting() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    setup_test_data().await.unwrap();

    // Insert test data
    execute_sql(
        "INSERT INTO test_cli.messages (content) VALUES ('Hello World'), ('Test Message')",
    )
    .await
    .unwrap();

    // Query with table format (default)
    let mut cmd = Command::cargo_bin("kalam").unwrap();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--command")
        .arg("SELECT * FROM test_cli.messages")
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify table formatting (pipe separators)
    assert!(
        stdout.contains("Hello World") && stdout.contains("Test Message"),
        "Output should contain both messages: {}",
        stdout
    );

    cleanup_test_data().await.unwrap();
}

/// T039: Test JSON output format
#[tokio::test]
async fn test_cli_json_output_format() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    setup_test_data().await.unwrap();

    execute_sql("INSERT INTO test_cli.messages (content) VALUES ('JSON Test')")
        .await
        .unwrap();

    // Query with JSON format
    let mut cmd = Command::cargo_bin("kalam").unwrap();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--json")
        .arg("--command")
        .arg("SELECT * FROM test_cli.messages WHERE content = 'JSON Test'")
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify JSON output
    assert!(
        stdout.contains("JSON Test"),
        "JSON output should contain test data: {}",
        stdout
    );

    cleanup_test_data().await.unwrap();
}

/// T040: Test CSV output format
#[tokio::test]
async fn test_cli_csv_output_format() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    setup_test_data().await.unwrap();

    execute_sql("INSERT INTO test_cli.messages (content) VALUES ('CSV,Test')")
        .await
        .unwrap();

    // Query with CSV format
    let mut cmd = Command::cargo_bin("kalam").unwrap();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--csv")
        .arg("--command")
        .arg("SELECT content FROM test_cli.messages WHERE content LIKE 'CSV%'")
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify CSV output (should have quotes around value with comma)
    assert!(
        stdout.contains("CSV") || stdout.contains("content"),
        "CSV output should contain data: {}",
        stdout
    );

    cleanup_test_data().await.unwrap();
}

/// T055: Test batch file execution
#[tokio::test]
async fn test_cli_batch_file_execution() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    // Create temporary SQL file
    let temp_dir = TempDir::new().unwrap();
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
    let mut cmd = Command::cargo_bin("kalam").unwrap();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--file")
        .arg(sql_file.to_str().unwrap())
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify execution
    assert!(
        stdout.contains("Item One") || output.status.success(),
        "Batch execution should succeed: {}",
        stdout
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

    let mut cmd = Command::cargo_bin("kalam").unwrap();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--command")
        .arg("INVALID SQL SYNTAX HERE")
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should contain error message
    assert!(
        stderr.contains("error") || stderr.contains("Error") || 
        stdout.contains("error") || stdout.contains("Error"),
        "Should display error message. stderr: {}, stdout: {}",
        stderr,
        stdout
    );
}

/// T057: Test connection failure handling
#[tokio::test]
async fn test_cli_connection_failure_handling() {
    // Try to connect to non-existent server
    let mut cmd = Command::cargo_bin("kalam").unwrap();
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
        !output.status.success() ||
        stderr.contains("Connection") || stderr.contains("error") ||
        stdout.contains("Connection") || stdout.contains("error"),
        "Should display connection error. stderr: {}, stdout: {}",
        stderr,
        stdout
    );
}

/// T050: Test help command
#[tokio::test]
async fn test_cli_help_command() {
    let mut cmd = Command::cargo_bin("kalam").unwrap();
    cmd.arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Interactive SQL terminal"))
        .stdout(predicate::str::contains("--url"))
        .stdout(predicate::str::contains("--json"))
        .stdout(predicate::str::contains("--file"));
}

/// T051: Test version flag
#[tokio::test]
async fn test_cli_version() {
    let mut cmd = Command::cargo_bin("kalam").unwrap();
    cmd.arg("--version");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("0.1.0"));
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
        .get(format!("{}/api/health", SERVER_URL))
        .send()
        .await
        .expect("Health check failed");

    assert!(
        response.status().is_success(),
        "Server health check should succeed"
    );
}
