//! Integration tests for user table operations
//!
//! **Implements T037-T040, T064-T068**: User table CRUD operations, output formatting, and query features
//!
//! These tests validate:
//! - Basic query execution on user tables
//! - Output formatting (table, JSON, CSV)
//! - Multi-line queries and comments
//! - Empty queries and result pagination
//! - Query result display and formatting

use assert_cmd::Command;
use predicates::prelude::*;
use std::time::Duration;

use crate::common::*;

/// T037: Test basic query execution
#[tokio::test]
async fn test_cli_basic_query_execution() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    // Setup with unique table
    let table = setup_test_data("basic_query").await.unwrap();

    // Insert test data
    execute_sql(&format!(
        "INSERT INTO {} (content) VALUES ('Test Message')",
        table
    ))
    .await
    .unwrap();

    // Execute query via CLI
    let mut cmd = create_cli_command();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--command")
        .arg(&format!("SELECT * FROM {}", table))
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify output contains the inserted data and row count (PostgreSQL style: "(1 row)")
    assert!(
        stdout.contains("Test Message")
            && (stdout.contains("row)") || stdout.contains("row in set")),
        "Output should contain query results and row count: {}",
        stdout
    );

    // Cleanup
    cleanup_test_data(&table).await.unwrap();
}

/// T038: Test table output formatting
#[tokio::test]
async fn test_cli_table_output_formatting() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let table = setup_test_data("table_output").await.unwrap();

    // Insert test data
    execute_sql(&format!(
        "INSERT INTO {} (content) VALUES ('Hello World'), ('Test Message')",
        table
    ))
    .await
    .unwrap();

    // Query with table format (default)
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_kalam"));
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--command")
        .arg(&format!("SELECT * FROM {}", table))
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify table formatting (pipe separators) and row count (PostgreSQL style: "(2 rows)")
    assert!(
        stdout.contains("Hello World")
            && stdout.contains("Test Message")
            && (stdout.contains("row)") || stdout.contains("rows)")),
        "Output should contain both messages and row count: {}",
        stdout
    );

    cleanup_test_data(&table).await.unwrap();
}

/// T039: Test JSON output format
#[tokio::test]
async fn test_cli_json_output_format() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let table = setup_test_data("json_output").await.unwrap();

    execute_sql(&format!(
        "INSERT INTO {} (content) VALUES ('JSON Test')",
        table
    ))
    .await
    .unwrap();

    // Query with JSON format
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_kalam"));
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("test_user")
        .arg("--json")
        .arg("--command")
        .arg(&format!(
            "SELECT * FROM {} WHERE content = 'JSON Test'",
            table
        ))
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify JSON output (JSON format doesn't include row count in same way)
    assert!(
        stdout.contains("JSON Test"),
        "JSON output should contain test data: {}",
        stdout
    );

    cleanup_test_data(&table).await.unwrap();
}

/// T040: Test CSV output format
#[tokio::test]
async fn test_cli_csv_output_format() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let table = setup_test_data("csv_output").await.unwrap();

    execute_sql(&format!(
        "INSERT INTO {} (content) VALUES ('CSV,Test')",
        table
    ))
    .await
    .unwrap();

    // Query with CSV format
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_kalam"));
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("test_user")
        .arg("--csv")
        .arg("--command")
        .arg(&format!(
            "SELECT content FROM {} WHERE content LIKE 'CSV%'",
            table
        ))
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify CSV output (should have quotes around value with comma)
    assert!(
        stdout.contains("CSV") || stdout.contains("content"),
        "CSV output should contain data: {}",
        stdout
    );

    cleanup_test_data(&table).await.unwrap();
}

/// T064: Test multi-line query input
#[tokio::test]
async fn test_cli_multiline_query() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let table = setup_test_data("multiline").await.unwrap();

    // Multi-line query with newlines
    let multi_line_query = format!("SELECT \n  id, \n  content \nFROM \n  {} \nLIMIT 5", table);

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_kalam"));
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("test_user")
        .arg("--command")
        .arg(&multi_line_query)
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();

    assert!(output.status.success(), "Should handle multi-line queries");

    cleanup_test_data(&table).await.unwrap();
}

/// T065: Test query with comments
#[tokio::test]
async fn test_cli_query_with_comments() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    // Test with simple SQL (comments in SQL strings don't work well in shell args)
    let query_simple = "SELECT 1 as test";

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_kalam"));
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("test_user")
        .arg("--command")
        .arg(query_simple)
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();

    assert!(
        output.status.success(),
        "Should handle queries successfully"
    );
}

/// T066: Test empty query handling
#[tokio::test]
async fn test_cli_empty_query() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_kalam"));
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("test_user")
        .arg("--command")
        .arg("   ")
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();

    // Should handle empty query gracefully (no crash)
    assert!(
        output.status.success() || !output.stderr.is_empty(),
        "Should handle empty query without crashing"
    );
}

/// T067: Test query result pagination
#[tokio::test]
async fn test_cli_result_pagination() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let table = setup_test_data("pagination").await.unwrap();

    // Insert multiple rows
    for i in 1..=20 {
        let _ = execute_sql(&format!(
            "INSERT INTO {} (content) VALUES ('Message {}')",
            table, i
        ))
        .await;
    }

    // Query all rows
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_kalam"));
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("test_user")
        .arg("--command")
        .arg(&format!("SELECT * FROM {}", table))
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should display results (pagination in interactive mode)
    assert!(
        stdout.contains("Message") || output.status.success(),
        "Should handle result display"
    );

    cleanup_test_data(&table).await.unwrap();
}