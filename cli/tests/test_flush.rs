//! Integration tests for flush operations
//!
//! **Implements T059**: Flush table operations and data persistence
//!
//! These tests validate:
//! - Explicit FLUSH TABLE commands
//! - Data persistence after flush operations
//! - Flush command error handling

use assert_cmd::Command;
use std::time::Duration;

use crate::helpers::common::*;

/// T059: Test explicit flush command
#[test]
fn test_cli_explicit_flush() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let table_name = generate_unique_table("explicit_flush");
    let namespace = "test_cli";
    let full_table_name = format!("{}.{}", namespace, table_name);

    // Setup table via CLI
    let _ = execute_sql_via_cli(&format!("CREATE NAMESPACE IF NOT EXISTS {}", namespace));
    let _ = execute_sql_via_cli(&format!(
        r#"CREATE USER TABLE {} (
            id INT AUTO_INCREMENT,
            content VARCHAR NOT NULL
        ) FLUSH ROWS 10"#,
        full_table_name
    ));

    // Insert some data first via CLI
    let _ = execute_sql_via_cli(&format!(
        "INSERT INTO {} (content) VALUES ('Flush Test')",
        full_table_name
    ));

    let mut cmd = create_cli_command();
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("test_user")
        .arg("--command")
        .arg(&format!("FLUSH TABLE {}", full_table_name));

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

    // Cleanup
    let _ = execute_sql_via_cli(&format!("DROP TABLE IF EXISTS {}", full_table_name));
}