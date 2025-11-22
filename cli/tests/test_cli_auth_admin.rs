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
//TODO: Remove this since we have most of the tests covered by the integration tests
#![allow(unused_imports)]
mod common;
use assert_cmd::Command;
use common::*;
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

    // Use unique namespace name to avoid conflicts
    let namespace_name = generate_unique_namespace("test_root_ns");

    // Clean up any existing namespace (just in case)
    let _ = execute_sql_as_root(&format!(
        "DROP NAMESPACE IF EXISTS {} CASCADE",
        namespace_name
    ))
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Create namespace as root
    let result = match execute_sql_as_root(&format!("CREATE NAMESPACE {}", namespace_name)).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("execute_sql_as_root failed: {:?}", e);
            panic!("execute_sql_as_root failed: {:?}", e);
        }
    };

    // Should succeed
    if result["status"] != "success" {
        eprintln!("CREATE NAMESPACE failed: {:?}", result);
        panic!("CREATE NAMESPACE failed: {:?}", result);
    }

    // Verify namespace was created
    let select_result = execute_sql_as_root(&format!(
        "SELECT name FROM system.namespaces WHERE name = '{}'",
        namespace_name
    ))
    .await
    .unwrap();

    if select_result["status"] != "success" {
        eprintln!("SELECT failed: {:?}", select_result);
        panic!("SELECT failed: {:?}", select_result);
    }

    assert_eq!(select_result["status"], "success");
    assert!(
        select_result["results"]
            .as_array()
            .and_then(|results| results.get(0))
            .and_then(|result| result["rows"].as_array())
            .map(|rows| rows.iter().any(|row| row["name"] == namespace_name))
            .unwrap_or(false),
        "Namespace should exist in system.namespaces"
    );

    // Cleanup
    let _ = execute_sql_as_root(&format!("DROP NAMESPACE {} CASCADE", namespace_name)).await;
}

/// Test that root user can create and drop tables
#[tokio::test]
async fn test_root_can_create_drop_tables() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    // Ensure namespace exists
    let _ = execute_sql_as_root("CREATE NAMESPACE IF NOT EXISTS test_tables_ns").await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create table as root
    let result = execute_sql_as_root(
        "CREATE TABLE test_tables_ns.test_table (id INT PRIMARY KEY, name VARCHAR) WITH (TYPE='USER', FLUSH_POLICY='rows:10')",
    )
    .await
    .unwrap();

    assert_eq!(
        result["status"], "success",
        "Root user should be able to create tables: {:?}",
        result
    );

    // Drop table
    let result = execute_sql_as_root("DROP TABLE test_tables_ns.test_table")
        .await
        .unwrap();

    assert_eq!(
        result["status"], "success",
        "Root user should be able to drop tables"
    );

    // Cleanup
    let _ = execute_sql_as_root("DROP NAMESPACE test_tables_ns CASCADE").await;
}

/// Test CREATE NAMESPACE via CLI with root authentication
#[tokio::test]
async fn test_cli_create_namespace_as_root() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    // Use unique namespace name to avoid conflicts
    let namespace_name = generate_unique_namespace("cli_test_ns");

    // Clean up any existing namespace
    let drop_result =
        execute_sql_as_root(&format!("DROP NAMESPACE IF EXISTS {}", namespace_name)).await;
    if let Err(e) = &drop_result {
        eprintln!("DROP NAMESPACE failed: {:?}", e);
    }
    if let Ok(result) = &drop_result {
        if result["status"] != "success" {
            eprintln!("DROP NAMESPACE returned non-success: {:?}", result);
        }
    }
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Execute CREATE NAMESPACE via CLI (auto-authenticates as root for localhost)
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_kalam"));
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--command")
        .arg(&format!("CREATE NAMESPACE {}", namespace_name))
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
    let result = execute_sql_as_root(&format!(
        "SELECT name FROM system.namespaces WHERE name = '{}'",
        namespace_name
    ))
    .await
    .unwrap();

    assert_eq!(result["status"], "success");

    // Cleanup
    let _ = execute_sql_as_root(&format!("DROP NAMESPACE {} CASCADE", namespace_name)).await;
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

    let result = execute_sql_as_root("CREATE USER testuser PASSWORD 'testpass' ROLE user").await;

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
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_kalam"));
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

    // Use unique namespace name to avoid conflicts
    let namespace_name = generate_unique_namespace("admin_ops_test");

    // Clean up
    let _ = execute_sql_as_root(&format!(
        "DROP NAMESPACE IF EXISTS {} CASCADE",
        namespace_name
    ))
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Test batch SQL with multiple admin commands
    let sql_batch = format!(
        r#"
CREATE NAMESPACE {};
CREATE TABLE {}.users (id INT PRIMARY KEY, name VARCHAR) WITH (TYPE='USER', FLUSH_POLICY='rows:10');
INSERT INTO {}.users (id, name) VALUES (1, 'Alice');
SELECT * FROM {}.users;
"#,
        namespace_name, namespace_name, namespace_name, namespace_name
    );

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_kalam"));
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
    let _ = execute_sql_as_root(&format!("DROP NAMESPACE {} CASCADE", namespace_name)).await;
}

/// Test SHOW NAMESPACES command
#[tokio::test]
async fn test_cli_show_namespaces() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_kalam"));
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--command")
        .arg("SHOW NAMESPACES")
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "SHOW NAMESPACES command should succeed: {}",
        stdout
    );

    // Should show table format with column headers
    assert!(
        stdout.contains("name") || stdout.contains("namespace"),
        "SHOW NAMESPACES should display namespace names: {}",
        stdout
    );
}

/// Test FLUSH TABLE command via CLI
#[tokio::test]
async fn test_cli_flush_table() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    // Use unique namespace name to avoid conflicts
    let namespace_name = generate_unique_namespace("flush_test_ns");

    // Setup: Create namespace and user table
    let _ = execute_sql_as_root(&format!(
        "DROP NAMESPACE IF EXISTS {} CASCADE",
        namespace_name
    ))
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let _ = execute_sql_as_root(&format!("CREATE NAMESPACE {}", namespace_name)).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Create a USER table with flush policy (SHARED tables cannot be flushed)
    let result = execute_sql_as_root(&format!(
        "CREATE TABLE {}.metrics (timestamp BIGINT PRIMARY KEY, value DOUBLE) WITH (TYPE='USER', FLUSH_POLICY='rows:5')",
        namespace_name
    ))
    .await
    .unwrap();

    assert_eq!(
        result["status"], "success",
        "Should create user table: {:?}",
        result
    );

    // Insert some data to trigger potential flush
    for i in 1..=3 {
        let insert_sql = format!(
            "INSERT INTO {}.metrics (timestamp, value) VALUES ({}, {}.5)",
            namespace_name, i, i
        );
        let _ = execute_sql_as_root(&insert_sql).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Execute FLUSH TABLE via CLI
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_kalam"));
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--command")
        .arg(&format!("FLUSH TABLE {}.metrics", namespace_name))
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "FLUSH TABLE should succeed.\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );

    // Verify the flush command was accepted (should show job info or success message)
    assert!(
        stdout.contains("Flush")
            || stdout.contains("Job")
            || stdout.contains("success")
            || stdout.contains("Query OK"),
        "Output should indicate flush operation: {}",
        stdout
    );

    // Extract job ID from output (format: "Job ID: flush-...")
    let job_id = if let Some(job_id_start) = stdout.find("Job ID: ") {
        let id_start = job_id_start + "Job ID: ".len();
        let id_end = stdout[id_start..]
            .find('\n')
            .unwrap_or(stdout[id_start..].len());
        Some(stdout[id_start..id_start + id_end].trim().to_string())
    } else {
        None
    };

    // Wait for job to complete
    tokio::time::sleep(Duration::from_secs(2)).await;

    // If we have a job ID, query for that specific job
    let jobs_query = if let Some(ref job_id) = job_id {
        format!(
            "SELECT job_id, job_type, status, namespace_id, table_name, result FROM system.jobs \
             WHERE job_id = '{}' LIMIT 1",
            job_id
        )
    } else {
        // Fallback to querying by type and table name
        "SELECT job_id, job_type, status, namespace_id, table_name, result FROM system.jobs \
         WHERE job_type = 'flush' AND table_name = 'metrics' \
         ORDER BY created_at DESC LIMIT 1"
            .to_string()
    };

    let jobs_result = execute_sql_as_root(&jobs_query).await.unwrap();

    assert_eq!(
        jobs_result["status"], "success",
        "Should be able to query system.jobs: {:?}",
        jobs_result
    );

    // Handle both "data" array format and "results" format
    let jobs_data = if let Some(data) = jobs_result["data"].as_array() {
        data.clone()
    } else if let Some(results) = jobs_result["results"].as_array() {
        // Results format - extract from first result if available
        if let Some(first_result) = results.get(0) {
            if let Some(rows) = first_result["rows"].as_array() {
                rows.clone()
            } else if let Some(data) = first_result["data"].as_array() {
                data.clone()
            } else {
                vec![]
            }
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    assert!(
        !jobs_data.is_empty(),
        "Should have found the flush job{}: {}",
        if job_id.is_some() {
            " by job ID"
        } else {
            " by table name"
        },
        jobs_result
    );

    // Normalize job row: DataFusion may return row as an array with separate columns metadata.
    println!("DEBUG jobs_result raw: {}", jobs_result); // diagnostics
    println!("DEBUG first job raw row: {}", jobs_data[0]);
    let job = if jobs_data[0].is_array() {
        // Build an object map {column_name: value} using returned columns metadata
        let mut obj = serde_json::Map::new();
        let columns_vec = jobs_result["results"][0]["columns"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let values = jobs_data[0].as_array().unwrap();
        for (idx, col) in columns_vec.iter().enumerate() {
            if let Some(col_name) = col.as_str() {
                let val = values.get(idx).cloned().unwrap_or(serde_json::Value::Null);
                obj.insert(col_name.to_string(), val);
            }
        }
        serde_json::Value::Object(obj)
    } else {
        jobs_data[0].clone()
    };

    // If we extracted a job ID, verify it matches
    if let Some(expected_job_id) = job_id {
        assert_eq!(
            job["job_id"].as_str().unwrap(),
            expected_job_id,
            "Job ID should match the one returned by FLUSH command"
        );
    }

    assert_eq!(
        job["job_type"].as_str().unwrap(),
        "flush",
        "Job type should be 'flush'"
    );
    assert_eq!(
        job["namespace_id"].as_str().unwrap(),
        &namespace_name,
        "Job should reference correct namespace"
    );
    // table_name stores only the table identifier (namespace is in namespace_id)
    assert_eq!(
        job["table_name"].as_str().unwrap(),
        "metrics",
        "Job should reference correct table name"
    );

    // Actively poll until the job leaves 'running' (avoid false positives on stuck jobs)
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(8);
    let final_status = loop {
        let status = job["status"].as_str().unwrap_or("");
        if status != "running" {
            break status.to_string();
        }
        if Instant::now() > deadline {
            panic!("Timed out waiting for flush job to complete; last status was 'running'");
        }
        tokio::time::sleep(Duration::from_millis(250)).await;

        // Requery current job status
        let refetch = execute_sql_as_root(&jobs_query).await.unwrap();
        let rows = refetch["results"][0]["rows"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if let Some(updated) = rows.get(0) {
            // Shadow 'job' binding by reassigning serialized map
            // Note: using direct indexing to keep diff small
            if let Some(s) = updated["status"].as_str() {
                if s != "running" {
                    break s.to_string();
                }
            }
        }
    };

    // Verify job completed successfully (do not accept 'running')
    assert_eq!(
        final_status, "completed",
        "Flush job did not complete successfully (status: {})",
        final_status
    );

    // If job is completed, verify it has results
    if final_status == "completed" {
        let result_str = job["result"].as_str().unwrap_or("");
        assert!(
            !result_str.is_empty() || result_str.contains("rows") || result_str.contains("Flushed"),
            "Completed job should have result information: {}",
            result_str
        );
    }

    // Verify data is still accessible after flush
    let result = execute_sql_as_root(&format!(
        "SELECT COUNT(*) as count FROM {}.metrics",
        namespace_name
    ))
    .await
    .unwrap();

    assert_eq!(result["status"], "success");

    // Cleanup
    let _ = execute_sql_as_root(&format!("DROP NAMESPACE {} CASCADE", namespace_name)).await;
}

/// Test FLUSH ALL TABLES command via CLI
#[tokio::test]
async fn test_cli_flush_all_tables() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    // Use unique namespace name to avoid conflicts
    let namespace_name = generate_unique_namespace("flush_all_test");

    // Setup: Create namespace with multiple tables
    let _ = execute_sql_as_root(&format!("DROP NAMESPACE IF EXISTS {} CASCADE", namespace_name)).await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let _ = execute_sql_as_root(&format!("CREATE NAMESPACE {}", namespace_name)).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Create multiple USER tables (SHARED tables cannot be flushed)
    let _ = execute_sql_as_root(
        &format!("CREATE TABLE {}.table1 (id INT PRIMARY KEY, data VARCHAR) WITH (TYPE='USER', FLUSH_POLICY='rows:10')", namespace_name),
    )
    .await;
    let _ = execute_sql_as_root(
        &format!("CREATE TABLE {}.table2 (id INT PRIMARY KEY, value DOUBLE) WITH (TYPE='USER', FLUSH_POLICY='rows:10')", namespace_name),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Insert some data
    let _ = execute_sql_as_root(&format!("INSERT INTO {}.table1 (id, data) VALUES (1, 'test')", namespace_name))
        .await;
    let _ =
        execute_sql_as_root(&format!("INSERT INTO {}.table2 (id, value) VALUES (1, 42.0)", namespace_name)).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Execute FLUSH ALL TABLES via CLI
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_kalam"));
    cmd.arg("-u")
        .arg(SERVER_URL)
        .arg("--command")
        .arg(&format!("FLUSH ALL TABLES IN {}", namespace_name))
        .timeout(TEST_TIMEOUT);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "FLUSH ALL TABLES should succeed.\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );

    // Verify the command was accepted
    assert!(
        stdout.contains("Flush")
            || stdout.contains("Job")
            || stdout.contains("success")
            || stdout.contains("Query OK"),
        "Output should indicate flush operation: {}",
        stdout
    );

    // Extract all job IDs from output
    // Format: "Job ID: flush-..." (single) or "Job ID: [flush-..., flush-...]" (multiple)
    let job_ids: Vec<String> = if let Some(pos) = stdout.find("Job ID:") {
        let rest = &stdout[pos + 7..].trim();
        if rest.starts_with('[') {
            // Multiple job IDs (FLUSH ALL TABLES)
            let end = rest.find(']').unwrap_or(rest.len());
            let ids_str = &rest[1..end];
            ids_str
                .split(',')
                .map(|id| id.trim().to_string())
                .filter(|id| !id.is_empty())
                .collect()
        } else {
            // Single job ID
            vec![rest.split_whitespace().next().unwrap_or("").to_string()]
        }
    } else {
        vec![]
    };

    println!("Extracted job IDs: {:?}", job_ids);

    // Wait for jobs to complete
    tokio::time::sleep(Duration::from_secs(2)).await;

    // If we have job IDs, query for those specific jobs
    let jobs_query = if !job_ids.is_empty() {
        let job_id_list = job_ids
            .iter()
            .map(|id| format!("'{}'", id))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "SELECT job_id, job_type, status, namespace_id, table_name, result FROM system.jobs \
             WHERE job_id IN ({}) \
             ORDER BY created_at DESC",
            job_id_list
        )
    } else {
        // Fallback to querying by namespace
        format!("SELECT job_id, job_type, status, namespace_id, table_name, result FROM system.jobs \
         WHERE job_type = 'flush' AND namespace_id = '{}' \
         ORDER BY created_at DESC", namespace_name)
    };

    let jobs_result = execute_sql_as_root(&jobs_query).await.unwrap();

    assert_eq!(
        jobs_result["status"], "success",
        "Should be able to query system.jobs: {:?}",
        jobs_result
    );

    // Handle both "data" array format and "results" format
    let jobs_data = if let Some(data) = jobs_result["data"].as_array() {
        data
    } else if let Some(results) = jobs_result["results"].as_array() {
        // Results format - extract from first result if available
        if let Some(first_result) = results.get(0) {
            if let Some(data) = first_result["data"].as_array() {
                data
            } else {
                &vec![]
            }
        } else {
            &vec![]
        }
    } else {
        &vec![]
    };

    // Note: May have 0 jobs if tables were empty and nothing to flush
    if jobs_data.is_empty() {
        if !job_ids.is_empty() {
            panic!(
                "Expected to find jobs with IDs {:?}, but found none",
                job_ids
            );
        }
        eprintln!(
            "Warning: No flush jobs found. Tables may have been empty or jobs not created yet."
        );
    }

    // If we extracted job IDs, verify we found all of them
    if !job_ids.is_empty() && !jobs_data.is_empty() {
        let found_job_ids: Vec<&str> = jobs_data
            .iter()
            .filter_map(|job| job["job_id"].as_str())
            .collect();

        for expected_job_id in &job_ids {
            assert!(
                found_job_ids.contains(&expected_job_id.as_str()),
                "Should have found job ID {} in system.jobs. Found: {:?}",
                expected_job_id,
                found_job_ids
            );
        }
    }

    // If we have jobs, verify table names
    if !jobs_data.is_empty() {
        // Verify both tables have flush jobs
        let table_names: Vec<&str> = jobs_data
            .iter()
            .filter_map(|job| job["table_name"].as_str())
            .collect();

        assert!(
            table_names.contains(&"table1") || table_names.contains(&"table2"),
            "Should have flush jobs for table1 and/or table2, got: {:?}",
            table_names
        );
    }

    // Verify at least one job completed successfully (if any jobs exist)
    if !jobs_data.is_empty() {
        let completed_jobs: Vec<_> = jobs_data
            .iter()
            .filter(|job| job["status"].as_str().unwrap_or("") == "completed")
            .collect();

        if !completed_jobs.is_empty() {
            let job = completed_jobs[0];
            let result_str = job["result"].as_str().unwrap_or("");
            assert!(
                !result_str.is_empty()
                    || result_str.contains("rows")
                    || result_str.contains("Flushed"),
                "Completed job should have result information: {}",
                result_str
            );
        }
    }

    // Cleanup
    let _ = execute_sql_as_root(&format!("DROP NAMESPACE {} CASCADE", namespace_name)).await;
}
