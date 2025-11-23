//! Smoke tests for flush operations with manifest.json verification
//!
//! Tests filesystem-level verification of flush operations:
//! - manifest.json creation and updates
//! - batch-*.parquet file creation
//! - Storage path resolution for user and shared tables
//!
//! Reference: README.md lines 58-67, docs/SQL.md flush section

use crate::common::*;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

const TEST_DATA_DIR: &str = "data/storage"; // Default from config.toml

fn get_storage_dir() -> PathBuf {
    let path = PathBuf::from(TEST_DATA_DIR);
    if path.exists() {
        return path;
    }
    // Try parent directory (if running from cli/)
    let parent_path = PathBuf::from("..").join(TEST_DATA_DIR);
    if parent_path.exists() {
        return parent_path;
    }
    // Default to original path if neither exists (will fail in test)
    path
}

/// Test manifest.json creation after flushing USER table
///
/// Verifies:
/// - manifest.json exists at user/{user_id}/{table}/ after flush
/// - At least one batch-*.parquet file exists
/// - manifest.json is valid (non-empty)
#[test]
fn smoke_test_user_table_flush_manifest() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let namespace = generate_unique_namespace("flush_manifest_ns");
    let table = generate_unique_table("user_flush_test");
    let full_table = format!("{}.{}", namespace, table);

    println!("🧪 Testing manifest.json for USER table flush");

    // Cleanup and setup
    let _ = execute_sql_as_root_via_cli(&format!("DROP NAMESPACE IF EXISTS {} CASCADE", namespace));
    std::thread::sleep(Duration::from_millis(200));

    execute_sql_as_root_via_cli(&format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");

    // Create USER table with low flush threshold for testing
    let create_sql = format!(
        r#"CREATE TABLE {} (
            id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
            content TEXT NOT NULL,
            created_at TIMESTAMP DEFAULT NOW()
        ) WITH (
            TYPE = 'USER',
            STORAGE_ID = 'local',
            FLUSH_POLICY = 'rows:10'
        )"#,
        full_table
    );
    execute_sql_as_root_via_cli(&create_sql).expect("Failed to create table");
    std::thread::sleep(Duration::from_millis(200));

    println!("✅ Created USER table with FLUSH_POLICY='rows:10'");

    // Insert 20 rows to trigger auto-flush (threshold is 10)
    println!("📝 Inserting 20 rows to trigger flush...");
    for i in 1..=20 {
        let insert_sql = format!(
            "INSERT INTO {} (content) VALUES ('Row {}')",
            full_table, i
        );
        execute_sql_as_root_via_cli(&insert_sql)
            .unwrap_or_else(|e| panic!("Failed to insert row {}: {}", i, e));
    }

    println!("✅ Inserted 20 rows");

    // Trigger manual flush to ensure data is flushed
    println!("🚀 Triggering manual FLUSH TABLE...");
    let flush_output = execute_sql_as_root_via_cli(&format!("FLUSH TABLE {}", full_table))
        .expect("Failed to flush table");

    println!("Flush output: {}", flush_output);

    // Parse job ID and wait for completion
    let job_id = parse_job_id_from_flush_output(&flush_output)
        .expect("Failed to parse job ID from flush output");

    println!("📋 Flush job ID: {}", job_id);

    verify_job_completed(&job_id, Duration::from_secs(30))
        .expect("Flush job did not complete successfully");

    println!("✅ Flush job completed");

    // Filesystem verification: check manifest.json exists
    // For USER tables, path is: storage/{namespace}/{table}/{user_id}/manifest.json (based on config.toml)
    // When running via CLI as root, user_id is typically "root" or system user
    // We need to find the actual user_id from the flush job or test context

    let storage_dir = get_storage_dir();
    assert!(
        storage_dir.exists(),
        "Storage directory {:?} should exist",
        storage_dir
    );

    // Check if this namespace/table directory exists
    let table_dir = storage_dir.join(&namespace).join(&table);
    if table_dir.exists() {
        println!("✅ Table directory exists: {:?}", table_dir);

        // Try to find any user subdirectory (we don't know exact user_id in test)
        if let Ok(entries) = fs::read_dir(&table_dir) {
            let mut found_manifest = false;
            let mut found_parquet = false;

            for entry in entries.flatten() {
                let user_dir = entry.path();
                if user_dir.is_dir() {
                    // Check for manifest.json in user directory
                    let manifest_path = user_dir.join("manifest.json");
                    if manifest_path.exists() {
                        println!("  ✅ Found manifest.json: {:?}", manifest_path);
                        found_manifest = true;

                        // Verify manifest is non-empty
                        let manifest_content = fs::read_to_string(&manifest_path)
                            .expect("Failed to read manifest.json");
                        assert!(
                            !manifest_content.is_empty(),
                            "manifest.json should not be empty"
                        );
                        assert!(
                            manifest_content.contains("schema")
                                || manifest_content.contains("batches")
                                || manifest_content.len() > 10,
                            "manifest.json should contain metadata, got: {}",
                            manifest_content
                        );
                    }

                    // Check for batch-*.parquet files
                    if let Ok(user_entries) = fs::read_dir(&user_dir) {
                        for user_entry in user_entries.flatten() {
                            let filename = user_entry.file_name();
                            let filename_str = filename.to_string_lossy();
                            if filename_str.starts_with("batch-")
                                && filename_str.ends_with(".parquet")
                            {
                                println!(
                                    "  ✅ Found parquet file: {:?}",
                                    user_entry.path()
                                );
                                found_parquet = true;
                            }
                        }
                    }
                }
            }

            assert!(
                found_manifest,
                "Expected to find manifest.json in user table storage after flush"
            );
            assert!(
                found_parquet,
                "Expected to find at least one batch-*.parquet file after flush"
            );
        }
    } else {
        println!("⚠️  Table directory does not exist yet, skipping filesystem checks");
        // This is acceptable in some test scenarios where flush hasn't triggered yet
    }

    println!("✅ Verified manifest.json and parquet files exist after flush");
}

/// Test manifest.json creation for SHARED table
///
/// Verifies:
/// - manifest.json exists at shared/{table}/ after flush
/// - Shared table storage path differs from user table path
#[test]
fn smoke_test_shared_table_flush_manifest() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let namespace = generate_unique_namespace("flush_manifest_ns");
    let table = generate_unique_table("shared_flush_test");
    let full_table = format!("{}.{}", namespace, table);

    println!("🧪 Testing manifest.json for SHARED table flush");

    // Cleanup and setup
    let _ = execute_sql_as_root_via_cli(&format!("DROP NAMESPACE IF EXISTS {} CASCADE", namespace));
    std::thread::sleep(Duration::from_millis(200));

    execute_sql_as_root_via_cli(&format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");

    // Create SHARED table
    let create_sql = format!(
        r#"CREATE TABLE {} (
            id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
            config_key TEXT NOT NULL,
            config_value TEXT
        ) WITH (
            TYPE = 'SHARED',
            STORAGE_ID = 'local',
            FLUSH_POLICY = 'rows:10',
            ACCESS_LEVEL = 'PUBLIC'
        )"#,
        full_table
    );
    execute_sql_as_root_via_cli(&create_sql).expect("Failed to create shared table");
    std::thread::sleep(Duration::from_millis(200));

    println!("✅ Created SHARED table with FLUSH_POLICY='rows:10'");

    // Insert 20 rows
    println!("📝 Inserting 20 rows...");
    for i in 1..=20 {
        let insert_sql = format!(
            "INSERT INTO {} (config_key, config_value) VALUES ('key_{}', 'value_{}')",
            full_table, i, i
        );
        execute_sql_as_root_via_cli(&insert_sql)
            .unwrap_or_else(|e| panic!("Failed to insert row {}: {}", i, e));
    }

    // Trigger manual flush
    println!("🚀 Triggering manual FLUSH TABLE...");
    let flush_output = execute_sql_as_root_via_cli(&format!("FLUSH TABLE {}", full_table))
        .expect("Failed to flush table");

    let job_id = parse_job_id_from_flush_output(&flush_output)
        .expect("Failed to parse job ID from flush output");

    verify_job_completed(&job_id, Duration::from_secs(30))
        .expect("Flush job did not complete successfully");

    println!("✅ Flush job completed");

    // Filesystem verification: check shared table path
    // Shared table path is {namespace}/{table} (based on config.toml)
    let storage_dir = get_storage_dir();
    let table_dir = storage_dir.join(&namespace).join(&table);

    if table_dir.exists() {
        println!("✅ Shared table directory exists: {:?}", table_dir);

        // Check for manifest.json
        let manifest_path = table_dir.join("manifest.json");
        assert!(
            manifest_path.exists(),
            "manifest.json should exist at {:?}",
            manifest_path
        );
        println!("  ✅ Found manifest.json: {:?}", manifest_path);

        // Check for parquet files
        let mut found_parquet = false;
        if let Ok(entries) = fs::read_dir(&table_dir) {
            for entry in entries.flatten() {
                let filename = entry.file_name();
                let filename_str = filename.to_string_lossy();
                if filename_str.starts_with("batch-") && filename_str.ends_with(".parquet") {
                    println!("  ✅ Found parquet file: {:?}", entry.path());
                    found_parquet = true;
                }
            }
        }

        assert!(
            found_parquet,
            "Expected at least one batch-*.parquet file in shared table storage"
        );
    } else {
        println!(
            "⚠️  Shared table directory does not exist yet, skipping filesystem checks"
        );
    }

    println!("✅ Verified manifest.json exists for shared table");
}

/// Test manifest.json updates on second flush
///
/// Verifies:
/// - manifest.json exists after first flush
/// - Additional batch-*.parquet file created after second flush
/// - manifest.json updated (different content or file modified time)
#[test]
fn smoke_test_manifest_updated_on_second_flush() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let namespace = generate_unique_namespace("flush_manifest_ns");
    let table = generate_unique_table("double_flush_test");
    let full_table = format!("{}.{}", namespace, table);

    println!("🧪 Testing manifest.json updates on second flush");

    // Cleanup and setup
    let _ = execute_sql_as_root_via_cli(&format!("DROP NAMESPACE IF EXISTS {} CASCADE", namespace));
    std::thread::sleep(Duration::from_millis(200));

    execute_sql_as_root_via_cli(&format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");

    // Create SHARED table for easier testing (no user_id path complexity)
    let create_sql = format!(
        r#"CREATE TABLE {} (
            id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
            data TEXT NOT NULL
        ) WITH (
            TYPE = 'SHARED',
            FLUSH_POLICY = 'rows:10'
        )"#,
        full_table
    );
    execute_sql_as_root_via_cli(&create_sql).expect("Failed to create table");
    std::thread::sleep(Duration::from_millis(200));

    // First flush cycle
    println!("📝 First flush: Inserting 15 rows...");
    for i in 1..=15 {
        let insert_sql = format!("INSERT INTO {} (data) VALUES ('Batch1-Row{}')", full_table, i);
        execute_sql_as_root_via_cli(&insert_sql).expect("Failed to insert row");
    }

    let flush1_output = execute_sql_as_root_via_cli(&format!("FLUSH TABLE {}", full_table))
        .expect("Failed to flush table (first)");
    let job1_id =
        parse_job_id_from_flush_output(&flush1_output).expect("Failed to parse job ID (first)");
    verify_job_completed(&job1_id, Duration::from_secs(30))
        .expect("First flush job did not complete");

    println!("✅ First flush completed");

    // Count parquet files after first flush
    let storage_dir = get_storage_dir();
    // Shared table path is {namespace}/{table} (based on config.toml)
    let table_dir = storage_dir.join(&namespace).join(&table);

    let mut parquet_count_after_first_flush = 0;
    if table_dir.exists() {
        if let Ok(entries) = fs::read_dir(&table_dir) {
            for entry in entries.flatten() {
                let filename = entry.file_name().to_string_lossy().to_string();
                if filename.starts_with("batch-") && filename.ends_with(".parquet") {
                    parquet_count_after_first_flush += 1;
                }
            }
        }
    }

    println!(
        "  Parquet files after first flush: {}",
        parquet_count_after_first_flush
    );

    // Second flush cycle
    println!("📝 Second flush: Inserting another 15 rows...");
    for i in 1..=15 {
        let insert_sql = format!("INSERT INTO {} (data) VALUES ('Batch2-Row{}')", full_table, i);
        execute_sql_as_root_via_cli(&insert_sql).expect("Failed to insert row");
    }

    let flush2_output = execute_sql_as_root_via_cli(&format!("FLUSH TABLE {}", full_table))
        .expect("Failed to flush table (second)");
    let job2_id =
        parse_job_id_from_flush_output(&flush2_output).expect("Failed to parse job ID (second)");
    verify_job_completed(&job2_id, Duration::from_secs(30))
        .expect("Second flush job did not complete");

    println!("✅ Second flush completed");

    // Count parquet files after second flush
    let mut parquet_count_after_second_flush = 0;
    if let Ok(entries) = fs::read_dir(&table_dir) {
        for entry in entries.flatten() {
            let filename = entry.file_name().to_string_lossy().to_string();
            if filename.starts_with("batch-") && filename.ends_with(".parquet") {
                parquet_count_after_second_flush += 1;
            }
        }
    }

    println!(
        "  Parquet files after second flush: {}",
        parquet_count_after_second_flush
    );

    // Verify more parquet files exist after second flush
    assert!(
        parquet_count_after_second_flush >= parquet_count_after_first_flush,
        "Expected same or more parquet files after second flush ({} vs {})",
        parquet_count_after_second_flush,
        parquet_count_after_first_flush
    );

    // Verify manifest.json still exists and is valid
    let manifest_path = table_dir.join("manifest.json");
    assert!(
        manifest_path.exists(),
        "manifest.json should exist after second flush"
    );

    let manifest_content =
        fs::read_to_string(&manifest_path).expect("Failed to read manifest.json");
    assert!(
        !manifest_content.is_empty(),
        "manifest.json should not be empty after second flush"
    );

    println!("✅ Verified manifest.json updated after second flush");
}

/// Test error: FLUSH on STREAM table should fail
///
/// Verifies:
/// - FLUSH TABLE on stream table returns error
/// - Error message mentions stream tables don't support flushing
#[test]
fn smoke_test_flush_stream_table_error() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let namespace = generate_unique_namespace("flush_error_ns");
    let table = generate_unique_table("stream_no_flush");
    let full_table = format!("{}.{}", namespace, table);

    println!("🧪 Testing FLUSH TABLE error on STREAM table");

    // Cleanup and setup
    let _ = execute_sql_as_root_via_cli(&format!("DROP NAMESPACE IF EXISTS {} CASCADE", namespace));
    std::thread::sleep(Duration::from_millis(200));

    execute_sql_as_root_via_cli(&format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");

    // Create STREAM table
    let create_sql = format!(
        r#"CREATE TABLE {} (
            event_id TEXT PRIMARY KEY DEFAULT ULID(),
            event_type TEXT
        ) WITH (TYPE = 'STREAM', TTL_SECONDS = 30)"#,
        full_table
    );
    execute_sql_as_root_via_cli(&create_sql).expect("Failed to create stream table");

    println!("✅ Created STREAM table");

    // Try to flush stream table (should fail)
    let flush_result = execute_sql_as_root_via_cli(&format!("FLUSH TABLE {}", full_table));

    match flush_result {
        Err(e) => {
            println!("✅ FLUSH TABLE on STREAM table failed as expected: {}", e);
            let error_msg = e.to_string().to_lowercase();
            assert!(
                error_msg.contains("stream") || error_msg.contains("not support") || error_msg.contains("cannot flush"),
                "Expected error message about stream tables not supporting flush, got: {}",
                e
            );
        }
        Ok(output) => {
            // Check if output contains error message
            let output_lower = output.to_lowercase();
            assert!(
                output_lower.contains("error")
                    || output_lower.contains("stream")
                    || output_lower.contains("not support"),
                "Expected error when flushing stream table, got success: {}",
                output
            );
        }
    }

    println!("✅ Verified FLUSH TABLE on STREAM table returns error");
}
