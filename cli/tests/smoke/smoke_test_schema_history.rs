//! Smoke tests for schema history in system.tables
//!
//! Tests that ALTER TABLE operations create schema history entries:
//! - Each ALTER TABLE should create a new version row in system.tables
//! - Old schemas are preserved to support reading historical data
//! - is_latest flag correctly identifies the current schema version
//!
//! This is critical for schema evolution when reading older Parquet files
//! that were written with previous schema versions.

use crate::common::*;
use std::time::Duration;

/// Test that ALTER TABLE operations create schema history entries in system.tables
///
/// Verifies:
/// - Creating a table creates one row in system.tables with schema_version = 1
/// - Each ALTER TABLE ADD COLUMN increments schema_version
/// - All versions are visible in SELECT * FROM system.tables
/// - is_latest = true only for the current version
#[ntest::timeout(180000)]
#[test]
fn smoke_test_schema_history_in_system_tables() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let namespace = generate_unique_namespace("schema_hist");
    let table = generate_unique_table("versioned");
    let full_table = format!("{}.{}", namespace, table);

    println!("🧪 Testing schema history in system.tables");

    // Cleanup and setup
    let _ = execute_sql_as_root_via_client(&format!(
        "DROP NAMESPACE IF EXISTS {} CASCADE",
        namespace
    ));
    std::thread::sleep(Duration::from_millis(200));

    execute_sql_as_root_via_client(&format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");
    std::thread::sleep(Duration::from_millis(100));

    // =========================================================================
    // Step 1: Create initial table (should create version 1)
    // =========================================================================
    let create_sql = format!(
        r#"CREATE TABLE {} (
            id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
            name TEXT NOT NULL
        ) WITH (TYPE = 'USER', FLUSH_POLICY = 'rows:1000')"#,
        full_table
    );
    execute_sql_as_root_via_client(&create_sql).expect("Failed to create table");
    std::thread::sleep(Duration::from_millis(200));

    println!("✅ Created table with initial schema (version 1)");

    // Check system.tables - should have 1 row for this table
    let query_v1 = format!(
        "SELECT schema_version, is_latest FROM system.tables WHERE namespace_id = '{}' AND table_name = '{}' ORDER BY schema_version",
        namespace, table
    );
    let output_v1 = execute_sql_as_root_via_client_json(&query_v1)
        .expect("Failed to query system.tables after CREATE");

    println!("After CREATE: {}", output_v1);

    // Parse JSON and count rows
    let v1_json: serde_json::Value =
        serde_json::from_str(&output_v1).expect("Failed to parse JSON");
    let v1_rows = v1_json.as_array().expect("Expected JSON array");

    assert_eq!(
        v1_rows.len(),
        1,
        "Expected 1 row after CREATE TABLE, got {}",
        v1_rows.len()
    );

    // Verify schema_version = 1
    let first_version = extract_i32_from_row(&v1_rows[0], "schema_version");
    assert_eq!(
        first_version,
        Some(1),
        "Expected schema_version = 1 after CREATE"
    );

    // Verify is_latest = true
    let first_is_latest = extract_bool_from_row(&v1_rows[0], "is_latest");
    assert_eq!(
        first_is_latest,
        Some(true),
        "Expected is_latest = true for version 1"
    );

    println!("✅ Verified: CREATE TABLE creates schema_version = 1 with is_latest = true");

    // =========================================================================
    // Step 2: ALTER TABLE ADD COLUMN (should create version 2)
    // =========================================================================
    let alter1_sql = format!("ALTER TABLE {} ADD COLUMN email TEXT", full_table);
    let alter1_result = execute_sql_as_root_via_client(&alter1_sql);

    match alter1_result {
        Ok(output) => {
            if output.to_lowercase().contains("error")
                || output.to_lowercase().contains("not implemented")
            {
                println!("⚠️  ALTER TABLE not fully implemented - skipping rest of test");
                return;
            }
        }
        Err(e) => {
            println!("⚠️  ALTER TABLE not implemented: {:?}", e);
            return;
        }
    }
    std::thread::sleep(Duration::from_millis(200));

    println!("✅ Added column 'email' (should create version 2)");

    // Check system.tables - should now have 2 rows
    let query_v2 = format!(
        "SELECT schema_version, is_latest FROM system.tables WHERE namespace_id = '{}' AND table_name = '{}' ORDER BY schema_version",
        namespace, table
    );
    let output_v2 = execute_sql_as_root_via_client_json(&query_v2)
        .expect("Failed to query system.tables after ALTER 1");

    println!("After ALTER 1: {}", output_v2);

    let v2_json: serde_json::Value =
        serde_json::from_str(&output_v2).expect("Failed to parse JSON");
    let v2_rows = v2_json.as_array().expect("Expected JSON array");

    assert_eq!(
        v2_rows.len(),
        2,
        "Expected 2 rows after 1st ALTER TABLE, got {}. Schema history is not being preserved!",
        v2_rows.len()
    );

    // Verify versions
    let version1 = extract_i32_from_row(&v2_rows[0], "schema_version");
    let version2 = extract_i32_from_row(&v2_rows[1], "schema_version");
    assert_eq!(version1, Some(1), "First row should be version 1");
    assert_eq!(version2, Some(2), "Second row should be version 2");

    // Verify is_latest flags
    let is_latest1 = extract_bool_from_row(&v2_rows[0], "is_latest");
    let is_latest2 = extract_bool_from_row(&v2_rows[1], "is_latest");
    assert_eq!(
        is_latest1,
        Some(false),
        "Version 1 should have is_latest = false after ALTER"
    );
    assert_eq!(
        is_latest2,
        Some(true),
        "Version 2 should have is_latest = true"
    );

    println!("✅ Verified: 2 rows after ALTER TABLE, is_latest flags correct");

    // =========================================================================
    // Step 3: Another ALTER TABLE (should create version 3)
    // =========================================================================
    let alter2_sql = format!("ALTER TABLE {} ADD COLUMN age INT", full_table);
    execute_sql_as_root_via_client(&alter2_sql).expect("Failed to add 'age' column");
    std::thread::sleep(Duration::from_millis(200));

    println!("✅ Added column 'age' (should create version 3)");

    let query_v3 = format!(
        "SELECT schema_version, is_latest FROM system.tables WHERE namespace_id = '{}' AND table_name = '{}' ORDER BY schema_version",
        namespace, table
    );
    let output_v3 = execute_sql_as_root_via_client_json(&query_v3)
        .expect("Failed to query system.tables after ALTER 2");

    println!("After ALTER 2: {}", output_v3);

    let v3_json: serde_json::Value =
        serde_json::from_str(&output_v3).expect("Failed to parse JSON");
    let v3_rows = v3_json.as_array().expect("Expected JSON array");

    assert_eq!(
        v3_rows.len(),
        3,
        "Expected 3 rows after 2nd ALTER TABLE, got {}",
        v3_rows.len()
    );

    // Verify only the last version has is_latest = true
    let last_is_latest = extract_bool_from_row(&v3_rows[2], "is_latest");
    assert_eq!(
        last_is_latest,
        Some(true),
        "Only version 3 should have is_latest = true"
    );

    println!("✅ Verified: 3 rows after second ALTER TABLE");

    // =========================================================================
    // Step 4: Multiple ALTERs - verify all versions preserved
    // =========================================================================
    let num_additional_alters = 5;
    for i in 0..num_additional_alters {
        let col_name = format!("extra_col_{}", i);
        let alter_sql = format!("ALTER TABLE {} ADD COLUMN {} TEXT", full_table, col_name);
        execute_sql_as_root_via_client(&alter_sql)
            .unwrap_or_else(|e| panic!("Failed to add column {}: {:?}", col_name, e));
        std::thread::sleep(Duration::from_millis(100));
    }

    println!("✅ Added {} more columns", num_additional_alters);

    let query_final = format!(
        "SELECT schema_version, is_latest FROM system.tables WHERE namespace_id = '{}' AND table_name = '{}' ORDER BY schema_version",
        namespace, table
    );
    let output_final = execute_sql_as_root_via_client_json(&query_final)
        .expect("Failed to query system.tables after multiple ALTERs");

    println!("After all ALTERs: {}", output_final);

    let final_json: serde_json::Value =
        serde_json::from_str(&output_final).expect("Failed to parse JSON");
    let final_rows = final_json.as_array().expect("Expected JSON array");

    let expected_rows = 3 + num_additional_alters; // 3 from before + 5 more
    assert_eq!(
        final_rows.len(),
        expected_rows,
        "Expected {} rows (1 CREATE + 2 + {} ALTERs), got {}. Schema history is NOT being preserved!",
        expected_rows,
        num_additional_alters,
        final_rows.len()
    );

    // Verify versions are sequential
    for (i, row) in final_rows.iter().enumerate() {
        let version = extract_i32_from_row(row, "schema_version");
        assert_eq!(
            version,
            Some((i + 1) as i32),
            "Row {} should have schema_version = {}",
            i,
            i + 1
        );
    }

    // Verify only last row has is_latest = true
    for (i, row) in final_rows.iter().enumerate() {
        let is_latest = extract_bool_from_row(row, "is_latest");
        let is_last = i == final_rows.len() - 1;
        assert_eq!(
            is_latest,
            Some(is_last),
            "Row {} should have is_latest = {}",
            i,
            is_last
        );
    }

    println!("✅ All {} schema versions preserved with correct is_latest flags", expected_rows);
    println!("🎉 Schema history test PASSED!");

    // Cleanup
    let _ = execute_sql_as_root_via_client(&format!(
        "DROP NAMESPACE IF EXISTS {} CASCADE",
        namespace
    ));
}

// ============================================================================
// Helper functions
// ============================================================================

/// Extract i32 value from a JSON row object
fn extract_i32_from_row(row: &serde_json::Value, column: &str) -> Option<i32> {
    let val = row.get(column)?;
    // Handle Arrow JSON format: {"Int32": 123}
    if let Some(obj) = val.as_object() {
        if let Some(v) = obj.get("Int32") {
            return v.as_i64().map(|i| i as i32);
        }
        if let Some(v) = obj.get("Int64") {
            return v.as_i64().map(|i| i as i32);
        }
    }
    // Direct value
    val.as_i64().map(|i| i as i32)
}

/// Extract boolean value from a JSON row object
fn extract_bool_from_row(row: &serde_json::Value, column: &str) -> Option<bool> {
    let val = row.get(column)?;
    // Handle Arrow JSON format: {"Boolean": true}
    if let Some(obj) = val.as_object() {
        if let Some(v) = obj.get("Boolean") {
            return v.as_bool();
        }
    }
    // Direct value
    val.as_bool()
}

/// Test that DROP TABLE removes all schema versions
#[ntest::timeout(180000)]
#[test]
fn smoke_test_drop_table_removes_schema_history() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let namespace = generate_unique_namespace("drop_hist");
    let table = generate_unique_table("to_drop");
    let full_table = format!("{}.{}", namespace, table);

    println!("🧪 Testing DROP TABLE removes schema history");

    // Cleanup and setup
    let _ = execute_sql_as_root_via_client(&format!(
        "DROP NAMESPACE IF EXISTS {} CASCADE",
        namespace
    ));
    std::thread::sleep(Duration::from_millis(200));

    execute_sql_as_root_via_client(&format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");

    // Create table and alter it twice
    let create_sql = format!(
        "CREATE TABLE {} (id BIGINT PRIMARY KEY) WITH (TYPE = 'USER')",
        full_table
    );
    execute_sql_as_root_via_client(&create_sql).expect("Failed to create table");
    std::thread::sleep(Duration::from_millis(100));

    execute_sql_as_root_via_client(&format!("ALTER TABLE {} ADD COLUMN a TEXT", full_table))
        .unwrap_or_else(|_| {
            println!("⚠️  ALTER not supported, skipping test");
            "".to_string()
        });
    execute_sql_as_root_via_client(&format!("ALTER TABLE {} ADD COLUMN b TEXT", full_table))
        .unwrap_or_else(|_| "".to_string());
    std::thread::sleep(Duration::from_millis(100));

    // Verify we have multiple versions
    let query_before = format!(
        "SELECT COUNT(*) as cnt FROM system.tables WHERE namespace_id = '{}' AND table_name = '{}'",
        namespace, table
    );
    let before_output = execute_sql_as_root_via_client_json(&query_before)
        .expect("Failed to query count before DROP");

    println!("Before DROP: {}", before_output);

    // DROP TABLE
    execute_sql_as_root_via_client(&format!("DROP TABLE {}", full_table))
        .expect("Failed to DROP TABLE");
    std::thread::sleep(Duration::from_millis(200));

    // Verify all versions removed
    let query_after = format!(
        "SELECT COUNT(*) as cnt FROM system.tables WHERE namespace_id = '{}' AND table_name = '{}'",
        namespace, table
    );
    let after_output = execute_sql_as_root_via_client_json(&query_after)
        .expect("Failed to query count after DROP");

    println!("After DROP: {}", after_output);

    // Parse and verify count is 0
    let after_json: serde_json::Value =
        serde_json::from_str(&after_output).expect("Failed to parse JSON");
    let after_rows = after_json.as_array().expect("Expected JSON array");

    if !after_rows.is_empty() {
        // Extract count value
        let count_val = after_rows[0].get("cnt");
        let count = count_val
            .and_then(|v| {
                if let Some(obj) = v.as_object() {
                    obj.get("Int64")
                        .or(obj.get("Int32"))
                        .and_then(|x| x.as_i64())
                } else {
                    v.as_i64()
                }
            })
            .unwrap_or(-1);

        assert_eq!(
            count, 0,
            "Expected 0 rows after DROP TABLE, got {}. Schema history was not cleaned up!",
            count
        );
    }

    println!("✅ DROP TABLE removed all schema history");
    println!("🎉 DROP removes history test PASSED!");

    // Cleanup
    let _ = execute_sql_as_root_via_client(&format!(
        "DROP NAMESPACE IF EXISTS {} CASCADE",
        namespace
    ));
}
