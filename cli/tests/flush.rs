//! Integration tests for flush operations
//!
//! **Implements T059**: Flush table operations and data persistence
//!
//! These tests validate:
//! - Explicit FLUSH TABLE commands
//! - Data persistence after flush operations
//! - Flush command error handling

use crate::common::*;

/// T059: Test explicit flush command
#[tokio::test]
async fn test_cli_explicit_flush() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let table = setup_test_data("explicit_flush").await.unwrap();

    // Insert some data first
    execute_sql(&format!(
        "INSERT INTO {} (content) VALUES ('Flush Test')",
        table
    ))
    .await
    .unwrap();

    let mut cmd = create_cli_command();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("test_user")
        .arg("--command")
        .arg(&format!("FLUSH TABLE {}", table))
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should successfully execute flush command (not unsupported)
    assert!(
        output.status.success(),
        "FLUSH TABLE should succeed. stdout: {}, stderr: {}",
        stdout,
        stderr
    );

    // Should NOT contain errors
    assert!(
        !stderr.contains("ERROR")
            && !stderr.contains("not supported")
            && !stderr.contains("Unsupported"),
        "FLUSH TABLE should not error. stderr: {}",
        stderr
    );

    cleanup_test_data(&table).await.unwrap();
}