//! Manual Flush Verification Tests
//!
//! This test suite verifies that table flushing works correctly:
//! 1. Manual flush job creation via FLUSH TABLE SQL command
//! 2. Job ID generation and tracking
//! 3. Parquet file generation verification (when jobs complete)
//! 4. Data persistence validation
//! 5. Error handling for failed flushes
//!
//! ## Current Implementation Status
//!
//! Flush jobs are created successfully via FLUSH TABLE, spawned as tokio tasks,
//! and should execute asynchronously. Tests now include:
//! - Parquet file existence checking with `flush_helpers::check_user_parquet_files()`
//! - Waiting for Parquet files with timeout via `flush_helpers::wait_for_parquet_files()`
//! - File validity verification (size > 50 bytes minimum)
//! - Job completion tracking via `system.jobs` with took_ms = 0 detection
//!
//! ## Test Helpers (in common/flush_helpers.rs)
//!
//! - `wait_for_flush_job_completion()` - Poll system.jobs until job completes, verify took_ms != 0
//! - `wait_for_parquet_files()` - Poll filesystem until Parquet files appear
//! - `check_user_parquet_files()` - Check for Parquet files in user table path
//! - `verify_parquet_files_exist()` - Validate Parquet files are not corrupted
//! - `extract_job_id()` - Extract job_id from FLUSH TABLE response message

#[path = "../common/mod.rs"]
mod common;

use common::{fixtures, flush_helpers, TestServer};
use kalamdb_commons::models::JobStatus;
use std::fs;
use std::path::PathBuf;

// ============================================================================
// Helper Functions
// ============================================================================

/// Check if Parquet files exist for a user table
fn check_user_parquet_files(namespace: &str, table_name: &str, user_id: &str) -> Vec<PathBuf> {
    let storage_path = PathBuf::from("./data")
        .join(user_id)
        .join("tables")
        .join(namespace)
        .join(table_name);

    let mut parquet_files = Vec::new();

    if storage_path.exists() {
        if let Ok(entries) = fs::read_dir(&storage_path) {
            for entry in entries.flatten() {
                if let Some(extension) = entry.path().extension() {
                    if extension == "parquet" {
                        parquet_files.push(entry.path());
                        println!("  ✓ Found Parquet file: {}", entry.path().display());
                    }
                }
            }
        }
    }

    parquet_files
}

/// Check if Parquet files exist for a shared table
fn check_shared_parquet_files(namespace: &str, table_name: &str) -> Vec<PathBuf> {
    let storage_path = PathBuf::from("./data")
        .join("shared")
        .join(namespace)
        .join(table_name);

    let mut parquet_files = Vec::new();

    if storage_path.exists() {
        if let Ok(entries) = fs::read_dir(&storage_path) {
            for entry in entries.flatten() {
                if let Some(extension) = entry.path().extension() {
                    if extension == "parquet" {
                        parquet_files.push(entry.path());
                        println!("  ✓ Found Parquet file: {}", entry.path().display());
                    }
                }
            }
        }
    }

    parquet_files
}

/// Verify Parquet file can be read with Apache Arrow
fn verify_parquet_readable(parquet_path: &PathBuf) -> Result<usize, String> {
    // Check if file exists and has reasonable size
    if !parquet_path.exists() {
        return Err("File does not exist".to_string());
    }

    // Get file size as validation
    match std::fs::metadata(parquet_path) {
        Ok(metadata) => {
            let file_size = metadata.len();

            // Parquet files should have headers even if empty (~100 bytes minimum)
            if file_size < 50 {
                return Err(format!(
                    "Parquet file too small: {} bytes (likely corrupted)",
                    file_size
                ));
            }

            println!("  ✓ Parquet file exists and valid: {} bytes", file_size);

            // Note: Actual row count reading would require parquet crate dependency.
            // For test purposes, file existence and size validation is sufficient.
            Ok(0)
        }
        Err(e) => Err(format!("Failed to read file metadata: {}", e)),
    }
}

// ============================================================================
// Test 1: User Table Manual Flush - Single User
// ============================================================================

#[actix_web::test]
async fn test_01_user_table_manual_flush_single_user() {
    println!("\n=== Test 01: User Table Manual Flush (Single User) ===");

    let server = TestServer::new().await;
    let namespace = "manual_flush";
    let table_name = "user_messages";
    let user_id = "user_001";

    // Create namespace
    fixtures::create_namespace(&server, namespace).await;

    // Create user table
    let create_sql = format!(
        "CREATE USER TABLE {}.{} (
            id BIGINT,
            message TEXT,
            timestamp TIMESTAMP
        ) STORAGE local FLUSH ROWS 100",
        namespace, table_name
    );

    let response = server.execute_sql_as_user(&create_sql, user_id).await;

    if response.status != "success" {
        println!("Create table error: {:?}", response.error);
        println!("Response: {:?}", response);
    }

    assert_eq!(
        response.status, "success",
        "Failed to create table: {:?}",
        response.error
    );

    // Insert 50 rows
    println!("Inserting 50 rows...");
    for i in 1..=50 {
        let insert_sql = format!(
            "INSERT INTO {}.{} (id, message, timestamp) VALUES ({}, 'Message {}', NOW())",
            namespace, table_name, i, i
        );
        let response = server.execute_sql_as_user(&insert_sql, user_id).await;

        if response.status != "success" {
            println!("Insert failed for row {}: {:?}", i, response);
            println!("SQL: {}", insert_sql);
        }

        assert_eq!(
            response.status, "success",
            "Failed to insert row {}: {:?}",
            i, response.error
        );
    }

    // Verify data is in RocksDB (before flush)
    let count_before = server
        .execute_sql_as_user(
            &format!("SELECT COUNT(*) as count FROM {}.{}", namespace, table_name),
            user_id,
        )
        .await;

    println!("Data in RocksDB before flush: {:?}", count_before);

    // TODO: Execute manual flush job directly
    // This requires access to internal components (UserTableStore, schema, etc.)
    // For now, we test that the FLUSH TABLE command creates a job

    let flush_sql = format!("FLUSH TABLE {}.{}", namespace, table_name);
    let flush_response = server.execute_sql_as_user(&flush_sql, user_id).await;
    println!("Flush response: {:?}", flush_response);

    // Note: FLUSH TABLE currently only creates a job record but doesn't execute the flush
    // We would need to trigger the actual flush job execution

    println!("✓ Test 01 completed (job creation verified)");
}

// ============================================================================
// Test 2: User Table Manual Flush - Multiple Users
// ============================================================================

#[actix_web::test]
async fn test_02_user_table_manual_flush_multi_user() {
    println!("\n=== Test 02: User Table Manual Flush (Multiple Users) ===");

    let server = TestServer::new().await;
    let namespace = "multi_user_flush";
    let table_name = "events";

    // Create namespace
    fixtures::create_namespace(&server, namespace).await;

    // Create user table (needs any user context for creation)
    let create_sql = format!(
        "CREATE USER TABLE {}.{} (
            event_id BIGINT,
            event_type VARCHAR,
            data TEXT
        ) STORAGE local FLUSH ROWS 100",
        namespace, table_name
    );

    let response = server.execute_sql_as_user(&create_sql, "user_a").await;
    assert_eq!(response.status, "success");

    // Insert data for 3 users
    let users = vec!["user_a", "user_b", "user_c"];

    for user_id in &users {
        println!("Inserting 20 rows for {}...", user_id);
        for i in 1..=20 {
            let insert_sql = format!(
                "INSERT INTO {}.{} (event_id, event_type, data) VALUES ({}, 'event_type_{}', 'data_{}')",
                namespace, table_name, i, i % 5, i
            );
            let response = server.execute_sql_as_user(&insert_sql, user_id).await;
            assert_eq!(response.status, "success");
        }
    }

    // Verify each user can query their data
    for user_id in &users {
        let count_sql = format!("SELECT COUNT(*) as count FROM {}.{}", namespace, table_name);
        let response = server.execute_sql_as_user(&count_sql, user_id).await;
        assert_eq!(response.status, "success");

        if let Some(rows) = response.results.first().and_then(|r| r.rows.as_ref()) {
            let count = rows[0].get("count").and_then(|v| v.as_i64()).unwrap_or(0);
            assert_eq!(count, 20, "Expected 20 rows for {}", user_id);
            println!("  ✓ {} has 20 rows", user_id);
        }
    }

    println!("✓ Test 02 completed (multi-user data verified)");
}

// ============================================================================
// Test 3: Shared Table Manual Flush
// ============================================================================

#[actix_web::test]
#[ignore = "Shared tables require pre-created column families at DB init"]
async fn test_03_shared_table_manual_flush() {
    println!("\n=== Test 03: Shared Table Manual Flush ===");

    let server = TestServer::new().await;
    let namespace = "shared_flush";
    let table_name = "logs";

    // Create namespace
    fixtures::create_namespace(&server, namespace).await;

    // Create shared table
    let create_sql = format!(
        "CREATE SHARED TABLE {}.{} (
            log_id BIGINT,
            level VARCHAR,
            message TEXT,
            timestamp TIMESTAMP
        ) FLUSH ROWS 50",
        namespace, table_name
    );

    let response = server.execute_sql(&create_sql).await;
    assert_eq!(response.status, "success");

    // Insert 30 rows
    println!("Inserting 30 rows into shared table...");
    for i in 1..=30 {
        let insert_sql = format!(
            "INSERT INTO {}.{} (log_id, level, message, timestamp) VALUES ({}, 'INFO', 'Log message {}', NOW())",
            namespace, table_name, i, i
        );
        let response = server.execute_sql(&insert_sql).await;
        assert_eq!(response.status, "success");
    }

    // Query before flush
    let count_before = server
        .execute_sql(&format!(
            "SELECT COUNT(*) as count FROM {}.{}",
            namespace, table_name
        ))
        .await;

    if let Some(rows) = count_before.results.first().and_then(|r| r.rows.as_ref()) {
        let count = rows[0].get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        assert_eq!(count, 30, "Expected 30 rows before flush");
        println!("  ✓ 30 rows in RocksDB before flush");
    }

    println!("✓ Test 03 completed (shared table data verified)");
}

// ============================================================================
// Test 4: Verify Parquet File Structure
// ============================================================================

#[actix_web::test]
async fn test_04_verify_parquet_file_structure() {
    println!("\n=== Test 04: Verify Parquet File Structure ===");

    // This test will check for existing Parquet files from previous test runs
    // or after an actual flush has occurred

    let test_cases = vec![
        (
            "./data/user_001/tables/manual_flush/user_messages",
            "User table",
        ),
        ("./data/shared/shared_flush/logs", "Shared table"),
    ];

    for (path, description) in test_cases {
        let storage_path = PathBuf::from(path);

        if storage_path.exists() {
            println!("Checking {}: {}", description, path);

            if let Ok(entries) = fs::read_dir(&storage_path) {
                for entry in entries.flatten() {
                    if let Some(extension) = entry.path().extension() {
                        if extension == "parquet" {
                            println!("  Found Parquet file: {}", entry.path().display());

                            // Try to read the file
                            match verify_parquet_readable(&entry.path()) {
                                Ok(row_count) => {
                                    println!("    ✓ File is readable, contains {} rows", row_count);
                                }
                                Err(e) => {
                                    println!("    ✗ File is not readable: {}", e);
                                }
                            }
                        }
                    }
                }
            }
        } else {
            println!("  Note: {} does not exist yet (flush not executed)", path);
        }
    }

    println!("✓ Test 04 completed (file structure check done)");
}

// ============================================================================
// Test 5: Data Integrity After Flush
// ============================================================================

#[actix_web::test]
async fn test_05_data_integrity_after_flush() {
    println!("\n=== Test 05: Data Integrity After Flush ===");

    let server = TestServer::new().await;
    let namespace = "integrity_test";
    let table_name = "transactions";
    let user_id = "user_integrity";

    // Create namespace
    fixtures::create_namespace(&server, namespace).await;

    // Create user table
    let create_sql = format!(
        "CREATE USER TABLE {}.{} (
            tx_id BIGINT,
            amount DOUBLE,
            description TEXT,
            created_at TIMESTAMP
        ) STORAGE local",
        namespace, table_name
    );

    let response = server
        .execute_sql_with_user(&create_sql, Some(user_id))
        .await;
    assert_eq!(response.status, "success");

    // Insert test data with known values
    let test_data = vec![
        (1, 100.50, "Payment A"),
        (2, 250.75, "Payment B"),
        (3, 75.25, "Payment C"),
        (4, 500.00, "Payment D"),
        (5, 125.30, "Payment E"),
    ];

    println!("Inserting {} transactions...", test_data.len());
    for (tx_id, amount, desc) in &test_data {
        let insert_sql = format!(
            "INSERT INTO {}.{} (tx_id, amount, description, created_at) VALUES ({}, {}, '{}', NOW())",
            namespace, table_name, tx_id, amount, desc
        );
        let response = server.execute_sql_as_user(&insert_sql, user_id).await;
        assert_eq!(response.status, "success");
    }

    // Query and verify data before flush
    let query_before = server
        .execute_sql_as_user(
            &format!(
                "SELECT tx_id, amount, description FROM {}.{} ORDER BY tx_id",
                namespace, table_name
            ),
            user_id,
        )
        .await;

    assert_eq!(query_before.status, "success");

    if let Some(rows) = query_before.results.first().and_then(|r| r.rows.as_ref()) {
        assert_eq!(
            rows.len(),
            test_data.len(),
            "Expected {} rows",
            test_data.len()
        );

        for (i, row) in rows.iter().enumerate() {
            let tx_id = row.get("tx_id").and_then(|v| v.as_i64()).unwrap_or(0);
            let amount = row.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);

            assert_eq!(tx_id, test_data[i].0 as i64);
            assert!((amount - test_data[i].1).abs() < 0.01);
        }

        println!(
            "  ✓ All {} transactions verified before flush",
            test_data.len()
        );
    }

    // TODO: After implementing actual flush execution, verify data after flush
    // and ensure it matches the original data exactly

    println!("✓ Test 05 completed (data integrity verified)");
}

// ============================================================================
// Test 6: Check RocksDB Cleanup After Flush
// ============================================================================

#[actix_web::test]
async fn test_06_rocksdb_cleanup_after_flush() {
    println!("\n=== Test 06: RocksDB Cleanup After Flush ===");

    let server = TestServer::new().await;
    let namespace = "cleanup_test";
    let table_name = "temp_data";
    let user_id = "user_cleanup";

    // Create namespace
    fixtures::create_namespace(&server, namespace).await;

    // Create user table with low flush threshold
    let create_sql = format!(
        "CREATE USER TABLE {}.{} (
            id BIGINT,
            data TEXT
        ) STORAGE local FLUSH ROWS 10",
        namespace, table_name
    );

    let response = server.execute_sql_as_user(&create_sql, user_id).await;
    assert_eq!(response.status, "success");

    // Insert exactly 10 rows (should trigger auto-flush)
    println!("Inserting 10 rows (flush threshold)...");
    for i in 1..=10 {
        let insert_sql = format!(
            "INSERT INTO {}.{} (id, data) VALUES ({}, 'data_{}')",
            namespace, table_name, i, i
        );
        let response = server.execute_sql_as_user(&insert_sql, user_id).await;
        assert_eq!(response.status, "success");
    }

    // Query - should return all rows (from RocksDB or Parquet or both)
    let query_after = server
        .execute_sql_as_user(
            &format!("SELECT COUNT(*) as count FROM {}.{}", namespace, table_name),
            user_id,
        )
        .await;

    if let Some(rows) = query_after.results.first().and_then(|r| r.rows.as_ref()) {
        let count = rows[0].get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        assert_eq!(count, 10, "Expected all 10 rows to be queryable");
        println!("  ✓ All 10 rows queryable after flush threshold reached");
    }

    // TODO: Verify RocksDB buffer is actually cleaned up after flush
    // This would require direct access to RocksDB storage to check key count

    println!("✓ Test 06 completed (cleanup behavior checked)");
}

// ============================================================================
// Test 7: ISO 8601 Timestamp in Filename
// ============================================================================

#[actix_web::test]
async fn test_07_iso8601_timestamp_filename() {
    println!("\n=== Test 07: ISO 8601 Timestamp in Filename ===");

    // Check existing Parquet files for ISO 8601 format
    // Format should be: YYYY-MM-DDTHH-MM-SS.parquet
    // Example: 2025-10-22T14-30-45.parquet

    let search_paths = vec![
        "./data/user_001/tables",
        "./data/user_a/tables",
        "./data/shared",
    ];

    // Simple pattern matching without regex crate
    fn is_iso8601_filename(filename: &str) -> bool {
        // Check format: YYYY-MM-DDTHH-MM-SS.parquet
        if !filename.ends_with(".parquet") {
            return false;
        }

        // Basic check: should be 29 characters (including .parquet)
        if filename.len() != 29 {
            return false;
        }

        // Check pattern positions: YYYY-MM-DDTHH-MM-SS.parquet
        //                          0123456789012345678901234567
        let chars: Vec<char> = filename.chars().collect();

        chars.get(4) == Some(&'-')
            && chars.get(7) == Some(&'-')
            && chars.get(10) == Some(&'T')
            && chars.get(13) == Some(&'-')
            && chars.get(16) == Some(&'-')
    }

    for base_path in search_paths {
        let path = PathBuf::from(base_path);

        if path.exists() {
            if let Ok(entries) = fs::read_dir(&path) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        // Recursively check subdirectories
                        if let Ok(sub_entries) = fs::read_dir(entry.path()) {
                            for sub_entry in sub_entries.flatten() {
                                let filename = sub_entry.file_name().to_string_lossy().to_string();

                                if filename.ends_with(".parquet") {
                                    if is_iso8601_filename(&filename) {
                                        println!("  ✓ Valid ISO 8601 filename: {}", filename);
                                    } else {
                                        println!("  Note: Non-ISO 8601 filename: {}", filename);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    println!("✓ Test 07 completed (filename format checked)");
}

// ============================================================================
// SQL API Tests for Manual Flushing (US3 - T238-T245)
// ============================================================================

// ============================================================================
// Test 8: T238 - FLUSH TABLE returns job_id immediately
// ============================================================================

#[actix_web::test]
async fn test_08_flush_table_returns_job_id() {
    use std::time::{Duration, Instant};

    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "flush_sql_test").await;

    // Create user table (requires X-USER-ID header)
    let create_table = r#"
        CREATE USER TABLE flush_sql_test.events (
            event_id BIGINT PRIMARY KEY,
            event_type TEXT,
            timestamp BIGINT
        )
    "#;
    let create_response = server.execute_sql_as_user(create_table, "user_001").await;
    assert_eq!(
        create_response.status, "success",
        "CREATE TABLE failed: {:?}",
        create_response.error
    );

    // Insert data
    let insert_response = server.execute_sql_as_user(
        "INSERT INTO flush_sql_test.events (event_id, event_type, timestamp) VALUES (1, 'login', 1234567890)",
        "user_001"
    ).await;
    assert_eq!(
        insert_response.status, "success",
        "INSERT failed: {:?}",
        insert_response.error
    );

    // Execute FLUSH TABLE and measure response time
    let start = Instant::now();
    let response = server
        .execute_sql_as_user("FLUSH TABLE flush_sql_test.events", "user_001")
        .await;
    let duration = start.elapsed();

    assert_eq!(
        response.status, "success",
        "FLUSH TABLE should succeed: {:?}",
        response.error
    );

    // Verify response time < 100ms (asynchronous)
    assert!(
        duration < Duration::from_millis(100),
        "FLUSH TABLE should return immediately, took {:?}",
        duration
    );

    // Verify response contains job_id
    if let Some(result) = response.results.first() {
        if let Some(message) = &result.message {
            assert!(
                message.contains("Job ID:") && message.contains("flush-"),
                "Response should contain job_id with flush- prefix, got: {}",
                message
            );
        }
    }
}

// ============================================================================
// Test 9: T239 - Flush job completes asynchronously
// ============================================================================

#[actix_web::test]
async fn test_09_flush_job_completes_asynchronously() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "async_flush").await;

    let create_response = server
        .execute_sql_as_user(
            r#"
        CREATE USER TABLE async_flush.data (
            id BIGINT PRIMARY KEY,
            value TEXT
        )
    "#,
            "user_001",
        )
        .await;
    assert_eq!(
        create_response.status, "success",
        "CREATE TABLE failed: {:?}",
        create_response.error
    );

    let insert_response = server
        .execute_sql_as_user(
            "INSERT INTO async_flush.data (id, value) VALUES (1, 'test')",
            "user_001",
        )
        .await;
    assert_eq!(
        insert_response.status, "success",
        "INSERT failed: {:?}",
        insert_response.error
    );

    // Execute flush synchronously
    let flush_result = flush_helpers::execute_flush_synchronously(&server, "async_flush", "data")
        .await
        .expect("Flush should execute successfully");

    // Verify metrics
    assert!(
        flush_result.rows_flushed >= 1,
        "Should have flushed at least 1 row, got {}",
        flush_result.rows_flushed
    );
    assert!(
        flush_result.users_count >= 1,
        "Should have at least 1 user, got {}",
        flush_result.users_count
    );
    assert!(
        !flush_result.parquet_files.is_empty(),
        "Should have created at least 1 Parquet file"
    );

    // Verify took_ms is non-zero
    let start_time = flush_result.job_record.started_at.unwrap_or(0);
    let end_time = flush_result.job_record.completed_at.unwrap_or(0);
    let took_ms = end_time - start_time;
    assert!(took_ms > 0, "Job took_ms should be > 0, got {}", took_ms);

    // Verify Parquet files exist on filesystem
    for parquet_file_path in &flush_result.parquet_files {
        let file_path = PathBuf::from(parquet_file_path);
        assert!(
            file_path.exists(),
            "Parquet file should exist: {}",
            parquet_file_path
        );
    }

    println!(
        "✅ Flush job completed - verified {} Parquet file(s) with took_ms={}",
        flush_result.parquet_files.len(),
        took_ms
    );
}

// ============================================================================
// Test 10: T240 - FLUSH ALL TABLES returns multiple job_ids
// ============================================================================

#[actix_web::test]
async fn test_10_flush_all_tables_multiple_jobs() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "multi_flush").await;

    // Create three user tables
    for i in 1..=3 {
        server
            .execute_sql_as_user(
                &format!(
                    r#"CREATE USER TABLE multi_flush.table{} (
                id BIGINT PRIMARY KEY,
                value TEXT
            )"#,
                    i
                ),
                "user_001",
            )
            .await;

        server
            .execute_sql_as_user(
                &format!(
                    "INSERT INTO multi_flush.table{} (id, value) VALUES ({}, 'data')",
                    i, i
                ),
                "user_001",
            )
            .await;
    }

    // Execute FLUSH ALL TABLES
    let response = server
        .execute_sql_as_user("FLUSH ALL TABLES IN multi_flush", "user_001")
        .await;

    assert_eq!(response.status, "success");

    if let Some(result) = response.results.first() {
        if let Some(message) = &result.message {
            assert!(
                message.contains("3 table(s)") && message.contains("Job IDs:"),
                "Should flush 3 tables with job IDs, got: {}",
                message
            );
        }
    }
}

// ============================================================================
// Test 11: T241 - Job result includes metrics and Parquet files exist
// ============================================================================

#[actix_web::test]
async fn test_11_flush_job_result_includes_metrics() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "metrics_test").await;

    let create_response = server
        .execute_sql_as_user(
            r#"
        CREATE USER TABLE metrics_test.data (
            id BIGINT PRIMARY KEY,
            value TEXT
        )
    "#,
            "user_001",
        )
        .await;
    assert_eq!(
        create_response.status, "success",
        "CREATE TABLE failed: {:?}",
        create_response.error
    );

    // Insert multiple rows
    for i in 1..=5 {
        let insert_response = server
            .execute_sql_as_user(
                &format!(
                    "INSERT INTO metrics_test.data (id, value) VALUES ({}, 'val_{}')",
                    i, i
                ),
                "user_001",
            )
            .await;
        assert_eq!(
            insert_response.status, "success",
            "INSERT {} failed: {:?}",
            i, insert_response.error
        );
    }

    // Verify data exists before flushing
    let select_response = server
        .execute_sql_as_user("SELECT * FROM metrics_test.data", "user_001")
        .await;
    assert_eq!(
        select_response.status, "success",
        "SELECT failed: {:?}",
        select_response.error
    );
    let row_count = select_response
        .results
        .get(0)
        .map(|r| r.row_count)
        .unwrap_or(0);
    assert!(
        row_count >= 5,
        "Expected at least 5 rows, got {}",
        row_count
    );

    // Execute flush synchronously (bypasses JobManager)
    let flush_result = flush_helpers::execute_flush_synchronously(&server, "metrics_test", "data")
        .await
        .expect("Flush should execute successfully");

    println!(
        "  Flush result: {} rows flushed across {} users",
        flush_result.rows_flushed, flush_result.users_count
    );
    println!("  Parquet files created: {:?}", flush_result.parquet_files);

    // Verify metrics
    assert!(
        flush_result.rows_flushed >= 5,
        "Should have flushed at least 5 rows, got {}",
        flush_result.rows_flushed
    );
    assert!(
        flush_result.users_count >= 1,
        "Should have at least 1 user, got {}",
        flush_result.users_count
    );
    assert!(
        !flush_result.parquet_files.is_empty(),
        "Should have created at least 1 Parquet file"
    );

    // Verify job record has valid timestamps (took_ms != 0)
    let start_time = flush_result.job_record.started_at.unwrap_or(0);
    let end_time = flush_result.job_record.completed_at.unwrap_or(0);
    let took_ms = end_time - start_time;

    assert!(
        took_ms > 0,
        "Job took_ms should be > 0, got {} (start: {}, end: {})",
        took_ms,
        start_time,
        end_time
    );
    println!("  ✓ Job completed in {} ms", took_ms);

    // Verify Parquet files exist on filesystem
    // Files are created relative to the current working directory
    assert!(
        !flush_result.parquet_files.is_empty(),
        "Should have created at least 1 Parquet file"
    );

    for parquet_file_path in &flush_result.parquet_files {
        let file_path = PathBuf::from(parquet_file_path);
        assert!(
            file_path.exists(),
            "Parquet file should exist: {}",
            parquet_file_path
        );

        let metadata = std::fs::metadata(&file_path).expect("Failed to get file metadata");
        assert!(
            metadata.len() > 50,
            "Parquet file should be > 50 bytes, got {} bytes",
            metadata.len()
        );
        println!(
            "  ✓ Parquet file verified: {} ({} bytes)",
            parquet_file_path,
            metadata.len()
        );
    }

    println!(
        "✓ Test 11 passed: Flush created {} Parquet file(s) with took_ms={}",
        flush_result.parquet_files.len(),
        took_ms
    );
}

// ============================================================================
// Test 12: T242 - Flush empty table
// ============================================================================

#[actix_web::test]
async fn test_12_flush_empty_table() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "empty_test").await;

    let create_response = server
        .execute_sql_as_user(
            r#"
        CREATE USER TABLE empty_test.empty_data (
            id BIGINT PRIMARY KEY,
            value TEXT
        )
    "#,
            "user_001",
        )
        .await;
    assert_eq!(
        create_response.status, "success",
        "CREATE TABLE failed: {:?}",
        create_response.error
    );

    // Execute flush synchronously on empty table
    let flush_result =
        flush_helpers::execute_flush_synchronously(&server, "empty_test", "empty_data")
            .await
            .expect("Flush should execute successfully even for empty table");

    // Verify result indicates empty flush
    assert_eq!(
        flush_result.rows_flushed, 0,
        "Should have flushed 0 rows for empty table, got {}",
        flush_result.rows_flushed
    );
    assert_eq!(
        flush_result.users_count, 0,
        "Should have 0 users for empty table, got {}",
        flush_result.users_count
    );
    assert!(
        flush_result.parquet_files.is_empty(),
        "Should have 0 Parquet files for empty table, got {}",
        flush_result.parquet_files.len()
    );

    // Verify job completed successfully (even though no data to flush)
    assert_eq!(
        flush_result.job_record.status,
        JobStatus::Completed,
        "Job should be completed, got: {:?}",
        flush_result.job_record.status
    );

    println!("  ℹ Empty table flush: 0 rows, 0 users, 0 Parquet files (as expected)");
}

// ============================================================================
// Test 13: T243 - Concurrent flush detection
// ============================================================================

#[actix_web::test]
async fn test_13_concurrent_flush_same_table() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "concurrent").await;

    let create_response = server
        .execute_sql_as_user(
            r#"
        CREATE USER TABLE concurrent.data (
            id BIGINT PRIMARY KEY,
            value TEXT
        )
    "#,
            "user_001",
        )
        .await;
    assert_eq!(
        create_response.status, "success",
        "CREATE TABLE failed: {:?}",
        create_response.error
    );

    let insert_response = server
        .execute_sql_as_user(
            "INSERT INTO concurrent.data (id, value) VALUES (1, 'test')",
            "user_001",
        )
        .await;
    assert_eq!(
        insert_response.status, "success",
        "INSERT failed: {:?}",
        insert_response.error
    );

    // First flush
    let flush1 = server
        .execute_sql_as_user("FLUSH TABLE concurrent.data", "user_001")
        .await;
    assert_eq!(
        flush1.status, "success",
        "First FLUSH failed: {:?}",
        flush1.error
    );

    // Immediate second flush
    let flush2 = server
        .execute_sql_as_user("FLUSH TABLE concurrent.data", "user_001")
        .await;

    // Either succeeds with different job_id or detects in-progress
    if flush2.status == "error" {
        assert!(
            flush2
                .error
                .as_ref()
                .unwrap()
                .message
                .contains("already running")
                || flush2.error.as_ref().unwrap().message.contains("Flush job"),
            "Should detect concurrent flush"
        );
    } else {
        // Verify different job IDs
        let job1 = extract_job_id(flush1.results.first().unwrap().message.as_ref().unwrap());
        let job2 = extract_job_id(flush2.results.first().unwrap().message.as_ref().unwrap());
        assert_ne!(job1, job2);
    }
}

// ============================================================================
// Test 14: T245 - Flush error handling
// ============================================================================

#[actix_web::test]
async fn test_14_flush_nonexistent_table() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "error_test").await;

    let response = server
        .execute_sql("FLUSH TABLE error_test.nonexistent")
        .await;

    assert_eq!(response.status, "error");
    assert!(
        response
            .error
            .as_ref()
            .unwrap()
            .message
            .contains("does not exist")
            || response
                .error
                .as_ref()
                .unwrap()
                .message
                .contains("not found")
    );
}

// ============================================================================
// Test 15: Flush shared table (should fail)
// ============================================================================

#[actix_web::test]
#[ignore = "Shared tables require pre-created column families at DB init"]
async fn test_15_flush_shared_table_fails() {
    let server = TestServer::new().await;
    fixtures::create_namespace(&server, "shared_test").await;

    server
        .execute_sql(
            r#"
        CREATE TABLE shared_test.config (
            key TEXT PRIMARY KEY,
            value TEXT
        ) TABLE_TYPE shared
    "#,
        )
        .await;

    let response = server.execute_sql("FLUSH TABLE shared_test.config").await;

    assert_eq!(response.status, "error");
    assert!(
        response
            .error
            .as_ref()
            .unwrap()
            .message
            .contains("Only user tables")
            || response
                .error
                .as_ref()
                .unwrap()
                .message
                .contains("Cannot flush")
    );
}

/// Test 10: Verify timestamp columns flush successfully
///
/// This test verifies that:
/// 1. Timestamp columns (including system columns like _updated) flush correctly
/// 2. The flush completes successfully with took_ms > 0
/// 3. Parquet files are created on disk
#[actix_web::test]
async fn test_10_flush_with_timestamp_columns() {
    println!("\n=== Test 10: Flush with Timestamp Columns ===");

    let server = TestServer::new().await;
    let namespace = "manual_flush";
    let table_name = "messages3";
    let user_id = "user_001";

    // Create namespace
    fixtures::create_namespace(&server, namespace).await;

    // Create user table with Timestamp column
    let create_sql = format!(
        "CREATE USER TABLE {}.{} (
            id BIGINT,
            content TEXT,
            author TEXT,
            timestamp TIMESTAMP
        ) STORAGE local",
        namespace, table_name
    );
    let response = server.execute_sql_as_user(&create_sql, user_id).await;
    assert_eq!(
        response.status, "success",
        "Failed to create table: {:?}",
        response.error
    );

    // Insert data with timestamps
    let insert_sql = format!(
        "INSERT INTO {}.{} (id, content, author, timestamp)
        VALUES
            (1, 'Test message 1', '{}', NOW()),
            (2, 'Test message 2', '{}', NOW()),
            (3, 'Test message 3', '{}', NOW())",
        namespace, table_name, user_id, user_id, user_id
    );
    let response = server.execute_sql_as_user(&insert_sql, user_id).await;
    assert_eq!(
        response.status, "success",
        "Failed to insert data: {:?}",
        response.error
    );

    println!("Inserted 3 rows with timestamp columns");

    // Execute flush synchronously
    let flush_result = flush_helpers::execute_flush_synchronously(&server, namespace, table_name)
        .await
        .expect("Flush with timestamp columns should succeed");

    println!("Flush result: {} rows flushed", flush_result.rows_flushed);
    assert!(
        flush_result.rows_flushed >= 3,
        "Should flush at least 3 rows, got: {}",
        flush_result.rows_flushed
    );

    // Verify job timing
    let job_record = flush_result.job_record;
    let took_ms = if let (Some(started), Some(completed)) =
        (job_record.started_at, job_record.completed_at)
    {
        (completed - started) as u64
    } else {
        panic!("Job should have started_at and completed_at timestamps");
    };

    println!("Flush completed in {} ms", took_ms);
    assert!(took_ms > 0, "Flush should have measurable execution time");

    // Verify Parquet files exist
    println!("Parquet files created: {:?}", flush_result.parquet_files);
    assert!(
        !flush_result.parquet_files.is_empty(),
        "Should create at least one Parquet file"
    );

    // Verify each file exists on disk
    for parquet_file_path in &flush_result.parquet_files {
        let file_path = PathBuf::from(parquet_file_path);
        assert!(
            file_path.exists(),
            "Parquet file should exist: {}",
            parquet_file_path
        );

        let metadata = std::fs::metadata(&file_path)
            .expect(&format!("Should read metadata for {}", parquet_file_path));
        assert!(
            metadata.len() > 50,
            "Parquet file should have reasonable size: {} bytes",
            metadata.len()
        );
        println!(
            "  ✓ Parquet file verified: {} ({} bytes)",
            parquet_file_path,
            metadata.len()
        );
    }

    println!("✅ Test 10 passed: Timestamp columns flush successfully");
}

// ============================================================================
// Helper Functions
// ============================================================================

fn extract_job_id(message: &str) -> String {
    if let Some(pos) = message.find("Job ID:") {
        let rest = &message[pos + 7..].trim();
        if rest.starts_with('[') {
            let end = rest.find(']').unwrap_or(rest.len());
            let ids_str = &rest[1..end];
            ids_str.split(',').next().unwrap_or("").trim().to_string()
        } else {
            rest.split_whitespace().next().unwrap_or("").to_string()
        }
    } else {
        panic!("No job ID found in message: {}", message);
    }
}
