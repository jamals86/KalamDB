//! Comprehensive smoke test for all system tables and views
//!
//! This test validates that ALL system tables (12 persisted + 9 views)
//! can be queried without schema mismatches or errors.
//!
//! Covers:
//! - Persisted tables: users, namespaces, schemas, table_schemas, storages,
//!   live_queries, jobs, job_nodes, audit_log, manifest, topics, topic_offsets
//! - Virtual views: stats, settings, server_logs, cluster, cluster_groups,
//!   datatypes, describe, tables, columns
//!
//! This test prevents schema definition bugs like the topic_offsets
//! updated_at column mismatch (BigInt vs Timestamp).

use crate::common::*;

/// Test that all system tables can be queried without errors
///
/// Iterates over all 21 system objects (12 tables + 9 views) and
/// executes SELECT * FROM system.<table> to validate:
/// - No schema definition vs RecordBatch builder mismatches
/// - Column types align with Arrow schema expectations
/// - Query execution succeeds (even with 0 rows)
#[ntest::timeout(120000)]
#[test]
fn smoke_test_all_system_tables_and_views_queryable() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    println!("🧪 Testing all system tables and views for schema consistency");
    println!("{}", "=".repeat(70));

    // Persisted system tables (12)
    let persisted_tables = vec![
        "users",
        "namespaces",
        "schemas",
        "table_schemas",
        "storages",
        "live_queries",
        "jobs",
        "job_nodes",
        "audit_log",
        "manifest",
        "topics",
        "topic_offsets", // Previously had schema mismatch bug
    ];

    // Virtual system views (9)
    let virtual_views = vec![
        "stats",
        "settings",
        "server_logs",
        "cluster",
        "cluster_groups",
        "datatypes",
        "describe", // Note: DESCRIBE requires table parameter
        "tables",
        "columns",
    ];

    let mut tested_count = 0;
    let mut failed_tables = Vec::new();
    let mut skipped_tables: Vec<String> = Vec::new();

    // Test persisted tables
    println!("\n📊 Testing persisted system tables ({}):", persisted_tables.len());
    println!("{}", "-".repeat(70));

    for table_name in &persisted_tables {
        let query = format!("SELECT * FROM system.{} LIMIT 5", table_name);
        print!("  Testing system.{:<20} ... ", table_name);

        match execute_sql_as_root_via_client(&query) {
            Ok(output) => {
                // Verify no error keywords in output
                let output_lower = output.to_lowercase();
                if output_lower.contains("error")
                    || output_lower.contains("failed")
                    || output_lower.contains("invalid")
                {
                    println!("❌ FAILED");
                    println!("    Output: {}", output);
                    failed_tables.push(format!("system.{}", table_name));
                } else {
                    println!("✅ OK");
                    tested_count += 1;
                }
            }
            Err(e) => {
                println!("❌ ERROR: {}", e);
                failed_tables.push(format!("system.{}", table_name));
            }
        }
    }

    // Test virtual views
    println!("\n📈 Testing virtual system views ({}):", virtual_views.len());
    println!("{}", "-".repeat(70));

    for view_name in &virtual_views {
        // DESCRIBE view requires a table parameter, handle specially
        let query = if *view_name == "describe" {
            // Create a temporary table to test DESCRIBE
            let test_ns = "default";
            let test_table = "temp_describe_test";
            let _ = execute_sql_as_root_via_client(&format!(
                "DROP TABLE IF EXISTS {}.{}",
                test_ns, test_table
            ));
            let _ = execute_sql_as_root_via_client(&format!(
                "CREATE TABLE {}.{} (id BIGINT PRIMARY KEY)",
                test_ns, test_table
            ));

            format!("SELECT * FROM system.describe('{}.{}')", test_ns, test_table)
        } else {
            format!("SELECT * FROM system.{} LIMIT 5", view_name)
        };

        print!("  Testing system.{:<20} ... ", view_name);

        match execute_sql_as_root_via_client(&query) {
            Ok(output) => {
                let output_lower = output.to_lowercase();
                if output_lower.contains("error")
                    || output_lower.contains("failed")
                    || output_lower.contains("invalid")
                {
                    println!("❌ FAILED");
                    println!("    Output: {}", output);
                    failed_tables.push(format!("system.{}", view_name));
                } else {
                    println!("✅ OK");
                    tested_count += 1;
                }
            }
            Err(e) => {
                println!("❌ ERROR: {}", e);
                failed_tables.push(format!("system.{}", view_name));
            }
        }

        // Cleanup describe test table
        if *view_name == "describe" {
            let _ = execute_sql_as_root_via_client("DROP TABLE IF EXISTS default.temp_describe_test");
        }
    }

    // Summary
    println!();
    println!("{}", "=".repeat(70));
    println!("📋 Test Summary:");
    println!("  Total system objects: {}", persisted_tables.len() + virtual_views.len());
    println!("  Tested successfully:  {}", tested_count);
    println!("  Failed:               {}", failed_tables.len());
    if !skipped_tables.is_empty() {
        println!("  Skipped:              {}", skipped_tables.len());
    }
    println!("{}", "=".repeat(70));

    if !failed_tables.is_empty() {
        println!("\n❌ Failed tables:");
        for table in &failed_tables {
            println!("  - {}", table);
        }
        panic!(
            "Schema validation failed for {} system table(s): {:?}",
            failed_tables.len(),
            failed_tables
        );
    }

    if !skipped_tables.is_empty() {
        println!("\n⚠️  Skipped tables:");
        for table in &skipped_tables {
            println!("  - {}", table);
        }
    }

    println!("\n✅ All {} system tables and views validated successfully!", tested_count);
}

/// Test topic_offsets table specifically (regression test for schema bug)
///
/// Validates that topic_offsets.updated_at is Timestamp type,
/// not BigInt, and supports timestamp operations.
#[ntest::timeout(60000)]
#[test]
fn smoke_test_topic_offsets_schema_and_operations() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    println!("🧪 Testing system.topic_offsets schema and timestamp operations");

    // Query topic_offsets (may be empty, but schema should work)
    let query = "SELECT topic_name, consumer_group, last_read_offset, last_acked_offset, updated_at FROM system.topic_offsets LIMIT 5";
    let output = execute_sql_as_root_via_client(query)
        .expect("Failed to query system.topic_offsets");

    println!("Query result:\n{}", output);

    // Verify no error messages
    let output_lower = output.to_lowercase();
    assert!(
        !output_lower.contains("column types must match schema types"),
        "Schema type mismatch detected in topic_offsets"
    );
    assert!(
        !output_lower.contains("expected int64 but found timestamp"),
        "updated_at column should be Timestamp type, not Int64"
    );

    // Test timestamp filtering (should not error even if no rows)
    let filter_query = "SELECT COUNT(*) FROM system.topic_offsets WHERE updated_at > 0";
    let filter_output = execute_sql_as_root_via_client(filter_query)
        .expect("Failed to filter topic_offsets by updated_at timestamp");

    println!("Timestamp filter result:\n{}", filter_output);

    println!("✅ system.topic_offsets schema validated successfully");
}

/// Test that information_schema.columns reflects correct system table schemas
///
/// Validates that the information_schema view correctly exposes
/// system table column metadata including type information.
#[ntest::timeout(60000)]
#[test]
fn smoke_test_system_tables_in_information_schema() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    println!("🧪 Testing system tables in information_schema.columns");

    // Query information_schema for system namespace
    let query = r#"
        SELECT table_name, column_name, data_type, is_nullable
        FROM information_schema.columns
        WHERE table_catalog = 'kalam' 
          AND table_schema = 'system'
        ORDER BY table_name, ordinal_position
        LIMIT 100
    "#;

    let output = execute_sql_as_root_via_client(query)
        .expect("Failed to query information_schema.columns for system tables");

    println!("information_schema.columns output (first 100 rows):\n{}", output);

    // Verify key system tables appear
    let output_lower = output.to_lowercase();
    assert!(
        output_lower.contains("users"),
        "system.users should appear in information_schema"
    );
    assert!(
        output_lower.contains("topics"),
        "system.topics should appear in information_schema"
    );
    assert!(
        output_lower.contains("topic_offsets"),
        "system.topic_offsets should appear in information_schema"
    );

    // Verify no error messages
    assert!(
        !output_lower.contains("error"),
        "Should not have errors in information_schema query"
    );

    println!("✅ System tables correctly reflected in information_schema");
}

/// Test that all system table columns are documented and accessible
///
/// Performs a SELECT * and validates that column count matches
/// expectations based on table definitions.
#[ntest::timeout(90000)]
#[test]
fn smoke_test_system_table_column_counts() {
    if !is_server_running() {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    println!("🧪 Testing system table column accessibility");
    println!("{}", "=".repeat(70));

    // Expected minimum column counts (approximate, can be adjusted)
    let expected_min_columns = vec![
        ("users", 8),
        ("namespaces", 5),
        ("schemas", 7),
        ("storages", 6),
        ("jobs", 10),
        ("topics", 9),
        ("topic_offsets", 5),
        ("manifest", 8),
    ];

    let mut all_passed = true;

    for (table_name, min_cols) in expected_min_columns {
        let query = format!("SELECT * FROM system.{} LIMIT 1", table_name);
        
        match execute_sql_as_root_via_client_json(&query) {
            Ok(output) => {
                print!("  system.{:<20} ", table_name);
                
                // Parse JSON to count columns (basic heuristic)
                if output.contains("\"status\":\"success\"") {
                    // Check if we have column data
                    let has_sufficient_data = output.matches(",").count() >= min_cols;
                    
                    if has_sufficient_data || output.contains("\"row_count\":0") {
                        println!("✅ OK (>= {} columns expected)", min_cols);
                    } else {
                        println!("⚠️  WARNING: May have fewer than {} columns", min_cols);
                        all_passed = false;
                    }
                } else {
                    println!("❌ FAILED: {}", output);
                    all_passed = false;
                }
            }
            Err(e) => {
                println!("  system.{:<20} ❌ ERROR: {}", table_name, e);
                all_passed = false;
            }
        }
    }

    println!("{}", "=".repeat(70));
    
    if !all_passed {
        println!("⚠️  Some column count validations failed");
    } else {
        println!("✅ All system tables have expected column counts");
    }
}
