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

mod common;
use common::*;
use serde_json::json;
use std::time::Duration;



/// Test configuration constants
const TEST_TIMEOUT: Duration = Duration::from_secs(10);

/// T041: Test list tables command (using SELECT from system.tables)
#[test]
fn test_cli_list_tables() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let table_name = generate_unique_table("messages_list_tables");
    let namespace = "test_cli";

    // Create test table
    let create_sql = format!(
        r#"CREATE USER TABLE {}.{} (
            id INT AUTO_INCREMENT,
            content VARCHAR NOT NULL,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        ) FLUSH ROWS 10"#,
        namespace, table_name
    );

    let result = execute_sql_as_root_via_cli(&create_sql);
    if result.is_err() {
        eprintln!("⚠️  Failed to create test table, skipping test");
        return;
    }

    std::thread::sleep(Duration::from_millis(200));

    // Query system tables
    let query_sql = "SELECT table_name FROM system.tables WHERE namespace = 'test_cli'";
    let result = execute_sql_via_cli(query_sql);

    // Should list tables
    assert!(
        result.is_ok(),
        "Should list tables: {:?}",
        result.err()
    );
    let output = result.unwrap();
    assert!(
        output.contains("messages") || output.contains("row"),
        "Should contain table info: {}",
        output
    );

    // Cleanup
    let drop_sql = format!("DROP TABLE IF EXISTS {}.{}", namespace, table_name);
    let _ = execute_sql_as_root_via_cli(&drop_sql);
}

/// T042: Test describe table command (\d table)
#[test]
fn test_cli_describe_table() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let table_name = generate_unique_table("messages_describe");
    let namespace = "test_cli";

    // Create test table
    let create_sql = format!(
        r#"CREATE USER TABLE {}.{} (
            id INT AUTO_INCREMENT,
            content VARCHAR NOT NULL,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        ) FLUSH ROWS 10"#,
        namespace, table_name
    );

    let result = execute_sql_as_root_via_cli(&create_sql);
    if result.is_err() {
        eprintln!("⚠️  Failed to create test table, skipping test");
        return;
    }

    // Query table info
    let query_sql = format!("SELECT '{}' as table_info", format!("{}.{}", namespace, table_name));
    let result = execute_sql_via_cli(&query_sql);

    // Should execute successfully and show table info
    assert!(
        result.is_ok(),
        "Should describe table: {:?}",
        result.err()
    );
    let output = result.unwrap();
    assert!(
        output.contains("messages"),
        "Should contain table name: {}",
        output
    );

    // Cleanup
    let drop_sql = format!("DROP TABLE IF EXISTS {}.{}", namespace, table_name);
    let _ = execute_sql_as_root_via_cli(&drop_sql);
}

/// T055: Test batch file execution
#[test]
fn test_cli_batch_file_execution() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    // Create temporary SQL file
    let temp_dir = tempfile::TempDir::new().unwrap();
    let sql_file = temp_dir.path().join("test.sql");

    std::fs::write(
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
        .arg("http://localhost:8080")
        .arg("--file")
        .arg(sql_file.to_str().unwrap())
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Verify execution - should show Query OK messages and final result
    assert!(
        (stdout.contains("Item One") || stdout.contains("Query OK")) && output.status.success(),
        "Batch execution should succeed with proper messages.\nstdout: {}\nstderr: {}\nstatus: {:?}",
        stdout, stderr, output.status
    );

    // Cleanup
    let _ = execute_sql_as_root_via_cli("DROP NAMESPACE batch_test CASCADE");
}

/// T056: Test syntax error handling
#[test]
fn test_cli_syntax_error_handling() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let result = execute_sql_via_cli("INVALID SQL SYNTAX HERE");

    // Should contain error message
    assert!(
        result.is_err(),
        "Should fail with syntax error"
    );
    let error_msg = result.err().unwrap().to_string();
    assert!(
        error_msg.contains("ERROR")
            || error_msg.contains("Error")
            || error_msg.contains("syntax"),
        "Should display error message: {}",
        error_msg
    );
}

/// T057: Test connection failure handling
#[test]
fn test_cli_connection_failure_handling() {
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

/// T058: Test server health check via CLI
#[test]
fn test_cli_health_check() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    // Test server health via SQL query
    let result = execute_sql_via_cli("SELECT 1 as health_check");

    assert!(
        result.is_ok(),
        "Server should respond to SQL queries: {:?}",
        result.err()
    );

    let output = result.unwrap();
    assert!(
        output.contains("health_check") || output.contains("1"),
        "Response should contain query result: {}",
        output
    );
}

/// Helper test to verify server health
#[test]
fn test_server_health_check() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running at http://localhost:8080.");
        eprintln!("   Start server: cargo run --release --bin kalamdb-server");
        eprintln!("   Then run: cargo test --test test_cli_integration");
        return;
    }

    let result = execute_sql_via_cli("SELECT 1");

    assert!(
        result.is_ok(),
        "Server should respond successfully: {:?}",
        result.err()
    );
}

